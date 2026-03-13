use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::cache;
use crate::error::AppError;
use crate::poster::serve::{self, FanartImageKind};
use crate::services::db::{self, validate_poster_settings_input, PosterSettingsInput, default_ratings_limit, default_logo_backdrop_ratings_limit, default_ratings_order, default_poster_position, default_poster_badge_style, default_logo_badge_style, default_backdrop_badge_style, default_label_style, default_poster_badge_direction};
use crate::AppState;

#[derive(Serialize)]
pub struct StatsResponse {
    pub total_posters: u64,
    pub total_api_keys: u64,
    pub mem_cache_entries: u64,
    pub id_cache_entries: u64,
    pub ratings_cache_entries: u64,
    pub poster_mem_cache_mb: u64,
}

pub async fn stats(State(state): State<Arc<AppState>>) -> Result<Json<StatsResponse>, AppError> {
    let total_posters = db::count_poster_meta(&state.db).await?;
    let total_api_keys = db::count_api_keys(&state.db).await?;

    let mem_cache_entries = state.poster_mem_cache.entry_count();
    let id_cache_entries = state.id_cache.entry_count();
    let ratings_cache_entries = state.ratings_cache.entry_count();
    let poster_mem_cache_mb = state.poster_mem_cache.weighted_size() / (1024 * 1024);

    Ok(Json(StatsResponse {
        total_posters,
        total_api_keys,
        mem_cache_entries,
        id_cache_entries,
        ratings_cache_entries,
        poster_mem_cache_mb,
    }))
}

#[derive(Deserialize)]
pub struct ListPostersQuery {
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
pub struct PosterMetaItem {
    pub cache_key: String,
    pub release_date: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize)]
pub struct ListPostersResponse {
    pub items: Vec<PosterMetaItem>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

async fn list_images(
    state: &AppState,
    query: &ListPostersQuery,
    image_type: cache::ImageType,
) -> Result<Json<ListPostersResponse>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);

    let (items, total) = db::list_poster_meta_by_kind(&state.db, image_type, page, page_size).await?;

    let items = items
        .into_iter()
        .map(|m| PosterMetaItem {
            cache_key: m.cache_key,
            release_date: m.release_date,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect();

    Ok(Json(ListPostersResponse { items, total, page, page_size }))
}

pub async fn list_posters(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListPostersQuery>,
) -> Result<Json<ListPostersResponse>, AppError> {
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
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
    pub fanart_available: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub free_api_key_enabled: bool,
    pub poster_position: String,
    pub logo_ratings_limit: i32,
    pub backdrop_ratings_limit: i32,
    pub poster_badge_style: String,
    pub logo_badge_style: String,
    pub backdrop_badge_style: String,
    pub poster_label_style: String,
    pub logo_label_style: String,
    pub backdrop_label_style: String,
    pub poster_badge_direction: String,
}

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GlobalSettingsResponse>, AppError> {
    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_poster_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    let free_api_key_enabled = state.is_free_api_key_enabled().await;
    Ok(Json(GlobalSettingsResponse {
        poster_source: settings.poster_source.to_string(),
        fanart_lang: settings.fanart_lang.to_string(),
        fanart_textless: settings.fanart_textless,
        fanart_available: state.fanart.is_some(),
        ratings_limit: settings.ratings_limit,
        ratings_order: settings.ratings_order.to_string(),
        free_api_key_enabled,
        poster_position: settings.poster_position.to_string(),
        logo_ratings_limit: settings.logo_ratings_limit,
        backdrop_ratings_limit: settings.backdrop_ratings_limit,
        poster_badge_style: settings.poster_badge_style.to_string(),
        logo_badge_style: settings.logo_badge_style.to_string(),
        backdrop_badge_style: settings.backdrop_badge_style.to_string(),
        poster_label_style: settings.poster_label_style.to_string(),
        logo_label_style: settings.logo_label_style.to_string(),
        backdrop_label_style: settings.backdrop_label_style.to_string(),
        poster_badge_direction: settings.poster_badge_direction.to_string(),
    }))
}

#[derive(Deserialize)]
pub struct UpdateGlobalSettingsRequest {
    pub poster_source: String,
    #[serde(default = "db::default_fanart_lang")]
    pub fanart_lang: String,
    #[serde(default)]
    pub fanart_textless: bool,
    #[serde(default = "default_ratings_limit")]
    pub ratings_limit: i32,
    #[serde(default = "default_ratings_order")]
    pub ratings_order: String,
    pub free_api_key_enabled: Option<bool>,
    #[serde(default = "default_poster_position")]
    pub poster_position: String,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub logo_ratings_limit: i32,
    #[serde(default = "default_logo_backdrop_ratings_limit")]
    pub backdrop_ratings_limit: i32,
    #[serde(default = "default_poster_badge_style")]
    pub poster_badge_style: String,
    #[serde(default = "default_logo_badge_style")]
    pub logo_badge_style: String,
    #[serde(default = "default_backdrop_badge_style")]
    pub backdrop_badge_style: String,
    #[serde(default = "default_label_style")]
    pub poster_label_style: String,
    #[serde(default = "default_label_style")]
    pub logo_label_style: String,
    #[serde(default = "default_label_style")]
    pub backdrop_label_style: String,
    #[serde(default = "default_poster_badge_direction")]
    pub poster_badge_direction: String,
}

impl PosterSettingsInput for UpdateGlobalSettingsRequest {
    fn poster_source(&self) -> &str { &self.poster_source }
    fn fanart_lang(&self) -> &str { &self.fanart_lang }
    fn ratings_limit(&self) -> i32 { self.ratings_limit }
    fn ratings_order(&self) -> &str { &self.ratings_order }
    fn poster_position(&self) -> &str { &self.poster_position }
    fn logo_ratings_limit(&self) -> i32 { self.logo_ratings_limit }
    fn backdrop_ratings_limit(&self) -> i32 { self.backdrop_ratings_limit }
    fn poster_badge_style(&self) -> &str { &self.poster_badge_style }
    fn logo_badge_style(&self) -> &str { &self.logo_badge_style }
    fn backdrop_badge_style(&self) -> &str { &self.backdrop_badge_style }
    fn poster_label_style(&self) -> &str { &self.poster_label_style }
    fn logo_label_style(&self) -> &str { &self.logo_label_style }
    fn backdrop_label_style(&self) -> &str { &self.backdrop_label_style }
    fn poster_badge_direction(&self) -> &str { &self.poster_badge_direction }
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateGlobalSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_poster_settings_input(&req)?;
    let textless_str = if req.fanart_textless { "true" } else { "false" };
    let limit_str = req.ratings_limit.to_string();
    let logo_limit_str = req.logo_ratings_limit.to_string();
    let backdrop_limit_str = req.backdrop_ratings_limit.to_string();
    let mut batch: Vec<(&str, &str)> = vec![
        ("poster_source", &req.poster_source),
        ("fanart_lang", &req.fanart_lang),
        ("fanart_textless", textless_str),
        ("ratings_limit", &limit_str),
        ("ratings_order", &req.ratings_order),
        ("poster_position", &req.poster_position),
        ("logo_ratings_limit", &logo_limit_str),
        ("backdrop_ratings_limit", &backdrop_limit_str),
        ("poster_badge_style", &req.poster_badge_style),
        ("logo_badge_style", &req.logo_badge_style),
        ("backdrop_badge_style", &req.backdrop_badge_style),
        ("poster_label_style", &req.poster_label_style),
        ("logo_label_style", &req.logo_label_style),
        ("backdrop_label_style", &req.backdrop_label_style),
        ("poster_badge_direction", &req.poster_badge_direction),
    ];
    let free_key_str;
    if let Some(enabled) = req.free_api_key_enabled {
        free_key_str = if enabled { "true" } else { "false" };
        batch.push(("free_api_key_enabled", free_key_str));
    }
    db::set_global_settings_batch(&state.db, &batch).await?;
    // Invalidate caches (preview_cache needs no invalidation — keys encode the config)
    state.global_settings_cache.invalidate(&()).await;
    state.settings_cache.invalidate_all();
    if req.free_api_key_enabled.is_some() {
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
            Ok::<_, AppError>(Arc::new(db::parse_global_poster_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let bytes = serve::handle_inner(&state, &id_type, &id_value, (*settings).clone(), None).await?;
    Ok(serve::jpeg_response(bytes))
}

// --- Logos ---

pub async fn list_logos(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListPostersQuery>,
) -> Result<Json<ListPostersResponse>, AppError> {
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
    fetch_fanart_image(&state, &id_type, &id_value, FanartImageKind::Logo, "image/png").await
}

// --- Backdrops ---

pub async fn list_backdrops(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListPostersQuery>,
) -> Result<Json<ListPostersResponse>, AppError> {
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
    fetch_fanart_image(&state, &id_type, &id_value, FanartImageKind::Backdrop, "image/jpeg").await
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

async fn fetch_fanart_image(
    state: &AppState,
    id_type: &str,
    id_value: &str,
    fanart_kind: FanartImageKind,
    content_type: &str,
) -> Result<Response, AppError> {
    crate::id::IdType::parse(id_type)?;

    let db_ref = state.db.clone();
    let settings = state
        .global_settings_cache
        .try_get_with((), async move {
            let globals = db::get_global_settings(&db_ref).await?;
            Ok::<_, AppError>(Arc::new(db::parse_global_poster_settings(&globals)))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let bytes = serve::handle_fanart_image_inner(state, id_type, id_value, &settings, fanart_kind, None).await?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, content_type)],
        bytes,
    ).into_response())
}
