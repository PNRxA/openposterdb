pub mod cache;
pub mod config;
pub mod entity;
pub mod error;
pub mod handlers;
pub mod id;
pub mod image;
pub mod routes;
pub mod services;
pub mod upgrade;

use std::sync::Arc;

use ab_glyph::FontArc;
use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use zeroize::Zeroizing;

use cache::MemCacheEntry;
use config::Config;
use id::ResolvedId;
use services::db::RenderSettings;
use services::fanart::{FanartClient, FanartImages};
use services::mdblist::MdblistClient;
use services::omdb::OmdbClient;
use services::tmdb::TmdbClient;
use services::trakt::TraktClient;

pub use routes::build_app;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub tmdb: TmdbClient,
    pub omdb: Option<OmdbClient>,
    pub mdblist: Option<MdblistClient>,
    pub trakt: Option<TraktClient>,

    pub font: FontArc,
    pub refresh_locks: moka::sync::Cache<String, ()>,
    pub db: DatabaseConnection,
    pub jwt_secret: Zeroizing<Vec<u8>>,
    pub secure_cookies: bool,
    pub api_key_cache: moka::future::Cache<String, Option<i32>>,
    pub image_inflight: moka::future::Cache<String, bytes::Bytes>,
    pub id_cache: moka::future::Cache<String, ResolvedId>,
    pub ratings_cache: moka::future::Cache<String, services::ratings::RatingsResult>,
    pub image_mem_cache: moka::future::Cache<String, MemCacheEntry>,
    pub pending_last_used: Arc<DashMap<i32, ()>>,
    pub fanart: Option<FanartClient>,
    pub fanart_cache: moka::future::Cache<String, Arc<FanartImages>>,
    /// Tracks negative fanart results — e.g. "movie:123:textless" means no textless poster exists.
    /// Entries expire after the same TTL as fanart_cache so we recheck periodically.
    pub fanart_negative: moka::future::Cache<String, ()>,
    pub tmdb_images_cache: moka::future::Cache<String, Arc<services::tmdb::TmdbImagesResponse>>,
    pub settings_cache: moka::future::Cache<i32, Arc<RenderSettings>>,
    pub global_settings_cache: moka::future::Cache<(), Arc<RenderSettings>>,
    pub preview_cache: moka::future::Cache<String, bytes::Bytes>,
    pub free_api_key_cache: moka::future::Cache<(), bool>,
    pub render_semaphore: Arc<tokio::sync::Semaphore>,
    pub cross_id_semaphore: Arc<tokio::sync::Semaphore>,
    /// Maps settings hash → RenderSettings for content-addressed `/c/` CDN routes.
    /// Populated lazily when API key requests produce redirects.
    pub settings_hash_registry: moka::future::Cache<String, Arc<RenderSettings>>,
    /// In-memory cache for `available_ratings` SQLite lookups.
    /// Avoids hitting the database on every image request when the entry is already known.
    pub available_ratings_cache: moka::future::Cache<String, Option<String>>,
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

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "OpenPosterDB API",
        description = "API for generating and serving posters, logos, and backdrops with rating overlays for movies, TV shows, and collections.",
        license(name = "MIT"),
    ),
    tags(
        (name = "Images", description = "Poster, logo, backdrop, and episode image endpoints"),
        (name = "Auth", description = "API key validation"),
    ),
    servers((url = "/", description = "This instance")),
    paths(
        handlers::image::handler,
        handlers::image::logo_handler,
        handlers::image::backdrop_handler,
        handlers::image::episode_handler,
        handlers::image::season_handler,
        handlers::image::is_valid_handler,
    ),
    components(schemas(
        handlers::image::IdTypeParam,
        handlers::image::ImageSizeParam,
    )),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn openapi_spec_has_expected_paths() {
        let spec = ApiDoc::openapi();
        let paths: Vec<&str> = spec.paths.paths.keys().map(|k: &String| k.as_str()).collect();
        assert!(paths.contains(&"/{api_key}/{id_type}/poster-default/{id_value}"));
        assert!(paths.contains(&"/{api_key}/{id_type}/logo-default/{id_value}"));
        assert!(paths.contains(&"/{api_key}/{id_type}/backdrop-default/{id_value}"));
        assert!(paths.contains(&"/{api_key}/{id_type}/episode-default/{id_value}"));
        assert!(paths.contains(&"/{api_key}/{id_type}/season-default/{id_value}"));
        assert!(paths.contains(&"/{api_key}/isValid"));
        assert_eq!(paths.len(), 6);
    }

    #[test]
    fn openapi_spec_serializes_to_valid_json() {
        let spec = ApiDoc::openapi();
        let json = spec.to_json().expect("spec should serialize to JSON");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should be valid JSON");
        assert_eq!(parsed["openapi"], "3.1.0");
        assert_eq!(parsed["info"]["title"], "OpenPosterDB API");
    }
}

pub const SCHEMA_SQL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS image_meta (
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
    "CREATE TABLE IF NOT EXISTS available_ratings (
        id_key       TEXT PRIMARY KEY,
        sources      TEXT NOT NULL,
        updated_at   INTEGER NOT NULL,
        release_date TEXT
    )",
    "CREATE TABLE IF NOT EXISTS api_key_settings (
        api_key_id             INTEGER PRIMARY KEY REFERENCES api_keys(id) ON DELETE CASCADE,
        image_source           TEXT NOT NULL DEFAULT 't',
        lang                   TEXT NOT NULL DEFAULT 'en',
        textless               INTEGER NOT NULL DEFAULT 0,
        ratings_limit          INTEGER NOT NULL DEFAULT 3,
        ratings_order          TEXT NOT NULL DEFAULT 'mal,imdb,lb,rt,mc,rta,tmdb,trakt,mdblist,ebert',
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
        // Frozen historical default — must not change. This only backfills DBs that
        // predate the column (whose per-key rows already hold a user-set value, so
        // the column default is never the effective render value). The authoritative
        // default for new rows is `default_ratings_order()` / `SCHEMA_SQL` above.
        "ALTER TABLE api_key_settings ADD COLUMN ratings_order TEXT NOT NULL DEFAULT 'mal,imdb,lb,rt,rta,mc,tmdb,trakt'",
        "duplicate column",
    ),
    (
        "ALTER TABLE image_meta ADD COLUMN image_type TEXT NOT NULL DEFAULT 'poster'",
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
    (
        "ALTER TABLE available_ratings ADD COLUMN release_date TEXT",
        "duplicate column",
    ),
    (
        "CREATE INDEX IF NOT EXISTS idx_available_ratings_updated_at ON available_ratings(updated_at)",
        "already exists",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_size TEXT NOT NULL DEFAULT 'm'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_badge_size TEXT NOT NULL DEFAULT 'm'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_badge_size TEXT NOT NULL DEFAULT 'm'",
        "duplicate column",
    ),
    (
        "DROP INDEX IF EXISTS idx_image_meta_type",
        "no such index",
    ),
    (
        "CREATE INDEX IF NOT EXISTS idx_image_meta_type_created ON image_meta(image_type, created_at DESC)",
        "already exists",
    ),
    (
        "ALTER TABLE api_key_settings RENAME COLUMN poster_source TO image_source",
        "no such column",
    ),
    (
        "ALTER TABLE api_key_settings RENAME COLUMN fanart_lang TO lang",
        "no such column",
    ),
    (
        "ALTER TABLE api_key_settings RENAME COLUMN fanart_textless TO textless",
        "no such column",
    ),
    (
        "UPDATE global_settings SET key = 'image_source' WHERE key = 'poster_source'",
        "no such table",
    ),
    (
        "UPDATE global_settings SET key = 'lang' WHERE key = 'fanart_lang'",
        "no such table",
    ),
    (
        "UPDATE global_settings SET key = 'textless' WHERE key = 'fanart_textless'",
        "no such table",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_ratings_limit INTEGER NOT NULL DEFAULT 1",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_badge_style TEXT NOT NULL DEFAULT 'v'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_label_style TEXT NOT NULL DEFAULT 'o'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_badge_size TEXT NOT NULL DEFAULT 'l'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_position TEXT NOT NULL DEFAULT 'tr'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_badge_direction TEXT NOT NULL DEFAULT 'v'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_blur INTEGER NOT NULL DEFAULT 0",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_position TEXT NOT NULL DEFAULT 'tr'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_badge_direction TEXT NOT NULL DEFAULT 'v'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN ratings_exclude TEXT NOT NULL DEFAULT ''",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_shape TEXT NOT NULL DEFAULT 'r'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_badge_shape TEXT NOT NULL DEFAULT 'r'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_badge_shape TEXT NOT NULL DEFAULT 'r'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_badge_shape TEXT NOT NULL DEFAULT 'r'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_background TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN logo_badge_background TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_badge_background TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN episode_badge_background TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_badge_split INTEGER NOT NULL DEFAULT 0",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN poster_fit TEXT NOT NULL DEFAULT 'native'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_edge_inset_x INTEGER NOT NULL DEFAULT 0",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN backdrop_edge_inset_y INTEGER NOT NULL DEFAULT 0",
        "duplicate column",
    ),
    // Season poster badge settings. Defaults mirror the poster badge defaults
    // (seasons render through the 2:3 poster pipeline).
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_ratings_limit INTEGER NOT NULL DEFAULT 3",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_badge_style TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_label_style TEXT NOT NULL DEFAULT 'o'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_badge_size TEXT NOT NULL DEFAULT 'm'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_position TEXT NOT NULL DEFAULT 'bc'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_badge_direction TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_badge_shape TEXT NOT NULL DEFAULT 'r'",
        "duplicate column",
    ),
    (
        "ALTER TABLE api_key_settings ADD COLUMN season_badge_background TEXT NOT NULL DEFAULT 'd'",
        "duplicate column",
    ),
];
