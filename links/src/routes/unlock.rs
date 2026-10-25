use actix_web::{route, web, HttpRequest, HttpResponse};
use context::Context;
use entity::Uuid;
use error::{AppResult, Error};

use crate::repository::Repository;
use crate::routes::guard;

/// Exchange the link password for a scoped capability cookie.
///
/// `POST /api/links/{link_id}/unlock`, body `{"password": "..."}` or the
/// `X-Link-Password` header. On success the response sets `link_pw` — an
/// HttpOnly cookie scoped to `/api/links` — whose value matches the hash the
/// server stored, so every later gated request (including the ones the wasm
/// transfer downloader builds, which cannot carry custom headers) presents
/// the proof automatically for the cookie's one-hour lifetime.
///
/// Response: 204 No Content
#[route("/api/links/{link_id}/unlock", method = "POST")]
pub(crate) async fn unlock(
    req: HttpRequest,
    context: web::Data<Context>,
    body: web::Bytes,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let link_id: Uuid = util::actix::path_var(&req, "link_id")?;
    let repository = Repository::new(&context);

    let link = repository.get(link_id).await?;

    if link.is_expired() {
        return Err(Error::Unauthorized("link_expired".to_string()));
    }

    let Some(hash) = link.password_hash.as_deref() else {
        return Err(Error::BadRequest("link_not_password_protected".to_string()));
    };

    let authorized = match guard::presented_password(&req, &body) {
        Some(password) => util::password::verify(&password, hash),
        None => false,
    };

    if !authorized {
        return Err(Error::Unauthorized("invalid_link_password".to_string()));
    }

    let mut response = HttpResponse::NoContent().finish();

    if let Some(cookie) = guard::password_cookie(&link, &context.config) {
        response
            .add_cookie(&cookie)
            .map_err(|e| Error::InternalError(format!("cookie: {e}")))?;
    }

    Ok(response)
}
