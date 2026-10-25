use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::claims::Claims;
use context::Context;
use entity::{
    permission::{permission, SharePermission},
    TransactionTrait, Uuid,
};
use error::{AppResult, Error};

use crate::repository::Repository;

/// Delete a file or directory by id. Owner → move to trash (soft delete:
/// the row and its subtree are stamped in `trashed_files` and vanish from
/// every listing; nothing is purged until the trash empties it or the
/// retention sweep runs). Reader / Editor / Co-owner → self-remove only
/// (drop caller's `user_files` row, recursively for folders). None → 403.
#[route("/api/storage/{file_id}", method = "DELETE")]
pub(crate) async fn delete(
    req: HttpRequest,
    claims: Claims,
    context: web::Data<Context>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let file_id: Uuid = util::actix::path_var(&req, "file_id")?;

    let connection = context.db.begin().await?;

    let perm = permission(&connection, file_id, claims.sub).await?;
    match perm {
        SharePermission::Owner => {
            Repository::new(&connection)
                .manage(claims.sub)
                .trash_many(vec![file_id])
                .await?;
            connection.commit().await?;
        }
        SharePermission::CoOwner | SharePermission::Editor | SharePermission::Reader => {
            Repository::new(&connection)
                .manage(claims.sub)
                .self_remove_recursive(vec![file_id])
                .await?;
            connection.commit().await?;
        }
        SharePermission::None => {
            return Err(Error::Forbidden("forbidden_not_owner".to_string()));
        }
    }

    Ok(HttpResponse::NoContent().finish())
}
