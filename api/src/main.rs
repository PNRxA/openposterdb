mod cache;
mod config;
mod error;
mod id;
mod poster;
mod routes;
mod services;

use std::sync::Arc;

use ab_glyph::FontArc;
use axum::Router;
use dashmap::DashMap;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use config::Config;
use services::mdblist::MdblistClient;
use services::omdb::OmdbClient;
use services::tmdb::TmdbClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub tmdb: TmdbClient,
    pub omdb: OmdbClient,
    pub mdblist: Option<MdblistClient>,
    pub http: reqwest::Client,
    pub font: FontArc,
    pub refresh_locks: Arc<DashMap<String, ()>>,
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

    let mdblist = config
        .mdblist_api_key
        .as_ref()
        .map(|key| MdblistClient::new(key.clone(), http.clone()));

    let state = AppState {
        tmdb: TmdbClient::new(config.tmdb_api_key.clone(), http.clone()),
        omdb: OmdbClient::new(config.omdb_api_key.clone(), http.clone()),
        mdblist,
        http,
        font,
        refresh_locks: Arc::new(DashMap::new()),
        config: config.clone(),
    };

    let app = Router::new()
        .route(
            "/{api_key}/{id_type}/poster-default/{id_value}",
            axum::routing::get(routes::poster::handler),
        )
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("failed to bind");

    tracing::info!(addr = %config.listen_addr, "server listening");

    axum::serve(listener, app).await.expect("server error");
}
