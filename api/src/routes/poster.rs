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

pub const FREE_API_KEY: &str = "t0-free-rpdb";

#[derive(Debug, Deserialize)]
pub struct PosterQuery {
    #[serde(default)]
    pub fallback: Option<String>,
}

/// Resolve settings for a free API key (global defaults, no per-key DB lookup).
async fn resolve_free_settings(
    state: &Arc<AppState>,
) -> Result<Arc<db::PosterSettings>, Response> {
    if !state.is_free_api_key_enabled().await {
        return Err(AppError::Unauthorized.into_response());
    }
    let db_ref = state.db.clone();
    Ok(state
        .global_settings_cache
        .try_get_with((), async move {
            let g = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_poster_settings(&g)))
        })
        .await
        .unwrap_or_else(|_| Arc::new(db::PosterSettings::default())))
}

/// Validate an API key and return settings. Handles both free and per-key paths.
async fn resolve_settings(
    state: &Arc<AppState>,
    api_key: &str,
) -> Result<Arc<db::PosterSettings>, Response> {
    if api_key == FREE_API_KEY {
        return resolve_free_settings(state).await;
    }

    let key_hash = hash_api_key(api_key);

    let db = state.db.clone();
    let hash_clone = key_hash.clone();
    let key_id = state
        .api_key_cache
        .try_get_with(key_hash, async move {
            match db::find_api_key_by_hash(&db, &hash_clone).await {
                Ok(opt) => Ok(opt.map(|m| m.id)),
                Err(e) => {
                    tracing::error!(error = %e, "DB error looking up API key");
                    Err(e)
                }
            }
        })
        .await;

    let key_id = match key_id {
        Ok(Some(id)) => id,
        Ok(None) => return Err(AppError::Unauthorized.into_response()),
        Err(e) => {
            tracing::error!(error = %e, "API key lookup failed");
            return Err(AppError::Other("internal error".into()).into_response());
        }
    };

    state.pending_last_used.insert(key_id, ());

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

    Ok(settings)
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    serve_poster(&state, &id_type_str, &id_value_jpg, &settings, use_fallback).await
}

async fn serve_poster(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_jpg: &str,
    settings: &db::PosterSettings,
    use_fallback: bool,
) -> Response {
    match serve::handle_inner(state, id_type_str, id_value_jpg, settings.clone()).await {
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

fn require_fanart(state: &AppState) -> Result<(), Response> {
    if state.fanart.is_none() {
        return Err((
            axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({"error": "FANART_API_KEY not configured"})),
        )
            .into_response());
    }
    Ok(())
}

pub async fn logo_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_png)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    serve_fanart_image(&state, &id_type_str, &id_value_png, &settings, use_fallback, serve::FanartImageKind::Logo).await
}

pub async fn backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    serve_fanart_image(&state, &id_type_str, &id_value_jpg, &settings, use_fallback, serve::FanartImageKind::Backdrop).await
}

async fn serve_fanart_image(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &db::PosterSettings,
    use_fallback: bool,
    kind: serve::FanartImageKind,
) -> Response {
    match serve::handle_fanart_image_inner(state, id_type_str, id_value_raw, settings, kind).await {
        Ok(bytes) => match kind {
            serve::FanartImageKind::Logo => serve::png_response(bytes),
            serve::FanartImageKind::Backdrop => serve::jpeg_response(bytes),
        },
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder");
                match kind {
                    serve::FanartImageKind::Logo => serve::png_response(generate::placeholder_png().into()),
                    serve::FanartImageKind::Backdrop => serve::jpeg_response(generate::placeholder_jpeg().into()),
                }
            } else {
                e.into_response()
            }
        }
    }
}
