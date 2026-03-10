use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;

use crate::error::AppError;
use crate::handlers::auth::hash_api_key;
use crate::poster::generate;
use crate::poster::serve;
use crate::services::db;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct PosterQuery {
    #[serde(default)]
    pub fallback: Option<String>,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    // Validate API key (cached, including negative results to prevent DB hammering)
    let key_hash = hash_api_key(&api_key);

    let db = state.db.clone();
    let hash_clone = key_hash.clone();
    let key_id = state
        .api_key_cache
        .try_get_with(key_hash, async move {
            match db::find_api_key_by_hash(&db, &hash_clone).await {
                Ok(opt) => Ok(opt.map(|m| m.id)),
                Err(e) => {
                    tracing::error!(error = %e, "DB error looking up API key");
                    // DB errors are not cached — only valid lookups are
                    Err(e)
                }
            }
        })
        .await;

    let key_id = match key_id {
        Ok(Some(id)) => id,
        Ok(None) => return AppError::Unauthorized.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "API key lookup failed");
            return AppError::Other("internal error".into()).into_response();
        }
    };

    state.pending_last_used.insert(key_id, ());

    // Load effective poster settings (cached, with global settings also cached)
    let db_ref = state.db.clone();
    let db_ref2 = state.db.clone();
    let global_cache = state.global_settings_cache.clone();
    let settings = state
        .settings_cache
        .try_get_with(key_id, async move {
            let globals = global_cache
                .try_get_with((), async move {
                    let g = db::get_global_settings(&db_ref2).await?;
                    Ok::<_, AppError>(Arc::new(db::parse_global_poster_settings(&g)))
                })
                .await
                .ok();
            let globals_ref = globals.as_deref();
            let s = db::get_effective_poster_settings(&db_ref, key_id, globals_ref).await;
            Ok::<_, AppError>(Arc::new(s))
        })
        .await
        .unwrap_or_else(|_| Arc::new(db::PosterSettings::default()));

    match serve::handle_inner(&state, &id_type_str, &id_value_jpg, &settings).await {
        Ok(bytes) => serve::jpeg_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder");
                serve::jpeg_response(generate::placeholder_jpeg().into())
            } else {
                e.into_response()
            }
        }
    }
}
