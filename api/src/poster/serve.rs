use axum::http::header;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{self, MemCacheEntry};
use crate::error::AppError;
use crate::id::{self, IdType, MediaType};
use crate::poster::generate;
use crate::services::db::{resolve_badge_direction, PosterSettings};
use crate::services::fanart::{FanartClient, FanartImages, FanartPoster, PosterMatch};
use crate::services::ratings;
use crate::AppState;

/// Which kind of image to select from the unified fanart cache.
#[derive(Debug, Clone, Copy)]
pub enum ImageKind {
    Poster,
    Logo,
    Backdrop,
}

/// Logo or backdrop — the subset of [`ImageKind`] served exclusively via fanart.tv.
/// Posters are excluded because they use a separate code path (`handle_inner`) with
/// TMDB fallback, staleness checks, and background refresh that these endpoints don't need.
#[derive(Debug, Clone, Copy)]
pub enum FanartImageKind {
    Logo,
    Backdrop,
}

impl From<FanartImageKind> for ImageKind {
    fn from(k: FanartImageKind) -> Self {
        match k {
            FanartImageKind::Logo => ImageKind::Logo,
            FanartImageKind::Backdrop => ImageKind::Backdrop,
        }
    }
}

impl From<ImageKind> for cache::ImageType {
    fn from(k: ImageKind) -> Self {
        match k {
            ImageKind::Poster => cache::ImageType::Poster,
            ImageKind::Logo => cache::ImageType::Logo,
            ImageKind::Backdrop => cache::ImageType::Backdrop,
        }
    }
}

impl From<FanartImageKind> for cache::ImageType {
    fn from(k: FanartImageKind) -> Self {
        match k {
            FanartImageKind::Logo => cache::ImageType::Logo,
            FanartImageKind::Backdrop => cache::ImageType::Backdrop,
        }
    }
}

impl ImageKind {
    fn kind_prefix(self) -> &'static str {
        match self {
            ImageKind::Poster => "",
            ImageKind::Logo => ":l",
            ImageKind::Backdrop => ":b",
        }
    }

    fn file_ext(self) -> &'static str {
        cache::ImageType::from(self).ext()
    }

    fn strip_ext(self, s: &str) -> &str {
        match self {
            ImageKind::Poster | ImageKind::Backdrop => s.strip_suffix(".jpg").unwrap_or(s),
            ImageKind::Logo => s.strip_suffix(".png").unwrap_or(s),
        }
    }

    fn label(self) -> &'static str {
        match self {
            ImageKind::Poster => "poster",
            ImageKind::Logo => "logo",
            ImageKind::Backdrop => "backdrop",
        }
    }
}

/// Returns a cache key suffix for poster position.
pub fn poster_position_cache_suffix(position: &str) -> String {
    let pos = if position.is_empty() { "bc" } else { position };
    format!(".p{pos}")
}

/// Returns a cache key suffix for badge style.
pub fn badge_style_cache_suffix(style: &str) -> String {
    format!(".s{style}")
}

/// Returns a cache key suffix for label style.
pub fn label_style_cache_suffix(style: &str) -> String {
    format!(".l{style}")
}

/// Returns a cache key suffix for badge direction.
pub fn badge_direction_cache_suffix(dir: &str) -> String {
    format!(".d{dir}")
}

pub async fn handle_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_jpg: &str,
    mut settings: PosterSettings,
) -> Result<Bytes, AppError> {
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    // Resolve "default" badge direction early, before cache key construction
    settings.poster_badge_direction = resolve_badge_direction(&settings.poster_badge_direction, &settings.poster_position);

    let use_fanart = settings.poster_source == "fanart";

    // Try the fanart path first; falls through to TMDB on miss
    if use_fanart {
        if let Some(bytes) = try_fanart_path(state, id_type_str, id_value, id_type, &settings).await? {
            return Ok(bytes);
        }
    }

    // TMDB path (default, or fanart fallback)
    if use_fanart {
        let mut defaults = PosterSettings::default();
        defaults.poster_badge_direction = resolve_badge_direction(&defaults.poster_badge_direction, &defaults.poster_position);
        settings = defaults;
    }
    let settings = &settings;
    let ratings_suffix = ratings::ratings_cache_suffix(&settings.ratings_order, settings.ratings_limit);
    let pos_suffix = poster_position_cache_suffix(&settings.poster_position);
    let bs_suffix = badge_style_cache_suffix(&settings.poster_badge_style);
    let ls_suffix = label_style_cache_suffix(&settings.poster_label_style);
    let bd_suffix = badge_direction_cache_suffix(&settings.poster_badge_direction);
    let cache_value = format!("{id_value}{ratings_suffix}{pos_suffix}{bs_suffix}{ls_suffix}{bd_suffix}");
    let cache_path = cache::typed_cache_path(&state.config.cache_dir, cache::ImageType::Poster, id_type_str, &cache_value)?;
    let cache_key = format!("{id_type_str}/{id_value}{ratings_suffix}{pos_suffix}{bs_suffix}{ls_suffix}{bd_suffix}");

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
            cache::upsert_meta_db(&state2.db, &cache_key2, rd.as_deref(), cache::ImageType::Poster).await?;
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

/// Build a fanart cache key and filesystem path from a variant suffix (e.g. ":f:tl").
fn fanart_variant_paths(
    cache_dir: &str,
    id_type_str: &str,
    id_value: &str,
    variant: &str,
    ratings_suffix: &str,
    pos_suffix: &str,
    bs_suffix: &str,
    ls_suffix: &str,
    bd_suffix: &str,
) -> Result<(String, std::path::PathBuf), AppError> {
    let cache_key = format!("{id_type_str}/{id_value}{variant}{ratings_suffix}{pos_suffix}{bs_suffix}{ls_suffix}{bd_suffix}");
    let path_variant = variant.replace(':', "_");
    let cache_path_base = format!("{id_value}{path_variant}{ratings_suffix}{pos_suffix}{bs_suffix}{ls_suffix}{bd_suffix}");
    let cache_path = cache::typed_cache_path(cache_dir, cache::ImageType::Poster, id_type_str, &cache_path_base)?;
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
    let neg_key = format!("{id_type_str}/{id_value}:f:tl:neg");
    let textless_known_missing = settings.fanart_textless
        && state.fanart_negative.get(&neg_key).await.is_some();

    let lang_variant = format!(":f:{}", settings.fanart_lang);
    let lang_neg_key = format!("{id_type_str}/{id_value}:f:{}:neg", settings.fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&lang_neg_key).await.is_some();

    // All fanart variants are known-missing — skip generation and fall through to TMDB
    if lang_known_missing && (!settings.fanart_textless || textless_known_missing) {
        return Ok(None);
    }

    // Compute ratings suffix once for all fanart variants
    let ratings_suffix = ratings::ratings_cache_suffix(&settings.ratings_order, settings.ratings_limit);
    let pos_suffix = poster_position_cache_suffix(&settings.poster_position);
    let bs_suffix = badge_style_cache_suffix(&settings.poster_badge_style);
    let ls_suffix = label_style_cache_suffix(&settings.poster_label_style);
    let bd_suffix = badge_direction_cache_suffix(&settings.poster_badge_direction);

    // Check cached variants (textless first if requested, then language)
    let mut variants_to_check: Vec<String> = Vec::new();
    if settings.fanart_textless && !textless_known_missing {
        variants_to_check.push(":f:tl".to_string());
    }
    if !lang_known_missing {
        variants_to_check.push(lang_variant.clone());
    }

    for variant in &variants_to_check {
        let (cache_key, cache_path) =
            fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, variant, &ratings_suffix, &pos_suffix, &bs_suffix, &ls_suffix, &bd_suffix)?;
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
                PosterMatch::Textless => ":f:tl".to_string(),
                PosterMatch::Language => format!(":f:{}", settings.fanart_lang),
            };
            let (cache_key, cache_path) =
                fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, &actual_variant, &ratings_suffix, &pos_suffix, &bs_suffix, &ls_suffix, &bd_suffix)?;
            let _ = cache::write(&cache_path, &bytes).await;
            let _ = cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref(), cache::ImageType::Poster).await;
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

/// Spawn a background refresh task. The `generate` future produces
/// `(image_bytes, release_date, image_type)` on success.
fn spawn_background_refresh<F>(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    generate: F,
)
where
    F: std::future::Future<Output = Result<(Vec<u8>, Option<String>, cache::ImageType), AppError>>
        + Send
        + 'static,
{
    if state.refresh_locks.contains_key(cache_key) {
        return;
    }
    state.refresh_locks.insert(cache_key.to_string(), ());
    let state = state.clone();
    let cache_path = cache_path.to_path_buf();
    let cache_key = cache_key.to_string();
    tokio::spawn(async move {
        tracing::info!(key = %cache_key, "background refresh started");
        match generate.await {
            Ok((bytes, rd, image_type)) => {
                if let Err(e) = cache::write(&cache_path, &bytes).await {
                    tracing::error!(error = %e, "failed to write cache");
                }
                if let Err(e) =
                    cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref(), image_type).await
                {
                    tracing::error!(error = %e, "failed to write meta to db");
                }
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

fn trigger_background_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    settings: &PosterSettings,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    spawn_background_refresh(state, cache_key, cache_path, async move {
        let (bytes, rd, _tier) =
            generate_poster_with_source(&state2, id_type, &id_value, &settings).await?;
        Ok((bytes, rd, cache::ImageType::Poster))
    });
}

fn trigger_fanart_background_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    settings: &PosterSettings,
    fanart_kind: FanartImageKind,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    spawn_background_refresh(state, cache_key, cache_path, async move {
        let fanart = state2
            .fanart
            .as_ref()
            .ok_or_else(|| AppError::Other("FANART_API_KEY not configured".into()))?;

        let kind: ImageKind = fanart_kind.into();
        let fanart_lang = match kind {
            ImageKind::Backdrop => "",
            _ => settings.fanart_lang.as_str(),
        };
        let fanart_textless = matches!(kind, ImageKind::Poster) && settings.fanart_textless;

        let resolved = id::resolve(id_type, &id_value, &state2.tmdb, &state2.id_cache).await?;

        let badges = ratings::fetch_ratings(
            &resolved,
            &state2.tmdb,
            state2.omdb.as_ref(),
            state2.mdblist.as_ref(),
            &state2.ratings_cache,
        )
        .await;
        let type_ratings_limit = match fanart_kind {
            FanartImageKind::Logo => settings.logo_ratings_limit,
            FanartImageKind::Backdrop => settings.backdrop_ratings_limit,
        };
        let badges = ratings::apply_rating_preferences(badges, &settings.ratings_order, type_ratings_limit);

        let fanart_result = fetch_fanart_image(
            fanart,
            &state2.tmdb,
            &state2.fanart_cache,
            &resolved,
            fanart_lang,
            fanart_textless,
            kind,
            &state2.config.cache_dir,
        )
        .await;

        let image_bytes = fanart_result
            .map(|r| r.bytes)
            .ok_or_else(|| AppError::Other("no fanart image available".into()))?;

        let image_type: cache::ImageType = fanart_kind.into();

        let type_badge_style = match fanart_kind {
            FanartImageKind::Logo => settings.logo_badge_style.clone(),
            FanartImageKind::Backdrop => settings.backdrop_badge_style.clone(),
        };
        let type_label_style = match fanart_kind {
            FanartImageKind::Logo => settings.logo_label_style.clone(),
            FanartImageKind::Backdrop => settings.backdrop_label_style.clone(),
        };

        let bytes = match fanart_kind {
            FanartImageKind::Logo => generate::generate_logo(image_bytes, badges, state2.font.clone(), type_badge_style, type_label_style).await?,
            FanartImageKind::Backdrop => generate::generate_backdrop(image_bytes, badges, state2.font.clone(), state2.config.poster_quality, type_badge_style, type_label_style).await?,
        };

        Ok((bytes, resolved.release_date, image_type))
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

    let badges = ratings::apply_rating_preferences(badges, &settings.ratings_order, settings.ratings_limit);

    // Try to fetch poster bytes from fanart.tv if configured
    let fanart_result = if settings.poster_source == "fanart" {
        if let Some(ref fanart) = state.fanart {
            fetch_fanart_image(
                fanart,
                &state.tmdb,
                &state.fanart_cache,
                &resolved,
                &settings.fanart_lang,
                settings.fanart_textless,
                ImageKind::Poster,
                &state.config.cache_dir,
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
        font: &state.font,
        quality: state.config.poster_quality,
        cache_dir: &state.config.cache_dir,
        poster_stale_secs: state.config.poster_stale_secs,
        poster_bytes_override: fanart_bytes,
        normalize_width: has_fanart,
        poster_position: settings.poster_position.clone(),
        badge_style: settings.poster_badge_style.clone(),
        label_style: settings.poster_label_style.clone(),
        badge_direction: settings.poster_badge_direction.clone(),
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

/// Fetch all fanart images (cached) and return the appropriate list for the given kind.
async fn fetch_fanart_images(
    fanart: &FanartClient,
    tmdb: &crate::services::tmdb::TmdbClient,
    cache: &moka::future::Cache<String, Arc<FanartImages>>,
    resolved: &id::ResolvedId,
) -> Option<Arc<FanartImages>> {
    let (cache_key, images_result) = match resolved.media_type {
        MediaType::Movie => {
            let key = format!("movie:{}", resolved.tmdb_id);
            let fanart = fanart.clone();
            let tmdb_id = resolved.tmdb_id;
            let images = cache
                .try_get_with(key.clone(), async move {
                    let imgs = fanart.get_movie_images(tmdb_id).await?;
                    Ok::<_, AppError>(Arc::new(imgs))
                })
                .await;
            (key, images)
        }
        MediaType::Tv => {
            let tv_id = match resolved.tvdb_id {
                Some(id) => id,
                None => resolve_tvdb_id(tmdb, resolved.tmdb_id).await.unwrap_or(resolved.tmdb_id),
            };
            let key = format!("tv:{tv_id}");
            let fanart = fanart.clone();
            let images = cache
                .try_get_with(key.clone(), async move {
                    let imgs = fanart.get_tv_images(tv_id).await?;
                    Ok::<_, AppError>(Arc::new(imgs))
                })
                .await;
            (key, images)
        }
    };

    match images_result {
        Ok(imgs) => Some(imgs),
        Err(e) => {
            tracing::warn!(error = %e, key = %cache_key, "failed to fetch fanart images");
            None
        }
    }
}

fn select_images_for_kind(images: &FanartImages, kind: ImageKind) -> &[FanartPoster] {
    match kind {
        ImageKind::Poster => &images.posters,
        ImageKind::Logo => &images.logos,
        ImageKind::Backdrop => &images.backdrops,
    }
}

async fn fetch_fanart_image(
    fanart: &FanartClient,
    tmdb: &crate::services::tmdb::TmdbClient,
    cache: &moka::future::Cache<String, Arc<FanartImages>>,
    resolved: &id::ResolvedId,
    lang: &str,
    textless: bool,
    kind: ImageKind,
    cache_dir: &str,
) -> Option<FanartResult> {
    let images = fetch_fanart_images(fanart, tmdb, cache, resolved).await?;
    let candidates = select_images_for_kind(&images, kind);

    let (selected, match_tier) = FanartClient::select_image(candidates, lang, textless)?;
    let url = selected.url.clone();
    let fanart_id = selected.id.clone();

    // Try to serve from base fanart cache
    let ext = kind.file_ext();
    let base_path = cache::base_fanart_path(cache_dir, &fanart_id, ext).ok()?;

    let bytes = match cache::read(&base_path, 0).await {
        Some(entry) => entry.bytes,
        None => {
            match fanart.fetch_poster_bytes(&url).await {
                Ok(fresh) => {
                    let _ = cache::write(&base_path, &fresh).await;
                    fresh
                }
                Err(e) => {
                    tracing::warn!(error = %e, url = %url, "failed to download fanart image");
                    return None;
                }
            }
        }
    };

    Some(FanartResult { bytes, match_tier })
}

pub fn jpeg_response(bytes: Bytes) -> Response {
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

pub fn png_response(bytes: Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (
                header::CACHE_CONTROL,
                "public, max-age=3600, stale-while-revalidate=86400",
            ),
        ],
        bytes,
    )
        .into_response()
}

/// Serve a fanart-only image (logo or backdrop). Handles caching and negative-cache lookups.
pub async fn handle_fanart_image_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &PosterSettings,
    fanart_kind: FanartImageKind,
) -> Result<Bytes, AppError> {
    let fanart = state
        .fanart
        .as_ref()
        .ok_or_else(|| AppError::Other("FANART_API_KEY not configured".into()))?;

    let kind: ImageKind = fanart_kind.into();
    let id_type = IdType::parse(id_type_str)?;
    let id_value = kind.strip_ext(id_value_raw);
    let kind_prefix = kind.kind_prefix();
    let label = kind.label();

    // Use per-type rating limit for logos/backdrops
    let type_ratings_limit = match fanart_kind {
        FanartImageKind::Logo => settings.logo_ratings_limit,
        FanartImageKind::Backdrop => settings.backdrop_ratings_limit,
    };
    let ratings_suffix = ratings::ratings_cache_suffix(&settings.ratings_order, type_ratings_limit);
    let type_badge_style = match fanart_kind {
        FanartImageKind::Logo => &settings.logo_badge_style,
        FanartImageKind::Backdrop => &settings.backdrop_badge_style,
    };
    let bs_suffix = badge_style_cache_suffix(type_badge_style);
    let type_label_style = match fanart_kind {
        FanartImageKind::Logo => &settings.logo_label_style,
        FanartImageKind::Backdrop => &settings.backdrop_label_style,
    };
    let ls_suffix = label_style_cache_suffix(type_label_style);

    // Backdrops are language-agnostic (no text) — skip lang/textless entirely.
    // Logos ARE the text — textless makes no sense, only lang matters.
    let fanart_lang = match kind {
        ImageKind::Backdrop => "",
        _ => settings.fanart_lang.as_str(),
    };
    let fanart_textless = matches!(kind, ImageKind::Poster) && settings.fanart_textless;

    // Check negative cache — skip generation if we already know there's nothing
    let neg_textless_key = format!("{id_type_str}/{id_value}{kind_prefix}:f:tl:neg");
    let textless_known_missing = fanart_textless
        && state.fanart_negative.get(&neg_textless_key).await.is_some();

    let neg_lang_key = format!("{id_type_str}/{id_value}{kind_prefix}:f:{}:neg", fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&neg_lang_key).await.is_some();

    if lang_known_missing && (!fanart_textless || textless_known_missing) {
        return Err(AppError::Other(format!("no {label} available").into()));
    }

    let variant = match kind {
        ImageKind::Backdrop => kind_prefix.to_string(),
        ImageKind::Logo => format!("{kind_prefix}:f:{fanart_lang}"),
        ImageKind::Poster => format!("{kind_prefix}:f:{fanart_lang}{}", if fanart_textless { ":tl" } else { "" }),
    };
    let image_type = match fanart_kind {
        FanartImageKind::Logo => cache::ImageType::Logo,
        FanartImageKind::Backdrop => cache::ImageType::Backdrop,
    };
    let cache_key = format!("{id_type_str}/{id_value}{variant}{ratings_suffix}{bs_suffix}{ls_suffix}");
    let path_variant = variant.replace(':', "_");
    let cache_path_base = format!("{id_value}{path_variant}{ratings_suffix}{bs_suffix}{ls_suffix}");
    let cache_path = cache::typed_cache_path(&state.config.cache_dir, image_type, id_type_str, &cache_path_base)?;

    // Check in-memory cache with staleness
    if let Some(entry) = state.poster_mem_cache.get(&cache_key).await {
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
                trigger_fanart_background_refresh(
                    state, &cache_key, &cache_path, id_type, id_value,
                    settings, fanart_kind,
                );
            }
            state
                .poster_mem_cache
                .insert(
                    cache_key.clone(),
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

    // Check filesystem cache with staleness
    if let Some(entry) = cache::read(&cache_path, stale_secs).await {
        if entry.is_stale {
            trigger_fanart_background_refresh(
                state, &cache_key, &cache_path, id_type, id_value,
                settings, fanart_kind,
            );
        }
        let bytes: Bytes = entry.bytes.into();
        state
            .poster_mem_cache
            .insert(cache_key, MemCacheEntry { bytes: bytes.clone(), last_checked: Instant::now() })
            .await;
        return Ok(bytes);
    }

    // Generate: resolve ID, fetch ratings, fetch image from fanart
    let resolved = id::resolve(id_type, id_value, &state.tmdb, &state.id_cache).await?;

    let badges = ratings::fetch_ratings(
        &resolved,
        &state.tmdb,
        state.omdb.as_ref(),
        state.mdblist.as_ref(),
        &state.ratings_cache,
    )
    .await;
    let badges = ratings::apply_rating_preferences(badges, &settings.ratings_order, type_ratings_limit);

    let fanart_result = fetch_fanart_image(
        fanart,
        &state.tmdb,
        &state.fanart_cache,
        &resolved,
        fanart_lang,
        fanart_textless,
        kind,
        &state.config.cache_dir,
    )
    .await;

    let image_bytes = match fanart_result {
        Some(r) => {
            // Record negative for textless if we only got a language match
            if fanart_textless && r.match_tier == PosterMatch::Language {
                state.fanart_negative.insert(neg_textless_key, ()).await;
            }
            r.bytes
        }
        None => {
            // No match at all — record negatives so we don't re-query
            if fanart_textless {
                state.fanart_negative.insert(neg_textless_key, ()).await;
            }
            state.fanart_negative.insert(neg_lang_key, ()).await;
            return Err(AppError::Other(format!("no {label} available").into()));
        }
    };

    let bytes = match fanart_kind {
        FanartImageKind::Logo => generate::generate_logo(image_bytes, badges, state.font.clone(), type_badge_style.clone(), type_label_style.clone()).await?,
        FanartImageKind::Backdrop => generate::generate_backdrop(image_bytes, badges, state.font.clone(), state.config.poster_quality, type_badge_style.clone(), type_label_style.clone()).await?,
    };

    let _ = cache::write(&cache_path, &bytes).await;
    let _ = cache::upsert_meta_db(&state.db, &cache_key, resolved.release_date.as_deref(), image_type).await;
    let bytes = Bytes::from(bytes);
    state
        .poster_mem_cache
        .insert(cache_key, MemCacheEntry { bytes: bytes.clone(), last_checked: Instant::now() })
        .await;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_kind_prefix() {
        assert_eq!(ImageKind::Poster.kind_prefix(), "");
        assert_eq!(ImageKind::Logo.kind_prefix(), ":l");
        assert_eq!(ImageKind::Backdrop.kind_prefix(), ":b");
    }

    #[test]
    fn image_kind_file_ext() {
        assert_eq!(ImageKind::Poster.file_ext(), "jpg");
        assert_eq!(ImageKind::Logo.file_ext(), "png");
        assert_eq!(ImageKind::Backdrop.file_ext(), "jpg");
    }

    #[test]
    fn image_kind_strip_ext() {
        assert_eq!(ImageKind::Poster.strip_ext("tt123.jpg"), "tt123");
        assert_eq!(ImageKind::Poster.strip_ext("tt123"), "tt123");
        assert_eq!(ImageKind::Logo.strip_ext("tt123.png"), "tt123");
        assert_eq!(ImageKind::Logo.strip_ext("tt123"), "tt123");
        assert_eq!(ImageKind::Backdrop.strip_ext("tt123.jpg"), "tt123");
        assert_eq!(ImageKind::Backdrop.strip_ext("tt123"), "tt123");
        // Wrong extension is not stripped
        assert_eq!(ImageKind::Logo.strip_ext("tt123.jpg"), "tt123.jpg");
        assert_eq!(ImageKind::Poster.strip_ext("tt123.png"), "tt123.png");
    }

    #[test]
    fn image_kind_label() {
        assert_eq!(ImageKind::Poster.label(), "poster");
        assert_eq!(ImageKind::Logo.label(), "logo");
        assert_eq!(ImageKind::Backdrop.label(), "backdrop");
    }

    #[test]
    fn poster_position_cache_suffix_all_positions() {
        assert_eq!(poster_position_cache_suffix("bc"), ".pbc");
        assert_eq!(poster_position_cache_suffix(""), ".pbc");
        assert_eq!(poster_position_cache_suffix("tc"), ".ptc");
        assert_eq!(poster_position_cache_suffix("l"), ".pl");
        assert_eq!(poster_position_cache_suffix("r"), ".pr");
        assert_eq!(poster_position_cache_suffix("tl"), ".ptl");
        assert_eq!(poster_position_cache_suffix("tr"), ".ptr");
        assert_eq!(poster_position_cache_suffix("bl"), ".pbl");
        assert_eq!(poster_position_cache_suffix("br"), ".pbr");
    }

    #[test]
    fn badge_style_cache_suffix_values() {
        assert_eq!(badge_style_cache_suffix("h"), ".sh");
        assert_eq!(badge_style_cache_suffix("v"), ".sv");
    }

    #[test]
    fn label_style_cache_suffix_values() {
        assert_eq!(label_style_cache_suffix("t"), ".lt");
        assert_eq!(label_style_cache_suffix("i"), ".li");
    }
}
