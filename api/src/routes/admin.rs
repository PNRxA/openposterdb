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
        .route("/api/admin/preview/poster", get(handlers::preview::preview_poster))
        .route(
            "/api/admin/settings",
            get(handlers::admin::get_settings).put(handlers::admin::update_settings),
        )
}
