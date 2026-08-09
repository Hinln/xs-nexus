use std::collections::HashMap;

use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgPoolOptions};
use thiserror::Error;

use crate::config::{ControllerConfig, MigrationConfig};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("unable to connect to PostgreSQL")]
    Connect(#[source] sqlx::Error),
    #[error("unable to create project database schema")]
    Schema(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database migrations are missing, dirty, or do not match this binary")]
    MigrationState,
    #[error("database runtime role is missing or does not match the configured role")]
    RuntimeRole,
}

/// Connects to an already migrated `PostgreSQL` schema using the runtime role.
///
/// # Errors
///
/// Returns `DatabaseError` when connection, schema creation, or migration fails.
pub async fn connect(config: &ControllerConfig) -> Result<PgPool, DatabaseError> {
    let pool = connect_pool(&config.database_url, &config.database_schema, None).await?;
    verify_runtime_role(&pool, config.database_expected_role.as_deref()).await?;
    verify_migrations(&pool).await?;
    Ok(pool)
}

/// Applies the embedded database migrations and closes the migration pool.
///
/// # Errors
///
/// Returns [`DatabaseError`] when the database cannot be reached or migrated.
pub async fn migrate(config: &MigrationConfig) -> Result<(), DatabaseError> {
    create_schema(
        &config.database_url,
        &config.database_schema,
        config.database_owner_role.as_deref(),
    )
    .await?;
    let pool = connect_pool(
        &config.database_url,
        &config.database_schema,
        config.database_owner_role.as_deref(),
    )
    .await?;
    MIGRATOR
        .run(&pool)
        .await
        .map_err(DatabaseError::Migration)?;
    pool.close().await;
    Ok(())
}

async fn connect_pool(
    database_url: &str,
    database_schema: &str,
    database_role: Option<&str>,
) -> Result<PgPool, DatabaseError> {
    let search_path = format!("SET search_path TO \"{database_schema}\", public");
    let set_role = database_role.map(|role| format!("SET ROLE \"{role}\""));
    let pool = PgPoolOptions::new()
        .min_connections(1)
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(move |connection, _metadata| {
            let search_path = search_path.clone();
            let set_role = set_role.clone();
            Box::pin(async move {
                if let Some(set_role) = set_role {
                    connection.execute(set_role.as_str()).await?;
                }
                connection.execute(search_path.as_str()).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .map_err(DatabaseError::Connect)?;

    Ok(pool)
}

async fn create_schema(
    database_url: &str,
    schema: &str,
    owner_role: Option<&str>,
) -> Result<(), DatabaseError> {
    let mut connection = PgConnection::connect(database_url)
        .await
        .map_err(DatabaseError::Connect)?;
    if let Some(owner_role) = owner_role {
        connection
            .execute(format!("SET ROLE \"{owner_role}\"").as_str())
            .await
            .map_err(DatabaseError::Schema)?;
    }
    connection
        .execute(format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"").as_str())
        .await
        .map_err(DatabaseError::Schema)?;
    Ok(())
}

async fn verify_runtime_role(
    pool: &PgPool,
    expected_role: Option<&str>,
) -> Result<(), DatabaseError> {
    let Some(expected_role) = expected_role else {
        return Ok(());
    };
    let row = sqlx::query("SELECT current_user AS role_name")
        .fetch_one(pool)
        .await
        .map_err(|_| DatabaseError::RuntimeRole)?;
    let role_name: String = row
        .try_get("role_name")
        .map_err(|_| DatabaseError::RuntimeRole)?;
    if role_name != expected_role {
        return Err(DatabaseError::RuntimeRole);
    }
    Ok(())
}

async fn verify_migrations(pool: &PgPool) -> Result<(), DatabaseError> {
    let rows = sqlx::query(
        "SELECT version, checksum, success
         FROM _sqlx_migrations
         ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| DatabaseError::MigrationState)?;
    let mut applied = HashMap::with_capacity(rows.len());
    for row in rows {
        let version = row
            .try_get::<i64, _>("version")
            .map_err(|_| DatabaseError::MigrationState)?;
        let checksum = row
            .try_get::<Vec<u8>, _>("checksum")
            .map_err(|_| DatabaseError::MigrationState)?;
        let success = row
            .try_get::<bool, _>("success")
            .map_err(|_| DatabaseError::MigrationState)?;
        if !success || applied.insert(version, checksum).is_some() {
            return Err(DatabaseError::MigrationState);
        }
    }
    let expected = MIGRATOR
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .collect::<Vec<_>>();
    if applied.len() != expected.len()
        || expected.iter().any(|migration| {
            applied
                .get(&migration.version)
                .is_none_or(|checksum| checksum.as_slice() != migration.checksum.as_ref())
        })
    {
        return Err(DatabaseError::MigrationState);
    }
    Ok(())
}
