use crate::config::DbConfig;
use anyhow::Result;
use mysql_async::{OptsBuilder, Pool};

/// Create a MySQL connection pool from separate config fields.
pub async fn create_pool(db: &DbConfig) -> Result<Pool> {
    tracing::info!(
        "Connecting to MySQL at {}:{}/{}",
        db.host,
        db.port,
        db.database
    );

    let opts = OptsBuilder::default()
        .ip_or_hostname(db.host.clone())
        .tcp_port(db.port)
        .user(Some(db.user.clone()))
        .pass(Some(db.password.clone()))
        .db_name(Some(db.database.clone()))
        // MySQL 5.0 has no @@socket variable; disabling this skips that query.
        .prefer_socket(false);

    Ok(Pool::new(opts))
}

/// Verify the pool works by running SELECT 1.
pub async fn test_connection(pool: &Pool) -> Result<()> {
    use mysql_async::prelude::*;

    let mut conn = pool.get_conn().await?;
    let result: Option<u32> = conn.query_first("SELECT 1").await?;

    if result == Some(1) {
        tracing::info!("Database connection test: OK");
    } else {
        anyhow::bail!("Database connection test failed");
    }

    Ok(())
}
