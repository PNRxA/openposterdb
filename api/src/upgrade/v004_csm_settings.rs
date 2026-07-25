use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::error::AppError;

/// Add CommonSense Media (CSM) age-rating badge settings to `api_key_settings`.
///
/// Three columns are added with SQLite `ALTER TABLE … ADD COLUMN` (which is
/// safe to run against an already-migrated database because we use `IF NOT
/// EXISTS` guards in the upgrade tracking table). Default values match the
/// application defaults defined in `services::db`:
///
/// - `csm_enabled`  INTEGER NOT NULL DEFAULT 0   (false)
/// - `csm_position` TEXT    NOT NULL DEFAULT 'TopRight'
/// - `csm_size`     TEXT    NOT NULL DEFAULT 'Medium'
///
/// SQLite `ADD COLUMN` with a DEFAULT is non-destructive and instantaneous —
/// no data is rewritten. Existing rows automatically receive the default value.
pub async fn run(db: &DatabaseConnection) -> Result<(), AppError> {
    run_db(db).await
}

pub async fn run_db(db: &impl ConnectionTrait) -> Result<(), AppError> {
    // SQLite does not support `ADD COLUMN IF NOT EXISTS`, so we attempt each
    // ALTER and swallow the "duplicate column name" error, making this
    // migration safe to re-run (e.g. after a partial failure).
    for stmt in [
        "ALTER TABLE api_key_settings ADD COLUMN csm_enabled  INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE api_key_settings ADD COLUMN csm_position TEXT    NOT NULL DEFAULT 'TopRight'",
        "ALTER TABLE api_key_settings ADD COLUMN csm_size     TEXT    NOT NULL DEFAULT 'Medium'",
    ] {
        match db.execute_unprepared(stmt).await {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("duplicate column name") {
                    tracing::debug!("v004: column already exists, skipping: {msg}");
                } else {
                    return Err(e.into());
                }
            }
        }
    }

    tracing::info!("v004: csm_enabled / csm_position / csm_size columns ensured in api_key_settings");
    Ok(())
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};

    use super::*;

    /// Spin up an in-memory SQLite DB, create a minimal `api_key_settings`
    /// table, run the migration, and verify the three new columns exist.
    #[tokio::test]
    async fn adds_csm_columns() {
        let db = Database::connect("sqlite::memory:").await.unwrap();

        db.execute_unprepared(
            "CREATE TABLE api_key_settings (
                api_key TEXT PRIMARY KEY
            )",
        )
        .await
        .unwrap();

        run_db(&db).await.unwrap();

        // Insert a row using the new columns — will fail if they don't exist.
        db.execute(Statement::from_string(
            DatabaseBackend::Sqlite,
            "INSERT INTO api_key_settings (api_key, csm_enabled, csm_position, csm_size)
             VALUES ('testkey', 0, 'TopRight', 'Medium')",
        ))
        .await
        .expect("columns should exist after migration");
    }

    #[tokio::test]
    async fn migration_is_idempotent() {
        let db = Database::connect("sqlite::memory:").await.unwrap();

        db.execute_unprepared(
            "CREATE TABLE api_key_settings (api_key TEXT PRIMARY KEY)",
        )
        .await
        .unwrap();

        // Running twice must not error.
        run_db(&db).await.unwrap();
        run_db(&db).await.unwrap();
    }

    #[tokio::test]
    async fn defaults_applied_to_existing_rows() {
        let db = Database::connect("sqlite::memory:").await.unwrap();

        db.execute_unprepared(
            "CREATE TABLE api_key_settings (api_key TEXT PRIMARY KEY)",
        )
        .await
        .unwrap();

        // Insert a row BEFORE migration — simulates an existing API key.
        db.execute_unprepared(
            "INSERT INTO api_key_settings (api_key) VALUES ('existingkey')",
        )
        .await
        .unwrap();

        run_db(&db).await.unwrap();

        // The pre-existing row should have received the default values.
        let row = db
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT csm_enabled, csm_position, csm_size
                 FROM api_key_settings WHERE api_key = 'existingkey'",
            ))
            .await
            .unwrap()
            .expect("row should exist");

        let enabled: i32 = row.try_get("", "csm_enabled").unwrap();
        let position: String = row.try_get("", "csm_position").unwrap();
        let size: String = row.try_get("", "csm_size").unwrap();

        assert_eq!(enabled, 0);
        assert_eq!(position, "TopRight");
        assert_eq!(size, "Medium");
    }
}
