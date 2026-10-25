use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::staff::Staff;
use context::Context;
use error::{AppResult, Error};

use crate::{
    data::users::bulk::{error_message, Bulk, BulkAction, BulkResponse, BulkResult, BULK_MAX_IDS},
    repository::Repository,
};

/// Bulk delete many files.
///
/// Request: [crate::data::users::bulk::Bulk]
///
/// Response: [crate::data::users::bulk::BulkResponse]
///
/// Each id is handled independently — one bad id never fails the batch.
/// Only `delete` is supported; any other action is reported per id.
#[route("/api/admin/files/bulk", method = "POST")]
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

    if bulk.action != BulkAction::Delete {
        return Err(Error::BadRequest("unsupported_action".to_string()));
    }

    let context = context.into_inner();
    let repository = Repository::new(&context, &context.db);
    let (user_agent, ip) = util::actix::extract_ip_ua(&req);

    let mut results = Vec::with_capacity(bulk.ids.len());

    for id in bulk.ids {
        let result = repository.files().delete_one(id).await;

        results.push(match result {
            Ok(()) => {
                entity::audit_log::record(
                    &context.db,
                    Some(staff.claims.sub),
                    "admin.file.delete",
                    Some(format!("file:{}", id)),
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
