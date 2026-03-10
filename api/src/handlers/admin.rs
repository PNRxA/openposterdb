use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::cache;
use crate::error::AppError;
use crate::services::db::{self, validate_fanart_lang, validate_poster_source};
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

pub async fn list_posters(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListPostersQuery>,
) -> Result<Json<ListPostersResponse>, AppError> {
    let page = query.page.max(1);
    let page_size = query.page_size.clamp(1, 100);

    let (items, total) = db::list_poster_meta(&state.db, page, page_size).await?;

    let items = items
        .into_iter()
        .map(|m| PosterMetaItem {
            cache_key: m.cache_key,
            release_date: m.release_date,
            created_at: m.created_at,
            updated_at: m.updated_at,
        })
        .collect();

    Ok(Json(ListPostersResponse {
        items,
        total,
        page,
        page_size,
    }))
}

pub async fn poster_image(
    State(state): State<Arc<AppState>>,
    Path((id_type, id_value)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // Validate id_type is one of the known variants (imdb, tmdb, tvdb)
    crate::id::IdType::parse(&id_type)?;

    let path = cache::cache_path(&state.config.cache_dir, &id_type, &id_value)?;

    // Canonicalize and verify the resolved path is within cache_dir to prevent traversal
    let canonical_path = tokio::fs::canonicalize(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::IdNotFound(format!("Poster not found: {id_type}/{id_value}"))
        } else {
            tracing::error!(error = %e, path = %path.display(), "Failed to canonicalize poster path");
            AppError::Io(e)
        }
    })?;
    let canonical_cache_dir = tokio::fs::canonicalize(&state.config.cache_dir)
        .await
        .map_err(|e| AppError::Other(format!("Failed to resolve cache dir: {e}")))?;
    if !canonical_path.starts_with(&canonical_cache_dir) {
        return Err(AppError::IdNotFound(format!(
            "Poster not found: {id_type}/{id_value}"
        )));
    }

    let bytes = tokio::fs::read(&canonical_path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::IdNotFound(format!("Poster not found: {id_type}/{id_value}"))
        } else {
            tracing::error!(error = %e, path = %canonical_path.display(), "Failed to read poster image");
            AppError::Io(e)
        }
    })?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
        bytes,
    ))
}

#[derive(Serialize)]
pub struct GlobalSettingsResponse {
    pub poster_source: String,
    pub fanart_lang: String,
    pub fanart_textless: bool,
    pub fanart_available: bool,
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
    Ok(Json(GlobalSettingsResponse {
        poster_source: settings.poster_source.clone(),
        fanart_lang: settings.fanart_lang.clone(),
        fanart_textless: settings.fanart_textless,
        fanart_available: state.fanart.is_some(),
    }))
}

#[derive(Deserialize)]
pub struct UpdateGlobalSettingsRequest {
    pub poster_source: String,
    #[serde(default = "db::default_fanart_lang")]
    pub fanart_lang: String,
    #[serde(default)]
    pub fanart_textless: bool,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateGlobalSettingsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate_poster_source(&req.poster_source)?;
    validate_fanart_lang(&req.fanart_lang)?;
    let textless_str = if req.fanart_textless { "true" } else { "false" };
    db::set_global_settings_batch(
        &state.db,
        &[
            ("poster_source", &req.poster_source),
            ("fanart_lang", &req.fanart_lang),
            ("fanart_textless", textless_str),
        ],
    )
    .await?;
    // Invalidate caches
    state.global_settings_cache.invalidate(&()).await;
    state.settings_cache.invalidate_all();
    Ok(Json(serde_json::json!({ "ok": true })))
}
