use entity::Uuid;
use serde::{Deserialize, Serialize};
use validr::*;

/// Query for the admin audit view: newest-first page of `audit_logs` rows
/// with optional user / action-prefix / date-range filters.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Search {
    pub user_id: Option<Uuid>,
    /// Filter by action prefix, e.g. `login` matches `login.success` and
    /// `login.failure`; `admin.user` matches every admin user action.
    pub action: Option<String>,
    /// Inclusive lower bound on `created_at` (unix seconds).
    pub created_from: Option<i64>,
    /// Inclusive upper bound on `created_at` (unix seconds).
    pub created_to: Option<i64>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl Validation for Search {
    fn rules(&self) -> Vec<Rule<Self>> {
        vec![]
    }
}
