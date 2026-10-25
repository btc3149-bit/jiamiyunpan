use sea_orm_migration::prelude::*;

use crate::m20230409_091730_create_files::Files;

/// Side table marking soft-deleted ("trashed") files. See
/// `entity/src/trash.rs` for why this is a table and not a column.
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("trashed_files"))
                    .col(
                        ColumnDef::new(Alias::new("file_id"))
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("deleted_at")).big_integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_trashed_files_file_id_files")
                            .from(Alias::new("trashed_files"), Alias::new("file_id"))
                            .to(Files::Table, Files::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::NoAction),
                    )
                    .to_owned(),
            )
            .await?;

        // The retention sweep scans this table by age on every pass.
        manager
            .create_index(
                Index::create()
                    .name("idx_trashed_files_deleted_at")
                    .table(Alias::new("trashed_files"))
                    .col(Alias::new("deleted_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("trashed_files")).to_owned())
            .await
    }
}
