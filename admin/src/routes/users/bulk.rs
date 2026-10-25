use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::staff::Staff;
use context::Context;
use error::{AppResult, Error};

use crate::{
    data::users::bulk::{error_message, Bulk, BulkAction, BulkResponse, BulkResult, BULK_MAX_IDS},
    repository::Repository,
};

/// Bulk apply an action to many users.
///
/// Request: [crate::data::users::bulk::Bulk]
///
/// Response: [crate::data::users::bulk::BulkResponse]
///
/// Each id is handled independently — one bad id never fails the batch.
/// Bulk delete refuses the current admin's own account per id.
#[route("/api/admin/users/bulk", method = "POST")]
pub(crate) async fn bulk(
    req: HttpRequest,
    staff: Staff,
    context: web::Data<Context>,
    bulk: web::Json<Bulk>,
) -> AppResult<HttpResponse> {
    staff.is_admin_or_err()?;

    let bulk = bulk.into_inner();

    if bulk.ids.len() > BULK_MAX_IDS {
        return Err(Error::BadRequest("bulk_ids_limit_exceeded".to_string()));
    }

    let context = context.into_inner();
    let repository = Repository::new(&context, &context.db);
    let (user_agent, ip) = util::actix::extract_ip_ua(&req);
    let action_name = match bulk.action {
        BulkAction::Disable => "admin.user.disable",
        BulkAction::Enable => "admin.user.enable",
        BulkAction::Delete => "admin.user.delete",
    };

    let mut results = Vec::with_capacity(bulk.ids.len());

    for id in bulk.ids {
        let result = match bulk.action {
            BulkAction::Disable => repository.users().disable(id).await,
            BulkAction::Enable => repository.users().enable(id).await,
            // Deleting yourself through a bulk action would lock every
            // operator out of the instance, so it is refused per id —
            // mirroring the single-user DELETE route.
            BulkAction::Delete => match staff.forbidden_self(id) {
                Ok(()) => repository.users().delete(id).await,
                Err(e) => Err(e),
            },
        };

        results.push(match result {
            Ok(()) => {
                entity::audit_log::record(
                    &context.db,
                    Some(staff.claims.sub),
                    action_name,
                    Some(format!("user:{}", id)),
                    Some(ip.clone()),
                    Some(user_agent.clone()),
                    None,
                )
                .await;
                BulkResult::ok(id)
            }
            Err(e) => BulkResult::err(id, error_message(&e)),
        });
    }

    Ok(HttpResponse::Ok().json(BulkResponse { results }))
}
