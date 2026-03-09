use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{self, MemCacheEntry};
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

    // Validate API key (cached, including negative results to prevent DB hammering)
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let key_hash = format!("{:x}", hasher.finalize());

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

    match handle_inner(&state, &id_type_str, &id_value_jpg).await {
        Ok(bytes) => jpeg_response(bytes),
        Err(e) => {
            if use_fallback {
                tracing::warn!(error = %e, "returning fallback placeholder");
                jpeg_response(generate::placeholder_jpeg().into())
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
) -> Result<Bytes, AppError> {
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    let cache_path = cache::cache_path(&state.config.cache_dir, id_type_str, id_value);
    let cache_key = format!("{id_type_str}/{id_value}");

    // Check in-memory poster cache first
    if let Some(entry) = state.poster_mem_cache.get(&cache_key).await {
        // Only do the staleness check if we haven't checked recently
        if entry.last_checked.elapsed() >= std::time::Duration::from_secs(60) {
            let release_date = cache::read_meta_db(&state.db, &cache_key).await;
            let stale_secs = cache::compute_stale_secs(
                release_date.as_deref(),
                state.config.ratings_min_stale_secs,
                state.config.ratings_max_age_secs,
            );
            if let Some(fs_entry) = cache::read(&cache_path, stale_secs).await
                && fs_entry.is_stale
            {
                trigger_background_refresh(state, &cache_key, &cache_path, id_type, id_value);
            }
            // Update last_checked timestamp
            state
                .poster_mem_cache
                .insert(
                    cache_key,
                    MemCacheEntry {
                        bytes: entry.bytes.clone(),
                        last_checked: Instant::now(),
                    },
                )
                .await;
        }
        return Ok(entry.bytes.clone());
    }

    // Read release date from DB for dynamic staleness
    let release_date = cache::read_meta_db(&state.db, &cache_key).await;
    let stale_secs = cache::compute_stale_secs(
        release_date.as_deref(),
        state.config.ratings_min_stale_secs,
        state.config.ratings_max_age_secs,
    );

    // Check filesystem cache
    if let Some(entry) = cache::read(&cache_path, stale_secs).await {
        if entry.is_stale {
            trigger_background_refresh(state, &cache_key, &cache_path, id_type, id_value);
        }
        // Insert into memory cache
        let bytes: Bytes = entry.bytes.into();
        state
            .poster_mem_cache
            .insert(
                cache_key,
                MemCacheEntry {
                    bytes: bytes.clone(),
                    last_checked: Instant::now(),
                },
            )
            .await;
        return Ok(bytes);
    }

    // Request coalescing — concurrent requests for the same poster share one generation
    let state2 = state.clone();
    let cache_key2 = cache_key.clone();
    let id_value2 = id_value.to_owned();
    let cache_path2 = cache_path.clone();
    let bytes: Bytes = state
        .poster_inflight
        .try_get_with(cache_key.clone(), async move {
            let (bytes, rd) =
                generate_poster(&state2, id_type, &id_value2).await?;
            cache::write(&cache_path2, &bytes).await?;
            cache::upsert_meta_db(&state2.db, &cache_key2, rd.as_deref()).await?;
            Ok::<_, AppError>(Bytes::from(bytes))
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    // Insert into memory cache
    state
        .poster_mem_cache
        .insert(
            cache_key,
            MemCacheEntry {
                bytes: bytes.clone(),
                last_checked: Instant::now(),
            },
        )
        .await;
    Ok(bytes)
}

fn trigger_background_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
) {
    // Best-effort dedup: a narrow race can spawn a duplicate refresh, which is harmless
    if state.refresh_locks.contains_key(cache_key) {
        return;
    }
    state.refresh_locks.insert(cache_key.to_string(), ());
    let state = state.clone();
    let id_value = id_value.to_string();
    let cache_path = cache_path.to_path_buf();
    let cache_key = cache_key.to_string();
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
                // Update memory cache with fresh data
                state
                    .poster_mem_cache
                    .insert(
                        cache_key.clone(),
                        MemCacheEntry {
                            bytes: bytes.into(),
                            last_checked: Instant::now(),
                        },
                    )
                    .await;
            }
            Err(e) => {
                tracing::error!(error = %e, "background refresh failed");
            }
        }
        state.refresh_locks.invalidate(&cache_key);
    });
}

async fn generate_poster(
    state: &AppState,
    id_type: IdType,
    id_value: &str,
) -> Result<(Vec<u8>, Option<String>), AppError> {
    let resolved = id::resolve(id_type, id_value, &state.tmdb, &state.id_cache).await?;

    let poster_path = resolved
        .poster_path
        .as_deref()
        .ok_or_else(|| AppError::Other("no poster available".into()))?;

    let badges = ratings::fetch_ratings(
        &resolved,
        &state.tmdb,
        state.omdb.as_ref(),
        state.mdblist.as_ref(),
        &state.ratings_cache,
    )
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

fn jpeg_response(bytes: Bytes) -> Response {
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
