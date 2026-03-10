use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{self, MemCacheEntry};
use crate::error::AppError;
use crate::handlers::auth::hash_api_key;
use crate::id::{self, IdType, MediaType};
use crate::poster::generate;
use crate::services::db::PosterSettings;
use crate::services::fanart::{FanartClient, PosterMatch};
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
        .unwrap_or_else(|_| Arc::new(PosterSettings::default()));

    match handle_inner(&state, &id_type_str, &id_value_jpg, &settings).await {
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
    settings: &PosterSettings,
) -> Result<Bytes, AppError> {
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    let use_fanart = settings.poster_source == "fanart";

    // Try the fanart path first; falls through to TMDB on miss
    if use_fanart {
        if let Some(bytes) = try_fanart_path(state, id_type_str, id_value, id_type, settings).await? {
            return Ok(bytes);
        }
    }

    // TMDB path (default, or fanart fallback)
    let settings = if use_fanart {
        &PosterSettings::default()
    } else {
        settings
    };
    let cache_path = cache::cache_path(&state.config.cache_dir, id_type_str, id_value)?;
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
                trigger_background_refresh(state, &cache_key, &cache_path, id_type, id_value, settings);
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
            trigger_background_refresh(state, &cache_key, &cache_path, id_type, id_value, settings);
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
    let settings2 = settings.clone();
    let bytes: Bytes = state
        .poster_inflight
        .try_get_with(cache_key.clone(), async move {
            let (bytes, rd, _used_fanart) =
                generate_poster_with_source(&state2, id_type, &id_value2, &settings2).await?;
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

/// Check a single fanart cache variant (memory → filesystem).
/// Returns `Ok(Some(bytes))` on cache hit, `Ok(None)` on miss.
async fn check_fanart_cache_variant(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    settings: &PosterSettings,
) -> Result<Option<Bytes>, AppError> {
    // Check in-memory cache
    if let Some(entry) = state.poster_mem_cache.get(cache_key).await {
        if entry.last_checked.elapsed() >= std::time::Duration::from_secs(60) {
            let release_date = cache::read_meta_db(&state.db, cache_key).await;
            let stale_secs = cache::compute_stale_secs(
                release_date.as_deref(),
                state.config.ratings_min_stale_secs,
                state.config.ratings_max_age_secs,
            );
            if let Some(fs_entry) = cache::read(cache_path, stale_secs).await
                && fs_entry.is_stale
            {
                trigger_background_refresh(state, cache_key, cache_path, id_type, id_value, settings);
            }
            state
                .poster_mem_cache
                .insert(
                    cache_key.to_string(),
                    MemCacheEntry {
                        bytes: entry.bytes.clone(),
                        last_checked: Instant::now(),
                    },
                )
                .await;
        }
        return Ok(Some(entry.bytes.clone()));
    }

    // Check filesystem cache
    let release_date = cache::read_meta_db(&state.db, cache_key).await;
    let stale_secs = cache::compute_stale_secs(
        release_date.as_deref(),
        state.config.ratings_min_stale_secs,
        state.config.ratings_max_age_secs,
    );
    if let Some(entry) = cache::read(cache_path, stale_secs).await {
        if entry.is_stale {
            trigger_background_refresh(state, cache_key, cache_path, id_type, id_value, settings);
        }
        let bytes: Bytes = entry.bytes.into();
        state
            .poster_mem_cache
            .insert(
                cache_key.to_string(),
                MemCacheEntry {
                    bytes: bytes.clone(),
                    last_checked: Instant::now(),
                },
            )
            .await;
        return Ok(Some(bytes));
    }

    Ok(None)
}

/// Build a fanart cache key and filesystem path from a variant suffix (e.g. ":fanart:textless").
fn fanart_variant_paths(
    cache_dir: &str,
    id_type_str: &str,
    id_value: &str,
    variant: &str,
) -> Result<(String, std::path::PathBuf), AppError> {
    let cache_key = format!("{id_type_str}/{id_value}{variant}");
    let path_variant = variant.replace(':', "_");
    let cache_path_base = format!("{id_value}{path_variant}");
    let cache_path = cache::cache_path(cache_dir, id_type_str, &cache_path_base)?;
    Ok((cache_key, cache_path))
}

/// Try to serve a poster from the fanart cache (memory → filesystem → fresh generation).
/// Returns `Ok(Some(bytes))` on hit, `Ok(None)` to fall through to TMDB, or `Err` on hard failure.
async fn try_fanart_path(
    state: &AppState,
    id_type_str: &str,
    id_value: &str,
    id_type: IdType,
    settings: &PosterSettings,
) -> Result<Option<Bytes>, AppError> {
    // Build the list of cache variants to check.
    // When textless is requested but we know it's unavailable (negative cache),
    // skip the textless key and go straight to language.
    let neg_key = format!("{id_type_str}/{id_value}:fanart:textless:neg");
    let textless_known_missing = settings.fanart_textless
        && state.fanart_negative.get(&neg_key).await.is_some();

    let lang_variant = format!(":fanart:{}", settings.fanart_lang);
    let lang_neg_key = format!("{id_type_str}/{id_value}:fanart:{}:neg", settings.fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&lang_neg_key).await.is_some();

    // All fanart variants are known-missing — skip generation and fall through to TMDB
    if lang_known_missing && (!settings.fanart_textless || textless_known_missing) {
        return Ok(None);
    }

    // Check cached variants (textless first if requested, then language)
    let mut variants_to_check: Vec<String> = Vec::new();
    if settings.fanart_textless && !textless_known_missing {
        variants_to_check.push(":fanart:textless".to_string());
    }
    if !lang_known_missing {
        variants_to_check.push(lang_variant.clone());
    }

    for variant in &variants_to_check {
        let (cache_key, cache_path) =
            fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, variant)?;
        if let Some(bytes) =
            check_fanart_cache_variant(state, &cache_key, &cache_path, id_type, id_value, settings).await?
        {
            return Ok(Some(bytes));
        }
    }

    // No cache hit — generate with fanart. Cache under the key matching the actual
    // tier used (textless vs language). If no fanart match, fall through to TMDB.
    let result = generate_poster_with_source(state, id_type, id_value, settings).await;

    match result {
        Ok((bytes, rd, Some(tier))) => {
            if settings.fanart_textless && tier == PosterMatch::Language {
                state.fanart_negative.insert(neg_key, ()).await;
            }

            let actual_variant = match tier {
                PosterMatch::Textless => ":fanart:textless".to_string(),
                PosterMatch::Language => format!(":fanart:{}", settings.fanart_lang),
            };
            let (cache_key, cache_path) =
                fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, &actual_variant)?;
            let _ = cache::write(&cache_path, &bytes).await;
            let _ = cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref()).await;
            let bytes = Bytes::from(bytes);
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
            Ok(Some(bytes))
        }
        Ok((_bytes, _rd, None)) => {
            if settings.fanart_textless {
                state.fanart_negative.insert(neg_key, ()).await;
            }
            state.fanart_negative.insert(lang_neg_key, ()).await;
            Ok(None)
        }
        Err(e) => {
            tracing::warn!(error = %e, "fanart generation failed, falling through to TMDB");
            Ok(None)
        }
    }
}

fn trigger_background_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    settings: &PosterSettings,
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
    let settings = settings.clone();
    tokio::spawn(async move {
        tracing::info!(key = %cache_key, "background refresh started");
        match generate_poster_with_source(&state, id_type, &id_value, &settings).await {
            Ok((bytes, rd, _used_fanart)) => {
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

/// Returns (poster_bytes, release_date, fanart_match_tier)
async fn generate_poster_with_source(
    state: &AppState,
    id_type: IdType,
    id_value: &str,
    settings: &PosterSettings,
) -> Result<(Vec<u8>, Option<String>, Option<PosterMatch>), AppError> {
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

    // Try to fetch poster bytes from fanart.tv if configured
    let fanart_result = if settings.poster_source == "fanart" {
        if let Some(ref fanart) = state.fanart {
            fetch_fanart_poster(
                fanart,
                &state.tmdb,
                &state.fanart_cache,
                &resolved,
                &settings.fanart_lang,
                settings.fanart_textless,
            )
            .await
        } else {
            None
        }
    } else {
        None
    };
    let match_tier = fanart_result.as_ref().map(|r| r.match_tier);
    let fanart_bytes = fanart_result.map(|r| r.bytes);
    let has_fanart = fanart_bytes.is_some();

    let bytes = generate::generate_poster(generate::PosterParams {
        poster_path,
        badges: &badges,
        tmdb: &state.tmdb,
        http: &state.http,
        font: &state.font,
        quality: state.config.poster_quality,
        cache_dir: &state.config.cache_dir,
        poster_stale_secs: state.config.poster_stale_secs,
        poster_bytes_override: fanart_bytes,
        normalize_width: has_fanart,
    })
    .await?;

    Ok((bytes, resolved.release_date, match_tier))
}

/// Result of a fanart poster fetch, indicating what tier matched.
struct FanartResult {
    bytes: Vec<u8>,
    match_tier: PosterMatch,
}

async fn resolve_tvdb_id(
    tmdb: &crate::services::tmdb::TmdbClient,
    tmdb_id: u64,
) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct TvExternalIds {
        tvdb_id: Option<u64>,
    }
    #[derive(serde::Deserialize)]
    struct TvExtIds {
        external_ids: Option<TvExternalIds>,
    }
    let ext: Result<TvExtIds, _> = tmdb
        .get(
            &format!("/tv/{tmdb_id}"),
            &[("append_to_response", "external_ids")],
        )
        .await;
    ext.ok().and_then(|e| e.external_ids).and_then(|e| e.tvdb_id)
}

async fn fetch_fanart_poster(
    fanart: &FanartClient,
    tmdb: &crate::services::tmdb::TmdbClient,
    cache: &moka::future::Cache<String, Arc<Vec<crate::services::fanart::FanartPoster>>>,
    resolved: &id::ResolvedId,
    lang: &str,
    textless: bool,
) -> Option<FanartResult> {
    let (cache_key, posters_result) = match resolved.media_type {
        MediaType::Movie => {
            let key = format!("movie:{}", resolved.tmdb_id);
            let fanart = fanart.clone();
            let tmdb_id = resolved.tmdb_id;
            let posters = cache
                .try_get_with(key.clone(), async move {
                    let p = fanart.get_movie_posters(tmdb_id).await?;
                    Ok::<_, AppError>(Arc::new(p))
                })
                .await;
            (key, posters)
        }
        MediaType::Tv => {
            // Fanart.tv accepts TVDB, TMDB, or IMDb IDs for TV — prefer TVDB, fall back to TMDB.
            // Lazily resolve TVDB ID only when fanart is actually needed.
            let tv_id = match resolved.tvdb_id {
                Some(id) => id,
                None => resolve_tvdb_id(tmdb, resolved.tmdb_id).await.unwrap_or(resolved.tmdb_id),
            };
            let key = format!("tv:{tv_id}");
            let fanart = fanart.clone();
            let posters = cache
                .try_get_with(key.clone(), async move {
                    let p = fanart.get_tv_posters(tv_id).await?;
                    Ok::<_, AppError>(Arc::new(p))
                })
                .await;
            (key, posters)
        }
    };

    let posters = match posters_result {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, key = %cache_key, "failed to fetch fanart posters");
            return None;
        }
    };

    let (selected, match_tier) = FanartClient::select_poster(posters.as_ref(), lang, textless)?;
    let url = selected.url.clone();

    match fanart.fetch_poster_bytes(&url).await {
        Ok(bytes) => Some(FanartResult { bytes, match_tier }),
        Err(e) => {
            tracing::warn!(error = %e, url = %url, "failed to download fanart poster");
            None
        }
    }
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
