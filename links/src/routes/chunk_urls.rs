use actix_web::{route, web, HttpRequest, HttpResponse};
use context::Context;
use entity::Uuid;
use error::{AppResult, Error};
use fs::prelude::*;
use serde::{Deserialize, Serialize};

use crate::repository::Repository;
use crate::routes::guard;

/// One chunk's presigned URL.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkUrl {
    pub chunk: i64,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkUrls {
    pub urls: Vec<ChunkUrl>,
    pub expires_at: i64,
}

/// `POST /api/links/{link_id}/chunk-urls` — read URLs for a shared file.
///
/// Unauthenticated, exactly like [`super::download::download`]: a link is
/// itself the credential, and what comes back is ciphertext either way. The
/// recipient decrypts with the key from the URL fragment, which never reaches
/// this server or the storage bucket.
///
/// Password-protected links demand the same credentials as the download
/// route (header or unlock cookie) — this is the gate that matters, because
/// on direct-transfer deployments the bucket URLs minted here bypass the
/// relay route entirely.
///
/// The download counter moves here, and it counts something slightly
/// different than it did. The relaying route waits for the final chunk before
/// counting, so an abandoned transfer never registered. Nothing reports back
/// once a client is talking to the bucket, so this counts a download that
/// started.
///
/// With `max_downloads` set, the reservation happens inside the same atomic
/// conditional `UPDATE` that increments: a manifest is only minted when a
/// download slot was actually claimed. Once the cap is spent the client gets
/// a 410 and no URLs — a presigned URL is unforgeable afterwards, so the
/// cap holds even against the bucket.
#[route("/api/links/{link_id}/chunk-urls", method = "POST")]
pub(crate) async fn chunk_urls(
    req: HttpRequest,
    context: web::Data<Context>,
) -> AppResult<HttpResponse> {
    let context = context.into_inner();
    let link_id: Uuid = util::actix::path_var(&req, "link_id")?;
    let repository = Repository::new(&context);

    let link = repository.get(link_id).await?;

    if link.is_expired() {
        return Err(Error::Unauthorized("link_expired".to_string()));
    }

    guard::authorize_password(&link, &req)?;

    let fs = Fs::new(&context.config);
    let version = link.file_active_version;

    let chunks = if link.file_editable {
        fs.get_uploaded_chunks_v(&link, version).await?
    } else {
        fs.get_uploaded_chunks(&link).await?
    };

    let urls = fs
        .direct_get_urls(&link, version, &chunks)
        .await?
        .ok_or_else(|| Error::BadRequest("direct_transfer_unavailable".to_string()))?;

    if !repository.try_increment_downloads(link.id).await? {
        return Ok(guard::gone());
    }

    let expires_at = chrono::Utc::now().timestamp()
        + context
            .config
            .s3
            .as_ref()
            .map(|s3| s3.direct_expiry_secs)
            .unwrap_or(0) as i64;

    Ok(HttpResponse::Ok().json(ChunkUrls {
        urls: chunks
            .iter()
            .zip(urls)
            .map(|(chunk, url)| ChunkUrl { chunk: *chunk, url })
            .collect(),
        expires_at,
    }))
}
