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

    /// A blob produced by the native build (MSVC, libcrux SIMD paths), pinned
    /// so builds with different codegen must still recover the file key. The
    /// pinned value was verified to unwrap under the WASM portable build of
    /// the same source (wasm32-unknown-unknown, SIMD off) before being
    /// committed, so it is a true cross-build anchor: native wraps, WASM
    /// unwraps, and any drift in the HKDF domain separation or the chunked
    /// AEAD layout breaks one of the two sides.
    #[test]
    fn cross_build_golden_vector() {
        let recovered = unwrap(GOLDEN_BLOB, GOLDEN_PRIVATE).unwrap();
        assert_eq!(recovered, b"golden-file-key-0123456789abcdef");
    }

    /// Native-produced wrap of `b"golden-file-key-0123456789abcdef"` under the
    /// current WRAP_INFO, pinned after the WASM portable build reproduced the
    /// recovery on the same source.
    const GOLDEN_BLOB: &str = "AgetxE3P4TFd3BqRGMBkU01MTWaYdA1NDRlxM/Ecp2BkSgxy/aRaQpc1jxAqtLDAqUjM/biWks5hQ6SQKhv96bXqoIOkv8muJBfjQ3Z1PFYH9jmYKW92WVNPjbkZlGUl2ae8OLU2OwB0TDw9VsqMYhxK02B6RU2wCtc5so+3Wz7CyvF15jBD7qKcXkC1YjGH2EqeqLth9XecTQbOealRCJ3bpVYgBtwtDbbg0xwg29ipxslnfCzaAUz1ehdBmU8uWa3pxK1EGbEpIurC8wTzbRSUrWb06iF2WJWO2ZTW1tNiYdu0TJ+QGtDpY4Yrn7WuBHakUm1LKECarGg0jKbiVEd9zuGmf3aTDr/pRngDZddXu0KppQdrqV8E0FDhvSR/6lxTPFB25A89KqN1PBFCNxwNX+mjA4kDhLSFKKdx94UDxidcrgCq68v11q4PqH7rgzGz8lqV87ZrMCRMBkIvE6QEhfbztH8vSCNwIjh2HDjTirY3kLwmULFknbnWOB9Ua2zDEDY5x40F59XbwV0CZu0562nLWEkbbbQRoACu/fseDvwlAxigSta6jk+jYPPOGpNvTCemgwhJl8/LEDSBx2dkdiiA8yb4SHVOqLyuiuFq+pNN1B1LGcpnopZRVO0SRaQTk59HRu/zSj+GEJGZrkcroSrjVMIUbmMCWwBOkzQ+oXzqAmkg4j0/nxdX6sb0gGllle5CAQt7YumZiO1IS0XWYL0xzNr85N89gZ6fLiBnApX7tyS/QyiqYL8fbavYdZzVy6N2ZEb2y7VigOYJAnXK75bBZSZ3BHlQqrX9ddnXzRA+0ab8SzpuZ+aEMv3TfhUaA76lfbyDTfl7ekCRxrOWX5oY8x74d3u17zoTfXu/dEYayOd+rsfy8iwDnrzmwmCiOgon+sX5TKUXd8rjmz7UaeHeWoOWlftlP3KzIgdE9xd6j6nD31XDZyWEgg31s9RRptuMmY5yz8XNsuAboAvn/vbDK2nM0RXMqUtF/8QeAWYgiDFTA39XIReNDhl6UJTgOvP+TwZP+cdTpWTqoMhvdqA0jzKnfSwaAtxCpP6XYFcdS1g8FDRVs/izsx3qAXv5tiyKn8TYQRNQa69V0TtslJgYp42ipaXLxvGljOqfm62wS19uo1gNlFm3vz9HhIk3KWNeoPp/DvHZltYBSl3OzhokKmVv0J13VxCt78ofmnxJN1RrmEEImy+fPF3/9M+/Pe/gzNy0f6+dZJPm4Pe5XGwFae0Z+80FzC5q7JOVktVnfzQCHxUMqTrG4bgX/Zz77W8FIrRm1TYm7WGBtzRVqx40dmLFiNd8lBWaRkGnZVwCPrZu/m3TAbJIfkTVXtOFKYt+WT77gOjIfkA3CEVy1Ko3ylOIpCJjArW0XolR2ZqAlmy9HCBdmcIJ7caIYFROnGgHxd0f5Vaj2b5Ygq/QgVQEHt+92yyKN83+sj1n4xnJKm856uVFZPyLj2vH842AV2xTyVIr+DNVDE3342ZtwPK0UOb/Cmsh8f76a9E9X1euDnImO5SwYPElBCvMnfpBFgOlmqFvRDWluMgSOeCyC80kefr/chXyl66u9OJFF4JO/Eyjl3o5dGcN023hnw==";

    const GOLDEN_PRIVATE: &str = concat!(
        "-----BEGIN JIAM WRAPPING PRIVATE KEY-----\n",
        "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\n",
        "-----END JIAM WRAPPING PRIVATE KEY-----\n"
    );
}
