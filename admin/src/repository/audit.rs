use entity::{
    audit_log, paginated::Paginated, ColumnTrait, ConnectionTrait, EntityTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use error::AppResult;
use validr::Validation;

use crate::data::audit::search::Search;

use super::Repository;

pub(crate) struct AuditRepository<'repository, T: ConnectionTrait> {
    repository: &'repository Repository<'repository, T>,
}

impl<'repository, T> AuditRepository<'repository, T>
where
    T: ConnectionTrait,
{
    pub(crate) fn new(repository: &'repository Repository<'repository, T>) -> Self {
        Self { repository }
    }

    /// Paginated, newest-first audit trail with optional filters.
    pub(crate) async fn find(&self, search: Search) -> AppResult<Paginated<audit_log::Model>> {
        let search = search.validate()?;

        let mut query = audit_log::Entity::find();

        if let Some(user_id) = search.user_id {
            query = query.filter(audit_log::Column::UserId.eq(user_id));
        }

        if let Some(action) = search.action {
            // Prefix match: `login` covers `login.success` and `login.failure`.
            query = query.filter(audit_log::Column::Action.like(format!("{}%", action)));
        }

        if let Some(from) = search.created_from {
            query = query.filter(audit_log::Column::CreatedAt.gte(from));
        }

        if let Some(to) = search.created_to {
            query = query.filter(audit_log::Column::CreatedAt.lte(to));
        }

        let total = query.clone().count(self.repository.connection()).await?;

        // Stable newest-first order; `id` breaks ties between rows that land
        // on the same second.
        query = query
            .order_by(audit_log::Column::CreatedAt, Order::Desc)
            .order_by(audit_log::Column::Id, Order::Desc);

        query = query.limit(search.limit.unwrap_or(15));
        query = query.offset(search.offset.unwrap_or(0));

        let audit_logs = query
            .all(self.repository.connection())
            .await?;

        Ok(Paginated::new(audit_logs, total))
    }
}
