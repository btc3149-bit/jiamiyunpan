use entity::Uuid;
use error::Error;
use serde::{Deserialize, Serialize};

/// The maximum amount of ids a single bulk request may carry. Protects the
/// server from one oversized request hogging the worker for minutes.
pub const BULK_MAX_IDS: usize = 500;

/// Actions that can be applied to many users (or files) at once.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkAction {
    Disable,
    Enable,
    Delete,
}

/// Request body of the bulk endpoints: an action plus the ids to apply it to.
#[derive(Debug, Deserialize, Clone)]
pub struct Bulk {
    pub action: BulkAction,
    pub ids: Vec<Uuid>,
}

/// Per-id outcome. `error` carries a short machine readable reason and is
/// omitted when the id succeeded.
#[derive(Debug, Serialize, Clone)]
pub struct BulkResult {
    pub id: Uuid,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BulkResult {
    pub fn ok(id: Uuid) -> Self {
        Self {
            id,
            ok: true,
            error: None,
        }
    }

    pub fn err(id: Uuid, error: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            error: Some(error.into()),
        }
    }
}

/// Response envelope: one entry per requested id, in request order.
#[derive(Debug, Serialize, Clone)]
pub struct BulkResponse {
    pub results: Vec<BulkResult>,
}

/// Reduce an [error::Error] to the short message it carries, so a per-id
/// result stays machine readable instead of leaking a Rust debug dump.
pub(crate) fn error_message(error: &Error) -> String {
    match error {
        Error::NotFound(m)
        | Error::Forbidden(m)
        | Error::Unauthorized(m)
        | Error::BadRequest(m)
        | Error::Conflict(m) => m.clone(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bulk_action_serialization() {
        assert_eq!(
            serde_json::to_value(BulkAction::Disable).unwrap(),
            serde_json::json!("disable")
        );
        assert_eq!(
            serde_json::to_value(BulkAction::Enable).unwrap(),
            serde_json::json!("enable")
        );
        assert_eq!(
            serde_json::to_value(BulkAction::Delete).unwrap(),
            serde_json::json!("delete")
        );
        let action: BulkAction = serde_json::from_value(serde_json::json!("delete")).unwrap();
        assert_eq!(action, BulkAction::Delete);
    }

    #[test]
    fn test_bulk_result_skips_error_on_success() {
        let id = Uuid::new_v4();
        let value = serde_json::to_value(BulkResult::ok(id)).unwrap();
        assert_eq!(value, serde_json::json!({ "id": id, "ok": true }));

        let value = serde_json::to_value(BulkResult::err(id, "user_not_found")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "id": id, "ok": false, "error": "user_not_found" })
        );
    }
}
