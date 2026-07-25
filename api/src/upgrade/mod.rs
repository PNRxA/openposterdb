mod v001_backdrop_cache_keys;
mod v002_backdrop_position_direction_cache;
mod v003_badge_shape_background_cache;
mod v004_csm_settings;

use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::error::AppError;

/// Run all pending data upgrades. Each upgrade is tracked in the `upgrades`
/// table so it executes only once. New upgrades are added as modules and
/// registered below.
pub async fn run(
    db: &DatabaseConnection,
    cache_dir: &str,
    external_cache_only: bool,
) -> Result<(), AppError> {
    ensure_table(db).await?;

    run_once(db, "v001_backdrop_cache_keys", || {
        v001_backdrop_cache_keys::run(db, cache_dir, external_cache_only)
    })
    .await?;

    run_once(db, "v002_backdrop_position_direction_cache", || {
        v002_backdrop_position_direction_cache::run(db, cache_dir, external_cache_only)
    })
    .await?;

    run_once(db, "v003_badge_shape_background_cache", || {
        v003_badge_shape_background_cache::run(db, cache_dir, external_cache_only)
    })
    .await?;

    run_once(db, "v004_csm_settings", || v004_csm_settings::run(db))
    .await?;

    Ok(())
}

async fn ensure_table(db: &DatabaseConnection) -> Result<(), AppError> {
    db.execute_unprepared(
        "CREATE TABLE IF NOT EXISTS upgrades (
            name         TEXT PRIMARY KEY,
            completed_at INTEGER NOT NULL
        )",
    )
    .await?;
    Ok(())
}

async fn is_completed(db: &DatabaseConnection, name: &str) -> Result<bool, AppError> {
    let row = db
        .query_one(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT 1 FROM upgrades WHERE name = ?",
            [name.into()],
        ))
        .await?;
    Ok(row.is_some())
}

async fn mark_completed(db: &DatabaseConnection, name: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().timestamp();
    db.execute(sea_orm::Statement::from_sql_and_values(
        sea_orm::DatabaseBackend::Sqlite,
        "INSERT INTO upgrades (name, completed_at) VALUES (?, ?)",
        [name.into(), now.into()],
    ))
    .await?;
    Ok(())
}

async fn run_once<F, Fut>(db: &DatabaseConnection, name: &str, f: F) -> Result<(), AppError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppError>>,
{
    if is_completed(db, name).await? {
        return Ok(());
    }
    tracing::info!(name, "running data upgrade");
    f().await?;
    mark_completed(db, name).await?;
    tracing::info!(name, "data upgrade complete");
    Ok(())
}
