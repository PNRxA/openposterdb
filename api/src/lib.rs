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
use services::fanart::{FanartClient, FanartImages};
use services::mdblist::MdblistClient;
use services::omdb::OmdbClient;
use services::tmdb::TmdbClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub tmdb: TmdbClient,
    pub omdb: Option<OmdbClient>,
    pub mdblist: Option<MdblistClient>,

    pub font: FontArc,
    pub refresh_locks: moka::sync::Cache<String, ()>,
    pub db: DatabaseConnection,
    pub jwt_secret: Zeroizing<Vec<u8>>,
    pub secure_cookies: bool,
    pub api_key_cache: moka::future::Cache<String, Option<i32>>,
    pub poster_inflight: moka::future::Cache<String, bytes::Bytes>,
    pub id_cache: moka::future::Cache<String, ResolvedId>,
    pub ratings_cache: moka::future::Cache<String, services::ratings::RatingsResult>,
    pub poster_mem_cache: moka::future::Cache<String, MemCacheEntry>,
    pub pending_last_used: Arc<DashMap<i32, ()>>,
    pub fanart: Option<FanartClient>,
    pub fanart_cache: moka::future::Cache<String, Arc<FanartImages>>,
    /// Tracks negative fanart results — e.g. "movie:123:textless" means no textless poster exists.
    /// Entries expire after the same TTL as fanart_cache so we recheck periodically.
    pub fanart_negative: moka::future::Cache<String, ()>,
    pub settings_cache: moka::future::Cache<i32, Arc<PosterSettings>>,
    pub global_settings_cache: moka::future::Cache<(), Arc<PosterSettings>>,
    pub preview_cache: moka::future::Cache<String, bytes::Bytes>,
    pub free_api_key_cache: moka::future::Cache<(), bool>,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,
    pub cross_id_semaphore: Arc<tokio::sync::Semaphore>,
    /// Maps settings hash → PosterSettings for content-addressed `/c/` CDN routes.
    /// Populated lazily when API key requests produce redirects.
    pub settings_hash_registry: moka::future::Cache<String, Arc<PosterSettings>>,
}

impl AppState {
    pub async fn is_free_api_key_enabled(&self) -> bool {
        if let Some(val) = self.config.free_key_enabled {
            return val;
        }
        let db_ref = self.db.clone();
        self.free_api_key_cache
            .try_get_with((), async move {
                let globals = services::db::get_global_settings(&db_ref).await?;
                let val = globals.get("free_api_key_enabled").map(|v| v.as_str());
                Ok::<_, error::AppError>(val == Some("true"))
            })
            .await
            .unwrap_or(false)
    }
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
        api_key_id             INTEGER PRIMARY KEY REFERENCES api_keys(id) ON DELETE CASCADE,
        poster_source          TEXT NOT NULL DEFAULT 't',
        fanart_lang            TEXT NOT NULL DEFAULT 'en',
        fanart_textless        INTEGER NOT NULL DEFAULT 0,
        ratings_limit          INTEGER NOT NULL DEFAULT 3,
        ratings_order          TEXT NOT NULL DEFAULT 'mal,imdb,lb,rt,mc,rta,tmdb,trakt',
        poster_position        TEXT NOT NULL DEFAULT 'bc',
        logo_ratings_limit     INTEGER NOT NULL DEFAULT 5,
        backdrop_ratings_limit INTEGER NOT NULL DEFAULT 5,
        poster_badge_style     TEXT NOT NULL DEFAULT 'h',
        logo_badge_style       TEXT NOT NULL DEFAULT 'v',
        backdrop_badge_style   TEXT NOT NULL DEFAULT 'v',
        poster_label_style     TEXT NOT NULL DEFAULT 'i',
        logo_label_style       TEXT NOT NULL DEFAULT 'i',
        backdrop_label_style   TEXT NOT NULL DEFAULT 'i',
        poster_badge_direction TEXT NOT NULL DEFAULT 'd'
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
    (
        "ALTER TABLE poster_meta ADD COLUMN image_type TEXT NOT NULL DEFAULT 'poster'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_position TEXT NOT NULL DEFAULT 'bc'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_ratings_limit INTEGER NOT NULL DEFAULT 5",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_ratings_limit INTEGER NOT NULL DEFAULT 5",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_style TEXT NOT NULL DEFAULT 'h'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_badge_style TEXT NOT NULL DEFAULT 'v'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_badge_style TEXT NOT NULL DEFAULT 'v'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_label_style TEXT NOT NULL DEFAULT 'i'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_label_style TEXT NOT NULL DEFAULT 'i'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_label_style TEXT NOT NULL DEFAULT 'i'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_direction TEXT NOT NULL DEFAULT 'd'",
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

/// Returns true if `path` looks like an API management route (`/api/...`) or
/// a poster route (first segment is a 64-char hex API key). Used by the
/// fallback service to return JSON 404 instead of the SPA HTML page.
fn is_api_or_poster_path(path: &str) -> bool {
    if path.starts_with("/api/") || path == "/api" {
        return true;
    }
    // Content-addressed CDN routes
    if path.starts_with("/c/") {
        return true;
    }
    // Check if first path segment looks like an API key (64 lowercase hex chars).
    let without_slash = &path[1..]; // skip leading '/'
    let first_segment = match without_slash.find('/') {
        Some(pos) => &without_slash[..pos],
        None => without_slash,
    };
    first_segment.len() == 64 && first_segment.bytes().all(|b| b.is_ascii_hexdigit())
}

fn redact_path(path: &str) -> String {
    if !path.starts_with("/api/") && !path.starts_with("/c/") {
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

/// Extract client IP from `X-Forwarded-For` or `X-Real-IP` headers.
#[cfg(not(any(test, feature = "test-support")))]
fn extract_client_ip<T>(req: &Request<T>) -> String {
    req.headers()
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
        .unwrap_or_else(|| "unknown".to_string())
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
        let ip = extract_client_ip(req);
        Ok(format!("{key_prefix}:{ip}"))
    }
}

/// Rate-limit key extractor for unauthenticated `/c/` CDN routes — uses IP only.
#[cfg(not(any(test, feature = "test-support")))]
#[derive(Debug, Clone)]
struct IpKeyExtractor;

#[cfg(not(any(test, feature = "test-support")))]
impl tower_governor::key_extractor::KeyExtractor for IpKeyExtractor {
    type Key = String;

    fn extract<T>(
        &self,
        req: &Request<T>,
    ) -> Result<Self::Key, tower_governor::GovernorError> {
        Ok(extract_client_ip(req))
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
        let router = Router::new()
            .route(
                "/{api_key}/{id_type}/poster-default/{id_value}",
                axum::routing::get(routes::poster::handler),
            )
            .route(
                "/{api_key}/{id_type}/logo-default/{id_value}",
                axum::routing::get(routes::poster::logo_handler),
            )
            .route(
                "/{api_key}/{id_type}/backdrop-default/{id_value}",
                axum::routing::get(routes::poster::backdrop_handler),
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

    // isValid is lightweight (no image generation) so it has its own, more
    // generous rate limiter to avoid eating into poster request budget.
    let is_valid_route = {
        let router = Router::new().route(
            "/{api_key}/isValid",
            axum::routing::get(routes::poster::is_valid_handler),
        );

        #[cfg(not(any(test, feature = "test-support")))]
        let router = {
            use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

            let governor_conf = GovernorConfigBuilder::default()
                .per_millisecond(100) // 10 req/s
                .burst_size(30)
                .key_extractor(PosterKeyExtractor)
                .finish()
                .expect("valid governor config");

            router.layer(GovernorLayer::new(governor_conf))
        };

        router
    };

    let mut app = Router::new()
        .merge(poster_route)
        .merge(is_valid_route);

    // Content-addressed CDN routes — only registered when ENABLE_CDN_REDIRECTS is set,
    // since nothing populates the settings_hash_registry otherwise.
    if state.config.enable_cdn_redirects {
        let cdn_route = {
            let router = Router::new()
                .route(
                    "/c/{settings_hash}/{id_type}/poster-default/{id_value}",
                    axum::routing::get(routes::poster::cdn_poster_handler),
                )
                .route(
                    "/c/{settings_hash}/{id_type}/logo-default/{id_value}",
                    axum::routing::get(routes::poster::cdn_logo_handler),
                )
                .route(
                    "/c/{settings_hash}/{id_type}/backdrop-default/{id_value}",
                    axum::routing::get(routes::poster::cdn_backdrop_handler),
                );

            #[cfg(not(any(test, feature = "test-support")))]
            let router = {
                use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

                let governor_conf = GovernorConfigBuilder::default()
                    .per_millisecond(200)
                    .burst_size(240)
                    .key_extractor(IpKeyExtractor)
                    .finish()
                    .expect("valid governor config");

                router.layer(GovernorLayer::new(governor_conf))
            };

            router
        };
        app = app.merge(cdn_route);
    }

    let mut app = app
        .merge(compressed_routes);

    // Serve static frontend files when STATIC_DIR is set.
    // Falls back to index.html for SPA client-side routing, but returns
    // a proper JSON 404 for unmatched /api/ paths and paths that look like
    // poster requests (64-char hex first segment) so API consumers get JSON
    // errors instead of HTML.
    if let Some(ref dir) = state.config.static_dir {
        use axum::response::IntoResponse;
        use tower::ServiceExt as _;
        use tower_http::services::{ServeDir, ServeFile};

        let index = format!("{dir}/index.html");
        let spa = ServeDir::new(dir).fallback(ServeFile::new(index));

        app = app.fallback_service(tower::service_fn(move |req: Request<axum::body::Body>| {
            let spa = spa.clone();
            async move {
                let path = req.uri().path();
                if is_api_or_poster_path(path) {
                    // Constant-time-ish: always return the same JSON 404
                    // regardless of whether the key exists, to avoid leaking
                    // valid key prefixes via timing.
                    Ok((
                        axum::http::StatusCode::NOT_FOUND,
                        axum::Json(serde_json::json!({"error": "not found"})),
                    )
                        .into_response())
                } else {
                    spa.oneshot(req).await.map(|r| r.into_response())
                }
            }
        }));
    }

    let cors_layer = build_cors_layer(&state.config);

    app = app.layer(TraceLayer::new_for_http().make_span_with(RedactedMakeSpan));

    if state.secure_cookies {
        app = app.layer(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        ));
    }

    // Security headers — always present regardless of HTTPS mode
    app = app
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ));

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

    #[test]
    fn is_api_path() {
        assert!(is_api_or_poster_path("/api/auth/login"));
        assert!(is_api_or_poster_path("/api/keys"));
        assert!(is_api_or_poster_path("/api"));
    }

    #[test]
    fn is_poster_path_valid_key() {
        let key = "a".repeat(64);
        assert!(is_api_or_poster_path(&format!("/{key}/imdb/poster-default/tt123.jpg")));
        assert!(is_api_or_poster_path(&format!("/{key}/bad-path")));
        // Key alone (no trailing path)
        assert!(is_api_or_poster_path(&format!("/{key}")));
    }

    #[test]
    fn is_poster_path_invalid_key() {
        // Too short
        assert!(!is_api_or_poster_path("/abcdef/imdb/poster-default/tt123.jpg"));
        // Not hex
        let key = "g".repeat(64);
        assert!(!is_api_or_poster_path(&format!("/{key}/imdb/poster-default/tt123.jpg")));
    }

    #[test]
    fn spa_paths_not_matched() {
        assert!(!is_api_or_poster_path("/"));
        assert!(!is_api_or_poster_path("/login"));
        assert!(!is_api_or_poster_path("/settings"));
        assert!(!is_api_or_poster_path("/posters"));
    }

    #[test]
    fn cdn_paths_matched() {
        assert!(is_api_or_poster_path("/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"));
        assert!(is_api_or_poster_path("/c/abc123/tmdb/logo-default/12345.png"));
    }

    #[test]
    fn redact_path_cdn_route_unchanged() {
        assert_eq!(
            redact_path("/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"),
            "/c/a1b2c3d4e5f6/imdb/poster-default/tt123.jpg"
        );
    }
}
