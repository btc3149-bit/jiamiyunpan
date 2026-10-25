use serde::{Deserialize, Serialize};

use super::app_file::AppFile;

/// One entry of the trash listing: the file row itself (encrypted, with
/// the owner's key wrap so the client can decrypt the name), when it was
/// trashed, and the encrypted metadata of its ancestor folders so the
/// client can render the original path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrashItem {
    pub file: AppFile,
    /// Unix seconds at which this file (or its trash root) was deleted.
    pub deleted_at: i64,
    /// Ancestor folders, root-first, excluding the file itself. Present
    /// only for rows below the drive root; every entry carries the
    /// encrypted name and the caller's key wrap so the plaintext path
    /// never touches the server.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<TrashParent>,
}

/// An ancestor folder of a trashed item, minimized to what the client
/// needs to decrypt and render one path segment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrashParent {
    pub id: String,
    pub encrypted_name: String,
    pub cipher: String,
    pub encrypted_key: String,
}

/// Paginated `GET /api/storage/trash` response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrashPage {
    pub items: Vec<TrashItem>,
    pub total: u64,
    pub limit: u64,
    pub offset: u64,
}

/// Body of `POST /api/storage/trash/{file_id}/restore`.
///
/// All fields are optional. They are only needed when the restore hits a
/// name collision the server cannot resolve itself: names are encrypted,
/// so the server can detect a collision (via `name_hash`) but cannot
/// compute the " (restored)" suffix. A client that receives
/// `409 trash_restore_name_conflict` re-encrypts the suffixed name and
/// retries with these fields filled.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RestoreTrash {
    /// New encrypted name to write (the suffixed one, on retry).
    #[serde(default)]
    pub encrypted_name: Option<String>,
    /// Keyed tag of the new name, same shape create/rename send.
    #[serde(default)]
    pub name_hash: Option<String>,
    /// Name-source tags under the owner's root key for the new name.
    #[serde(default)]
    pub search_tokens_root: Option<Vec<String>>,
    /// Name-source tags under the file key for the new name.
    #[serde(default)]
    pub search_tokens_file: Option<Vec<String>>,
}

/// Response of a successful restore.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestoredTrash {
    pub restored: bool,
    /// True when the original parent folder no longer exists (or is
    /// itself trashed) and the file was re-attached to the drive root.
    pub moved_to_root: bool,
    /// True when a renamed (suffixed) name was applied to avoid a
    /// collision at the original location.
    pub renamed: bool,
}
