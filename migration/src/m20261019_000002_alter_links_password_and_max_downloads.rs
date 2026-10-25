use sea_orm_migration::prelude::*;

/// Optional shared-link protection: a download password (bcrypt hash) and a
/// cap on completed downloads. Both nullable — absent means the link behaves
/// exactly as before.
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("links"))
                    .add_column(ColumnDef::new(Alias::new("password_hash")).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("links"))
                    .add_column(ColumnDef::new(Alias::new("max_downloads")).integer().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("links"))
                    .drop_column(Alias::new("max_downloads"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("links"))
                    .drop_column(Alias::new("password_hash"))
                    .to_owned(),
            )
            .await
    }
}
