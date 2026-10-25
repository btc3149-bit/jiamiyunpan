use sea_orm_migration::prelude::*;

use crate::m20220101_000001_create_users::Users;

/// Admin account disable flag. A `None` value means the account is active;
/// a timestamp marks the moment an admin disabled it. Disabled accounts are
/// refused at login and lose their sessions when disabled.
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column(ColumnDef::new(Alias::new("disabled_at")).big_integer().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Alias::new("disabled_at"))
                    .to_owned(),
            )
            .await
    }
}
