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
use openposterdb_api::services::trakt::TraktClient;
use openposterdb_api::{build_app, upgrade, AppState, FONT_BYTES, MIGRATIONS, SCHEMA_SQL};

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
        // Trakt requires a User-Agent and fronts its API with a Cloudflare WAF
        // that returns 403 to requests without one; reqwest sends none by default.
        // Identify ourselves on every outbound provider call.
        .user_agent(concat!(
            "openposterdb/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/PNRxA/openposterdb)"
        ))
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

    let trakt = config
        .trakt_client_id
        .as_ref()
        .map(|key| TraktClient::new(key.clone(), http.clone()));

    // Load JWT secret
    let jwt_secret = db::load_secret_from_env("JWT_SECRET");
    let secure_cookies = std::env::var("COOKIE_SECURE")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    // Log startup configuration
    tracing::info!(
        mdblist = config.mdblist_api_key.is_some(),
        omdb = config.omdb_api_key.is_some(),
        fanart = config.fanart_api_key.is_some(),
        trakt = config.trakt_client_id.is_some(),
        "rating providers configured"
    );
    tracing::info!(
        cache_dir = %config.cache_dir,
        db_dir = %config.db_dir,
        image_quality = config.image_quality,
        mem_cache_mb = config.image_mem_cache_mb,
        secure_cookies,
        cdn_redirects = config.enable_cdn_redirects,
        external_cache_only = config.external_cache_only,
        free_key_enabled = ?config.free_key_enabled,
        "server configuration"
    );

    if config.external_cache_only && !config.enable_cdn_redirects {
        tracing::warn!(
            "EXTERNAL_CACHE_ONLY is enabled without ENABLE_CDN_REDIRECTS — \
             every request after the in-memory cache expires will regenerate the image. \
             Consider enabling CDN redirects so a CDN can absorb repeat traffic."
        );
    }

    // Ensure cache and database directories exist
    if !config.external_cache_only {
        tokio::fs::create_dir_all(&config.cache_dir)
            .await
            .expect("failed to create cache dir");
        // Sweep cache dirs left renamed-aside by a clear-all that was interrupted
        // (crash/restart) before its background removal finished. Run it off the
        // boot path so a large leftover can't delay the server coming up — the
        // staged dirs are inert (never served) until removed.
        let staged_cache_dir = config.cache_dir.clone();
        tokio::spawn(async move {
            if let Err(e) = openposterdb_api::cache::cleanup_staged_dirs(&staged_cache_dir).await {
                tracing::warn!(error = %e, "failed to sweep leftover staged cache dirs at startup");
            }
        });
    }
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

    // Run data upgrades (cache key migrations, etc.)
    upgrade::run(&database, &config.cache_dir, config.external_cache_only)
        .await
        .expect("data upgrade failed");

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
    // same image share one generation future. The 30s TTL only retains the completed
    // result briefly so that near-simultaneous requests don't re-generate.
    let image_inflight = moka::future::Cache::builder()
        .max_capacity(1_000)
        .time_to_live(Duration::from_secs(30))
        // A per-title purge must also drop any just-completed render held here, or
        // `try_get_with` would re-serve the stale bytes (and re-promote them into
        // image_mem_cache) for up to the 30s TTL. Needs invalidation-closure support.
        .support_invalidation_closures()
        .build();

    let id_cache = moka::future::Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let ratings_cache = moka::future::Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(1800))
        .build();

    let image_mem_cache = moka::future::Cache::builder()
        .weigher(|_key: &String, value: &MemCacheEntry| -> u32 {
            // Images are typically 50-500KB, well within u32 range
            u32::try_from(value.bytes.len()).unwrap_or(u32::MAX).saturating_add(64)
        })
        .max_capacity(config.image_mem_cache_mb * 1024 * 1024)
        .time_to_live(Duration::from_secs(3600))
        .time_to_idle(Duration::from_secs(1800))
        // Required so a per-title cache purge can evict every rendered variant
        // of a title via `invalidate_entries_if` (prefix predicate). Without
        // this opt-in moka returns `PredicateError::InvalidationClosuresDisabled`.
        .support_invalidation_closures()
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

    let tmdb_images_cache = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(1800))
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

    let settings_hash_registry = moka::future::Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(300))
        .build();

    let available_ratings_cache = moka::future::Cache::builder()
        .max_capacity(50_000)
        .time_to_live(Duration::from_secs(3600))
        .build();

    let available_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    tracing::info!(cpus = available_cpus, "detected available CPUs");

    let render_concurrency: usize = std::env::var("RENDER_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(available_cpus * 2);
    let render_semaphore = Arc::new(tokio::sync::Semaphore::new(render_concurrency));
    tracing::info!(permits = render_concurrency, "render semaphore initialized");

    let cross_id_concurrency: usize = std::env::var("CROSS_ID_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(available_cpus);
    let cross_id_semaphore = Arc::new(tokio::sync::Semaphore::new(cross_id_concurrency));
    tracing::info!(permits = cross_id_concurrency, "cross-id semaphore initialized");

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
        image_inflight,
        id_cache,
        ratings_cache,
        image_mem_cache,
        pending_last_used: pending_last_used.clone(),
        fanart,
        trakt,
        fanart_cache,
        fanart_negative,
        tmdb_images_cache,
        settings_cache,
        global_settings_cache,
        preview_cache,
        free_api_key_cache,
        render_semaphore,
        cross_id_semaphore,
        settings_hash_registry,
        available_ratings_cache,
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
        let ctrl_c = async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };
        
        #[cfg(unix)]
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
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
