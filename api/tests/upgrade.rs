mod common;

use sea_orm::{ConnectionTrait, DatabaseConnection, SqlxSqliteConnector};

async fn setup_db() -> DatabaseConnection {
    let sqlite_opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true)
        .pragma("foreign_keys", "ON");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlite_opts)
        .await
        .expect("failed to connect to test database");
    let db = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);

    for sql in openposterdb_api::SCHEMA_SQL {
        db.execute_unprepared(sql)
            .await
            .expect("failed to create table");
    }
    for (sql, expected_err) in openposterdb_api::MIGRATIONS {
        match db.execute_unprepared(sql).await {
            Ok(_) => {}
            Err(e) if e.to_string().to_lowercase().contains(expected_err) => {}
            Err(e) => panic!("migration failed: {e}\n  SQL: {sql}"),
        }
    }

    db
}

async fn count_upgrades(db: &DatabaseConnection, name: &str) -> u64 {
    let row = db
        .query_one(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT count(*) as cnt FROM upgrades WHERE name = ?",
            [name.into()],
        ))
        .await
        .unwrap();
    match row {
        Some(r) => {
            use sea_orm::QueryResult;
            r.try_get::<i32>("", "cnt").unwrap() as u64
        }
        None => 0,
    }
}

// --- upgrade framework tests ---

#[tokio::test]
async fn upgrade_run_creates_upgrades_table_and_records_completion() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade::run should succeed");

    // The upgrades table should exist and have an entry
    let count = count_upgrades(&db, "v001_backdrop_cache_keys").await;
    assert_eq!(count, 1, "v001 should be recorded as completed");
}

#[tokio::test]
async fn upgrade_run_is_idempotent() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path().to_str().unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir, false)
        .await
        .expect("first run should succeed");
    openposterdb_api::upgrade::run(&db, cache_dir, false)
        .await
        .expect("second run should succeed (idempotent)");

    let count = count_upgrades(&db, "v001_backdrop_cache_keys").await;
    assert_eq!(count, 1, "should still have exactly one record");
}

// --- v001 backdrop cache key migration: DB ---

#[tokio::test]
async fn v001_renames_backdrop_cache_keys_in_db() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    // Insert rows with old-style _b@ keys
    db.execute_unprepared(
        "INSERT INTO image_meta (cache_key, image_type, created_at, updated_at) VALUES
         ('imdb/tt1234567_b@abc', 'b', 1000, 1000),
         ('imdb/tt7654321_b@def', 'b', 1000, 1000),
         ('imdb/tt0000001_p@ghi', 'poster', 1000, 1000)",
    )
    .await
    .unwrap();

    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    // Backdrop keys should be renamed
    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt1234567_b_f@abc'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "first backdrop key should be renamed to _b_f@");

    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt7654321_b_f@def'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "second backdrop key should be renamed to _b_f@");

    // Poster key should be untouched
    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt0000001_p@ghi'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "poster key should be untouched");
}

// --- v001 backdrop cache key migration: filesystem ---

#[tokio::test]
async fn v001_renames_backdrop_files() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let backdrop_dir = cache_dir.join("backdrops");
    std::fs::create_dir_all(&backdrop_dir).unwrap();

    // Create files with old naming
    std::fs::write(backdrop_dir.join("tt1234567_b@abc.jpg"), b"data1").unwrap();
    std::fs::write(backdrop_dir.join("tt7654321_b@def.jpg"), b"data2").unwrap();
    // Already migrated file — should not be touched
    std::fs::write(backdrop_dir.join("tt0000001_b_f@ghi.jpg"), b"data3").unwrap();
    // TMDB-sourced file — should not be touched
    std::fs::write(backdrop_dir.join("tt9999999_b_t@jkl.jpg"), b"data4").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    // Old files should be renamed
    assert!(
        backdrop_dir.join("tt1234567_b_f@abc.jpg").exists(),
        "file should be renamed from _b@ to _b_f@"
    );
    assert!(
        backdrop_dir.join("tt7654321_b_f@def.jpg").exists(),
        "file should be renamed from _b@ to _b_f@"
    );
    assert!(
        !backdrop_dir.join("tt1234567_b@abc.jpg").exists(),
        "old file should no longer exist"
    );

    // Already-migrated files untouched
    assert!(backdrop_dir.join("tt0000001_b_f@ghi.jpg").exists());
    assert!(backdrop_dir.join("tt9999999_b_t@jkl.jpg").exists());
}

#[tokio::test]
async fn v001_handles_missing_backdrop_directory() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    // Don't create the backdrops subdirectory — should not error
    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed even without backdrops dir");
}

#[tokio::test]
async fn v001_renames_files_in_subdirectories() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let sub_dir = cache_dir.join("backdrops").join("subdir");
    std::fs::create_dir_all(&sub_dir).unwrap();
    std::fs::write(sub_dir.join("tt5555555_b@nested.jpg"), b"nested").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    assert!(
        sub_dir.join("tt5555555_b_f@nested.jpg").exists(),
        "nested file should be renamed"
    );
    assert!(
        !sub_dir.join("tt5555555_b@nested.jpg").exists(),
        "old nested file should be gone"
    );
}

#[tokio::test]
async fn v001_skips_filesystem_when_external_cache_only() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let backdrop_dir = cache_dir.join("backdrops");
    std::fs::create_dir_all(&backdrop_dir).unwrap();
    std::fs::write(backdrop_dir.join("tt1234567_b@abc.jpg"), b"data").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), true)
        .await
        .expect("upgrade should succeed");

    // File should NOT be renamed when external_cache_only is true
    assert!(
        backdrop_dir.join("tt1234567_b@abc.jpg").exists(),
        "file should be untouched in external_cache_only mode"
    );
    assert!(
        !backdrop_dir.join("tt1234567_b_f@abc.jpg").exists(),
        "renamed file should not exist in external_cache_only mode"
    );
}

// --- v002 backdrop position/direction cache key migration: DB ---

#[tokio::test]
async fn v002_inserts_position_and_direction_in_db() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    // Insert rows with old-style backdrop cache keys (no position/direction suffix)
    db.execute_unprepared(
        "INSERT INTO image_meta (cache_key, image_type, created_at, updated_at) VALUES
         ('imdb/tt1234567_b_f@mil.sv.lt.bm.zm', 'b', 1000, 1000),
         ('imdb/tt7654321_b_t@ir.sh.li.bl.zl', 'b', 1000, 1000),
         ('imdb/tt0000001_p@mil.sv.lt.bm.zm', 'poster', 1000, 1000)"
    )
    .await
    .unwrap();

    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    // Backdrop keys should have position and direction inserted (and the later
    // v003 migration appends the default shape/background after the badge size).
    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt1234567_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "first backdrop key should have .ptr and .dv inserted");

    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt7654321_b_t@ir.ptr.sh.li.dv.bl.shr.bgd.zl'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "second backdrop key should have .ptr and .dv inserted");

    // Poster key gets no position/direction (v002 is backdrop-only); v003 still
    // appends the default shape/background.
    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt0000001_p@mil.sv.lt.bm.shr.bgd.zm'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "poster key should not gain position/direction");
}

#[tokio::test]
async fn v002_db_is_idempotent() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    db.execute_unprepared(
        "INSERT INTO image_meta (cache_key, image_type, created_at, updated_at) VALUES
         ('imdb/tt1234567_b_f@mil.sv.lt.bm.zm', 'b', 1000, 1000)"
    )
    .await
    .unwrap();

    let cache_dir = dir.path().to_str().unwrap();
    openposterdb_api::upgrade::run(&db, cache_dir, false)
        .await
        .expect("first run should succeed");
    openposterdb_api::upgrade::run(&db, cache_dir, false)
        .await
        .expect("second run should succeed (idempotent)");

    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT cache_key FROM image_meta WHERE cache_key = 'imdb/tt1234567_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm'".to_string(),
        ))
        .await
        .unwrap();
    assert!(row.is_some(), "key should be migrated exactly once");
}

// --- v002 backdrop position/direction cache key migration: filesystem ---

#[tokio::test]
async fn v002_renames_backdrop_files() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let backdrop_dir = cache_dir.join("backdrops").join("imdb");
    std::fs::create_dir_all(&backdrop_dir).unwrap();

    // Old-style filenames
    std::fs::write(backdrop_dir.join("tt1234567_b_f@mil.sv.lt.bm.zm.jpg"), b"data1").unwrap();
    std::fs::write(backdrop_dir.join("tt7654321_b_t@ir.sh.li.bl.zl.jpg"), b"data2").unwrap();
    // Already migrated — should not be touched
    std::fs::write(backdrop_dir.join("tt0000001_b_f@mil.ptr.sv.lt.dv.bm.zm.jpg"), b"data3").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    // Full pipeline also runs v003, which appends the default shape/background.
    assert!(
        backdrop_dir.join("tt1234567_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm.jpg").exists(),
        "first file should be renamed with .ptr and .dv"
    );
    assert!(
        backdrop_dir.join("tt7654321_b_t@ir.ptr.sh.li.dv.bl.shr.bgd.zl.jpg").exists(),
        "second file should be renamed with .ptr and .dv"
    );
    assert!(
        !backdrop_dir.join("tt1234567_b_f@mil.sv.lt.bm.zm.jpg").exists(),
        "old first file should no longer exist"
    );
    // The v002-migrated file gains no further position/direction, but v003 does
    // append shape/background.
    assert!(
        backdrop_dir.join("tt0000001_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm.jpg").exists(),
        "already position/direction-migrated file should not gain duplicate ptr/dv"
    );
}

#[tokio::test]
async fn v002_skips_filesystem_when_external_cache_only() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let backdrop_dir = cache_dir.join("backdrops").join("imdb");
    std::fs::create_dir_all(&backdrop_dir).unwrap();
    std::fs::write(backdrop_dir.join("tt1234567_b_f@mil.sv.lt.bm.zm.jpg"), b"data").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), true)
        .await
        .expect("upgrade should succeed");

    assert!(
        backdrop_dir.join("tt1234567_b_f@mil.sv.lt.bm.zm.jpg").exists(),
        "file should be untouched in external_cache_only mode"
    );
}

#[tokio::test]
async fn v002_handles_missing_backdrop_directory() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    // Don't create the backdrops subdirectory — should not error
    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed even without backdrops dir");
}

// --- v003 badge shape/background cache key migration: DB ---

async fn key_exists(db: &DatabaseConnection, key: &str) -> bool {
    db.query_one(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        format!("SELECT 1 FROM image_meta WHERE cache_key = '{key}'"),
    ))
    .await
    .unwrap()
    .is_some()
}

#[tokio::test]
async fn v003_inserts_shape_background_in_db() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    // Keys are in their final pre-v003 form (post v001/v002) so the earlier
    // migrations leave them alone and only v003 acts on them.
    db.execute_unprepared(
        "INSERT INTO image_meta (cache_key, image_type, created_at, updated_at) VALUES
         ('imdb/tt1_p@mil.pbc.sh.li.dh.bm.zm', 'p', 1000, 1000),
         ('imdb/tt2_p@mil.pbc.sh.li.dh.bm.x1.zm', 'p', 1000, 1000),
         ('imdb/tt3_l_t@mil.sh.li.bm.zm', 'l', 1000, 1000),
         ('imdb/tt4_b_f@mil.ptr.sv.lt.dv.bm.zm', 'b', 1000, 1000),
         ('imdb/tt5_e_t@m.ptr.sv.lo.dv.bl.blur.zm', 'e', 1000, 1000),
         ('imdb/tt6_p@mil.pbc.sh.li.dh.bm.shr.bgd.zm', 'p', 1000, 1000)",
    )
    .await
    .unwrap();

    openposterdb_api::upgrade::run(&db, dir.path().to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    // Default look (.shr.bgd) inserted right after the badge size token.
    assert!(key_exists(&db, "imdb/tt1_p@mil.pbc.sh.li.dh.bm.shr.bgd.zm").await, "poster");
    // Split posters keep their .x1 token after the inserted shape/background.
    assert!(key_exists(&db, "imdb/tt2_p@mil.pbc.sh.li.dh.bm.shr.bgd.x1.zm").await, "split poster");
    assert!(key_exists(&db, "imdb/tt3_l_t@mil.sh.li.bm.shr.bgd.zm").await, "logo");
    assert!(key_exists(&db, "imdb/tt4_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm").await, "backdrop");
    // Episode blur token stays after the inserted shape/background.
    assert!(key_exists(&db, "imdb/tt5_e_t@m.ptr.sv.lo.dv.bl.shr.bgd.blur.zm").await, "episode w/ blur");
    // Already-migrated key (has .bg) is left untouched.
    assert!(key_exists(&db, "imdb/tt6_p@mil.pbc.sh.li.dh.bm.shr.bgd.zm").await, "already migrated");
}

#[tokio::test]
async fn v003_db_is_idempotent() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();

    db.execute_unprepared(
        "INSERT INTO image_meta (cache_key, image_type, created_at, updated_at) VALUES
         ('imdb/tt1_b_f@mil.ptr.sv.lt.dv.bm.zm', 'b', 1000, 1000)",
    )
    .await
    .unwrap();

    let cache_dir = dir.path().to_str().unwrap();
    openposterdb_api::upgrade::run(&db, cache_dir, false).await.expect("first run");
    openposterdb_api::upgrade::run(&db, cache_dir, false).await.expect("second run (idempotent)");

    assert!(key_exists(&db, "imdb/tt1_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm").await, "migrated exactly once");
    // The double-inserted form must not exist.
    assert!(!key_exists(&db, "imdb/tt1_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.shr.bgd.zm").await, "no double insert");
}

// --- v003 badge shape/background cache key migration: filesystem ---

#[tokio::test]
async fn v003_renames_files_across_image_types() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let poster_dir = cache_dir.join("posters").join("imdb");
    let logo_dir = cache_dir.join("logos").join("imdb");
    let backdrop_dir = cache_dir.join("backdrops").join("imdb");
    let episode_dir = cache_dir.join("episodes").join("imdb");
    for d in [&poster_dir, &logo_dir, &backdrop_dir, &episode_dir] {
        std::fs::create_dir_all(d).unwrap();
    }

    std::fs::write(poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.zm.jpg"), b"p").unwrap();
    std::fs::write(logo_dir.join("tt2_l_t@mil.sh.li.bm.zm.png"), b"l").unwrap();
    std::fs::write(backdrop_dir.join("tt3_b_f@mil.ptr.sv.lt.dv.bm.zm.jpg"), b"b").unwrap();
    std::fs::write(episode_dir.join("tt4_e_t@m.ptr.sv.lo.dv.bl.blur.zm.jpg"), b"e").unwrap();
    // Already migrated — must not be touched.
    std::fs::write(poster_dir.join("tt5@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg"), b"done").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), false)
        .await
        .expect("upgrade should succeed");

    assert!(poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg").exists(), "poster renamed");
    assert!(!poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.zm.jpg").exists(), "old poster gone");
    assert!(logo_dir.join("tt2_l_t@mil.sh.li.bm.shr.bgd.zm.png").exists(), "logo renamed");
    assert!(backdrop_dir.join("tt3_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm.jpg").exists(), "backdrop renamed");
    assert!(episode_dir.join("tt4_e_t@m.ptr.sv.lo.dv.bl.shr.bgd.blur.zm.jpg").exists(), "episode renamed");
    assert!(poster_dir.join("tt5@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg").exists(), "already-migrated untouched");
}

#[tokio::test]
async fn v003_skips_filesystem_when_external_cache_only() {
    let db = setup_db().await;
    let dir = tempfile::tempdir().unwrap();
    let cache_dir = dir.path();

    let poster_dir = cache_dir.join("posters").join("imdb");
    std::fs::create_dir_all(&poster_dir).unwrap();
    std::fs::write(poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.zm.jpg"), b"p").unwrap();

    openposterdb_api::upgrade::run(&db, cache_dir.to_str().unwrap(), true)
        .await
        .expect("upgrade should succeed");

    assert!(
        poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.zm.jpg").exists(),
        "file untouched in external_cache_only mode"
    );
    assert!(!poster_dir.join("tt1@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg").exists());
}

// --- schema migrations ---

/// An `api_key_settings` row created before the font-size columns existed gets
/// the default (`m`) for every image type once the migrations run, and running
/// the migrations again (every startup) is a no-op.
#[tokio::test]
async fn font_size_columns_backfill_existing_rows_with_default() {
    let sqlite_opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true)
        .pragma("foreign_keys", "ON");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlite_opts)
        .await
        .unwrap();
    let db = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);

    // Original schema only (no migrations yet) + a pre-existing settings row.
    for sql in openposterdb_api::SCHEMA_SQL {
        db.execute_unprepared(sql).await.unwrap();
    }
    db.execute_unprepared("INSERT INTO admin_users (username, password_hash) VALUES ('a', 'x')").await.unwrap();
    db.execute_unprepared("INSERT INTO api_keys (name, key_hash, key_prefix, created_by) VALUES ('k', 'h', 'p', 1)").await.unwrap();
    db.execute_unprepared("INSERT INTO api_key_settings (api_key_id) VALUES (1)").await.unwrap();

    for _ in 0..2 {
        for (sql, expected_err) in openposterdb_api::MIGRATIONS {
            match db.execute_unprepared(sql).await {
                Ok(_) => {}
                Err(e) if e.to_string().to_lowercase().contains(expected_err) => {}
                Err(e) => panic!("migration failed: {e}\n  SQL: {sql}"),
            }
        }
    }

    let row = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT poster_font_size, logo_font_size, backdrop_font_size, episode_font_size FROM api_key_settings WHERE api_key_id = 1",
        ))
        .await
        .unwrap()
        .expect("row should exist");
    for col in ["poster_font_size", "logo_font_size", "backdrop_font_size", "episode_font_size"] {
        let v: String = row.try_get("", col).unwrap();
        assert_eq!(v, "m", "{col} should default to 'm'");
    }
}
