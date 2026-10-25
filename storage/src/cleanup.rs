//! Retention sweep for the trash. Runs on a timer from the server and is
//! a plain function of `(db, fs, now, retention_days)` so the age cutoff
//! is injectable in tests — no `Utc::now()` reads inside.

use entity::{files, trash, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use error::AppResult;
use fs::prelude::{Fs, FsProviderContract};

/// Physically purge every trashed file whose `deleted_at` is older than
/// `retention_days`, relative to the injected `now` (unix seconds).
///
/// DB rows go first — one delete, cascading the trash markers, the
/// recipients' key wraps and the search index — then chunk storage is
/// wiped best-effort per file: a failed purge after the row delete leaks
/// orphaned ciphertext, which the next sweep can no longer see, but the
/// alternative (keeping the rows) resurrects deleted files on every
/// failing storage backend, which is worse.
pub async fn purge_expired<C: ConnectionTrait>(
    db: &C,
    fs: &Fs<'_>,
    now: i64,
    retention_days: u64,
) -> AppResult<usize> {
    let cutoff = now - (retention_days as i64) * 86_400;

    let rows: Vec<(trash::Model, files::Model)> = trash::Entity::find()
        .filter(trash::Column::DeletedAt.lte(cutoff))
        .find_also_related(files::Entity)
        .all(db)
        .await?
        .into_iter()
        .filter_map(|(t, f)| f.map(|f| (t, f)))
        .collect();

    if rows.is_empty() {
        return Ok(0);
    }

    let ids: Vec<entity::Uuid> = rows.iter().map(|(t, _)| t.file_id).collect();

    files::Entity::delete_many()
        .filter(files::Column::Id.is_in(ids))
        .exec(db)
        .await?;

    for (_, file) in &rows {
        if file.mime == "dir" {
            continue;
        }
        if let Err(e) = fs.purge_all(file).await {
            log::warn!("trash sweep: failed to purge chunks of {}: {e}", file.id);
        }
    }

    Ok(rows.len())
}
