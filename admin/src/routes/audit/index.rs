use actix_web::{route, web, HttpResponse};
use auth::data::staff::Staff;
use context::Context;
use error::AppResult;

use crate::{data::audit::search::Search, repository::Repository};

/// List and query the platform audit trail (logins and admin actions).
///
/// Request: [crate::data::audit::search::Search]
///
/// Response: [entity::paginated::Paginated<entity::audit_log::Model>]
#[route("/api/admin/audit", method = "GET")]
pub(crate) async fn index(
    staff: Staff,
    context: web::Data<Context>,
    data: web::Query<Search>,
) -> AppResult<HttpResponse> {
    staff.is_admin_or_err()?;

    let context = context.into_inner();

    let response = Repository::new(&context, &context.db)
        .audit()
        .find(data.into_inner())
        .await?;

    Ok(HttpResponse::Ok().json(response))
}
