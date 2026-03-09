mod cache;
mod config;
mod entity;
mod error;
mod handlers;
mod id;
mod poster;
mod routes;
mod services;

use std::net::SocketAddr;
use std::sync::Arc;

use ab_glyph::FontArc;
use axum::middleware;
use axum::Router;
use dashmap::DashMap;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;
use zeroize::Zeroizing;

use config::Config;
use services::db;
use services::mdblist::MdblistClient;
use services::omdb::OmdbClient;
use services::tmdb::TmdbClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub tmdb: TmdbClient,
    pub omdb: Option<OmdbClient>,
    pub mdblist: Option<MdblistClient>,
    pub http: reqwest::Client,
    pub font: FontArc,
    pub refresh_locks: Arc<DashMap<String, ()>>,
    pub db: DatabaseConnection,
    pub jwt_secret: Zeroizing<Vec<u8>>,
    pub secure_cookies: bool,
}

static FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");

#[tokio::main]
async fn main() {
    // Load .env file if present (ignored if missing)
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env();
    let http = reqwest::Client::new();
    let font = FontArc::try_from_slice(FONT_BYTES).expect("failed to load font");

    let omdb = config
        .omdb_api_key
        .as_ref()
        .map(|key| OmdbClient::new(key.clone(), http.clone()));

    let mdblist = config
        .mdblist_api_key
        .as_ref()
        .map(|key| MdblistClient::new(key.clone(), http.clone()));

    // Load JWT secret
    let jwt_secret = db::load_secret_from_env("JWT_SECRET");
    let secure_cookies = std::env::var("COOKIE_SECURE")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);

    // Initialize SQLite database
    tokio::fs::create_dir_all(&config.cache_dir)
        .await
        .expect("failed to create cache dir");
    let cache_dir_abs = tokio::fs::canonicalize(&config.cache_dir)
        .await
        .expect("failed to canonicalize cache dir");
    let db_url = format!(
        "sqlite:{}?mode=rwc",
        cache_dir_abs.join("openposterdb.db").display()
    );
    let database = Database::connect(&db_url)
        .await
        .expect("failed to connect to database");
    database
        .execute_unprepared("PRAGMA journal_mode=WAL")
        .await
        .expect("failed to enable WAL mode");
    database
        .execute_unprepared(
            "CREATE TABLE IF NOT EXISTS poster_meta (
            cache_key TEXT PRIMARY KEY,
            release_date TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        )
        .await
        .expect("failed to create poster_meta table");
    database
        .execute_unprepared(
            "CREATE TABLE IF NOT EXISTS admin_users (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            username      TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            created_at    TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        )
        .await
        .expect("failed to create admin_users table");
    database
        .execute_unprepared(
            "CREATE TABLE IF NOT EXISTS refresh_tokens (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
            token_hash  TEXT NOT NULL UNIQUE,
            expires_at  TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        )
        .await
        .expect("failed to create refresh_tokens table");
    database
        .execute_unprepared(
            "CREATE TABLE IF NOT EXISTS api_keys (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            name         TEXT NOT NULL,
            key_hash     TEXT NOT NULL UNIQUE,
            key_prefix   TEXT NOT NULL,
            created_by   INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            last_used_at TEXT
        )",
        )
        .await
        .expect("failed to create api_keys table");

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

    let state = Arc::new(AppState {
        tmdb: TmdbClient::new(config.tmdb_api_key.clone(), http.clone()),
        omdb,
        mdblist,
        http,
        font,
        refresh_locks: Arc::new(DashMap::new()),
        db: database,
        jwt_secret,
        secure_cookies,
        config: config.clone(),
    });

    // Admin-protected routes (JWT required)
    let admin_routes = routes::api_keys::api_key_routes()
        .route(
            "/api/auth/logout",
            axum::routing::post(handlers::auth::logout),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::middleware::require_auth,
        ));

    let app = Router::new()
        .route(
            "/{api_key}/{id_type}/poster-default/{id_value}",
            axum::routing::get(routes::poster::handler),
        )
        .merge(routes::auth::auth_routes())
        .merge(admin_routes)
        .layer(middleware::from_fn(
            handlers::middleware::validate_origin,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind");

    tracing::info!(addr = %config.listen_addr, "server listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("server error");
}
