//! Trash (soft-delete) repository. Every method re-checks the caller's
//! owner row — the trash is an owner-only surface: only the person holding
//! the owner `user_files` row can list, restore, or purge.

use entity::{
    files, trash, user_files, ActiveModelTrait, ActiveValue, ColumnTrait, Condition,
    ConnectionTrait, EntityTrait, Expr, IntoCondition, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait, Select, Uuid,
};
use error::{AppResult, Error};

use crate::data::{
    app_file::AppFile,
    trash::{RestoreTrash, RestoredTrash, TrashItem, TrashPage, TrashParent},
};

use super::Repository;

pub(crate) struct Trash<'repository, T: ConnectionTrait> {
    repository: &'repository Repository<'repository, T>,
    user_id: Uuid,
}

impl<'repository, T> Trash<'repository, T>
where
    T: ConnectionTrait,
{
    pub(crate) fn new(repository: &'repository Repository<'repository, T>, user_id: Uuid) -> Self {
        Self {
            repository,
            user_id,
        }
    }

    fn connection(&self) -> &impl ConnectionTrait {
        self.repository.connection()
    }

    /// 403 unless the caller holds the owner `user_files` row for `id`.
    async fn ensure_owner(&self, id: Uuid) -> AppResult<()> {
        let row = user_files::Entity::find()
            .filter(user_files::Column::FileId.eq(id))
            .filter(user_files::Column::UserId.eq(self.user_id))
            .filter(user_files::Column::IsOwner.eq(true))
            .one(self.connection())
            .await?;

        if row.is_none() {
            return Err(Error::Forbidden("forbidden_not_owner".to_string()));
        }

        Ok(())
    }

    /// The trash listing base: rows the caller owns that have a
    /// `trashed_files` marker, restricted to trash *roots* — entries whose
    /// parent is the drive root or is not itself trashed. Children of a
    /// trashed folder are hidden behind their root and come back with it.
    fn roots_selector(&self) -> Select<files::Entity> {
        self.repository
            .selector_with_trashed(self.user_id, true)
            .filter(trash::Column::FileId.is_not_null())
            .filter(
                Condition::any()
                    .add(files::Column::FileId.is_null())
                    .add(files::Column::FileId.not_in_subquery(
                        entity::Query::select()
                            .column(trash::Column::FileId)
                            .from(trash::Entity)
                            .to_owned(),
                    )),
            )
    }

    /// Paginated trash listing, newest deletions first.
    pub(crate) async fn list(&self, limit: u64, offset: u64) -> AppResult<TrashPage> {
        let total = self.roots_selector().count(self.connection()).await?;

        let rows: Vec<AppFile> = self
            .roots_selector()
            .order_by_desc(trash::Column::DeletedAt)
            .order_by_desc(files::Column::CreatedAt)
            .limit(limit)
            .offset(offset)
            .into_model::<AppFile>()
            .all(self.connection())
            .await?;

        let mut items = Vec::with_capacity(rows.len());
        for file in rows {
            let deleted_at = trash::Entity::find_by_id(file.id)
                .one(self.connection())
                .await?
                .map(|row| row.deleted_at)
                .unwrap_or_default();
            let parents = self.parent_chain(&file).await?;
            items.push(TrashItem {
                file,
                deleted_at,
                parents,
            });
        }

        Ok(TrashPage {
            items,
            total,
            limit,
            offset,
        })
    }

    /// Encrypted metadata of the ancestor folders of `file`, root-first,
    /// so the client can decrypt and render the original path. Ancestors
    /// of a trash root are always live (otherwise the root wouldn't be a
    /// root), so the standard owner-scoped selector reads them.
    async fn parent_chain(&self, file: &AppFile) -> AppResult<Vec<TrashParent>> {
        let Some(mut cursor) = file.file_id else {
            return Ok(Vec::new());
        };

        let mut ids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while cursor != Uuid::nil() && seen.insert(cursor) {
            ids.push(cursor);
            let parent = files::Entity::find_by_id(cursor)
                .select_only()
                .column(files::Column::FileId)
                .into_tuple::<Option<Uuid>>()
                .one(self.connection())
                .await?
                .flatten();
            match parent {
                Some(next) => cursor = next,
                None => break,
            }
        }

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<AppFile> = self
            .repository
            .selector(self.user_id, true)
            .filter(files::Column::Id.is_in(ids))
            .into_model::<AppFile>()
            .all(self.connection())
            .await?;

        Ok(rows
            .into_iter()
            .map(|f| TrashParent {
                id: f.id.to_string(),
                encrypted_name: f.encrypted_name,
                cipher: f.cipher,
                encrypted_key: f.encrypted_key,
            })
            .collect())
    }

    /// Bring a trashed file (and the subtree trashed by the same
    /// operation) back to life.
    ///
    /// The original parent is kept when it still exists and is live;
    /// otherwise the file is re-attached to the drive root. Names are
    /// encrypted, so a collision at the destination can be detected (via
    /// `name_hash`) but not resolved server-side: with no rename payload
    /// the restore answers `409 trash_restore_name_conflict`, and the
    /// client retries with the " (restored)" name re-encrypted.
    pub(crate) async fn restore(
        &self,
        id: Uuid,
        data: RestoreTrash,
    ) -> AppResult<RestoredTrash> {
        self.ensure_owner(id).await?;

        let trash_row = trash::Entity::find_by_id(id)
            .one(self.connection())
            .await?
            .ok_or_else(|| Error::NotFound("file_not_in_trash".to_string()))?;

        let file = files::Entity::find_by_id(id)
            .one(self.connection())
            .await?
            .ok_or_else(|| Error::NotFound("file_not_found".to_string()))?;

        // Where does the file go back to?
        let mut moved_to_root = false;
        let mut target_parent = file.file_id;
        if let Some(pid) = file.file_id {
            let parent_exists = files::Entity::find_by_id(pid)
                .one(self.connection())
                .await?
                .is_some();
            let parent_trashed = trash::Entity::find_by_id(pid)
                .one(self.connection())
                .await?
                .is_some();
            if !parent_exists || parent_trashed {
                target_parent = None;
                moved_to_root = true;
            }
        }

        // Collision check against the caller's live files in the target
        // folder. The rename payload's `name_hash` wins when provided, so
        // the retried restore checks the suffixed name, not the old one.
        let name_hash = data.name_hash.clone().unwrap_or(file.name_hash.clone());
        let renamed = data.encrypted_name.is_some();
        if !renamed && self.name_taken(&name_hash, target_parent, id).await? {
            return Err(Error::Conflict("trash_restore_name_conflict".to_string()));
        }

        let mut active = files::ActiveModel {
            id: ActiveValue::Set(id),
            ..Default::default()
        };
        if moved_to_root {
            active.file_id = ActiveValue::Set(None);
        }
        if renamed {
            active.encrypted_name = ActiveValue::Set(
                data.encrypted_name
                    .clone()
                    .expect("checked above"),
            );
            if let Some(hash) = &data.name_hash {
                active.name_hash = ActiveValue::Set(hash.clone());
            }
        }
        active.update(self.connection()).await?;

        if renamed {
            self.repository
                .tokens(self.user_id)
                .reindex(
                    id,
                    entity::file_tokens::SearchTags {
                        root: data.search_tokens_root.clone(),
                        file: data.search_tokens_file.clone(),
                    },
                )
                .await?;
        }

        // Untrash this row and every descendant stamped by the same delete
        // operation (same `deleted_at`). A child trashed separately keeps
        // its older stamp and stays in the trash.
        let subtree = self
            .repository
            .manage(self.user_id)
            .subtree_ids(id)
            .await?;
        trash::Entity::delete_many()
            .filter(trash::Column::FileId.is_in(subtree))
            .filter(trash::Column::DeletedAt.eq(trash_row.deleted_at))
            .exec(self.connection())
            .await?;

        Ok(RestoredTrash {
            restored: true,
            moved_to_root,
            renamed,
        })
    }

    /// Does the caller own a live file with this `name_hash` in `parent`?
    async fn name_taken(
        &self,
        name_hash: &str,
        parent: Option<Uuid>,
        exclude_id: Uuid,
    ) -> AppResult<bool> {
        let user_id = self.user_id;
        let mut query = files::Entity::find()
            .join(
                JoinType::InnerJoin,
                files::Relation::UserFiles.def().on_condition(
                    move |_left, right| {
                        Expr::col((right, user_files::Column::UserId))
                            .eq(user_id)
                            .and(user_files::Column::IsOwner.eq(true))
                            .into_condition()
                    },
                ),
            )
            .join(JoinType::LeftJoin, files::Relation::TrashedFiles.def())
            .filter(files::Column::Id.ne(exclude_id))
            .filter(files::Column::NameHash.eq(name_hash))
            .filter(trash::Column::FileId.is_null());

        query = match parent {
            Some(pid) => query.filter(files::Column::FileId.eq(pid)),
            None => query.filter(files::Column::FileId.is_null()),
        };

        Ok(query.one(self.connection()).await?.is_some())
    }

    /// Permanently delete one trashed file and its whole subtree: DB rows
    /// first (cascades user_files, tokens, trash markers), chunk storage
    /// returned to the caller for post-commit purging — same split the old
    /// hard-delete route used.
    pub(crate) async fn purge(&self, id: Uuid) -> AppResult<Vec<files::Model>> {
        self.ensure_owner(id).await?;

        trash::Entity::find_by_id(id)
            .one(self.connection())
            .await?
            .ok_or_else(|| Error::NotFound("file_not_in_trash".to_string()))?;

        // Raw-SQL subtree walk — unlike `file_tree`, this sees trashed
        // descendants too, which is exactly what a purge must remove.
        let mut ids = self
            .repository
            .manage(self.user_id)
            .subtree_ids(id)
            .await?;
        ids.sort();
        ids.dedup();

        let rows = files::Entity::find()
            .filter(files::Column::Id.is_in(ids.clone()))
            .all(self.connection())
            .await?;

        files::Entity::delete_many()
            .filter(files::Column::Id.is_in(ids))
            .exec(self.connection())
            .await?;

        Ok(rows)
    }

    /// Empty the caller's trash: every trashed file they own, whatever
    /// folder it came from. Returns the rows for the caller to purge from
    /// chunk storage after the transaction commits.
    pub(crate) async fn purge_all(&self) -> AppResult<Vec<files::Model>> {
        let user_id = self.user_id;
        let ids: Vec<Uuid> = trash::Entity::find()
            .join(JoinType::InnerJoin, trash::Relation::Files.def())
            .join(
                JoinType::InnerJoin,
                files::Relation::UserFiles.def().on_condition(
                    move |_left, right| {
                        Expr::col((right, user_files::Column::UserId))
                            .eq(user_id)
                            .and(user_files::Column::IsOwner.eq(true))
                            .into_condition()
                    },
                ),
            )
            .select_only()
            .column(trash::Column::FileId)
            .into_tuple::<Uuid>()
            .all(self.connection())
            .await?;

        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = files::Entity::find()
            .filter(files::Column::Id.is_in(ids.clone()))
            .all(self.connection())
            .await?;

        files::Entity::delete_many()
            .filter(files::Column::Id.is_in(ids))
            .exec(self.connection())
            .await?;

        Ok(rows)
    }
}
