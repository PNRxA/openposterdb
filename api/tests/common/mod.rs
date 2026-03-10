use std::sync::Arc;
use std::time::Duration;

use ab_glyph::FontArc;
use dashmap::DashMap;
use sea_orm::{ConnectionTrait, DatabaseConnection, SqlxSqliteConnector};
use zeroize::Zeroizing;

use openposterdb_api::config::Config;
use openposterdb_api::services::fanart::FanartClient;
use openposterdb_api::services::tmdb::TmdbClient;
use openposterdb_api::{build_app, AppState, FONT_BYTES, SCHEMA_SQL};

pub struct TestAppOptions {
    pub cors_origin: Option<String>,
    pub secure_cookies: bool,
}

impl Default for TestAppOptions {
    fn default() -> Self {
        Self {
            cors_origin: None,
            secure_cookies: false,
        }
    }
}

pub async fn setup_test_app_with_options(opts: TestAppOptions) -> (axum::Router, Arc<AppState>) {
    let cors_origin = opts.cors_origin;
    let secure_cookies = opts.secure_cookies;
    _setup_test_app(cors_origin, secure_cookies).await
}

pub async fn setup_test_app_with_cors(cors_origin: Option<String>) -> (axum::Router, Arc<AppState>) {
    _setup_test_app(cors_origin, false).await
}

async fn _setup_test_app(cors_origin: Option<String>, secure_cookies: bool) -> (axum::Router, Arc<AppState>) {
    let sqlite_opts = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(":memory:")
        .create_if_missing(true)
        .pragma("foreign_keys", "ON");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlite_opts)
        .await
        .expect("failed to connect to test database");
    let db: DatabaseConnection = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);

    for sql in SCHEMA_SQL {
        db.execute_unprepared(sql).await.unwrap();
    }

    let jwt_secret = Zeroizing::new(vec![0xAB; 32]);
    let http = reqwest::Client::new();
    let font = FontArc::try_from_slice(FONT_BYTES).expect("failed to load font");

    let api_key_cache = moka::future::Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(300))
        .build();
    let poster_inflight = moka::future::Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(30))
        .build();
    let id_cache = moka::future::Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(3600))
        .build();
    let ratings_cache = moka::future::Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(1800))
        .build();
    let poster_mem_cache = moka::future::Cache::builder()
        .max_capacity(1024 * 1024)
        .time_to_live(Duration::from_secs(3600))
        .build();
    let refresh_locks = moka::sync::Cache::builder()
        .max_capacity(100)
        .time_to_live(Duration::from_secs(300))
        .build();

    let state = Arc::new(AppState {
        config: Config {
            tmdb_api_key: "test".into(),
            omdb_api_key: None,
            cache_dir: "/tmp/openposterdb-test".into(),
            listen_addr: "127.0.0.1:0".into(),
            ratings_min_stale_secs: 86400,
            ratings_max_age_secs: 31_536_000,
            poster_stale_secs: 0,
            poster_quality: 85,
            mdblist_api_key: None,
            poster_mem_cache_mb: 1,
            static_dir: None,
            cors_origin,
            fanart_api_key: Some("test".into()),
        },
        tmdb: TmdbClient::new("test".into(), http.clone()),
        omdb: None,
        mdblist: None,
        http: http.clone(),
        font,
        refresh_locks,
        db,
        jwt_secret,
        secure_cookies,
        api_key_cache,
        poster_inflight,
        id_cache,
        ratings_cache,
        poster_mem_cache,
        pending_last_used: Arc::new(DashMap::new()),
        fanart: Some(FanartClient::new("test".into(), http)),
        fanart_cache: moka::future::Cache::builder()
            .max_capacity(100)
            .time_to_live(Duration::from_secs(3600))
            .build(),
        fanart_negative: moka::future::Cache::builder()
            .max_capacity(100)
            .time_to_live(Duration::from_secs(3600))
            .build(),
        settings_cache: moka::future::Cache::builder()
            .max_capacity(100)
            .time_to_live(Duration::from_secs(300))
            .build(),
        global_settings_cache: moka::future::Cache::builder()
            .max_capacity(1)
            .time_to_live(Duration::from_secs(300))
            .build(),
    });

    let app = build_app(state.clone());
    (app, state)
}

pub async fn setup_test_app() -> (axum::Router, Arc<AppState>) {
    setup_test_app_with_cors(None).await
}

/// Helper to perform setup and get back an access token.
pub async fn setup_admin(app: &axum::Router) -> String {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/setup")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username": "admin", "password": "testpassword123"}).to_string(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["token"].as_str().unwrap().to_string()
}

/// Extract Set-Cookie header value from a response.
pub fn extract_set_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract the refresh_token value from a Set-Cookie header.
pub fn extract_refresh_token(set_cookie: &str) -> Option<String> {
    set_cookie
        .split(';')
        .next()
        .and_then(|s| s.strip_prefix("refresh_token="))
        .map(|s| s.to_string())
}
