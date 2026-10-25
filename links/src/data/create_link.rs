use ::error::AppResult;
use chrono::Utc;
use entity::{links::ActiveModel, ActiveValue, Uuid};
use serde::{Deserialize, Serialize};
use validr::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLink {
    /// Id of the file that will be shared.
    pub file_id: Option<String>,

    /// Signature that the user created when the link was initially created.
    pub signature: Option<String>,

    /// Name of the file encrypted with the link key.
    pub encrypted_name: Option<String>,

    /// Link AES key encrypted with the user's public RSA key.
    pub encrypted_link_key: Option<String>,

    /// If the file has a thumbnail it is encrypted with the link key.
    pub encrypted_thumbnail: Option<String>,

    /// AES key for the file encrypted with a link AES key.
    pub encrypted_file_key: Option<String>,

    /// Optional date when the link will expire.
    pub expires_at: Option<i64>,

    /// Optional plaintext download password. Hashed with bcrypt (the same
    /// `util::password` scheme as account passwords) before it ever touches
    /// the database; the plaintext is not persisted or logged anywhere.
    /// A shared-link password is not an account credential, so no strength
    /// policy is enforced here.
    pub password: Option<String>,

    /// Optional cap on completed downloads. `None` = unlimited; values <= 0
    /// are ignored (treated as unlimited) rather than rejecting the create.
    pub max_downloads: Option<i32>,
}

impl Validation for CreateLink {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![
            rule_required!(file_id),
            rule_required!(encrypted_name),
            rule_required!(encrypted_link_key),
            rule_required!(encrypted_file_key),
        ]
    }
}

impl CreateLink {
    pub fn into_active_model(self, user_id: Uuid) -> AppResult<(ActiveModel, String, Uuid)> {
        let data = self.validate()?;

        let file_id = match data.file_id.as_deref() {
            Some(v) => Uuid::parse_str(v)?,
            None => {
                return Err("file_id is required".into());
            }
        };

        // Hash once, outside the ActiveModel: an empty password means "no
        // password", not "password is the empty string" — a link locked with
        // an empty secret would be indistinguishable from an unlocked one.
        let password_hash = match data.password.as_deref() {
            Some(password) if !password.is_empty() => {
                Some(util::password::hash(password))
            }
            _ => None,
        };

        let max_downloads = data.max_downloads.filter(|max| *max > 0);

        Ok((
            ActiveModel {
                id: ActiveValue::Set(Uuid::new_v4()),
                user_id: ActiveValue::Set(user_id),
                file_id: ActiveValue::Set(file_id),
                signature: ActiveValue::Set(data.signature.clone().unwrap()),
                downloads: ActiveValue::Set(0),
                encrypted_name: ActiveValue::Set(data.encrypted_name.unwrap()),
                encrypted_link_key: ActiveValue::Set(data.encrypted_link_key.unwrap()),
                encrypted_thumbnail: ActiveValue::Set(data.encrypted_thumbnail),
                encrypted_file_key: ActiveValue::Set(data.encrypted_file_key),
                created_at: ActiveValue::Set(Utc::now().timestamp()),
                expires_at: ActiveValue::Set(data.expires_at),
                password_hash: ActiveValue::Set(password_hash),
                max_downloads: ActiveValue::Set(max_downloads),
            },
            data.signature.unwrap(),
            file_id,
        ))
    }
}
