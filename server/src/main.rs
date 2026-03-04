use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::trace::TraceLayer;

mod config;
mod db;
mod handlers;
mod services;

use config::Config;
use handlers::{AppState, encrypted_request, health};
use services::AuthService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting DNF Login Server...");

    let config = Config::from_env()?;
    tracing::info!("Configuration loaded");

    let pool = db::create_pool(&config.db).await?;
    db::test_connection(&pool).await?;

    let rsa_pem = tokio::fs::read_to_string(&config.rsa_private_key_path).await?;
    let token_generator = dnf_shared::crypto::TokenGenerator::from_pem(&rsa_pem)?;
    tracing::info!(
        "RSA private key loaded (key size: {} bits)",
        token_generator.key_size()
    );

    let aes_cipher = dnf_shared::crypto::AesGcmCipher::from_hex_key(&config.aes_key_hex)?;
    tracing::info!("AES-256-GCM cipher initialized");

    let auth_service = AuthService::new(
        pool.clone(),
        token_generator,
        config.initial_cera,
        config.initial_cera_point,
    );

    let state = AppState {
        auth_service: Arc::new(auth_service),
        aes_cipher: Arc::new(aes_cipher),
        rate_limiter: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/auth", post(encrypted_request))
        .layer(DefaultBodyLimit::max(4096))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = config.bind_address.parse()?;

    tracing::info!("Server starting on http://{}", addr);
    tracing::warn!("TLS disabled -- use a reverse proxy (nginx/caddy) for HTTPS in production");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
