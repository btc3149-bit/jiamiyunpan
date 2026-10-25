use actix_web::cookie::{time::OffsetDateTime, Cookie, SameSite};
use actix_web::{HttpRequest, HttpResponse};
use chrono::Utc;
use config::Config;
use error::Error;
use serde::Deserialize;

use crate::data::app_link::AppLink;

/// Cookie that carries the proof a client already unlocked the link. Its
/// value is `{link_id}:{password_hash}` — minted by the unlock route only
/// after a successful bcrypt verification, and matched verbatim afterwards,
/// so possessing the cookie is equivalent to possessing the password, for
/// this one link and for the cookie's lifetime.
pub(crate) const PASSWORD_COOKIE: &str = "link_pw";

/// Header used by JS-issued requests to present the password directly. A
/// header never lands in access logs or proxies the way a query parameter
/// would, and it works for methods that cannot carry a body (HEAD).
pub(crate) const PASSWORD_HEADER: &str = "X-Link-Password";

/// How long a successful unlock stays valid. Short on purpose: the cookie is
/// a download capability, not a session.
const PASSWORD_COOKIE_TTL_SECONDS: i64 = 3600;

/// Cookie path scope. Everything a link recipient can call — metadata,
/// download, chunk-urls, unlock — lives under `/api/links`, so the cookie
/// never rides requests to any other route.
const PASSWORD_COOKIE_PATH: &str = "/api/links";

/// Optional JSON body of the unlock request.
#[derive(Debug, Deserialize)]
pub struct UnlockBody {
    pub password: Option<String>,
}

/// Gate shared by every anonymous link route (download, head, chunk-urls,
/// unlock). Links without a password pass through untouched.
///
/// Credential order: the explicit header wins over the cookie — a client
/// that sends both is telling us the password, not leaning on a previous
/// unlock. The cookie path exists for the wasm relay downloader, whose
/// requests are built inside the transfer crate and cannot carry custom
/// headers, but which always sends same-origin cookies.
pub(crate) fn authorize_password(link: &AppLink, req: &HttpRequest) -> Result<(), Error> {
    let Some(hash) = link.password_hash.as_deref() else {
        return Ok(());
    };

    if let Some(password) = header_password(req) {
        return if util::password::verify(&password, hash) {
            Ok(())
        } else {
            Err(Error::Unauthorized("invalid_link_password".to_string()))
        };
    }

    if let Some(expected) = link.password_cookie_value() {
        if let Some(cookie) = req.cookie(PASSWORD_COOKIE) {
            if constant_time_eq(cookie.value().as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
        }
    }

    Err(Error::Unauthorized("link_password_required".to_string()))
}

/// The password as presented on the request: header first, JSON body second.
pub(crate) fn presented_password(req: &HttpRequest, body: &[u8]) -> Option<String> {
    if let Some(password) = header_password(req) {
        return Some(password);
    }

    if body.is_empty() {
        return None;
    }

    serde_json::from_slice::<UnlockBody>(body)
        .ok()
        .and_then(|body| body.password)
        .filter(|password| !password.is_empty())
}

fn header_password(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get(PASSWORD_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The capability cookie for a successfully unlocked link, following the
/// same security switches as the session cookies.
pub(crate) fn password_cookie(link: &AppLink, config: &Config) -> Option<Cookie<'static>> {
    let value = link.password_cookie_value()?;

    let mut cookie = Cookie::build(PASSWORD_COOKIE, value)
        .path(PASSWORD_COOKIE_PATH)
        .http_only(config.auth.cookie_http_only)
        .secure(config.auth.cookie_secure)
        .finish();

    match config.auth.cookie_same_site.as_ref() {
        "Lax" => cookie.set_same_site(SameSite::Lax),
        "Strict" => cookie.set_same_site(SameSite::Strict),
        _ => cookie.set_same_site(SameSite::None),
    };

    cookie.set_expires(
        OffsetDateTime::from_unix_timestamp(
            Utc::now().timestamp() + PASSWORD_COOKIE_TTL_SECONDS,
        )
        .ok()?,
    );

    Some(cookie)
}

/// The HTTP answer when a link's download cap is exhausted. 410 Gone rather
/// than 403: the link was once valid and is now permanently spent — a retry
/// with better credentials cannot fix it. The JSON shape matches the shared
/// `ErrorResponse` so clients read `message` as usual. Built inline instead
/// of as an `Error` variant to keep the shared error crate untouched.
pub(crate) fn gone() -> HttpResponse {
    HttpResponse::Gone().json(serde_json::json!({
        "message": "link_download_limit_reached"
    }))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
