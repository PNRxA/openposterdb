use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::handlers;
use crate::AppState;

pub fn api_key_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/keys", get(handlers::api_keys::list))
        .route("/api/keys", post(handlers::api_keys::create))
        .route("/api/keys/{id}", delete(handlers::api_keys::delete))
        .route(
            "/api/keys/{id}/settings",
            get(handlers::api_keys::get_settings)
                .put(handlers::api_keys::update_settings)
                .delete(handlers::api_keys::delete_settings),
        )
}

pub fn api_key_self_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/key/me", get(handlers::api_keys::get_own_key_info))
        .route(
            "/api/key/me/settings",
            get(handlers::api_keys::get_own_settings)
                .put(handlers::api_keys::update_own_settings)
                .delete(handlers::api_keys::reset_own_settings),
        )
}
