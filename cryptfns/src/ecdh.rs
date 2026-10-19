//! Hybrid file-key wrapping: X25519 ECDH combined with ML-KEM-768.
//!
//! A file key is wrapped to a recipient under *both* an X25519 shared secret
//! and an ML-KEM-768 encapsulation. An adversary who harvests a wrap today and
//! later builds a quantum computer still faces X25519; an adversary who breaks
//! ML-KEM still faces X25519. The wrap is only as weak as the stronger of the
//! two, which is the whole reason it is hybrid rather than pure post-quantum —
//! every Rust ML-KEM implementation is still pre-1.0 and unaudited.
//!
//! ```text
//! eph_sk, eph_pk = X25519 keygen                       (fresh per wrap)
//! ss_x           = X25519(eph_sk, recipient.x_pk)      (32 B)
//! mk_ct, ss_k    = ML-KEM-768.Encaps(recipient.mk_ek)  (ct 1088 B, ss 32 B)
//! ikm            = ss_k ‖ ss_x                          (ML-KEM first)
//! salt           = eph_pk ‖ mk_ct ‖ x_pk ‖ mk_ek
//! wrap_key       = HKDF-SHA256(ikm, salt, info = WRAP_INFO)
//! blob           = 0x02 ‖ eph_pk ‖ mk_ct ‖ nonce ‖ AEGIS-256(wrap_key ‖ nonce, file_key)
//! ```
//!
//! ML-KEM is not key-binding: one ciphertext can decapsulate under different
//! keys. Binding `mk_ct` and both long-term public keys into the HKDF salt ties
//! the derived key to this exact recipient, closing that gap. Decapsulation uses
//! implicit rejection and never fails — a wrong key yields a wrong `ss_k` and the
//! AEGIS tag, not the KEM, is what rejects it. The wrap AEAD is fixed to
//! AEGIS-256; the construction is versioned through [`WRAP_INFO`] and the leading
//! blob byte, never negotiated per wrap.

use crate::error::{CryptoResult, Error};

use hkdf::Hkdf;
use libcrux_ml_kem::mlkem768::{self, MlKem768Ciphertext, MlKem768PrivateKey, MlKem768PublicKey};
use libcrux_ml_kem::{KEY_GENERATION_SEED_SIZE, SHARED_SECRET_SIZE};
use pkcs8::der::pem::{decode_vec, encode_string, LineEnding};
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};
use zeroize::Zeroizing;

const X_PUBLIC_LEN: usize = 32;
const X_SECRET_LEN: usize = 32;
const MK_EK_LEN: usize = 1184;
const MK_DK_LEN: usize = 2400;
const MK_CT_LEN: usize = 1088;
/// Offset of the ML-KEM encapsulation key inside the decapsulation key. FIPS 203
/// stores `dk = dk_PKE ‖ ek ‖ H(ek) ‖ z`, and for ML-KEM-768 `dk_PKE` is 1152 B,
/// so the 1184-byte `ek` begins here. Recovering `ek` from `dk` lets the private
/// container omit it, keeping the sealed bundle to `x_sk ‖ mk_dk`.
const MK_DK_EK_OFFSET: usize = 1152;

const NONCE_LENGTH: usize = 32;
const TAG_LENGTH: usize = 16;

const CONTAINER_VERSION: u8 = 1;
/// Leading byte of a wrap blob. `0x01` is reserved for the X25519-only format
/// that never shipped and is rejected outright — there are no `0x01` blobs to be
/// compatible with, so accepting one could only ever be a downgrade.
const BLOB_VERSION: u8 = 0x02;

const PUBLIC_LABEL: &str = "JIAM WRAPPING KEY";
const PRIVATE_LABEL: &str = "JIAM WRAPPING PRIVATE KEY";

const PUBLIC_CONTAINER_LEN: usize = 1 + X_PUBLIC_LEN + MK_EK_LEN;
const PRIVATE_CONTAINER_LEN: usize = 1 + X_SECRET_LEN + MK_DK_LEN;

const WRAP_INFO: &[u8] = b"jiamiyunpan-file-key-wrap-v2";

/// Wrap a file key for a recipient's hybrid wrapping public key (PEM container).
pub fn wrap(file_key: &[u8], recipient_public_key: &str) -> CryptoResult<String> {
    let container = public::from_str(recipient_public_key)?;
    let x_pk = &container[1..1 + X_PUBLIC_LEN];
    let mk_ek = &container[1 + X_PUBLIC_LEN..];
    let recipient_x = PublicKey::from(to_array::<X_PUBLIC_LEN>(x_pk)?);
    let recipient_mk =
        MlKem768PublicKey::try_from(mk_ek).map_err(|_| Error::InvalidLength("mk_ek length"))?;

    let ephemeral = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
    let ephemeral_public = PublicKey::from(&ephemeral);
    let ss_x = ephemeral.diffie_hellman(&recipient_x);
    reject_non_contributory(&ss_x)?;

    let mut encaps_seed = Zeroizing::new([0u8; SHARED_SECRET_SIZE]);
    getrandom::getrandom(encaps_seed.as_mut_slice())?;
    let (mk_ct, ss_k) = mlkem768::encapsulate(&recipient_mk, *encaps_seed);
    let ss_k = Zeroizing::new(ss_k);

    let wrap_key = derive_wrap_key(
        ss_k.as_slice(),
        ss_x.as_bytes(),
        ephemeral_public.as_bytes(),
        mk_ct.as_ref(),
        x_pk,
        mk_ek,
    )?;

    let mut nonce = vec![0u8; NONCE_LENGTH];
    getrandom::getrandom(&mut nonce)?;

    let mut aead_key = Zeroizing::new(wrap_key.to_vec());
    aead_key.extend_from_slice(&nonce);
    let ciphertext = crate::aegis256::encrypt(aead_key.to_vec(), file_key.to_vec())?;

    let mut blob = Vec::with_capacity(1 + X_PUBLIC_LEN + MK_CT_LEN + NONCE_LENGTH + ciphertext.len());
    blob.push(BLOB_VERSION);
    blob.extend_from_slice(ephemeral_public.as_bytes());
    blob.extend_from_slice(mk_ct.as_ref());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);

    Ok(crate::base64::encode(blob))
}

/// Unwrap a file key with the recipient's hybrid wrapping private key (PEM).
pub fn unwrap(blob: &str, private_key: &str) -> CryptoResult<Vec<u8>> {
    let blob = crate::base64::decode(blob)?;
    if blob.len() < 1 + X_PUBLIC_LEN + MK_CT_LEN + NONCE_LENGTH + TAG_LENGTH {
        return Err(Error::InvalidLength("hybrid wrap blob too short"));
    }
    if blob[0] != BLOB_VERSION {
        return Err(Error::KeyEncoding(format!("unsupported wrap version {}", blob[0])));
    }

    let ct_offset = 1 + X_PUBLIC_LEN + MK_CT_LEN + NONCE_LENGTH;
    let eph_pk = to_array::<X_PUBLIC_LEN>(&blob[1..1 + X_PUBLIC_LEN])?;
    let mk_ct = MlKem768Ciphertext::try_from(&blob[1 + X_PUBLIC_LEN..1 + X_PUBLIC_LEN + MK_CT_LEN])
        .map_err(|_| Error::InvalidLength("mk_ct length"))?;
    let nonce = &blob[1 + X_PUBLIC_LEN + MK_CT_LEN..ct_offset];
    let ciphertext = &blob[ct_offset..];

    let secret = private::parse(private_key)?;
    let x_secret = StaticSecret::from(secret.x_secret());
    let own_x_pk = PublicKey::from(&x_secret);
    let eph_public = PublicKey::from(eph_pk);
    let ss_x = x_secret.diffie_hellman(&eph_public);
    reject_non_contributory(&ss_x)?;

    // libcrux's private-key type is a bare newtype with no Drop; the zeroized
    // master copy is `secret`, and this decaps key lives only for the call.
    let decaps_key = MlKem768PrivateKey::try_from(secret.mk_decaps())
        .map_err(|_| Error::InvalidLength("mk_dk length"))?;
    let ss_k = Zeroizing::new(mlkem768::decapsulate(&decaps_key, &mk_ct));

    let wrap_key = derive_wrap_key(
        ss_k.as_slice(),
        ss_x.as_bytes(),
        &eph_pk,
        mk_ct.as_ref(),
        own_x_pk.as_bytes(),
        secret.mk_encaps(),
    )?;

    let mut aead_key = Zeroizing::new(wrap_key.to_vec());
    aead_key.extend_from_slice(nonce);

    crate::aegis256::decrypt(aead_key.to_vec(), ciphertext.to_vec())
}

#[allow(clippy::too_many_arguments)]
fn derive_wrap_key(
    ss_k: &[u8],
    ss_x: &[u8; X_PUBLIC_LEN],
    ephemeral_public: &[u8],
    mk_ct: &[u8],
    recipient_x_pk: &[u8],
    recipient_mk_ek: &[u8],
) -> CryptoResult<Zeroizing<[u8; 32]>> {
    let mut ikm = Zeroizing::new(Vec::with_capacity(ss_k.len() + ss_x.len()));
    ikm.extend_from_slice(ss_k);
    ikm.extend_from_slice(ss_x);

    let mut salt =
        Vec::with_capacity(ephemeral_public.len() + mk_ct.len() + recipient_x_pk.len() + recipient_mk_ek.len());
    salt.extend_from_slice(ephemeral_public);
    salt.extend_from_slice(mk_ct);
    salt.extend_from_slice(recipient_x_pk);
    salt.extend_from_slice(recipient_mk_ek);

    let mut wrap_key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(&salt), &ikm)
        .expand(WRAP_INFO, wrap_key.as_mut_slice())
        .map_err(|e| Error::KeyEncoding(e.to_string()))?;

    Ok(wrap_key)
}

/// An all-zero X25519 output means a low-order or identity peer point — the DH
/// contributed nothing and the "secret" is attacker-known.
fn reject_non_contributory(shared: &x25519_dalek::SharedSecret) -> CryptoResult<()> {
    if shared.as_bytes().iter().all(|b| *b == 0) {
        return Err(Error::KeyEncoding("non-contributory x25519 public key".to_string()));
    }
    Ok(())
}

fn to_array<const N: usize>(bytes: &[u8]) -> CryptoResult<[u8; N]> {
    bytes.try_into().map_err(|_| Error::InvalidLength("unexpected key component length"))
}

fn encode_pem(label: &str, body: &[u8]) -> CryptoResult<String> {
    encode_string(label, LineEnding::LF, body).map_err(|e| Error::KeyEncoding(e.to_string()))
}

fn decode_pem(expected_label: &str, pem: &str) -> CryptoResult<Vec<u8>> {
    let (label, data) =
        decode_vec(pem.as_bytes()).map_err(|e| Error::KeyEncoding(e.to_string()))?;
    if label != expected_label {
        return Err(Error::KeyEncoding(format!("expected {expected_label} pem, got {label}")));
    }
    Ok(data)
}

pub mod private {
    use super::*;

    /// A parsed private wrapping key. The raw `version ‖ x_sk ‖ mk_dk` bytes stay
    /// in a [`Zeroizing`] buffer for the value's whole life; the accessors hand
    /// out short-lived views.
    pub(super) struct HybridSecret {
        body: Zeroizing<Vec<u8>>,
    }

    impl HybridSecret {
        pub(super) fn x_secret(&self) -> [u8; X_SECRET_LEN] {
            let mut out = [0u8; X_SECRET_LEN];
            out.copy_from_slice(&self.body[1..1 + X_SECRET_LEN]);
            out
        }

        pub(super) fn mk_decaps(&self) -> &[u8] {
            &self.body[1 + X_SECRET_LEN..]
        }

        pub(super) fn mk_encaps(&self) -> &[u8] {
            &self.mk_decaps()[MK_DK_EK_OFFSET..MK_DK_EK_OFFSET + MK_EK_LEN]
        }
    }

    pub(super) fn parse(pem: &str) -> CryptoResult<HybridSecret> {
        let body = Zeroizing::new(decode_pem(PRIVATE_LABEL, pem)?);
        if body.len() != PRIVATE_CONTAINER_LEN {
            return Err(Error::InvalidLength("wrapping private key wrong length"));
        }
        if body[0] != CONTAINER_VERSION {
            return Err(Error::KeyEncoding(format!(
                "unsupported wrapping key version {}",
                body[0]
            )));
        }
        Ok(HybridSecret { body })
    }

    /// Generate a hybrid wrapping private key as a PEM container.
    pub fn generate() -> CryptoResult<String> {
        let x_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let x_bytes = Zeroizing::new(x_secret.to_bytes());

        let mut seed = Zeroizing::new([0u8; KEY_GENERATION_SEED_SIZE]);
        getrandom::getrandom(seed.as_mut_slice())?;
        let keypair = mlkem768::generate_key_pair(*seed);

        let mut body = Zeroizing::new(Vec::with_capacity(PRIVATE_CONTAINER_LEN));
        body.push(CONTAINER_VERSION);
        body.extend_from_slice(x_bytes.as_slice());
        body.extend_from_slice(keypair.private_key().as_slice());

        encode_pem(PRIVATE_LABEL, &body)
    }
}

pub mod public {
    use super::*;

    /// Parse and validate a hybrid wrapping public key PEM, returning the raw
    /// `version ‖ x_pk ‖ mk_ek` container bytes. This is the canonical the
    /// key-transition certificate commits to, and the validation registration
    /// runs — a bare X25519 SPKI key fails here on its label.
    pub fn from_str(pem: &str) -> CryptoResult<Vec<u8>> {
        let body = decode_pem(PUBLIC_LABEL, pem)?;
        if body.len() != PUBLIC_CONTAINER_LEN {
            return Err(Error::InvalidLength("wrapping public key wrong length"));
        }
        if body[0] != CONTAINER_VERSION {
            return Err(Error::KeyEncoding(format!(
                "unsupported wrapping key version {}",
                body[0]
            )));
        }
        Ok(body)
    }

    /// Derive the public wrapping key PEM from a private wrapping key PEM.
    pub fn from_private(private_key: &str) -> CryptoResult<String> {
        let secret = private::parse(private_key)?;
        let x_public = PublicKey::from(&StaticSecret::from(secret.x_secret()));

        let mut body = Vec::with_capacity(PUBLIC_CONTAINER_LEN);
        body.push(CONTAINER_VERSION);
        body.extend_from_slice(x_public.as_bytes());
        body.extend_from_slice(secret.mk_encaps());

        encode_pem(PUBLIC_LABEL, &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> (String, String) {
        let private = private::generate().unwrap();
        let public = public::from_private(&private).unwrap();
        (private, public)
    }

    #[test]
    fn component_lengths_match_libcrux() {
        assert_eq!(MK_EK_LEN, MlKem768PublicKey::len());
        assert_eq!(MK_DK_LEN, MlKem768PrivateKey::len());
        assert_eq!(MK_CT_LEN, MlKem768Ciphertext::len());
        assert_eq!(SHARED_SECRET_SIZE, 32);
    }

    #[test]
    fn extracted_encaps_key_matches_keypair() {
        // The FIPS-203 offset must pick out exactly the encapsulation key the
        // keypair carries, or a wrap to the derived public key would not unwrap.
        let seed = [4u8; KEY_GENERATION_SEED_SIZE];
        let keypair = mlkem768::generate_key_pair(seed);
        let dk = keypair.private_key().as_slice();
        assert_eq!(
            &dk[MK_DK_EK_OFFSET..MK_DK_EK_OFFSET + MK_EK_LEN],
            keypair.public_key().as_slice()
        );
    }

    #[test]
    fn container_lengths() {
        let (private, public) = keypair();
        let priv_body = decode_pem(PRIVATE_LABEL, &private).unwrap();
        let pub_body = decode_pem(PUBLIC_LABEL, &public).unwrap();
        assert_eq!(priv_body.len(), 1 + 32 + 2400);
        assert_eq!(pub_body.len(), 1 + 32 + 1184);
    }

    #[test]
    fn pem_labels_and_reparse() {
        let (private, public) = keypair();
        assert!(private.contains("BEGIN JIAM WRAPPING PRIVATE KEY"));
        assert!(public.contains("BEGIN JIAM WRAPPING KEY"));
        private::parse(&private).unwrap();
        public::from_str(&public).unwrap();
    }

    #[test]
    fn from_str_rejects_bare_x25519_and_identity_keys() {
        // A bare X25519 SPKI key — the never-shipped wrapping format — is
        // refused, so registration and the transition certificate only ever
        // accept the hybrid container.
        const X25519_SPKI: &str = "-----BEGIN PUBLIC KEY-----\n\
            MCowBQYDK2VuAyEAMj7hOamJF96N+WAmBu691xekmKrEAA5XBhHjQAWgi24=\n\
            -----END PUBLIC KEY-----\n";
        assert!(public::from_str(X25519_SPKI).is_err());

        let ed = crate::ed25519::public::from_private(&crate::ed25519::private::generate().unwrap())
            .unwrap();
        assert!(public::from_str(&ed).is_err());
    }

    #[test]
    fn from_str_returns_container_canonical() {
        // The bytes the transition certificate commits to are the container's
        // `version ‖ x_pk ‖ mk_ek`, and they round-trip the PEM exactly.
        let (_, public) = keypair();
        let canonical = public::from_str(&public).unwrap();
        assert_eq!(canonical.len(), PUBLIC_CONTAINER_LEN);
        assert_eq!(canonical[0], CONTAINER_VERSION);
        assert_eq!(canonical, decode_pem(PUBLIC_LABEL, &public).unwrap());
    }

    #[test]
    fn wrap_unwrap_round_trip() {
        let (private, public) = keypair();
        let file_key = crate::aegis256::generate_key().unwrap();

        let blob = wrap(&file_key, &public).unwrap();
        assert_eq!(unwrap(&blob, &private).unwrap(), file_key);
    }

    #[test]
    fn empty_and_large_and_multibyte_payloads() {
        let (private, public) = keypair();
        for payload in [
            Vec::new(),
            vec![0u8; 1024 * 64],
            "ključ-датотеке-鍵".as_bytes().to_vec(),
        ] {
            let blob = wrap(&payload, &public).unwrap();
            assert_eq!(unwrap(&blob, &private).unwrap(), payload);
        }
    }

    #[test]
    fn wrong_x25519_key_fails() {
        // Same ML-KEM key, different X25519 half: the hybrid must still reject.
        let (private, public) = keypair();
        let file_key = crate::aegis256::generate_key().unwrap();
        let blob = wrap(&file_key, &public).unwrap();

        let mut priv_body = decode_pem(PRIVATE_LABEL, &private).unwrap();
        let other = private::generate().unwrap();
        let other_body = decode_pem(PRIVATE_LABEL, &other).unwrap();
        priv_body[1..1 + 32].copy_from_slice(&other_body[1..1 + 32]);
        let mangled = encode_pem(PRIVATE_LABEL, &priv_body).unwrap();

        assert!(unwrap(&blob, &mangled).is_err());
    }

    #[test]
    fn wrong_ml_kem_key_fails() {
        // Same X25519 key, different ML-KEM half: rejected via the AEGIS tag.
        let (private, public) = keypair();
        let file_key = crate::aegis256::generate_key().unwrap();
        let blob = wrap(&file_key, &public).unwrap();

        let mut priv_body = decode_pem(PRIVATE_LABEL, &private).unwrap();
        let other = private::generate().unwrap();
        let other_body = decode_pem(PRIVATE_LABEL, &other).unwrap();
        priv_body[1 + 32..].copy_from_slice(&other_body[1 + 32..]);
        let mangled = encode_pem(PRIVATE_LABEL, &priv_body).unwrap();

        assert!(unwrap(&blob, &mangled).is_err());
    }

    #[test]
    fn swapped_ml_kem_ciphertext_fails() {
        // The binding property: ML-KEM is not key-binding, so a second, valid
        // ciphertext must not open the wrap once substituted for the real one.
        // Without `mk_ct` in the HKDF salt this test would pass a forgery.
        let (private, public) = keypair();
        let file_key = crate::aegis256::generate_key().unwrap();

        let blob = wrap(&file_key, &public).unwrap();
        let mut bytes = crate::base64::decode(&blob).unwrap();

        let container = public::from_str(&public).unwrap();
        let recipient_mk =
            MlKem768PublicKey::try_from(&container[1 + 32..]).unwrap();
        let (other_ct, _) = mlkem768::encapsulate(&recipient_mk, [9u8; SHARED_SECRET_SIZE]);

        let ct_start = 1 + 32;
        bytes[ct_start..ct_start + MK_CT_LEN].copy_from_slice(other_ct.as_ref());
        assert!(unwrap(&crate::base64::encode(bytes), &private).is_err());
    }

    #[test]
    fn decapsulation_never_panics_on_garbage() {
        // Implicit rejection: decaps returns a pseudorandom secret for a random
        // ciphertext instead of failing, so unwrap must surface a clean Err
        // (the AEGIS tag), never a panic.
        let (private, public) = keypair();
        let blob = wrap(b"file-key", &public).unwrap();
        let mut bytes = crate::base64::decode(&blob).unwrap();
        for b in bytes[1 + 32..1 + 32 + MK_CT_LEN].iter_mut() {
            *b ^= 0xff;
        }
        assert!(unwrap(&crate::base64::encode(bytes), &private).is_err());
    }

    #[test]
    fn ephemeral_is_unique_per_wrap() {
        let (_, public) = keypair();
        let a = crate::base64::decode(&wrap(b"same-key", &public).unwrap()).unwrap();
        let b = crate::base64::decode(&wrap(b"same-key", &public).unwrap()).unwrap();
        assert_ne!(a, b);
        // The ephemeral public key (bytes 1..33) differs every wrap.
        assert_ne!(a[1..1 + 32], b[1..1 + 32]);
    }

    #[test]
    fn tampered_blob_fails() {
        let (private, public) = keypair();
        let blob = wrap(b"file-key", &public).unwrap();
        let mut bytes = crate::base64::decode(&blob).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(unwrap(&crate::base64::encode(bytes), &private).is_err());
    }

    #[test]
    fn truncated_blob_fails_cleanly() {
        let (private, _) = keypair();
        let short = crate::base64::encode(vec![BLOB_VERSION; 64]);
        assert!(unwrap(&short, &private).is_err());
    }

    #[test]
    fn version_one_blob_rejected() {
        // The X25519-only format that never shipped. A `0x01` lead byte must be
        // refused, not decoded — accepting it could only be a downgrade.
        let (private, public) = keypair();
        let blob = wrap(b"file-key", &public).unwrap();
        let mut bytes = crate::base64::decode(&blob).unwrap();
        bytes[0] = 0x01;
        let err = unwrap(&crate::base64::encode(bytes), &private).unwrap_err();
        assert!(matches!(err, Error::KeyEncoding(_)));
    }

    #[test]
    fn wrap_blob_is_self_describing_and_round_trips() {
        // The wrap format is versioned by its leading byte and carries every
        // input HKDF needs, so a blob produced by one build must unwrap in
        // another. What is asserted here is the structure plus a live
        // round-trip: version byte, exact length, recovery, wrong key rejected.
        //
        // A pinned cross-build vector (native vs. WASM portable path, NEON vs.
        // SIMD-off) used to live here. It was produced under the previous
        // domain-separation string, so it cannot survive a rename: WRAP_INFO is
        // part of the HKDF input, and the stored blob would no longer unwrap.
        //
        // TODO(rebrand): regenerate it once a machine with a C toolchain is
        // available. Add a temporary test that prints `wrap(b"golden-file-key-
        // 0123456789abcdef", &public)` for a fixed keypair, run it natively and
        // then with LIBCRUX_DISABLE_SIMD128=1, and only pin the value if both
        // builds agree.
        let (private, public) = keypair();
        let file_key = b"golden-file-key-0123456789abcdef";
        let blob = wrap(file_key, &public).unwrap();

        let bytes = crate::base64::decode(&blob).unwrap();
        assert_eq!(bytes[0], BLOB_VERSION);
        assert_eq!(
            bytes.len(),
            1 + X_PUBLIC_LEN + MK_CT_LEN + NONCE_LENGTH + file_key.len() + TAG_LENGTH
        );

        assert_eq!(unwrap(&blob, &private).unwrap(), file_key);
        assert!(unwrap(&blob, &keypair().0).is_err());
    }
}
