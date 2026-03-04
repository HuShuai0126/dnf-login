use anyhow::Result;
use std::path::PathBuf;

/// Database connection parameters (separate fields, no URL encoding needed)
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

/// Server configuration loaded from environment variables or .env file
#[derive(Debug, Clone)]
pub struct Config {
    pub db: DbConfig,

    /// AES-256 encryption key (64 hex characters = 32 bytes)
    pub aes_key_hex: String,

    /// Path to RSA private key PEM file
    pub rsa_private_key_path: PathBuf,

    /// Server bind address (e.g., "0.0.0.0:5505")
    pub bind_address: String,

    /// Starting cera balance granted to newly registered accounts.
    pub initial_cera: u32,

    /// Starting cera_point balance granted to newly registered accounts.
    pub initial_cera_point: u32,
}

impl Config {
    /// Load configuration from environment variables (or .env file).
    ///
    /// Database variables:
    ///   DB_HOST      — MySQL host          (default: 127.0.0.1)
    ///   DB_PORT      — MySQL port          (default: 3306)
    ///   DB_USER      — MySQL username      (default: game)
    ///   DB_PASSWORD  — MySQL password, plain text, no encoding needed
    ///   DB_NAME      — database name       (default: d_taiwan)
    ///
    /// Other variables:
    ///   AES_KEY              — 64 hex chars (32 bytes)
    ///   RSA_PRIVATE_KEY_PATH — path to PEM file (default: ./keys/private_key.pem)
    ///   BIND_ADDRESS         — listen address  (default: 0.0.0.0:5505)
    ///   INITIAL_CERA         — starting cera balance on registration (default: 1000)
    ///   INITIAL_CERA_POINT   — starting cera_point balance          (default: 0)
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let db = DbConfig {
            host: std::env::var("DB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("DB_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3306),
            user: std::env::var("DB_USER").unwrap_or_else(|_| "game".to_string()),
            password: std::env::var("DB_PASSWORD").map_err(|_| {
                anyhow::anyhow!("DB_PASSWORD environment variable is required but not set")
            })?,
            database: std::env::var("DB_NAME").unwrap_or_else(|_| "d_taiwan".to_string()),
        };

        tracing::info!(
            "Database: {}@{}:{}/{}",
            db.user,
            db.host,
            db.port,
            db.database
        );

        let aes_key_hex = std::env::var("AES_KEY").map_err(|_| {
            anyhow::anyhow!(
                "AES_KEY environment variable is required but not set. \
                 Generate a key with: openssl rand -hex 32"
            )
        })?;
        if aes_key_hex.len() != 64 || !aes_key_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!(
                "AES_KEY must be exactly 64 hexadecimal characters (got {}). \
                 Generate a valid key with: openssl rand -hex 32",
                aes_key_hex.len()
            );
        }

        let rsa_private_key_path = std::env::var("RSA_PRIVATE_KEY_PATH")
            .unwrap_or_else(|_| "./keys/private_key.pem".to_string())
            .into();

        let bind_address =
            std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:5505".to_string());

        let initial_cera = std::env::var("INITIAL_CERA")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000u32);

        let initial_cera_point = std::env::var("INITIAL_CERA_POINT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0u32);

        Ok(Self {
            db,
            aes_key_hex,
            rsa_private_key_path,
            bind_address,
            initial_cera,
            initial_cera_point,
        })
    }
}
