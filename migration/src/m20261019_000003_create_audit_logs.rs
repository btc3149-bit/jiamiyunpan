use sea_orm_migration::prelude::*;

/// Append-only audit trail of security-relevant events (logins, admin
/// actions). See `entity/src/audit_log.rs` for the entity and the
/// fire-and-forget recording helper.
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("audit_logs"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    // Nullable: failed logins have no authenticated user yet.
                    .col(ColumnDef::new(Alias::new("user_id")).uuid())
                    .col(ColumnDef::new(Alias::new("action")).string().not_null())
                    .col(ColumnDef::new(Alias::new("target")).string())
                    .col(ColumnDef::new(Alias::new("ip")).string())
                    .col(ColumnDef::new(Alias::new("user_agent")).string())
                    // Free-form JSON payload for action-specific details.
                    .col(ColumnDef::new(Alias::new("metadata")).text())
                    .col(ColumnDef::new(Alias::new("created_at")).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        // "Who did what to whom, when" lookups from the admin audit view.
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_user_id")
                    .table(Alias::new("audit_logs"))
                    .col(Alias::new("user_id"))
                    .to_owned(),
            )
            .await?;

        // Action is filtered by prefix (`login`, `admin.user`, ...).
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_action")
                    .table(Alias::new("audit_logs"))
                    .col(Alias::new("action"))
                    .to_owned(),
            )
            .await?;

        // The listing is always newest-first and filterable by date range.
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_logs_created_at")
                    .table(Alias::new("audit_logs"))
                    .col(Alias::new("created_at"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("audit_logs")).to_owned())
            .await
    }
}
