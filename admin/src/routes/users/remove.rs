use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::staff::Staff;
use context::Context;
use entity::Uuid;
use error::AppResult;

use crate::repository::Repository;

/// Delete user and all of it data.
#[route("/api/admin/users/{id}", method = "DELETE")]
pub(crate) async fn remove(
    req: HttpRequest,
    staff: Staff,
    context: web::Data<Context>,
) -> AppResult<HttpResponse> {
    staff.is_admin_or_err()?;

    let id = util::actix::path_var::<Uuid>(&req, "id")?;
    staff.forbidden_self(id)?;
    let (user_agent, ip) = util::actix::extract_ip_ua(&req);

    let context = context.into_inner();

    Repository::new(&context, &context.db)
        .users()
        .delete(id)
        .await?;

    entity::audit_log::record(
        &context.db,
        Some(staff.claims.sub),
        "admin.user.delete",
        Some(format!("user:{}", id)),
        Some(ip),
        Some(user_agent),
        None,
    )
    .await;

    Ok(HttpResponse::NoContent().finish())
}
