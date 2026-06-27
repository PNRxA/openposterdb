use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::cache;
use crate::error::AppError;
use crate::image::serve::{self, LogoBackdropKind};
use crate::services::db::{self, default_ratings_limit, default_logo_backdrop_ratings_limit, default_ratings_order, BadgeBackground, BadgeDirection, BadgeShape, BadgeSize, BadgeStyle, LabelStyle, BadgePosition, ImageSource, PosterFit};
use crate::AppState;

#[derive(Serialize)]
pub struct StatsResponse {
    pub total_images: u64,
    pub total_api_keys: u64,
    pub mem_cache_entries: u64,
    pub id_cache_entries: u64,
    pub ratings_cache_entries: u64,
    pub image_mem_cache_mb: u64,
}

pub async fn stats(State(state): State<Arc<AppState>>) -> Result<Json<StatsResponse>, AppError> {
    let total_images = db::count_image_meta(&state.db).await?;
    let total_api_keys = db::count_api_keys(&state.db).await?;

    let mem_cache_entries = state.image_mem_cache.entry_count();
    let id_cache_entries = state.id_cache.entry_count();
    let ratings_cache_entries = state.ratings_cache.entry_count();
    let image_mem_cache_mb = state.image_mem_cache.weighted_size() / (1024 * 1024);

    Ok(Json(StatsResponse {
        total_images,
        total_api_keys,
        mem_cache_entries,
        id_cache_entries,
        ratings_cache_entries,
        image_mem_cache_mb,
    }))
}

#[derive(Deserialize)]
pub struct ListImagesQuery {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_page_size")]
    pub page_size: u64,
}

fn default_page() -> u64 {
    1
}
fn default_page_size() -> u64 {
    50
}

#[derive(Serialize)]
pub struct ImageMetaItem {
    pub cache_key: String,
    pub release_date: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize)]
pub struct ListImagesResponse {
    pub items: Vec<ImageMetaItem>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

async fn list_images(
    state: &AppState,
    query: &ListImagesQuery,
    image_type: cache::ImageType,
) -> Result<Json<ListImagesResponse>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);

    let (items, total) = db::list_image_meta_by_kind(&state.db, image_type, page, page_size).await?;

    let items = items
        .into_iter()
        .map(|m| ImageMetaItem {
            cache_key: m.cache_key,
            release_date: m.release_date,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect();

    Ok(Json(ListImagesResponse { items, total, page, page_size }))
}

pub async fn list_posters(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListImagesQuery>,
) -> Result<Json<ListImagesResponse>, AppError> {
    list_images(&state, &query, cache::ImageType::Poster).await
}

pub async fn poster_image(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    image_from_cache_key(&state, &id_type, &id_value, cache::ImageType::Poster, "image/jpeg").await
}

#[derive(Serialize)]
pub struct GlobalSettingsResponse {
    pub image_source: ImageSource,
    pub lang: String,
    pub textless: bool,
    pub fanart_available: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub ratings_exclude: String,
    pub free_api_key_enabled: bool,
    pub free_api_key_locked: bool,
    pub poster_position: BadgePosition,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: BadgeStyle,
    pub logo_badge_style: BadgeStyle,
    pub backdrop_badge_style: BadgeStyle,
    pub poster_label_style: LabelStyle,
    pub logo_label_style: LabelStyle,
    pub backdrop_label_style: LabelStyle,
    pub poster_badge_direction: BadgeDirection,
    pub poster_badge_split: bool,
    pub poster_fit: PosterFit,
    pub poster_badge_size: BadgeSize,
    pub logo_badge_size: BadgeSize,
    pub backdrop_badge_size: BadgeSize,
    pub backdrop_position: BadgePosition,
    pub backdrop_badge_direction: BadgeDirection,
    pub backdrop_edge_inset_x: i32,
    pub backdrop_edge_inset_y: i32,
    pub episode_ratings_limit: i32,
    pub episode_badge_style: BadgeStyle,
    pub episode_label_style: LabelStyle,
    pub episode_badge_size: BadgeSize,
    pub episode_position: BadgePosition,
    pub episode_badge_direction: BadgeDirection,
    pub episode_blur: bool,
    pub poster_badge_shape: BadgeShape,
    pub logo_badge_shape: BadgeShape,
    pub backdrop_badge_shape: BadgeShape,
    pub episode_badge_shape: BadgeShape,
    pub poster_badge_background: BadgeBackground,
    pub logo_badge_background: BadgeBackground,
    pub backdrop_badge_background: BadgeBackground,
    pub episode_badge_background: BadgeBackground,
    pub csm_enabled: bool,
    pub csm_position: BadgePosition,
    pub csm_size: BadgeSize,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GlobalSettingsResponse>, AppError> {
    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let free_api_key_locked = state.config.free_key_enabled.is_some();
    let free_api_key_enabled = state.is_free_api_key_enabled().await;
    Ok(Json(GlobalSettingsResponse {
        image_source: settings.image_source,
        lang: settings.lang.to_string(),
        textless: settings.textless,
        fanart_available: state.fanart.is_some(),
        ratings_limit: settings.ratings_limit,
        ratings_order: settings.ratings_order.to_string(),
        ratings_exclude: settings.ratings_exclude.to_string(),
        free_api_key_enabled,
        free_api_key_locked,
        poster_position: settings.poster_position,
        logo_ratings_limit: settings.logo_ratings_limit,
        backdrop_ratings_limit: settings.backdrop_ratings_limit,
        poster_badge_style: settings.poster_badge_style,
        logo_badge_style: settings.logo_badge_style,
        backdrop_badge_style: settings.backdrop_badge_style,
        poster_label_style: settings.poster_label_style,
        logo_label_style: settings.logo_label_style,
        backdrop_label_style: settings.backdrop_label_style,
        poster_badge_direction: settings.poster_badge_direction,
        poster_badge_split: settings.poster_badge_split,
        poster_fit: settings.poster_fit,
        poster_badge_size: settings.poster_badge_size,
        logo_badge_size: settings.logo_badge_size,
        backdrop_badge_size: settings.backdrop_badge_size,
        backdrop_position: settings.backdrop_position,
        backdrop_badge_direction: settings.backdrop_badge_direction,
        backdrop_edge_inset_x: settings.backdrop_edge_inset_x,
        backdrop_edge_inset_y: settings.backdrop_edge_inset_y,
        episode_ratings_limit: settings.episode_ratings_limit,
        episode_badge_style: settings.episode_badge_style,
        episode_label_style: settings.episode_label_style,
        episode_badge_size: settings.episode_badge_size,
        episode_position: settings.episode_position,
        episode_badge_direction: settings.episode_badge_direction,
        episode_blur: settings.episode_blur,
        poster_badge_shape: settings.poster_badge_shape,
        logo_badge_shape: settings.logo_badge_shape,
        backdrop_badge_shape: settings.backdrop_badge_shape,
        episode_badge_shape: settings.episode_badge_shape,
        poster_badge_background: settings.poster_badge_background,
        logo_badge_background: settings.logo_badge_background,
        backdrop_badge_background: settings.backdrop_badge_background,
        episode_badge_background: settings.episode_badge_background,
        csm_enabled: settings.csm_enabled,
        csm_position: settings.csm_position,
        csm_size: settings.csm_size,
    }))
}

#[derive(Deserialize)]
pub struct UpdateGlobalSettingsRequest {
    #[serde(alias = "poster_source")]
    pub image_source: ImageSource,
    #[serde(default = "db::default_lang", alias = "fanart_lang")]
    pub lang: String,
    #[serde(default, alias = "fanart_textless")]
    pub textless: bool,
    #[serde(default = "default_ratings_limit")]
    pub ratings_limit: i32,
    #[serde(default = "default_ratings_order")]
    pub ratings_order: String,
    #[serde(default = "db::default_ratings_exclude")]
    pub ratings_exclude: String,
    pub free_api_key_enabled: Option<bool>,
    #[serde(default = "db::default_poster_position")]
    pub poster_position: BadgePosition,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub logo_ratings_limit: i32,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub backdrop_ratings_limit: i32,
    #[serde(default = "db::default_poster_badge_style")]
    pub poster_badge_style: BadgeStyle,
    #[serde(default = "db::default_logo_badge_style")]
    pub logo_badge_style: BadgeStyle,
    #[serde(default = "db::default_backdrop_badge_style")]
    pub backdrop_badge_style: BadgeStyle,
    #[serde(default = "db::default_label_style")]
    pub poster_label_style: LabelStyle,
    #[serde(default = "db::default_label_style")]
    pub logo_label_style: LabelStyle,
    #[serde(default = "db::default_label_style")]
    pub backdrop_label_style: LabelStyle,
    #[serde(default = "db::default_poster_badge_direction")]
    pub poster_badge_direction: BadgeDirection,
    #[serde(default)]
    pub poster_badge_split: bool,
    #[serde(default = "db::default_poster_fit")]
    pub poster_fit: PosterFit,
    #[serde(default = "db::default_badge_size")]
    pub poster_badge_size: BadgeSize,
    #[serde(default = "db::default_badge_size")]
    pub logo_badge_size: BadgeSize,
    #[serde(default = "db::default_badge_size")]
    pub backdrop_badge_size: BadgeSize,
    #[serde(default = "db::default_backdrop_position")]
    pub backdrop_position: BadgePosition,
    #[serde(default = "db::default_backdrop_badge_direction")]
    pub backdrop_badge_direction: BadgeDirection,
    #[serde(default = "db::default_backdrop_edge_inset")]
    pub backdrop_edge_inset_x: i32,
    #[serde(default = "db::default_backdrop_edge_inset")]
    pub backdrop_edge_inset_y: i32,
    #[serde(default = "db::default_episode_ratings_limit")]
    pub episode_ratings_limit: i32,
    #[serde(default = "db::default_episode_badge_style")]
    pub episode_badge_style: BadgeStyle,
    #[serde(default = "db::default_label_style")]
    pub episode_label_style: LabelStyle,
    #[serde(default = "db::default_episode_badge_size")]
    pub episode_badge_size: BadgeSize,
    #[serde(default = "db::default_episode_position")]
    pub episode_position: BadgePosition,
    #[serde(default = "db::default_episode_badge_direction")]
    pub episode_badge_direction: BadgeDirection,
    #[serde(default)]
    pub episode_blur: bool,
    #[serde(default = "db::default_badge_shape")]
    pub poster_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub logo_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub backdrop_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_shape")]
    pub episode_badge_shape: BadgeShape,
    #[serde(default = "db::default_badge_background")]
    pub poster_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub logo_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub backdrop_badge_background: BadgeBackground,
    #[serde(default = "db::default_badge_background")]
    pub episode_badge_background: BadgeBackground,
    #[serde(default)]
    pub csm_enabled: bool,
    #[serde(default = "db::default_csm_position")]
    pub csm_position: BadgePosition,
    #[serde(default = "db::default_csm_size")]
    pub csm_size: BadgeSize,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateGlobalSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    db::validate_render_settings(&req.lang, req.ratings_limit, &req.ratings_order, &req.ratings_exclude, req.logo_ratings_limit, req.backdrop_ratings_limit, req.episode_ratings_limit)?;
    let textless_str = if req.textless { "true" } else { "false" };
    let limit_str = req.ratings_limit.to_string();
    let logo_limit_str = req.logo_ratings_limit.to_string();
    let backdrop_limit_str = req.backdrop_ratings_limit.to_string();
    let episode_limit_str = req.episode_ratings_limit.to_string();
    let episode_blur_str = if req.episode_blur { "true" } else { "false" };
    let poster_badge_split_str = if req.poster_badge_split { "true" } else { "false" };
    let backdrop_edge_inset_x_str = db::clamp_edge_inset(req.backdrop_edge_inset_x).to_string();
    let backdrop_edge_inset_y_str = db::clamp_edge_inset(req.backdrop_edge_inset_y).to_string();
    let csm_enabled_str = if req.csm_enabled { "true" } else { "false" };
    let mut batch: Vec<(&str, &str)> = vec![
        ("image_source", req.image_source.as_str()),
        ("lang", &req.lang),
        ("textless", textless_str),
        ("ratings_limit", &limit_str),
        ("ratings_order", &req.ratings_order),
        ("ratings_exclude", &req.ratings_exclude),
        ("poster_position", req.poster_position.as_str()),
        ("logo_ratings_limit", &logo_limit_str),
        ("backdrop_ratings_limit", &backdrop_limit_str),
        ("poster_badge_style", req.poster_badge_style.as_str()),
        ("logo_badge_style", req.logo_badge_style.as_str()),
        ("backdrop_badge_style", req.backdrop_badge_style.as_str()),
        ("poster_label_style", req.poster_label_style.as_str()),
        ("logo_label_style", req.logo_label_style.as_str()),
        ("backdrop_label_style", req.backdrop_label_style.as_str()),
        ("poster_badge_direction", req.poster_badge_direction.as_str()),
        ("poster_badge_split", poster_badge_split_str),
        ("poster_fit", req.poster_fit.as_str()),
        ("poster_badge_size", req.poster_badge_size.as_str()),
        ("logo_badge_size", req.logo_badge_size.as_str()),
        ("backdrop_badge_size", req.backdrop_badge_size.as_str()),
        ("backdrop_position", req.backdrop_position.as_str()),
        ("backdrop_badge_direction", req.backdrop_badge_direction.as_str()),
        ("backdrop_edge_inset_x", &backdrop_edge_inset_x_str),
        ("backdrop_edge_inset_y", &backdrop_edge_inset_y_str),
        ("episode_ratings_limit", &episode_limit_str),
        ("episode_badge_style", req.episode_badge_style.as_str()),
        ("episode_label_style", req.episode_label_style.as_str()),
        ("episode_badge_size", req.episode_badge_size.as_str()),
        ("episode_position", req.episode_position.as_str()),
        ("episode_badge_direction", req.episode_badge_direction.as_str()),
        ("episode_blur", episode_blur_str),
        ("poster_badge_shape", req.poster_badge_shape.as_str()),
        ("logo_badge_shape", req.logo_badge_shape.as_str()),
        ("backdrop_badge_shape", req.backdrop_badge_shape.as_str()),
        ("episode_badge_shape", req.episode_badge_shape.as_str()),
        ("poster_badge_background", req.poster_badge_background.as_str()),
        ("logo_badge_background", req.logo_badge_background.as_str()),
        ("backdrop_badge_background", req.backdrop_badge_background.as_str()),
        ("episode_badge_background", req.episode_badge_background.as_str()),
        ("csm_enabled", csm_enabled_str),
        ("csm_position", req.csm_position.as_str()),
        ("csm_size", req.csm_size.as_str()),
    ];
    let free_key_str;
    if state.config.free_key_enabled.is_none() {
        if let Some(enabled) = req.free_api_key_enabled {
            free_key_str = if enabled { "true" } else { "false" };
            batch.push(("free_api_key_enabled", free_key_str));
        }
    }
    db::set_global_settings_batch(&state.db, &batch).await?;
    // Invalidate caches (preview_cache needs no invalidation — keys encode the config)
    state.global_settings_cache.invalidate(&()).await;
    state.settings_cache.invalidate_all();
    if req.free_api_key_enabled.is_some() && state.config.free_key_enabled.is_none() {
        state.free_api_key_cache.invalidate(&()).await;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn fetch_poster(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    // Validate id_type
    crate::id::IdType::parse(&id_type)?;

    // Load global settings (cached)
    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let (bytes, _) = serve::handle_inner(&state, &id_type, &id_value, (*settings).clone(), None).await?;
    Ok(serve::image_response(bytes, "image/jpeg"))
}

// --- Logos ---

pub async fn list_logos(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListImagesQuery>,
) -> Result<Json<ListImagesResponse>, AppError> {
    list_images(&state, &query, cache::ImageType::Logo).await
}

pub async fn logo_image(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    image_from_cache_key(&state, &id_type, &id_value, cache::ImageType::Logo, "image/png").await
}

pub async fn fetch_logo(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    fetch_logo_backdrop_image(&state, &id_type, &id_value, LogoBackdropKind::Logo, "image/png").await
}

// --- Backdrops ---

pub async fn list_backdrops(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListImagesQuery>,
) -> Result<Json<ListImagesResponse>, AppError> {
    list_images(&state, &query, cache::ImageType::Backdrop).await
}

pub async fn backdrop_image(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    image_from_cache_key(&state, &id_type, &id_value, cache::ImageType::Backdrop, "image/jpeg").await
}

pub async fn fetch_backdrop(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    fetch_logo_backdrop_image(&state, &id_type, &id_value, LogoBackdropKind::Backdrop, "image/jpeg").await
}

// --- Episodes ---

pub async fn list_episodes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListImagesQuery>,
) -> Result<Json<ListImagesResponse>, AppError> {
    list_images(&state, &query, cache::ImageType::Episode).await
}

pub async fn episode_image(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    image_from_cache_key(&state, &id_type, &id_value, cache::ImageType::Episode, "image/jpeg").await
}

pub async fn fetch_episode(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<Response, AppError> {
    crate::id::IdType::parse(&id_type)?;

    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let (bytes, _) = serve::handle_episode_inner(&state, &id_type, &id_value, (*settings).clone(), None).await?;
    Ok(serve::image_response(bytes, "image/jpeg"))
}

// --- Helpers ---

async fn image_from_cache_key(
    state: &AppState,
    id_type: &str,
    id_value: &str,
    image_type: cache::ImageType,
    content_type: &str,
) -> Result<Response, AppError> {
    crate::id::IdType::parse(id_type)?;

    // id_value contains colons (e.g. "tt123:logo:fanart:en:r_imdb").
    // Replace colons with underscores to get the filesystem filename base.
    let file_base = id_value.replace(':', "_");
    let path = cache::typed_cache_path(&state.config.cache_dir, image_type, id_type, &file_base)?;

    let canonical_cache_dir = tokio::fs::canonicalize(&state.config.cache_dir)
        .await
        .map_err(|e| AppError::Other(format!("Failed to resolve cache dir: {e}")))?;

    // Resolve the target path and verify it falls within the cache directory
    let canonical_path = tokio::fs::canonicalize(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::IdNotFound(format!("Image not found: {id_type}/{id_value}"))
        } else {
            AppError::Io(e)
        }
    })?;
    if !canonical_path.starts_with(&canonical_cache_dir) {
        return Err(AppError::IdNotFound(format!("Image not found: {id_type}/{id_value}")));
    }

    let bytes = tokio::fs::read(&canonical_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::IdNotFound(format!("Image not found: {id_type}/{id_value}"))
        } else {
            AppError::Io(e)
        }
    })?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type.to_string())],
        bytes,
    ).into_response())
}

async fn fetch_logo_backdrop_image(
    state: &AppState,
    id_type: &str,
    id_value: &str,
    lb_kind: LogoBackdropKind,
    content_type: &str,
) -> Result<Response, AppError> {
    crate::id::IdType::parse(id_type)?;

    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_render_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let (bytes, _) = serve::handle_logo_backdrop_inner(state, id_type, id_value, &settings, lb_kind, None).await?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type)],
        bytes,
    ).into_response())
}

// --- Cache purge ---

#[derive(Serialize)]
pub struct PurgeTitleResponse {
    pub ok: bool,
    /// When true there are no rendered files on disk (CDN-backed); the cached
    /// CDN copies cannot be purged from here, so the purge is partial.
    pub external_cache_only: bool,
    pub files_deleted: u64,
    pub meta_deleted: u64,
    pub ratings_deleted: u64,
}

#[derive(Serialize)]
pub struct PurgeAllResponse {
    pub ok: bool,
    pub external_cache_only: bool,
    /// Number of on-disk cache subdirectories cleared (renamed aside for
    /// background removal). 0 under `EXTERNAL_CACHE_ONLY`.
    pub dirs_cleared: u64,
    pub meta_deleted: u64,
    pub ratings_deleted: u64,
}

/// Scope of a `DELETE` purge: an entire logical title (all rendered variants of
/// one image kind — the default) or a single rendered variant (one exact cache
/// key, i.e. one row in the admin list).
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PurgeScope {
    #[default]
    Title,
    Variant,
}

#[derive(Deserialize)]
pub struct PurgeQuery {
    #[serde(default)]
    pub scope: PurgeScope,
}

/// Purge every cached variant of one logical title for a single image kind,
/// across all cache layers: on-disk rendered files, SQLite metadata
/// (`image_meta` + the shared `available_ratings` index), and the in-memory
/// moka caches (`image_mem_cache` by prefix, plus the exact-keyed `id_cache`
/// and `available_ratings_cache`).
async fn purge_title(
    state: &AppState,
    image_type: cache::ImageType,
    id_type: &str,
    id_value: &str,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    // Reject unknown id types and path-traversal / empty id values up front.
    crate::id::IdType::parse(id_type)?;
    cache::validate_id_value(id_value)?;

    let id_key = format!("{id_type}/{id_value}");

    // 1. On-disk rendered variants. Skipped under EXTERNAL_CACHE_ONLY (no files
    //    exist on disk — images are served from a CDN we can't purge from here).
    let files_deleted = if state.config.external_cache_only {
        0
    } else {
        cache::purge_title_files(&state.config.cache_dir, image_type, id_type, id_value).await?
    };

    // 2. SQLite: this kind's rendered-variant rows, plus the title's shared
    //    available-ratings index row so the next request re-resolves sources.
    let meta_deleted = db::delete_image_meta_for_title(&state.db, image_type, id_type, id_value).await?;
    let ratings_deleted = db::delete_available_ratings(&state.db, &id_key).await?;

    // 3. In-memory caches keyed by the full cache_key. Evict every variant via a
    //    delimiter-anchored prefix predicate (a bare prefix would let `tt123`
    //    capture `tt1234567`). Both builders opt into `support_invalidation_closures()`
    //    (see api/src/main.rs). This is intentionally cross-kind: it also drops the
    //    title's logos/backdrops from memory, which simply re-render from the
    //    still-intact per-kind disk/DB layers on the next request.
    let exact = id_key.clone();
    let prefix_underscore = format!("{id_key}_");
    let prefix_at = format!("{id_key}@");
    {
        let (exact, prefix_underscore, prefix_at) =
            (exact.clone(), prefix_underscore.clone(), prefix_at.clone());
        if let Err(e) = state.image_mem_cache.invalidate_entries_if(move |k, _v| {
            *k == exact || k.starts_with(&prefix_underscore) || k.starts_with(&prefix_at)
        }) {
            tracing::warn!(error = %e, key = %id_key, "image_mem_cache purge predicate rejected");
        }
    }
    // image_inflight briefly holds the just-completed render result; drop it too,
    // or `try_get_with` would re-serve the stale bytes (and re-promote them into
    // image_mem_cache) within its 30s TTL.
    if let Err(e) = state.image_inflight.invalidate_entries_if(move |k, _v| {
        *k == exact || k.starts_with(&prefix_underscore) || k.starts_with(&prefix_at)
    }) {
        tracing::warn!(error = %e, key = %id_key, "image_inflight purge predicate rejected");
    }
    // Resolution / fast-path index caches are keyed by the exact id_key.
    state.id_cache.invalidate(&id_key).await;
    state.available_ratings_cache.invalidate(&id_key).await;

    Ok(Json(PurgeTitleResponse {
        ok: true,
        external_cache_only: state.config.external_cache_only,
        files_deleted,
        meta_deleted,
        ratings_deleted,
    }))
}

/// Purge a single rendered variant — one exact cache key (one row in the admin
/// list) — without disturbing the rest of the title. Clears just that file, its
/// `image_meta` row, and its `image_mem_cache` / `image_inflight` entries; the
/// title's shared `available_ratings` index and resolution caches are left intact.
///
/// `cache_value` is the full `{id_value}{variant}{suffix}` form (the value the
/// admin list shows as the row's "ID Value"), so the matching cache key is
/// `{id_type}/{cache_value}` and the file is `{subdir}/{id_type}/{cache_value}.{ext}`.
async fn purge_variant(
    state: &AppState,
    image_type: cache::ImageType,
    id_type: &str,
    cache_value: &str,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    crate::id::IdType::parse(id_type)?;
    cache::validate_id_value(cache_value)?;

    let cache_key = format!("{id_type}/{cache_value}");

    let files_deleted = if state.config.external_cache_only {
        0
    } else {
        cache::purge_variant_file(&state.config.cache_dir, image_type, id_type, cache_value).await?
    };

    let meta_deleted = db::delete_image_meta_exact(&state.db, image_type, &cache_key).await?;

    state.image_mem_cache.invalidate(&cache_key).await;
    state.image_inflight.invalidate(&cache_key).await;

    Ok(Json(PurgeTitleResponse {
        ok: true,
        external_cache_only: state.config.external_cache_only,
        files_deleted,
        meta_deleted,
        ratings_deleted: 0,
    }))
}

/// Route a `DELETE` purge to the title-wide or single-variant path by `?scope=`.
async fn purge_dispatch(
    state: &AppState,
    image_type: cache::ImageType,
    id_type: &str,
    id_value: &str,
    scope: PurgeScope,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    match scope {
        PurgeScope::Title => purge_title(state, image_type, id_type, id_value).await,
        PurgeScope::Variant => purge_variant(state, image_type, id_type, id_value).await,
    }
}

pub async fn purge_poster(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
    Query(q): Query<PurgeQuery>,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    purge_dispatch(&state, cache::ImageType::Poster, &id_type, &id_value, q.scope).await
}

pub async fn purge_logo(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
    Query(q): Query<PurgeQuery>,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    purge_dispatch(&state, cache::ImageType::Logo, &id_type, &id_value, q.scope).await
}

pub async fn purge_backdrop(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
    Query(q): Query<PurgeQuery>,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    purge_dispatch(&state, cache::ImageType::Backdrop, &id_type, &id_value, q.scope).await
}

pub async fn purge_episode(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
    Query(q): Query<PurgeQuery>,
) -> Result<Json<PurgeTitleResponse>, AppError> {
    purge_dispatch(&state, cache::ImageType::Episode, &id_type, &id_value, q.scope).await
}

#[derive(Serialize)]
pub struct PurgeKindResponse {
    pub ok: bool,
    pub external_cache_only: bool,
    /// Whether the kind's on-disk rendered directory was cleared (false under
    /// `EXTERNAL_CACHE_ONLY`, or if nothing was cached yet).
    pub dir_cleared: bool,
    pub meta_deleted: u64,
}

/// Clear every cached image of one kind (e.g. all posters). Stages that kind's
/// rendered directory aside for instant clearing + background removal, deletes
/// its `image_meta` rows, and drops the in-memory render caches.
///
/// The render caches (`image_mem_cache` / `image_inflight`) are keyed by the
/// full cache key, which has no clean per-kind marker for posters/episodes, so
/// they're cleared wholesale rather than by a fragile predicate — the other
/// kinds simply re-render from their still-intact disk/DB layers (the in-memory
/// cache is a size-bounded hot path, capped by `IMAGE_MEM_CACHE_MB`). The shared
/// `available_ratings` index and upstream source caches are left untouched.
async fn clear_kind(
    state: &AppState,
    image_type: cache::ImageType,
) -> Result<Json<PurgeKindResponse>, AppError> {
    let dir_cleared = if state.config.external_cache_only {
        false
    } else {
        match cache::stage_dir_for_clear(&state.config.cache_dir, image_type.subdir()).await? {
            Some(staged) => {
                tokio::spawn(cache::remove_staged_dirs(vec![staged]));
                true
            }
            None => false,
        }
    };

    let meta_deleted = db::delete_image_meta_by_kind(&state.db, image_type).await?;

    state.image_mem_cache.invalidate_all();
    state.image_inflight.invalidate_all();
    state.image_mem_cache.run_pending_tasks().await;

    Ok(Json(PurgeKindResponse {
        ok: true,
        external_cache_only: state.config.external_cache_only,
        dir_cleared,
        meta_deleted,
    }))
}

pub async fn clear_posters(State(state): State<Arc<AppState>>) -> Result<Json<PurgeKindResponse>, AppError> {
    clear_kind(&state, cache::ImageType::Poster).await
}

pub async fn clear_logos(State(state): State<Arc<AppState>>) -> Result<Json<PurgeKindResponse>, AppError> {
    clear_kind(&state, cache::ImageType::Logo).await
}

pub async fn clear_backdrops(State(state): State<Arc<AppState>>) -> Result<Json<PurgeKindResponse>, AppError> {
    clear_kind(&state, cache::ImageType::Backdrop).await
}

pub async fn clear_episodes(State(state): State<Arc<AppState>>) -> Result<Json<PurgeKindResponse>, AppError> {
    clear_kind(&state, cache::ImageType::Episode).await
}

/// Clear the entire image cache: all rendered files + raw downloads on disk,
/// all `image_meta` / `available_ratings` rows, and every image-related
/// in-memory cache. Auth and settings caches are intentionally left intact.
pub async fn purge_all(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PurgeAllResponse>, AppError> {
    // Rename the cache subdirs aside (O(1) regardless of file count) so the cache
    // is cleared instantly, then delete the staged dirs on a background task — a
    // huge cache (hundreds of thousands of files) must not block the request. Any
    // staged dir left by a crash mid-delete is swept on the next startup.
    let dirs_cleared = if state.config.external_cache_only {
        0
    } else {
        let staged = cache::stage_cache_for_clear(&state.config.cache_dir).await?;
        let count = staged.len() as u64;
        if !staged.is_empty() {
            tokio::spawn(cache::remove_staged_dirs(staged));
        }
        count
    };

    let meta_deleted = db::delete_all_image_meta(&state.db).await?;
    let ratings_deleted = db::delete_all_available_ratings(&state.db).await?;

    state.image_mem_cache.invalidate_all();
    state.image_inflight.invalidate_all();
    state.id_cache.invalidate_all();
    state.ratings_cache.invalidate_all();
    state.available_ratings_cache.invalidate_all();
    state.fanart_cache.invalidate_all();
    state.fanart_negative.invalidate_all();
    state.tmdb_images_cache.invalidate_all();
    state.preview_cache.invalidate_all();

    // `invalidate_all` is lazy. Flush the caches whose entry counts feed the stats
    // endpoint so the dashboard's immediate post-purge refetch shows zeroed counts
    // rather than stale ones. (Only on this rare admin action, never on stats polls.)
    state.image_mem_cache.run_pending_tasks().await;
    state.id_cache.run_pending_tasks().await;
    state.ratings_cache.run_pending_tasks().await;

    Ok(Json(PurgeAllResponse {
        ok: true,
        external_cache_only: state.config.external_cache_only,
        dirs_cleared,
        meta_deleted,
        ratings_deleted,
    }))
}
