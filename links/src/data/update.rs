use ::error::AppResult;
use serde::Deserialize;
use validr::*;

/// Owner-facing link update. Only the fields the client actually sends are
/// touched — a protection field left out of the JSON keeps its current value.
///
/// Password semantics:
/// - `password: Some("...")` (non-empty) — set/replace the password;
/// - `remove_password: Some(true)` — clear the password;
/// - both absent — leave the password as it is.
///
/// Max-downloads semantics:
/// - `max_downloads: Some(n)` with `n > 0` — set the cap;
/// - `max_downloads: Some(n)` with `n <= 0` — clear the cap (unlimited);
/// - absent — leave the cap as it is.
#[derive(Clone, Debug, Deserialize)]
pub struct Update {
    pub expires_at: Option<i64>,
    pub password: Option<String>,
    pub remove_password: Option<bool>,
    pub max_downloads: Option<i32>,
}

impl Validation for Update {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![]
    }
}

/// The resolved outcome of an [`Update`]: what each protection column should
/// become. `Keep` leaves the stored value untouched.
#[derive(Clone, Debug, Default)]
pub struct UpdateLink {
    pub expires_at: Option<Option<i64>>,
    pub password_hash: PasswordChange,
    pub max_downloads: MaxDownloadsChange,
}

#[derive(Clone, Debug, Default)]
pub enum PasswordChange {
    #[default]
    Keep,
    Set(Option<String>),
}

#[derive(Clone, Copy, Debug, Default)]
pub enum MaxDownloadsChange {
    #[default]
    Keep,
    Set(Option<i32>),
}

impl Update {
    pub fn into_value(self) -> AppResult<UpdateLink> {
        let data = self.validate()?;

        let password_hash = match (&data.password, data.remove_password) {
            // Explicit removal wins over a concurrent password value — the
            // client asked for the lock off, that is the instruction.
            (_, Some(true)) => PasswordChange::Set(None),
            (Some(password), _) if !password.is_empty() => {
                PasswordChange::Set(Some(util::password::hash(password)))
            }
            _ => PasswordChange::Keep,
        };

        let max_downloads = match data.max_downloads {
            Some(max) if max > 0 => MaxDownloadsChange::Set(Some(max)),
            Some(_) => MaxDownloadsChange::Set(None),
            None => MaxDownloadsChange::Keep,
        };

        Ok(UpdateLink {
            expires_at: Some(data.expires_at),
            password_hash,
            max_downloads,
        })
    }
}
