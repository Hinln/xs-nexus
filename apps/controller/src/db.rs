use sqlx::{
    Connection, Executor, PgConnection, PgPool,
    postgres::{PgPoolOptions, PgQueryResult},
};
use thiserror::Error;

use crate::config::ControllerConfig;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("unable to connect to PostgreSQL")]
    Connect(#[source] sqlx::Error),
    #[error("unable to create project database schema")]
    Schema(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
}

/// Connects to `PostgreSQL`, creates the project schema, and applies migrations.
///
/// # Errors
///
/// Returns `DatabaseError` when connection, schema creation, or migration fails.
pub async fn connect(config: &ControllerConfig) -> Result<PgPool, DatabaseError> {
    create_schema(&config.database_url, &config.database_schema).await?;

    let search_path = format!("SET search_path TO \"{}\", public", config.database_schema);
    let pool = PgPoolOptions::new()
        .min_connections(1)
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(move |connection, _metadata| {
            let search_path = search_path.clone();
            Box::pin(async move {
                connection.execute(search_path.as_str()).await?;
                Ok(())
            })
        })
        .connect(&config.database_url)
        .await
        .map_err(DatabaseError::Connect)?;

    MIGRATOR
        .run(&pool)
        .await
        .map_err(DatabaseError::Migration)?;
    Ok(pool)
}

async fn create_schema(database_url: &str, schema: &str) -> Result<PgQueryResult, DatabaseError> {
    let mut connection = PgConnection::connect(database_url)
        .await
        .map_err(DatabaseError::Connect)?;
    connection
        .execute(format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"").as_str())
        .await
        .map_err(DatabaseError::Schema)
}
