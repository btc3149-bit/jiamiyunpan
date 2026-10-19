use error::AppResult;
use jiamiyunpan::{Config, Context};
use migration::{Migrator, MigratorTrait};

#[actix_web::main]
async fn main() -> AppResult<()> {
    // Catch any panic from any thread running and dump it here
    // This enables us to kill the entire process if any of the child threads die
    let origin_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        origin_hook(panic_info);
        std::process::exit(1);
    }));

    // Initialize the configuration of the app
    let mut config = Config::new(
        "甲米云盘",
        env!("CARGO_PKG_VERSION"),
        "甲米云盘 —— 简单、快速、端到端加密的云存储。",
    );

    config.app.ensure_data_dir(None);

    // Handle subcommands before starting the server
    if config.subcommand.as_deref() == Some("migrate-storage") {
        config.announce();
        return jiamiyunpan::migrate::migrate_storage(&config).await;
    }

    config.announce();

    // Create context from the config
    let context = Context::new(config).await?;

    // Run database migrations
    Migrator::up(&context.db, None).await?;

    // Init logger
    env_logger::init();

    // Emit warnings after logger is initialized
    context.config.emit_warnings();

    // Start the server
    jiamiyunpan::server::engage(context).await
}
