pub mod cache;
pub mod config;
pub mod entity;
pub mod error;
pub mod handlers;
pub mod id;
pub mod poster;
pub mod routes;
pub mod services;

use std::sync::Arc;

use ab_glyph::FontArc;
use axum::http::header::{self, HeaderValue};
use axum::http::Request;
use axum::middleware;
use axum::Router;
use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{MakeSpan, TraceLayer};
use zeroize::Zeroizing;

use cache::MemCacheEntry;
use config::Config;
use id::ResolvedId;
use services::db::PosterSettings;
use services::fanart::{FanartClient, FanartPoster};
use services::mdblist::MdblistClient;
use services::omdb::OmdbClient;
use services::ratings::RatingBadge;
use services::tmdb::TmdbClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub tmdb: TmdbClient,
    pub omdb: Option<OmdbClient>,
    pub mdblist: Option<MdblistClient>,
    pub http: reqwest::Client,
    pub font: FontArc,
    pub refresh_locks: moka::sync::Cache<String, ()>,
    pub db: DatabaseConnection,
    pub jwt_secret: Zeroizing<Vec<u8>>,
    pub secure_cookies: bool,
    pub api_key_cache: moka::future::Cache<String, Option<i32>>,
    pub poster_inflight: moka::future::Cache<String, bytes::Bytes>,
    pub id_cache: moka::future::Cache<String, ResolvedId>,
    pub ratings_cache: moka::future::Cache<String, Vec<RatingBadge>>,
    pub poster_mem_cache: moka::future::Cache<String, MemCacheEntry>,
    pub pending_last_used: Arc<DashMap<i32, ()>>,
    pub fanart: Option<FanartClient>,
    pub fanart_cache: moka::future::Cache<String, Arc<Vec<FanartPoster>>>,
    /// Tracks negative fanart results — e.g. "movie:123:textless" means no textless poster exists.
    /// Entries expire after the same TTL as fanart_cache so we recheck periodically.
    pub fanart_negative: moka::future::Cache<String, ()>,
    pub settings_cache: moka::future::Cache<i32, Arc<PosterSettings>>,
    pub global_settings_cache: moka::future::Cache<(), Arc<PosterSettings>>,
}

pub static FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");

pub const SCHEMA_SQL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS poster_meta (
        cache_key TEXT PRIMARY KEY,
        release_date TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS admin_users (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        username      TEXT NOT NULL UNIQUE,
        password_hash TEXT NOT NULL,
        created_at    TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS refresh_tokens (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id     INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
        token_hash  TEXT NOT NULL UNIQUE,
        expires_at  TEXT NOT NULL,
        created_at  TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE TABLE IF NOT EXISTS api_keys (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        name         TEXT NOT NULL,
        key_hash     TEXT NOT NULL UNIQUE,
        key_prefix   TEXT NOT NULL,
        created_by   INTEGER NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
        created_at   TEXT NOT NULL DEFAULT (datetime('now')),
        last_used_at TEXT
    )",
    "CREATE TABLE IF NOT EXISTS global_settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS api_key_settings (
        api_key_id      INTEGER PRIMARY KEY REFERENCES api_keys(id) ON DELETE CASCADE,
        poster_source   TEXT NOT NULL DEFAULT 'tmdb',
        fanart_lang     TEXT NOT NULL DEFAULT 'en',
        fanart_textless INTEGER NOT NULL DEFAULT 0,
        ratings_limit   INTEGER NOT NULL DEFAULT 3,
        ratings_order   TEXT NOT NULL DEFAULT 'mal,imdb,lb,rt,rta,mc,tmdb,trakt'
    )",
];

/// Migrations that run after schema creation. Each is checked for a specific
/// expected error before being skipped (e.g. "duplicate column" for ADD COLUMN).
/// This avoids blindly swallowing all ALTER errors.
pub const MIGRATIONS: &[(&str, &str)] = &[
    (
        "ALTER TABLE api_key_settings ADD COLUMN ratings_limit INTEGER NOT NULL DEFAULT 3",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN ratings_order TEXT NOT NULL DEFAULT 'mal,imdb,lb,rt,rta,mc,tmdb,trakt'",
        "duplicate column",
    ),
];

fn build_cors_layer(config: &Config) -> CorsLayer {
    match config.cors_origin {
        Some(ref origin) => CorsLayer::new()
            .allow_origin(AllowOrigin::exact(
                HeaderValue::from_str(origin).expect("valid CORS_ORIGIN"),
            ))
            .allow_methods(AllowMethods::list([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
            ]))
            .allow_headers(AllowHeaders::list([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
            ]))
            .allow_credentials(true),
        None => CorsLayer::new(),
    }
}

fn redact_path(path: &str) -> String {
    if !path.starts_with("/api/") {
        // Poster route: /{api_key}/... -> /[REDACTED]/...
        match path[1..].find('/') {
            Some(pos) => format!("/[REDACTED]{}", &path[1 + pos..]),
            None => "/[REDACTED]".into(),
        }
    } else {
        path.to_string()
    }
}

#[derive(Clone)]
struct RedactedMakeSpan;

impl<B> MakeSpan<B> for RedactedMakeSpan {
    fn make_span(&mut self, req: &Request<B>) -> tracing::Span {
        let redacted_uri = redact_path(req.uri().path());
        tracing::info_span!("request", method = %req.method(), uri = %redacted_uri, version = ?req.version())
    }
}

#[cfg(not(any(test, feature = "test-support")))]
#[derive(Debug, Clone)]
struct PosterKeyExtractor;

#[cfg(not(any(test, feature = "test-support")))]
impl tower_governor::key_extractor::KeyExtractor for PosterKeyExtractor {
    type Key = String;

    fn extract<T>(
        &self,
        req: &Request<T>,
    ) -> Result<Self::Key, tower_governor::GovernorError> {
        let path = req.uri().path();
        let api_key = path.split('/').nth(1).unwrap_or("unknown");
        let key_prefix = &api_key[..api_key.len().min(16)];

        // Extract IP from headers or connection info
        let ip = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                req.headers()
                    .get("x-real-ip")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        Ok(format!("{key_prefix}:{ip}"))
    }
}

pub fn build_app(state: Arc<AppState>) -> Router {
    let admin_routes = routes::api_keys::api_key_routes()
        .merge(routes::admin::admin_routes())
        .route(
            "/api/auth/logout",
            axum::routing::post(handlers::auth::logout),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            handlers::middleware::require_auth,
        ));

    let key_self_routes = routes::api_keys::api_key_self_routes().layer(
        middleware::from_fn_with_state(
            state.clone(),
            handlers::middleware::require_api_key_auth,
        ),
    );

    let compressed_routes = Router::new()
        .merge(routes::auth::auth_routes())
        .merge(admin_routes)
        .merge(key_self_routes)
        .layer(CompressionLayer::new());

    let poster_route = {
        let router = Router::new().route(
            "/{api_key}/{id_type}/poster-default/{id_value}",
            axum::routing::get(routes::poster::handler),
        );

        #[cfg(not(any(test, feature = "test-support")))]
        let router = {
            use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

            let governor_conf = GovernorConfigBuilder::default()
                .per_millisecond(200)
                .burst_size(240)
                .key_extractor(PosterKeyExtractor)
                .finish()
                .expect("valid governor config");

            router.layer(GovernorLayer::new(governor_conf))
        };

        router
    };

    let mut app = Router::new().merge(poster_route).merge(compressed_routes);

    // Serve static frontend files when STATIC_DIR is set.
    // Falls back to index.html for SPA client-side routing.
    if let Some(ref dir) = state.config.static_dir {
        use tower_http::services::{ServeDir, ServeFile};
        let index = format!("{dir}/index.html");
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    let cors_layer = build_cors_layer(&state.config);

    app = app.layer(TraceLayer::new_for_http().make_span_with(RedactedMakeSpan));

    if state.secure_cookies {
        app = app.layer(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        ));
    }

    app.layer(cors_layer).with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_path_api_route_unchanged() {
        assert_eq!(redact_path("/api/auth/status"), "/api/auth/status");
        assert_eq!(redact_path("/api/keys"), "/api/keys");
    }

    #[test]
    fn redact_path_poster_route_hides_key() {
        assert_eq!(
            redact_path("/abc123def456/imdb/poster-default/tt1234567.jpg"),
            "/[REDACTED]/imdb/poster-default/tt1234567.jpg"
        );
    }

    #[test]
    fn redact_path_single_segment() {
        assert_eq!(redact_path("/abc123def456"), "/[REDACTED]");
    }

    #[test]
    fn redact_path_root() {
        // "/" — path[1..] is empty, find('/') returns None
        assert_eq!(redact_path("/"), "/[REDACTED]");
    }
}
