use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::cache;
use crate::error::AppError;
use crate::id::{self, IdType};
use crate::poster::generate;
use crate::services::{db, ratings};
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

    // Validate API key
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());

    let api_key_model = match db::find_api_key_by_hash(&state.db, &key_hash).await {
        Ok(Some(model)) => model,
        Ok(None) => return AppError::Unauthorized.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "failed to validate API key");
            return AppError::Unauthorized.into_response();
        }
    };

    // Update last_used_at in background
    let db = state.db.clone();
    let key_id = api_key_model.id;
    tokio::spawn(async move {
        if let Err(e) = db::update_api_key_last_used(&db, key_id).await {
            tracing::warn!(error = %e, "failed to update API key last_used_at");
        }
    });

    match handle_inner(&state, &id_type_str, &id_value_jpg).await {
        Ok(bytes) => jpeg_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder");
                jpeg_response(generate::placeholder_jpeg())
            } else {
                e.into_response()
            }
        }
    }
}

async fn handle_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_jpg: &str,
) -> Result<Vec<u8>, AppError> {
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    let cache_path = cache::cache_path(&state.config.cache_dir, id_type_str, id_value);
    let cache_key = format!("{id_type_str}/{id_value}");

    // Read release date from DB for dynamic staleness
    let release_date = cache::read_meta_db(&state.db, &cache_key).await;
    let stale_secs = cache::compute_stale_secs(
        release_date.as_deref(),
        state.config.ratings_min_stale_secs,
        state.config.ratings_max_age_secs,
    );

    // Check cache
    if let Some(entry) = cache::read(&cache_path, stale_secs).await {
        if entry.is_stale {
            // Spawn background refresh if not already in progress
            if state.refresh_locks.get(&cache_key).is_none() {
                state.refresh_locks.insert(cache_key.clone(), ());
                let state = state.clone();
                let id_value = id_value.to_string();
                let cache_path = cache_path.clone();
                let cache_key = cache_key.clone();
                tokio::spawn(async move {
                    tracing::info!(key = %cache_key, "background refresh started");
                    match generate_poster(&state, id_type, &id_value).await {
                        Ok((bytes, rd)) => {
                            if let Err(e) = cache::write(&cache_path, &bytes).await {
                                tracing::error!(error = %e, "failed to write cache");
                            }
                            if let Err(e) =
                                cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref()).await
                            {
                                tracing::error!(error = %e, "failed to write meta to db");
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "background refresh failed");
                        }
                    }
                    state.refresh_locks.remove(&cache_key);
                });
            }
        }
        return Ok(entry.bytes);
    }

    // Generate fresh
    let (bytes, rd) = generate_poster(state, id_type, id_value).await?;
    cache::write(&cache_path, &bytes).await?;
    cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref()).await?;
    Ok(bytes)
}

async fn generate_poster(
    state: &AppState,
    id_type: IdType,
    id_value: &str,
) -> Result<(Vec<u8>, Option<String>), AppError> {
    let resolved = id::resolve(id_type, id_value, &state.tmdb).await?;

    let poster_path = resolved
        .poster_path
        .as_deref()
        .ok_or_else(|| AppError::Other("no poster available".into()))?;

    let badges =
        ratings::fetch_ratings(&resolved, &state.tmdb, state.omdb.as_ref(), state.mdblist.as_ref())
            .await;

    let bytes = generate::generate_poster(generate::PosterParams {
        poster_path,
        badges: &badges,
        tmdb: &state.tmdb,
        http: &state.http,
        font: &state.font,
        quality: state.config.poster_quality,
        cache_dir: &state.config.cache_dir,
        poster_stale_secs: state.config.poster_stale_secs,
    })
    .await?;

    Ok((bytes, resolved.release_date))
}

fn jpeg_response(bytes: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (
                header::CACHE_CONTROL,
                "public, max-age=3600, stale-while-revalidate=86400",
            ),
        ],
        bytes,
    )
        .into_response()
}
