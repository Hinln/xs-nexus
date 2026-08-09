use std::env;

use sqlx::postgres::PgPoolOptions;

const ALLOWED_TEST_SCHEMAS: &[&str] = &[
    "xs_nexus_agent_systemd_test",
    "xs_nexus_m21_path_test",
    "xs_nexus_m23_relay_test",
    "xs_nexus_m52_deploy_dev",
    "xs_nexus_scale",
    "xs_nexus_console_e2e_test",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")?;
    let schema = env::var("DATABASE_SCHEMA")?;
    if !ALLOWED_TEST_SCHEMAS.contains(&schema.as_str()) {
        return Err("refusing to reset a non-allowlisted test schema".into());
    }

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    let statement = format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE");
    sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
