use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::handlers;
use crate::AppState;

pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/stats", get(handlers::admin::stats))
        .route("/api/admin/posters", get(handlers::admin::list_posters))
        .route("/api/admin/posters/{id_type}/{id_value}/image", get(handlers::admin::poster_image))
        .route("/api/admin/posters/{id_type}/{id_value}/fetch", post(handlers::admin::fetch_poster))
        .route("/api/admin/logos", get(handlers::admin::list_logos))
        .route("/api/admin/logos/{id_type}/{id_value}", get(handlers::admin::logo_image))
        .route("/api/admin/logos/{id_type}/{id_value}/fetch", post(handlers::admin::fetch_logo))
        .route("/api/admin/backdrops", get(handlers::admin::list_backdrops))
        .route("/api/admin/backdrops/{id_type}/{id_value}", get(handlers::admin::backdrop_image))
        .route("/api/admin/backdrops/{id_type}/{id_value}/fetch", post(handlers::admin::fetch_backdrop))
        .route("/api/admin/episodes", get(handlers::admin::list_episodes))
        .route("/api/admin/episodes/{id_type}/{id_value}/image", get(handlers::admin::episode_image))
        .route("/api/admin/episodes/{id_type}/{id_value}/fetch", post(handlers::admin::fetch_episode))
        .route("/api/admin/seasons", get(handlers::admin::list_seasons))
        .route("/api/admin/seasons/{id_type}/{id_value}/image", get(handlers::admin::season_image))
        .route("/api/admin/seasons/{id_type}/{id_value}/fetch", post(handlers::admin::fetch_season))
        .route("/api/admin/preview/poster", get(handlers::preview::preview_poster))
        .route("/api/admin/preview/logo", get(handlers::preview::preview_logo))
        .route("/api/admin/preview/backdrop", get(handlers::preview::preview_backdrop))
        .route("/api/admin/preview/episode", get(handlers::preview::preview_episode))
        .route("/api/admin/preview/season", get(handlers::preview::preview_season))
        .route(
            "/api/admin/settings",
            get(handlers::admin::get_settings).put(handlers::admin::update_settings),
        )
}
