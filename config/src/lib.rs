pub mod app;
pub mod auth;
pub mod config;
pub mod direct;
pub mod email;
pub(crate) mod helpers;
pub mod s3;
pub mod ssl;
pub mod vars;

use helpers::remove_trailing_slash;

/// Report whether a secret-like env var is set to a non-empty value.
/// Used by `announce` to print presence without ever printing the value.
fn secret_is_set(name: &str) -> &'static str {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => "set",
        _ => "not set",
    }
}

pub use crate::config::Config;

impl Config {
    pub fn get_app_name(&self) -> String {
        self.app.name.clone()
    }

    pub fn get_app_version(&self) -> String {
        self.app.version.clone()
    }

    pub fn get_app_url(&self) -> String {
        remove_trailing_slash(self.app.app_url.to_string())
    }

    pub fn get_client_url(&self) -> String {
        remove_trailing_slash(self.app.client_url.to_string())
    }

    pub fn get_full_bind_address(&self) -> String {
        format!("{}:{}", self.app.address, self.app.port)
    }

    pub fn announce(&self) {
        println!(
            "Starting {} v{} on {}",
            self.get_app_name(),
            self.get_app_version(),
            self.get_full_bind_address()
        );

        println!("-- Using data_dir: {}", self.app.data_dir);

        // Storage backend
        match self.app.storage_provider.as_str() {
            "s3" => {
                if let Some(ref s3) = self.s3 {
                    println!(
                        "-- Storage provider: s3 (bucket: {}, region: {})",
                        s3.bucket, s3.region
                    );
                    println!(
                        "-- S3 direct transfer (client presigned URLs): {}",
                        if s3.direct_transfer { "enabled" } else { "disabled" }
                    );
                }
            }
            _ => {
                println!("-- Storage provider: local");
            }
        }

        // Database backend — never print DATABASE_URL itself, it embeds the
        // password for Postgres setups.
        match self.app.database_url {
            Some(_) => println!("-- Database: PostgreSQL"),
            None => println!("-- Database: SQLite ({}/sqlite.db)", self.app.data_dir),
        }

        // TLS mode — state whether the configured cert files already exist
        // (custom cert) or the server will self-sign on first start.
        if self.ssl.disabled {
            println!("-- TLS: disabled (plain HTTP, development only)");
        } else {
            let have_cert = std::path::Path::new(&self.ssl.cert_file).exists()
                && std::path::Path::new(&self.ssl.key_file).exists();
            if have_cert {
                println!("-- TLS: enabled (custom cert: {})", self.ssl.cert_file);
            } else {
                println!(
                    "-- TLS: enabled (self-signed cert will be generated in DATA_DIR)"
                );
            }
            println!("-- Using ssl cert: {}", self.ssl.cert_file);
            println!("-- Using ssl key: {}", self.ssl.key_file);
        }

        // Secret presence checks — set/unset only, never the values.
        println!(
            "-- JWT_SECRET: {}",
            secret_is_set("JWT_SECRET")
        );
        println!(
            "-- S3_ACCESS_KEY: {}, S3_SECRET_KEY: {}",
            secret_is_set("S3_ACCESS_KEY"),
            secret_is_set("S3_SECRET_KEY")
        );
        println!(
            "-- SMTP_PASSWORD: {}",
            secret_is_set("SMTP_PASSWORD")
        );

        println!("-- RUST_LOG={:?}", std::env::var("RUST_LOG").ok());
        println!("------------------------------------------");
    }

    /// Emit any warnings collected during configuration initialization.
    /// Call this after env_logger::init() to ensure warnings are visible.
    pub fn emit_warnings(&self) {
        for warning in &self.warnings {
            log::warn!("{}", warning);
        }
    }
}
