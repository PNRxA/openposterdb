use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ab_glyph::FontArc;
use dashmap::DashMap;
use sea_orm::{ConnectionTrait, DatabaseConnection, SqlxSqliteConnector};
use tracing_subscriber::EnvFilter;

use openposterdb_api::cache::MemCacheEntry;
use openposterdb_api::config::Config;
use openposterdb_api::handlers;
use openposterdb_api::services::db;
use openposterdb_api::services::fanart::FanartClient;
use openposterdb_api::services::mdblist::MdblistClient;
use openposterdb_api::services::omdb::OmdbClient;
use openposterdb_api::services::tmdb::TmdbClient;
use openposterdb_api::{build_app, AppState, FONT_BYTES, MIGRATIONS, SCHEMA_SQL};

#[tokio::main]
async fn main() {
    // Load .env file if present (ignored if missing)
    dotenvy::dotenv().ok();

    #[cfg(feature = "test-support")]
    eprintln!("WARNING: test-support feature is enabled — rate limiting is disabled. Do not use in production.");

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sea_orm=warn,sea_orm_migration=warn,sqlx=warn".into()),
        )
        .init();

    let config = Config::from_env();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client");
    let font = FontArc::try_from_slice(FONT_BYTES).expect("failed to load font");

    let omdb = config
        .omdb_api_key
        .as_ref()
        .map(|key| OmdbClient::new(key.clone(), http.clone()));

    let mdblist = config
        .mdblist_api_key
        .as_ref()
        .map(|key| MdblistClient::new(key.clone(), http.clone()));

    let fanart = config
        .fanart_api_key
        .as_ref()
        .map(|key| FanartClient::new(key.clone(), http.clone()));

    // Load JWT secret
    let jwt_secret = db::load_secret_from_env("JWT_SECRET");
    let secure_cookies = std::env::var("COOKIE_SECURE")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    // Ensure cache and database directories exist
    tokio::fs::create_dir_all(&config.cache_dir)
        .await
        .expect("failed to create cache dir");
    tokio::fs::create_dir_all(&config.db_dir)
        .await
        .expect("failed to create db dir");
    let db_dir_abs = tokio::fs::canonicalize(&config.db_dir)
        .await
        .expect("failed to canonicalize db dir");
    let db_path = db_dir_abs.join("openposterdb.db");
    let sqlite_opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .pragma("busy_timeout", "5000")
        .pragma("synchronous", "NORMAL")
        .pragma("cache_size", "-8000")
        .pragma("foreign_keys", "ON");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(32)
        .min_connections(4)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect_with(sqlite_opts)
        .await
        .expect("failed to connect to database");
    let database: DatabaseConnection = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
    for sql in SCHEMA_SQL {
        database
            .execute_unprepared(sql)
            .await
            .expect("failed to create table");
    }
    for (sql, expected_err) in MIGRATIONS {
        match database.execute_unprepared(sql).await {
            Ok(_) => {}
            Err(e) if e.to_string().to_lowercase().contains(expected_err) => {
                tracing::debug!("Migration already applied: {e}");
            }
            Err(e) => panic!("migration failed: {e}\n  SQL: {sql}"),
        }
    }

    // Clean up expired refresh tokens
    match db::delete_expired_refresh_tokens(&database).await {
        Ok(count) if count > 0 => tracing::info!("Cleaned up {count} expired refresh tokens"),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to clean up expired refresh tokens: {e}"),
    }

    // Seed admin user from env if no admins exist
    if let (Ok(username), Ok(password)) = (
        std::env::var("ADMIN_USERNAME"),
        std::env::var("ADMIN_PASSWORD"),
    ) {
        if !username.is_empty() && !password.is_empty() {
            match db::count_admin_users(&database).await {
                Ok(0) => {
                    let hash = handlers::auth::hash_password(&password)
                        .expect("failed to hash admin password");
                    match db::create_admin_user(&database, &username, &hash).await {
                        Ok(_) => tracing::info!("Seeded admin user '{username}' from environment"),
                        Err(e) => tracing::error!("Failed to seed admin user: {e}"),
                    }
                }
                Ok(_) => {
                    tracing::debug!("Admin user already exists, skipping seed");
                }
                Err(e) => tracing::error!("Failed to check admin users: {e}"),
            }
        }
    }

    // Initialize caches
    let api_key_cache = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(300))
        .build();

    // `try_get_with` provides true in-flight coalescing — concurrent requests for the
    // same poster share one generation future. The 30s TTL only retains the completed
    // result briefly so that near-simultaneous requests don't re-generate.
    let poster_inflight = moka::future::Cache::builder()
        .max_capacity(1_000)
        .time_to_live(Duration::from_secs(30))
        .build();

    let id_cache = moka::future::Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let ratings_cache = moka::future::Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(1800))
        .build();

    let poster_mem_cache = moka::future::Cache::builder()
        .weigher(|_key: &String, value: &MemCacheEntry| -> u32 {
            // Poster JPEGs are typically 50-500KB, well within u32 range
            (value.bytes.len() as u32).saturating_add(64)
        })
        .max_capacity(config.poster_mem_cache_mb * 1024 * 1024)
        .time_to_live(Duration::from_secs(3600))
        .time_to_idle(Duration::from_secs(1800))
        .build();

    // Refresh locks use a moka cache with TTL so entries auto-expire if a task panics
    let refresh_locks = moka::sync::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(300))
        .build();

    let fanart_cache = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let fanart_negative = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let settings_cache = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(300))
        .build();

    let global_settings_cache = moka::future::Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(300))
        .build();

    let preview_cache = moka::future::Cache::builder()
        .max_capacity(500)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let free_api_key_cache = moka::future::Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(60))
        .build();

    let pending_last_used: Arc<DashMap<i32, ()>> = Arc::new(DashMap::new());

    let state = Arc::new(AppState {
        tmdb: TmdbClient::new(config.tmdb_api_key.clone(), http),
        omdb,
        mdblist,

        font,
        refresh_locks,
        db: database,
        jwt_secret,
        secure_cookies,
        api_key_cache,
        poster_inflight,
        id_cache,
        ratings_cache,
        poster_mem_cache,
        pending_last_used: pending_last_used.clone(),
        fanart,
        fanart_cache,
        fanart_negative,
        settings_cache,
        global_settings_cache,
        preview_cache,
        free_api_key_cache,
        config: config.clone(),
    });

    // Spawn background flush task for batched last_used_at updates
    {
        let db = state.db.clone();
        let pending = pending_last_used.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let ids: Vec<i32> = pending.iter().map(|r| *r.key()).collect();
                pending.clear();
                if !ids.is_empty()
                    && let Err(e) = db::batch_update_last_used(&db, &ids).await
                {
                    tracing::warn!(error = %e, "failed to batch update last_used_at");
                }
            }
        });
    }

    let app = build_app(state.clone());

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind");

    tracing::info!(addr = %config.listen_addr, "server listening");

    let shutdown_pending = pending_last_used;
    let shutdown_db = state.db.clone();
    let shutdown_signal = async move {
        let ctrl_c = tokio::signal::ctrl_c();
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
        tracing::info!("shutdown signal received, flushing pending last_used updates");
        let ids: Vec<i32> = shutdown_pending.iter().map(|r| *r.key()).collect();
        shutdown_pending.clear();
        if !ids.is_empty()
            && let Err(e) = db::batch_update_last_used(&shutdown_db, &ids).await
        {
            tracing::warn!(error = %e, "failed to flush last_used_at on shutdown");
        }
    };

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal)
    .await
    .expect("server error");
}
