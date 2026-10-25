use sea_orm::entity::prelude::*;
use sea_orm::ActiveValue;
use serde::{Deserialize, Serialize};

/// One append-only audit-trail row: a security-relevant event such as a
/// login (successful or not) or a privileged admin operation.
///
/// Rows are written fire-and-forget by [`record`]: an audit failure is
/// logged and swallowed, never failing the request that produced the event.
/// `user_id` deliberately has no foreign key — the trail must survive the
/// deletion of the account it refers to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// The acting user, `None` when unauthenticated (e.g. a failed login).
    pub user_id: Option<Uuid>,
    /// Dotted event name, e.g. `login.success`, `admin.user.delete`.
    /// Filtered by prefix in the admin audit view.
    pub action: String,
    /// The affected object, e.g. `user:<uuid>` or `email:<address>`.
    pub target: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// Free-form JSON string with action-specific details.
    pub metadata: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Record an audit event. Fire-and-forget by contract: on any database
/// failure the error is logged and dropped, so auditing can never break the
/// main request it observes.
#[allow(clippy::too_many_arguments)]
pub async fn record<C>(
    connection: &C,
    user_id: Option<Uuid>,
    action: &str,
    target: Option<String>,
    ip: Option<String>,
    user_agent: Option<String>,
    metadata: Option<String>,
) where
    C: ConnectionTrait,
{
    let active = ActiveModel {
        user_id: ActiveValue::Set(user_id),
        action: ActiveValue::Set(action.to_string()),
        target: ActiveValue::Set(target),
        ip: ActiveValue::Set(ip),
        user_agent: ActiveValue::Set(user_agent),
        metadata: ActiveValue::Set(metadata),
        created_at: ActiveValue::Set(chrono::Utc::now().timestamp()),
        ..Default::default()
    };

    if let Err(e) = Entity::insert(active).exec(connection).await {
        log::error!("entity::audit_log::record|failed to insert audit row: {e}");
    }
}
