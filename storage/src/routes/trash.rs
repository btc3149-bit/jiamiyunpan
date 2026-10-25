use actix_web::{route, web, HttpRequest, HttpResponse};
use auth::data::claims::Claims;
use context::Context;
use entity::{TransactionTrait, Uuid};
use error::{AppResult, Error};
use fs::prelude::{Fs, FsProviderContract};

use crate::{data::trash::RestoreTrash, repository::Repository};

/// Query string of `GET /api/storage/trash`.
#[derive(serde::Deserialize, Debug)]
pub(crate) struct TrashQuery {
    #[serde(default)]
    limit: Option<u64>,
    #[serde(default)]
    offset: Option<u64>,
}

/// `GET /api/storage/trash` — paginated list of the caller's soft-deleted
/// files (trash roots only; children of a trashed folder ride with it).
/// Every item carries the encrypted name/key material, the deletion
/// timestamp and the encrypted ancestor chain so the client can render
/// name, size and original path without the server seeing plaintext.
#[route("/api/storage/trash", method = "GET")]
pub(crate) async fn list(
    req: HttpRequest,
    claims: Claims,
    context: web::Data<Context>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();

    let query = web::Query::<TrashQuery>::from_query(req.query_string())
        .map_err(|_| Error::BadRequest("invalid_trash_query".to_string()))?
        .into_inner();

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0);

    let page = Repository::new(&context.db)
        .trash(claims.sub)
        .list(limit, offset)
        .await?;

    Ok(HttpResponse::Ok().json(page))
}

/// `POST /api/storage/trash/{file_id}/restore` — bring a trashed file back.
/// Re-attaches to the original folder, or to the drive root when the
/// folder is gone (or itself trashed). Name collisions come back as
/// `409 trash_restore_name_conflict`; the client re-encrypts a suffixed
/// name and retries with the same payload shape.
#[route("/api/storage/trash/{file_id}/restore", method = "POST")]
pub(crate) async fn restore(
    req: HttpRequest,
    claims: Claims,
    context: web::Data<Context>,
    data: web::Json<RestoreTrash>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let file_id: Uuid = util::actix::path_var(&req, "file_id")?;

    let connection = context.db.begin().await?;
    let restored = Repository::new(&connection)
        .trash(claims.sub)
        .restore(file_id, data.into_inner())
        .await?;
    connection.commit().await?;

    Ok(HttpResponse::Ok().json(restored))
}

/// `DELETE /api/storage/trash/{file_id}` — permanent delete of one trash
/// entry and its subtree. DB rows go in the transaction; chunk storage is
/// purged after commit, matching the old hard-delete route's split.
#[route("/api/storage/trash/{file_id}", method = "DELETE")]
pub(crate) async fn purge(
    req: HttpRequest,
    claims: Claims,
    context: web::Data<Context>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let file_id: Uuid = util::actix::path_var(&req, "file_id")?;

    let fs = Fs::new(&context.config);
    let connection = context.db.begin().await?;

    let rows = Repository::new(&connection)
        .trash(claims.sub)
        .purge(file_id)
        .await?;
    connection.commit().await?;

    for file in &rows {
        if file.mime != "dir" {
            fs.purge_all(file).await?;
        }
    }

    Ok(HttpResponse::NoContent().finish())
}

/// `POST /api/storage/trash/empty` — permanently delete everything in the
/// caller's trash. Same DB-then-disk split as the single purge.
///
/// Registered before the `POST /api/storage/{file_id}` upload route so
/// "empty" is never parsed as a file id.
#[route("/api/storage/trash/empty", method = "POST")]
pub(crate) async fn empty(claims: Claims, context: web::Data<Context>) -> AppResult<HttpResponse> {
    let context = context.into_inner();

    let fs = Fs::new(&context.config);
    let connection = context.db.begin().await?;

    let rows = Repository::new(&connection)
        .trash(claims.sub)
        .purge_all()
        .await?;
    connection.commit().await?;

    for file in &rows {
        if file.mime != "dir" {
            fs.purge_all(file).await?;
        }
    }

    Ok(HttpResponse::NoContent().finish())
}
