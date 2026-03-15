use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;

use crate::cache;
use crate::error::AppError;
use crate::handlers::auth::hash_api_key;
use crate::image::generate;
use crate::image::serve;
use crate::services::db;
use crate::services::db::RenderSettings;
use crate::AppState;

pub const FREE_API_KEY: &str = "t0-free-rpdb";

/// OpenAPI-only enum for the `id_type` path parameter.
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "lowercase")]
#[allow(dead_code)]
enum IdTypeParam {
    Imdb,
    Tmdb,
    Tvdb,
}

/// OpenAPI-only enum for the `fallback` query parameter.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
enum FallbackParam {
    #[schema(rename = "true")]
    True,
}

/// OpenAPI-only enum for the `imageSize` query parameter.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
enum ImageSizeParam {
    #[schema(rename = "small")]
    Small,
    #[schema(rename = "medium")]
    Medium,
    #[schema(rename = "large")]
    Large,
    #[schema(rename = "very-large")]
    VeryLarge,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ImageQuery {
    /// When set to `true`, returns a placeholder image instead of 404 when the media item is not found.
    #[serde(default)]
    #[param(value_type = Option<FallbackParam>, example = "true")]
    pub fallback: Option<String>,
    /// Language code for Fanart.tv image selection (e.g. `en`, `de`, `pt-BR`). 2-5 alphanumeric characters or hyphens.
    #[serde(default)]
    #[param(value_type = Option<String>, pattern = r"^[a-zA-Z0-9\-]{2,5}$")]
    pub lang: Option<String>,
    /// Output image size. `small` is only valid for backdrops. Defaults to `medium`.
    #[serde(default, rename = "imageSize")]
    #[param(rename = "imageSize", default = "medium", value_type = Option<ImageSizeParam>)]
    pub image_size: Option<String>,
}

/// Resolve settings for a free API key (global defaults, no per-key DB lookup).
async fn resolve_free_settings(
    state: &Arc<AppState>,
) -> Result<Arc<db::RenderSettings>, Response> {
    if !state.is_free_api_key_enabled().await {
        return Err(AppError::Unauthorized.into_response());
    }
    let db_ref = state.db.clone();
    Ok(state
        .global_settings_cache
        .try_get_with((), async move {
            let g = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&g)))
        })
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load global settings, using defaults");
            Arc::new(db::RenderSettings::default())
        }))
}

/// Validate an API key and return settings. Handles both free and per-key paths.
async fn resolve_settings(
    state: &Arc<AppState>,
    api_key: &str,
) -> Result<Arc<db::RenderSettings>, Response> {
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
                    Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&g)))
                })
                .await
                .ok();
            let globals_ref = globals.as_deref();
            let s = db::get_effective_render_settings(&db_ref, key_id, globals_ref).await;
            Ok::<_, AppError>(Arc::new(s))
        })
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to load render settings, using defaults");
            Arc::new(db::RenderSettings::default())
        });

    Ok(settings)
}

#[utoipa::path(
    get,
    path = "/{api_key}/isValid",
    operation_id = "isValid",
    tag = "Auth",
    summary = "Validate API key",
    description = "Returns 200 OK if the provided API key is valid. Useful for verifying that an API key is correctly configured.",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
    ),
    responses(
        (status = 200, description = "API key is valid"),
        (status = 401, description = "Invalid or missing API key."),
    ),
)]
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
    settings: Arc<db::RenderSettings>,
    lang: &Option<String>,
) -> Result<Arc<db::RenderSettings>, Response> {
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

/// URL path segment used in CDN redirect URLs for each image type.
fn cdn_route_segment(kind: cache::ImageType) -> &'static str {
    match kind {
        cache::ImageType::Poster => "poster-default",
        cache::ImageType::Logo => "logo-default",
        cache::ImageType::Backdrop => "backdrop-default",
    }
}

/// If CDN redirects are enabled, compute a settings hash, register it, and return
/// a 302 redirect to the content-addressed `/c/` URL. Returns `None` if disabled.
async fn try_cdn_redirect(
    state: &Arc<AppState>,
    settings: &Arc<RenderSettings>,
    kind: cache::ImageType,
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

fn placeholder_bytes(content_type: &str) -> Bytes {
    if content_type == "image/png" {
        generate::placeholder_png().into()
    } else {
        generate::placeholder_jpeg().into()
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

/// Dispatch image generation to the correct backend (TMDB for posters, fanart.tv for logos/backdrops).
async fn dispatch_image(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &db::RenderSettings,
    kind: cache::ImageType,
    image_size: Option<db::ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    match kind {
        cache::ImageType::Poster => {
            serve::handle_inner(state, id_type_str, id_value_raw, settings.clone(), image_size).await
        }
        cache::ImageType::Logo => {
            serve::handle_fanart_image_inner(state, id_type_str, id_value_raw, settings, serve::FanartImageKind::Logo, image_size).await
        }
        cache::ImageType::Backdrop => {
            serve::handle_fanart_image_inner(state, id_type_str, id_value_raw, settings, serve::FanartImageKind::Backdrop, image_size).await
        }
    }
}

async fn serve_image(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &db::RenderSettings,
    use_fallback: bool,
    kind: cache::ImageType,
    image_size: Option<db::ImageSize>,
) -> Response {
    let content_type = kind.content_type();
    match dispatch_image(state, id_type_str, id_value_raw, settings, kind, image_size).await {
        Ok((bytes, _)) => serve::image_response(bytes, content_type),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder");
                serve::image_response(placeholder_bytes(content_type), content_type)
            } else {
                e.into_response()
            }
        }
    }
}

async fn image_handler_inner(
    state: Arc<AppState>,
    api_key: &str,
    id_type_str: &str,
    id_value_raw: &str,
    query: ImageQuery,
    kind: cache::ImageType,
) -> Response {
    let image_size = match parse_image_size(&query.image_size, kind) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    if kind.requires_fanart() {
        if let Err(resp) = require_fanart(&state) {
            return resp;
        }
    }

    let settings = match resolve_settings(&state, api_key).await {
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
        kind,
        id_type_str,
        cdn_route_segment(kind),
        id_value_raw,
        query.fallback.as_deref(),
        image_size,
    )
    .await
    {
        return redirect;
    }

    let use_fallback = query.fallback.as_deref() == Some("true");
    serve_image(&state, id_type_str, id_value_raw, &settings, use_fallback, kind, image_size).await
}

#[utoipa::path(
    get,
    path = "/{api_key}/{id_type}/poster-default/{id_value}",
    operation_id = "getPoster",
    tag = "Images",
    summary = "Get poster",
    description = "Returns a JPEG poster image with rating badge overlays for the given media item.",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-` or `series-` (e.g. `movie-550`, `series-1399`). For TVDB use the numeric ID."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Poster image", content_type = "image/jpeg", body = Vec<u8>,
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found. Use `?fallback=true` to get a placeholder image instead."),
    ),
)]
pub async fn handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    image_handler_inner(state, &api_key, &id_type_str, &id_value, query, cache::ImageType::Poster).await
}

#[utoipa::path(
    get,
    path = "/{api_key}/{id_type}/logo-default/{id_value}",
    operation_id = "getLogo",
    tag = "Images",
    summary = "Get logo",
    description = "Returns a PNG logo image with rating badge overlays for the given media item. Requires the Fanart.tv integration to be configured on the server.",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-` or `series-` (e.g. `movie-550`, `series-1399`). For TVDB use the numeric ID."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Logo image", content_type = "image/png", body = Vec<u8>,
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found. Use `?fallback=true` to get a placeholder image instead."),
        (status = 501, description = "Fanart.tv integration is not configured on this server. Logos and backdrops require a Fanart.tv API key."),
    ),
)]
pub async fn logo_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    image_handler_inner(state, &api_key, &id_type_str, &id_value, query, cache::ImageType::Logo).await
}

#[utoipa::path(
    get,
    path = "/{api_key}/{id_type}/backdrop-default/{id_value}",
    operation_id = "getBackdrop",
    tag = "Images",
    summary = "Get backdrop",
    description = "Returns a JPEG backdrop image with rating badge overlays for the given media item. Requires the Fanart.tv integration to be configured on the server.",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-` or `series-` (e.g. `movie-550`, `series-1399`). For TVDB use the numeric ID."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Backdrop image", content_type = "image/jpeg", body = Vec<u8>,
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found. Use `?fallback=true` to get a placeholder image instead."),
        (status = 501, description = "Fanart.tv integration is not configured on this server. Logos and backdrops require a Fanart.tv API key."),
    ),
)]
pub async fn backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    image_handler_inner(state, &api_key, &id_type_str, &id_value, query, cache::ImageType::Backdrop).await
}

// --- Content-addressed CDN handlers (`/c/{settings_hash}/...`) ---

/// Cache errors on `/c/` routes for 1 hour so Cloudflare doesn't cache them indefinitely
/// but also doesn't hammer the origin for titles that don't exist yet.
const CDN_ERROR_CACHE_CONTROL: &str = "public, max-age=3600";

fn cdn_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, CDN_ERROR_CACHE_CONTROL)],
        axum::Json(serde_json::json!({"error": "not found"})),
    )
        .into_response()
}

fn cdn_error_response(e: AppError) -> Response {
    let mut resp = e.into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(CDN_ERROR_CACHE_CONTROL),
    );
    resp
}

async fn cdn_handler_inner(
    state: Arc<AppState>,
    settings_hash: &str,
    id_type_str: &str,
    id_value_raw: &str,
    query: ImageQuery,
    kind: cache::ImageType,
) -> Response {
    let settings = match state.settings_hash_registry.get(settings_hash).await {
        Some(s) => s,
        None => return cdn_not_found(),
    };

    if kind.requires_fanart() {
        if let Err(resp) = require_fanart(&state) {
            return resp;
        }
    }

    let use_fallback = query.fallback.as_deref() == Some("true");

    let image_size = match parse_image_size(&query.image_size, kind) {
        Ok(s) => s,
        Err(resp) => return resp,
    };

    let content_type = kind.content_type();

    match dispatch_image(&state, id_type_str, id_value_raw, &settings, kind, image_size).await {
        Ok((bytes, release_date)) => {
            let max_age = serve::compute_cdn_max_age(release_date.as_deref(), state.config.ratings_min_stale_secs, state.config.ratings_max_age_secs);
            serve::cdn_image_response(bytes, max_age, content_type)
        }
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder (cdn)");
                serve::cdn_image_response(placeholder_bytes(content_type), serve::PLACEHOLDER_CDN_MAX_AGE, content_type)
            } else {
                cdn_error_response(e)
            }
        }
    }
}

pub async fn cdn_poster_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    cdn_handler_inner(state, &settings_hash, &id_type_str, &id_value, query, cache::ImageType::Poster).await
}

pub async fn cdn_logo_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    cdn_handler_inner(state, &settings_hash, &id_type_str, &id_value, query, cache::ImageType::Logo).await
}

pub async fn cdn_backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    cdn_handler_inner(state, &settings_hash, &id_type_str, &id_value, query, cache::ImageType::Backdrop).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_not_found_has_cache_control() {
        let resp = cdn_not_found();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            CDN_ERROR_CACHE_CONTROL,
        );
    }

    #[test]
    fn cdn_error_response_has_cache_control() {
        let resp = cdn_error_response(AppError::IdNotFound("tt0000000".into()));
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            CDN_ERROR_CACHE_CONTROL,
        );
    }

    #[test]
    fn cdn_error_response_preserves_status_code() {
        let resp = cdn_error_response(AppError::BadRequest("bad".into()));
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            CDN_ERROR_CACHE_CONTROL,
        );
    }
}
