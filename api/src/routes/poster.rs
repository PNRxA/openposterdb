use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;

use crate::cache;
use crate::error::AppError;
use crate::handlers::auth::hash_api_key;
use crate::poster::generate;
use crate::poster::serve;
use crate::services::db;
use crate::services::db::PosterSettings;
use crate::AppState;

pub const FREE_API_KEY: &str = "t0-free-rpdb";

#[derive(Debug, Deserialize)]
pub struct PosterQuery {
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default, rename = "imageSize")]
    pub image_size: Option<String>,
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

pub async fn is_valid_handler(
    State(state): State<Arc<AppState>>,
    Path(api_key): Path<String>,
) -> Response {
    match resolve_settings(&state, &api_key).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(resp) => resp,
    }
}

/// If `?lang=` was provided, validate it and override `fanart_lang` in settings.
fn apply_lang_override(
    settings: Arc<db::PosterSettings>,
    lang: &Option<String>,
) -> Result<Arc<db::PosterSettings>, Response> {
    if let Some(lang) = lang {
        db::validate_fanart_lang(lang).map_err(|e| e.into_response())?;
        let mut s = (*settings).clone();
        s.fanart_lang = Arc::from(lang.as_str());
        s.lang_override = true;
        Ok(Arc::new(s))
    } else {
        Ok(settings)
    }
}

/// If CDN redirects are enabled, compute a settings hash, register it, and return
/// a 302 redirect to the content-addressed `/c/` URL. Returns `None` if disabled.
async fn try_cdn_redirect(
    state: &Arc<AppState>,
    settings: &Arc<PosterSettings>,
    kind: serve::ImageKind,
    id_type_str: &str,
    image_type_path: &str,
    id_value: &str,
    fallback: Option<&str>,
    image_size: Option<db::ImageSize>,
) -> Option<Response> {
    if !state.config.enable_cdn_redirects {
        return None;
    }
    let hash = serve::settings_hash(settings, kind, image_size);
    state
        .settings_hash_registry
        .insert(hash.clone(), settings.clone())
        .await;
    let mut url = format!("/c/{hash}/{id_type_str}/{image_type_path}/{id_value}");
    let mut has_query = false;
    if fallback == Some("true") {
        url.push_str("?fallback=true");
        has_query = true;
    }
    if let Some(size) = image_size {
        url.push(if has_query { '&' } else { '?' });
        url.push_str("imageSize=");
        url.push_str(size.query_str());
    }
    Some(serve::cdn_redirect_response(&url))
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Poster) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let settings = match apply_lang_override(settings, &query.lang) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if let Some(redirect) = try_cdn_redirect(
        &state,
        &settings,
        serve::ImageKind::Poster,
        &id_type_str,
        "poster-default",
        &id_value_jpg,
        query.fallback.as_deref(),
        image_size,
    )
    .await
    {
        return redirect;
    }

    serve_poster(&state, &id_type_str, &id_value_jpg, &settings, use_fallback, image_size).await
}

/// Parse and validate the optional `imageSize` query parameter.
fn parse_image_size(
    raw: &Option<String>,
    kind: cache::ImageType,
) -> Result<Option<db::ImageSize>, Response> {
    match raw {
        Some(s) => db::validate_image_size(s, kind)
            .map(Some)
            .map_err(|e| e.into_response()),
        None => Ok(None),
    }
}

async fn serve_poster(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_jpg: &str,
    settings: &db::PosterSettings,
    use_fallback: bool,
    image_size: Option<db::ImageSize>,
) -> Response {
    match serve::handle_inner(state, id_type_str, id_value_jpg, settings.clone(), image_size).await {
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

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Logo) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let settings = match apply_lang_override(settings, &query.lang) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if let Some(redirect) = try_cdn_redirect(
        &state,
        &settings,
        serve::ImageKind::Logo,
        &id_type_str,
        "logo-default",
        &id_value_png,
        query.fallback.as_deref(),
        image_size,
    )
    .await
    {
        return redirect;
    }

    serve_fanart_image(&state, &id_type_str, &id_value_png, &settings, use_fallback, serve::FanartImageKind::Logo, image_size).await
}

pub async fn backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Backdrop) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let settings = match resolve_settings(&state, &api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let settings = match apply_lang_override(settings, &query.lang) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if let Some(redirect) = try_cdn_redirect(
        &state,
        &settings,
        serve::ImageKind::Backdrop,
        &id_type_str,
        "backdrop-default",
        &id_value_jpg,
        query.fallback.as_deref(),
        image_size,
    )
    .await
    {
        return redirect;
    }

    serve_fanart_image(&state, &id_type_str, &id_value_jpg, &settings, use_fallback, serve::FanartImageKind::Backdrop, image_size).await
}

async fn serve_fanart_image(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &db::PosterSettings,
    use_fallback: bool,
    kind: serve::FanartImageKind,
    image_size: Option<db::ImageSize>,
) -> Response {
    match serve::handle_fanart_image_inner(state, id_type_str, id_value_raw, settings, kind, image_size).await {
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

// --- Content-addressed CDN handlers (`/c/{settings_hash}/...`) ---

fn cdn_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({"error": "not found"})),
    )
        .into_response()
}

pub async fn cdn_poster_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let settings = match state.settings_hash_registry.get(&settings_hash).await {
        Some(s) => s,
        None => return cdn_not_found(),
    };

    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Poster) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    match serve::handle_inner(&state, &id_type_str, &id_value_jpg, (*settings).clone(), image_size).await {
        Ok(bytes) => serve::cdn_jpeg_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder (cdn)");
                serve::cdn_jpeg_response(generate::placeholder_jpeg().into())
            } else {
                e.into_response()
            }
        }
    }
}

pub async fn cdn_logo_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value_png)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let settings = match state.settings_hash_registry.get(&settings_hash).await {
        Some(s) => s,
        None => return cdn_not_found(),
    };

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Logo) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    match serve::handle_fanart_image_inner(&state, &id_type_str, &id_value_png, &settings, serve::FanartImageKind::Logo, image_size).await {
        Ok(bytes) => serve::cdn_png_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder (cdn)");
                serve::cdn_png_response(generate::placeholder_png().into())
            } else {
                e.into_response()
            }
        }
    }
}

pub async fn cdn_backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value_jpg)): Path<(String, String, String)>,
    Query(query): Query<PosterQuery>,
) -> Response {
    let settings = match state.settings_hash_registry.get(&settings_hash).await {
        Some(s) => s,
        None => return cdn_not_found(),
    };

    if let Err(resp) = require_fanart(&state) {
        return resp;
    }

    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, cache::ImageType::Backdrop) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    match serve::handle_fanart_image_inner(&state, &id_type_str, &id_value_jpg, &settings, serve::FanartImageKind::Backdrop, image_size).await {
        Ok(bytes) => serve::cdn_jpeg_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder (cdn)");
                serve::cdn_jpeg_response(generate::placeholder_jpeg().into())
            } else {
                e.into_response()
            }
        }
    }
}
