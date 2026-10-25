use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// One row per soft-deleted (`trashed`) file. Kept in a side table rather
/// than a column on `files` so the live-file schema stays untouched: every
/// listing/query path that must hide trashed rows joins this table and keeps
/// only the rows with no match, and the trash views filter the join the
/// other way.
///
/// Every file in a trashed folder's subtree carries its own row, stamped
/// with the same `deleted_at` as the trash root the user actually deleted —
/// a child trashed separately earlier keeps its older stamp and will not be
/// resurrected by restoring the parent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "trashed_files")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: Uuid,
    /// Unix seconds at which the file moved into the trash. Drives both the
    /// listing order and the retention sweep (`deleted_at` older than the
    /// configured retention window is purged for good).
    pub deleted_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::files::Entity",
        from = "Column::FileId",
        to = "super::files::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Files,
}

impl Related<super::files::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Files.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
