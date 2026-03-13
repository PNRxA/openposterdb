use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use std::time::Instant;

use crate::cache::{self, MemCacheEntry};
use crate::error::AppError;
use crate::id::{self, IdType, MediaType, format_tmdb_id_value};
use crate::poster::generate;
use crate::services::db::{resolve_badge_direction, resolve_badge_style, ImageSize, PosterSettings, POS_BOTTOM_CENTER, SOURCE_FANART};
use crate::services::fanart::{FanartClient, FanartImages, FanartPoster, PosterMatch};
use crate::services::ratings;
use crate::AppState;

/// Hardcoded CDN TTL for placeholder/fallback images (1 day).
pub const PLACEHOLDER_CDN_MAX_AGE: u64 = 86400;

/// Threshold (ms) above which requests are logged as slow.
const SLOW_REQUEST_MS: u64 = 2000;

/// Alias for the unified image kind enum used throughout the serve layer.
pub type ImageKind = cache::ImageType;

/// Logo or backdrop — the subset of image kinds served exclusively via fanart.tv.
/// Posters are excluded because they use a separate code path (`handle_inner`) with
/// TMDB fallback, staleness checks, and background refresh that these endpoints don't need.
#[derive(Debug, Clone, Copy)]
pub enum FanartImageKind {
    Logo,
    Backdrop,
}

impl From<FanartImageKind> for cache::ImageType {
    fn from(k: FanartImageKind) -> Self {
        match k {
            FanartImageKind::Logo => cache::ImageType::Logo,
            FanartImageKind::Backdrop => cache::ImageType::Backdrop,
        }
    }
}

/// Returns a cache key suffix for poster position.
pub fn poster_position_cache_suffix(position: &str) -> String {
    let pos = if position.is_empty() { POS_BOTTOM_CENTER } else { position };
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

/// Resolve an optional image size, defaulting to Medium.
pub fn resolve_image_size(size: Option<ImageSize>) -> ImageSize {
    size.unwrap_or(ImageSize::Medium)
}

/// Returns a cache key suffix for image size.
pub fn image_size_cache_suffix(size: Option<ImageSize>) -> &'static str {
    resolve_image_size(size).cache_suffix()
}

/// Build the cache suffix string from settings for a given image kind.
///
/// Exhaustively destructures `PosterSettings` so adding a field without
/// handling it here produces a compile error.
///
/// Uses `ratings_cache_suffix()` to predict the ratings portion from user
/// settings. For cache keys that reflect *actual* rendered badges, use
/// `settings_cache_suffix_with_ratings()` with a pre-computed ratings suffix.
pub fn settings_cache_suffix(
    settings: &PosterSettings,
    kind: ImageKind,
    image_size: Option<ImageSize>,
) -> String {
    let ratings_suffix = match kind {
        ImageKind::Poster => ratings::ratings_cache_suffix(&settings.ratings_order, settings.ratings_limit),
        ImageKind::Logo => ratings::ratings_cache_suffix(&settings.ratings_order, settings.logo_ratings_limit),
        ImageKind::Backdrop => ratings::ratings_cache_suffix(&settings.ratings_order, settings.backdrop_ratings_limit),
    };
    settings_cache_suffix_with_ratings(settings, kind, image_size, &ratings_suffix)
}

/// Build the cache suffix string using a pre-computed ratings suffix.
///
/// This variant accepts the `@xyz` ratings portion directly (e.g. from
/// `badges_cache_suffix()`) so callers can use the *actual* badge sources
/// rather than the predicted ones from user settings.
pub fn settings_cache_suffix_with_ratings(
    settings: &PosterSettings,
    kind: ImageKind,
    image_size: Option<ImageSize>,
    ratings_suffix: &str,
) -> String {
    let PosterSettings {
        poster_source: _,       // handled by code path selection, not suffix
        fanart_lang: _,         // handled by variant string, not suffix
        fanart_textless: _,     // handled by variant string, not suffix
        ratings_limit: _,
        ratings_order: _,
        is_default: _,          // metadata, not a render setting
        poster_position,
        logo_ratings_limit: _,
        backdrop_ratings_limit: _,
        poster_badge_style,
        logo_badge_style,
        backdrop_badge_style,
        poster_label_style,
        logo_label_style,
        backdrop_label_style,
        poster_badge_direction,
        lang_override: _,       // handled by code path, not suffix
    } = settings;

    let resolved_size = resolve_image_size(image_size);
    let is_suffix = resolved_size.cache_suffix();
    let rs = ratings_suffix;

    match kind {
        ImageKind::Poster => {
            let ps = poster_position_cache_suffix(poster_position);
            let bs = badge_style_cache_suffix(poster_badge_style);
            let ls = label_style_cache_suffix(poster_label_style);
            let bd = badge_direction_cache_suffix(poster_badge_direction);
            format!("{rs}{ps}{bs}{ls}{bd}{is_suffix}")
        }
        ImageKind::Logo => {
            let bs = badge_style_cache_suffix(logo_badge_style);
            let ls = label_style_cache_suffix(logo_label_style);
            format!("{rs}{bs}{ls}{is_suffix}")
        }
        ImageKind::Backdrop => {
            let bs = badge_style_cache_suffix(backdrop_badge_style);
            let ls = label_style_cache_suffix(backdrop_label_style);
            format!("{rs}{bs}{ls}{is_suffix}")
        }
    }
}

/// Compute a stable 12-hex-char settings hash for CDN content-addressed URLs.
/// Two users with identical effective settings for the same image type produce
/// the same hash, enabling Cloudflare cache deduplication.
///
/// **Important:** Every field that affects rendered output must be included here.
/// When adding new settings, add them to the hash to prevent CDN cache collisions.
pub fn settings_hash(settings: &PosterSettings, kind: ImageKind, image_size: Option<ImageSize>) -> String {
    let mut hasher = Sha256::new();

    hasher.update(kind.label().as_bytes());
    hasher.update(b"\0");

    // Render-affecting settings (via exhaustive destructure in settings_cache_suffix)
    hasher.update(settings_cache_suffix(settings, kind, image_size).as_bytes());
    hasher.update(b"\0");

    // Source-selection settings (not in cache suffix because handled by code path/variant)
    hasher.update(settings.poster_source.as_bytes());
    hasher.update(b"\0");
    hasher.update(settings.fanart_lang.as_bytes());
    hasher.update(b"\0");
    hasher.update(if settings.fanart_textless { b"1" } else { b"0" });
    hasher.update(b"\0");
    hasher.update(if settings.lang_override { b"1" } else { b"0" });

    let hash = hasher.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
        hash[8], hash[9], hash[10], hash[11], hash[12], hash[13], hash[14], hash[15]
    )
}

/// 302 redirect response for CDN content-addressed URLs.
/// Compute `max-age` for CDN responses based on film age.
///
/// New/unreleased films change frequently (ratings settling) → short TTL.
/// Old films are stable → long TTL.  Maps the same release-date logic used
/// for internal cache staleness to CDN-appropriate durations:
///
/// - Unreleased / unknown release date → 1 day
/// - Just released → 1 day
/// - Linearly scales up to 1 year as film age approaches `max_age_secs`
/// - Older than `max_age_secs` → 1 year
pub fn compute_cdn_max_age(release_date: Option<&str>, min_stale_secs: u64, max_age_secs: u64) -> u64 {
    let stale = cache::compute_stale_secs(release_date, min_stale_secs, max_age_secs);
    if stale == 0 {
        // Film older than max_age — ratings are stable, cache for 1 year
        365 * 24 * 3600
    } else {
        // Use the staleness interval as the CDN TTL — the image won't be
        // regenerated before then anyway, so it's safe to cache that long.
        stale
    }
}

/// `public` lets the CDN cache the redirect at the edge so it can be served
/// during origin downtime.  The cache is keyed by the full URL (which includes
/// the API key), so one user's redirect is never served to another.
/// `stale-while-revalidate` allows the edge to keep serving the cached redirect
/// while the origin is unreachable.
pub fn cdn_redirect_response(location: &str) -> Response {
    (
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "public, max-age=300, stale-while-revalidate=3600"),
        ],
    )
        .into_response()
}

/// Image response with dynamic CDN cache TTL for content-addressed `/c/` routes.
pub fn cdn_image_response(bytes: Bytes, max_age: u64, content_type: &'static str) -> Response {
    let swr = max_age.saturating_mul(7);
    let cache_control = format!("public, max-age={max_age}, stale-while-revalidate={swr}");
    (
        [
            (header::CONTENT_TYPE, header::HeaderValue::from_static(content_type)),
            // SAFETY: the format string above only produces ASCII digits and commas.
            (header::CACHE_CONTROL, header::HeaderValue::from_str(&cache_control).unwrap()),
        ],
        bytes,
    )
        .into_response()
}

/// Read available rating sources for a movie, checking the in-memory cache
/// before falling through to SQLite.
async fn read_available_ratings_cached(state: &AppState, id_key: &str) -> Option<String> {
    let db = state.db.clone();
    let min_stale = state.config.ratings_min_stale_secs;
    let max_age = state.config.ratings_max_age_secs;
    let id_key_owned = id_key.to_owned();
    state
        .available_ratings_cache
        .try_get_with(id_key.to_string(), async move {
            Ok::<_, std::convert::Infallible>(
                cache::read_available_ratings(&db, &id_key_owned, min_stale, max_age).await,
            )
        })
        .await
        .unwrap_or(None)
}

/// Persist available sources to SQLite and update the in-memory cache.
async fn upsert_available_ratings_cached(
    state: &AppState,
    id_key: &str,
    sources: &str,
    release_date: Option<&str>,
) {
    if let Err(e) = cache::upsert_available_ratings(&state.db, id_key, sources, release_date).await {
        tracing::warn!(error = %e, key = %id_key, "available_ratings upsert failed");
    }
    // Update the in-memory cache so subsequent requests don't hit SQLite
    state
        .available_ratings_cache
        .insert(id_key.to_string(), Some(sources.to_string()))
        .await;
}

/// Resolve an ID and fetch its ratings in one step. Returns the resolved ID,
/// raw ratings result, and cross-ID info. Callers apply their own rating
/// preferences via `ratings::apply_rating_preferences`.
async fn resolve_with_ratings(
    state: &AppState,
    id_type: IdType,
    id_value: &str,
) -> Result<(id::ResolvedId, ratings::RatingsResult, CrossIdInfo), AppError> {
    let id_resolve_start = Instant::now();
    let resolved = id::resolve(id_type, id_value, &state.tmdb, &state.id_cache).await?;
    let id_resolve_ms = id_resolve_start.elapsed().as_millis() as u64;

    let ratings_result = ratings::fetch_ratings(
        &resolved,
        &state.tmdb,
        state.omdb.as_ref(),
        state.mdblist.as_ref(),
        &state.ratings_cache,
    )
    .await;

    if id_resolve_ms > SLOW_REQUEST_MS {
        tracing::warn!(
            id = %id_value,
            id_resolve_ms,
            "slow id resolution"
        );
    }

    let cross_ids = CrossIdInfo::from_resolved(&resolved, &ratings_result);
    Ok((resolved, ratings_result, cross_ids))
}

/// IDs available for cross-ID cache population, built from the resolved ID
/// with optional backfill from MDBList ratings response.
#[derive(Clone)]
struct CrossIdInfo {
    imdb_id: Option<String>,
    tmdb_id: u64,
    tvdb_id: Option<u64>,
    media_type: MediaType,
    release_date: Option<String>,
}

impl CrossIdInfo {
    /// Build from a resolved ID, merging in any extra IDs from the ratings response.
    fn from_resolved(resolved: &id::ResolvedId, ratings: &ratings::RatingsResult) -> Self {
        Self {
            imdb_id: resolved.imdb_id.clone().or_else(|| ratings.imdb_id.clone()),
            tmdb_id: resolved.tmdb_id,
            tvdb_id: resolved.tvdb_id.or(ratings.tvdb_id),
            media_type: resolved.media_type,
            release_date: resolved.release_date.clone(),
        }
    }
}

/// Spawn a background task to write cache entries for all alternate IDs.
/// Uses `CrossIdInfo` to determine alternate ID paths.
/// All writes are best-effort — errors are logged but not propagated.
/// Does NOT populate memory cache; alternate keys get promoted on first actual request.
fn spawn_cross_id_cache(
    state: &AppState,
    cross_ids: CrossIdInfo,
    id_type: IdType,
    cache_suffix: String,
    image_type: cache::ImageType,
    bytes: Bytes,
) {
    let permit = match state.cross_id_semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            tracing::debug!("cross-id cache skipped: semaphore full");
            return;
        }
    };
    let state = state.clone();
    tokio::spawn(async move {
        let _permit = permit;

        // Build list of (id_type_str, id_value) for alternate IDs
        let mut alternates: Vec<(&str, String)> = Vec::new();

        if let Some(ref imdb_id) = cross_ids.imdb_id {
            if id_type != IdType::Imdb {
                alternates.push(("imdb", imdb_id.clone()));
            }
        }
        {
            let tmdb_val = format_tmdb_id_value(cross_ids.tmdb_id, &cross_ids.media_type);
            if id_type != IdType::Tmdb {
                alternates.push(("tmdb", tmdb_val));
            }
        }
        if let Some(tvdb_id) = cross_ids.tvdb_id {
            if id_type != IdType::Tvdb {
                alternates.push(("tvdb", tvdb_id.to_string()));
            }
        }

        let mut set = tokio::task::JoinSet::new();
        for (alt_type, alt_value) in &alternates {
            let cache_value = format!("{alt_value}{cache_suffix}");
            let alt_cache_path = match cache::typed_cache_path(&state.config.cache_dir, image_type, alt_type, &cache_value) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = %e, alt_type, alt_value, "cross-id cache path failed");
                    continue;
                }
            };
            let alt_cache_key = format!("{alt_type}/{alt_value}{cache_suffix}");

            let state = state.clone();
            let bytes = bytes.clone();
            let release_date = cross_ids.release_date.clone();
            let external_cache_only = state.config.external_cache_only;
            set.spawn(async move {
                if !external_cache_only {
                    if let Err(e) = cache::write(&alt_cache_path, &bytes).await {
                        tracing::warn!(error = %e, key = %alt_cache_key, "cross-id cache write failed");
                    }
                }
                if let Err(e) = cache::upsert_meta_db(&state.db, &alt_cache_key, release_date.as_deref(), image_type).await {
                    tracing::warn!(error = %e, key = %alt_cache_key, "cross-id meta write failed");
                }
            });
        }
        while let Some(result) = set.join_next().await {
            if let Err(e) = result {
                tracing::error!(error = %e, "cross-id cache task panicked");
            }
        }
    });
}

/// Check in-memory and filesystem caches for a cached image, triggering a
/// background refresh when the entry is stale.  Returns `Ok(Some(bytes))` on
/// cache hit, `Ok(None)` on miss.
///
/// `on_stale` is called when a stale entry is found — it should spawn a
/// background refresh task.
async fn check_caches(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    on_stale: impl Fn(&AppState, &str, &std::path::Path),
) -> Result<Option<Bytes>, AppError> {
    // Check in-memory cache
    if let Some(entry) = state.poster_mem_cache.get(cache_key).await {
        if !state.config.external_cache_only
            && entry.last_checked.elapsed() >= std::time::Duration::from_secs(60)
        {
            let release_date = cache::read_meta_db(&state.db, cache_key).await;
            let stale_secs = cache::compute_stale_secs(
                release_date.as_deref(),
                state.config.ratings_min_stale_secs,
                state.config.ratings_max_age_secs,
            );
            if let Some(fs_entry) = cache::read(cache_path, stale_secs).await
                && fs_entry.is_stale
            {
                on_stale(state, cache_key, cache_path);
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

    // No filesystem cache when external_cache_only — no files or metadata on disk
    if state.config.external_cache_only {
        return Ok(None);
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
            on_stale(state, cache_key, cache_path);
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

pub async fn handle_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_jpg: &str,
    mut settings: PosterSettings,
    image_size: Option<ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    let request_start = Instant::now();
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    // Reject path traversal, null bytes, etc. before any network calls
    cache::validate_id_value(id_value)?;

    // Resolve "default" badge direction and style early, before cache key construction
    settings.poster_badge_direction = resolve_badge_direction(&settings.poster_badge_direction, &settings.poster_position);
    settings.poster_badge_style = resolve_badge_style(&settings.poster_badge_style, &settings.poster_badge_direction);

    let use_fanart = &*settings.poster_source == SOURCE_FANART || settings.lang_override;
    let id_key = format!("{id_type_str}/{id_value}");

    // Fast path (non-fanart): try to reconstruct the cache key from SQLite-stored
    // available sources, avoiding external API calls entirely on cache hits.
    if !use_fanart {
        let fast_path_start = Instant::now();
        if let Some(available) = read_available_ratings_cached(state, &id_key).await {
            let available_ratings_ms = fast_path_start.elapsed().as_millis() as u64;
            let ratings_suffix = ratings::badges_suffix_from_available(&available, &settings.ratings_order, settings.ratings_limit);
            let suffix = settings_cache_suffix_with_ratings(&settings, ImageKind::Poster, image_size, &ratings_suffix);
            let cache_value = format!("{id_value}{suffix}");
            let cache_path = cache::typed_cache_path(&state.config.cache_dir, cache::ImageType::Poster, id_type_str, &cache_value)?;
            let cache_key = format!("{id_type_str}/{cache_value}");

            let cache_suffix: Arc<str> = suffix.into();
            {
                let id_value = id_value.to_string();
                let cache_suffix = cache_suffix.clone();
                let settings = settings.clone();
                let cache_check_start = Instant::now();
                if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
                    trigger_background_refresh(s, k, p, id_type, &id_value, &cache_suffix, &settings, image_size);
                }).await? {
                    let cache_check_ms = cache_check_start.elapsed().as_millis() as u64;
                    let meta_start = Instant::now();
                    let release_date = cache::read_meta_db(&state.db, &cache_key).await;
                    let meta_ms = meta_start.elapsed().as_millis() as u64;
                    let total_ms = request_start.elapsed().as_millis() as u64;
                    if total_ms > SLOW_REQUEST_MS {
                        tracing::warn!(
                            id = %id_key,
                            total_ms,
                            available_ratings_ms,
                            cache_check_ms,
                            meta_db_ms = meta_ms,
                            "slow poster request (fast path hit)"
                        );
                    }
                    return Ok((bytes, release_date));
                }
            }
            let total_fast_path_ms = fast_path_start.elapsed().as_millis() as u64;
            if total_fast_path_ms > SLOW_REQUEST_MS {
                tracing::warn!(
                    id = %id_key,
                    total_fast_path_ms,
                    available_ratings_ms,
                    "slow fast path — cache miss, falling to slow path"
                );
            }
        }
    }

    // Slow path: resolve ID and fetch ratings (moka-cached, so still fast on
    // repeat requests within the 30-min TTL).
    let slow_path_start = Instant::now();
    let resolve_start = Instant::now();
    let (resolved, ratings_result, cross_ids) =
        resolve_with_ratings(state, id_type, id_value).await?;
    let resolve_ms = resolve_start.elapsed().as_millis() as u64;

    // Persist available sources for future fast-path lookups (always write,
    // even with external_cache_only — this is an optimization index, not a
    // disk cache, and the fast path depends on it).
    {
        let sources = ratings::available_sources_string(&ratings_result.badges);
        upsert_available_ratings_cached(state, &id_key, &sources, cross_ids.release_date.as_deref()).await;
    }

    // Fanart → TMDB fallback strategy:
    //
    // 1. If the user's source is fanart, or `?lang=` was provided, try the fanart
    //    path first. On hit, return immediately.
    //
    // 2. On fanart miss, fall through to TMDB. The settings used for TMDB depend
    //    on *why* we tried fanart:
    //    a. `?lang=` on a TMDB user — preserve the user's original settings
    //       (badge style, position, etc.) for the TMDB fallback; just clear
    //       the lang_override flag so we don't re-enter the fanart path.
    //    b. Fanart-source user — reset to defaults, since their per-key settings
    //       (e.g. fanart-specific lang/textless) don't apply to TMDB posters.
    if use_fanart {
        if let Some(bytes) = try_fanart_path(state, id_type_str, id_value, id_type, &resolved, &ratings_result, &cross_ids, &settings, image_size).await? {
            return Ok((bytes, cross_ids.release_date));
        }
    }

    // TMDB path (default, or fanart fallback)
    if use_fanart {
        if settings.lang_override && &*settings.poster_source != SOURCE_FANART {
            settings.lang_override = false;
        } else {
            let mut defaults = PosterSettings::default();
            defaults.poster_badge_direction = resolve_badge_direction(&defaults.poster_badge_direction, &defaults.poster_position);
            defaults.poster_badge_style = resolve_badge_style(&defaults.poster_badge_style, &defaults.poster_badge_direction);
            settings = defaults;
        }
    }
    let settings = &settings;

    let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);

    let suffix = settings_cache_suffix_with_ratings(settings, ImageKind::Poster, image_size, &ratings_suffix);
    let cache_value = format!("{id_value}{suffix}");
    let cache_path = cache::typed_cache_path(&state.config.cache_dir, cache::ImageType::Poster, id_type_str, &cache_value)?;
    let cache_key = format!("{id_type_str}/{cache_value}");

    // Check caches (memory → filesystem)
    let cache_suffix: Arc<str> = suffix.into();
    let release_date = cross_ids.release_date.clone();
    {
        let id_type = id_type;
        let id_value = id_value.to_string();
        let cache_suffix = cache_suffix.clone();
        let settings = settings.clone();
        let slow_cache_check_start = Instant::now();
        if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
            trigger_background_refresh(s, k, p, id_type, &id_value, &cache_suffix, &settings, image_size);
        }).await? {
            let total_ms = request_start.elapsed().as_millis() as u64;
            if total_ms > SLOW_REQUEST_MS {
                tracing::warn!(
                    id = %id_key,
                    total_ms,
                    resolve_ms,
                    cache_check_ms = slow_cache_check_start.elapsed().as_millis() as u64,
                    "slow poster request (slow path cache hit)"
                );
            }
            return Ok((bytes, release_date));
        }
    }

    // Request coalescing — concurrent requests for the same poster share one generation
    let generate_start = Instant::now();
    let state2 = state.clone();
    let cache_key2 = cache_key.clone();
    let cache_path2 = cache_path.clone();
    let settings2 = settings.clone();
    let bytes: Bytes = state
        .poster_inflight
        .try_get_with(cache_key.clone(), async move {
            let (bytes, rd, _used_fanart, gen_cross_ids) =
                generate_poster_with_source(&state2, &resolved, badges, &cross_ids, &settings2, image_size).await?;
            if !state2.config.external_cache_only {
                cache::write(&cache_path2, &bytes).await?;
            }
            cache::upsert_meta_db(&state2.db, &cache_key2, rd.as_deref(), cache::ImageType::Poster).await?;
            let bytes = Bytes::from(bytes);
            spawn_cross_id_cache(&state2, gen_cross_ids, id_type, cache_suffix.to_string(), cache::ImageType::Poster, bytes.clone());
            Ok::<_, AppError>(bytes)
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    let total_ms = request_start.elapsed().as_millis() as u64;
    if total_ms > SLOW_REQUEST_MS {
        tracing::warn!(
            id = %id_key,
            total_ms,
            resolve_ms,
            generate_ms = generate_start.elapsed().as_millis() as u64,
            slow_path_ms = slow_path_start.elapsed().as_millis() as u64,
            "slow poster request (generated)"
        );
    }

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
    Ok((bytes, release_date))
}

/// Check a single fanart cache variant (memory → filesystem).
/// Returns `Ok(Some(bytes))` on cache hit, `Ok(None)` on miss.
async fn check_fanart_cache_variant(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    cache_suffix: &str,
    settings: &PosterSettings,
    image_size: Option<ImageSize>,
) -> Result<Option<Bytes>, AppError> {
    let id_value = id_value.to_string();
    let cache_suffix = cache_suffix.to_string();
    let settings = settings.clone();
    check_caches(state, cache_key, cache_path, |s, k, p| {
        trigger_background_refresh(s, k, p, id_type, &id_value, &cache_suffix, &settings, image_size);
    }).await
}

/// Build a fanart cache key and filesystem path from a variant suffix (e.g. "_f_tl").
fn fanart_variant_paths(
    cache_dir: &str,
    id_type_str: &str,
    id_value: &str,
    variant: &str,
    suffix: &str,
) -> Result<(String, std::path::PathBuf), AppError> {
    let cache_key = format!("{id_type_str}/{id_value}{variant}{suffix}");
    let cache_path_base = format!("{id_value}{variant}{suffix}");
    let cache_path = cache::typed_cache_path(cache_dir, cache::ImageType::Poster, id_type_str, &cache_path_base)?;
    Ok((cache_key, cache_path))
}

/// Try to serve a poster from the fanart cache (memory → filesystem → fresh generation).
/// Returns `Ok(Some(bytes))` on hit, `Ok(None)` to fall through to TMDB, or `Err` on hard failure.
///
/// Accepts pre-resolved data from the caller to avoid duplicate resolve+fetch work.
async fn try_fanart_path(
    state: &AppState,
    id_type_str: &str,
    id_value: &str,
    id_type: IdType,
    resolved: &id::ResolvedId,
    ratings_result: &ratings::RatingsResult,
    cross_ids: &CrossIdInfo,
    settings: &PosterSettings,
    image_size: Option<ImageSize>,
) -> Result<Option<Bytes>, AppError> {
    // Build the list of cache variants to check.
    // When textless is requested but we know it's unavailable (negative cache),
    // skip the textless key and go straight to language.
    let neg_key = format!("{id_type_str}/{id_value}_f_tl_neg");
    let textless_known_missing = settings.fanart_textless
        && state.fanart_negative.get(&neg_key).await.is_some();

    let lang_variant = format!("_f_{}", settings.fanart_lang);
    let lang_neg_key = format!("{id_type_str}/{id_value}_f_{}_neg", settings.fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&lang_neg_key).await.is_some();

    // All fanart variants are known-missing — skip generation and fall through to TMDB
    if lang_known_missing && (!settings.fanart_textless || textless_known_missing) {
        return Ok(None);
    }

    let badges = ratings::apply_rating_preferences(ratings_result.badges.clone(), &settings.ratings_order, settings.ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);

    // Compute settings suffix once for all fanart variants
    let suffix = settings_cache_suffix_with_ratings(settings, ImageKind::Poster, image_size, &ratings_suffix);

    // Check cached variants (textless first if requested, then language)
    let mut variants_to_check: Vec<String> = Vec::new();
    if settings.fanart_textless && !textless_known_missing {
        variants_to_check.push("_f_tl".to_string());
    }
    if !lang_known_missing {
        variants_to_check.push(lang_variant.clone());
    }

    for variant in &variants_to_check {
        let (cache_key, cache_path) =
            fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, variant, &suffix)?;
        let variant_cache_suffix = format!("{variant}{suffix}");
        if let Some(bytes) =
            check_fanart_cache_variant(state, &cache_key, &cache_path, id_type, id_value, &variant_cache_suffix, settings, image_size).await?
        {
            return Ok(Some(bytes));
        }
    }

    // No cache hit — generate with fanart. Cache under the key matching the actual
    // tier used (textless vs language). If no fanart match, fall through to TMDB.
    let result = generate_poster_with_source(state, resolved, badges, cross_ids, settings, image_size).await;

    match result {
        Ok((bytes, rd, Some(tier), cross_ids)) => {
            if settings.fanart_textless && tier == PosterMatch::Language {
                state.fanart_negative.insert(neg_key, ()).await;
            }

            let actual_variant = match tier {
                PosterMatch::Textless => "_f_tl".to_string(),
                PosterMatch::Language => format!("_f_{}", settings.fanart_lang),
            };
            let (cache_key, cache_path) =
                fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, &actual_variant, &suffix)?;
            if !state.config.external_cache_only {
                let _ = cache::write(&cache_path, &bytes).await;
            }
            let _ = cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref(), cache::ImageType::Poster).await;
            let fanart_cache_suffix = format!("{actual_variant}{suffix}");
            let bytes = Bytes::from(bytes);
            spawn_cross_id_cache(state, cross_ids, id_type, fanart_cache_suffix, cache::ImageType::Poster, bytes.clone());
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
        Ok((_bytes, _rd, None, _cross_ids)) => {
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
/// `(image_bytes, release_date, image_type, cross_id_info)` on success.
/// `cross_id` optionally provides (id_type, cache_suffix) for cross-ID cache writes.
fn spawn_background_refresh<F>(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    cross_id: Option<(IdType, String)>,
    generate: F,
)
where
    F: std::future::Future<Output = Result<(Vec<u8>, Option<String>, cache::ImageType, CrossIdInfo), AppError>>
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
            Ok((bytes, rd, image_type, cross_ids)) => {
                if !state.config.external_cache_only {
                    if let Err(e) = cache::write(&cache_path, &bytes).await {
                        tracing::error!(error = %e, "failed to write cache");
                    }
                }
                if let Err(e) =
                    cache::upsert_meta_db(&state.db, &cache_key, rd.as_deref(), image_type).await
                {
                    tracing::error!(error = %e, "failed to write meta to db");
                }
                let bytes = Bytes::from(bytes);
                if let Some((id_type, suffix)) = cross_id {
                    spawn_cross_id_cache(&state, cross_ids, id_type, suffix, image_type, bytes.clone());
                }
                state
                    .poster_mem_cache
                    .insert(
                        cache_key.clone(),
                        MemCacheEntry {
                            bytes,
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
    cache_suffix: &str,
    settings: &PosterSettings,
    image_size: Option<ImageSize>,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    let cross_id = Some((id_type, cache_suffix.to_string()));
    spawn_background_refresh(state, cache_key, cache_path, cross_id, async move {
        let (resolved, ratings_result, cross_ids) =
            resolve_with_ratings(&state2, id_type, &id_value).await?;
        {
            let id_key = format!("{}/{id_value}", id_type.as_str());
            let sources = ratings::available_sources_string(&ratings_result.badges);
            upsert_available_ratings_cached(&state2, &id_key, &sources, cross_ids.release_date.as_deref()).await;
        }
        let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.ratings_limit);
        let (bytes, rd, _tier, cross_ids) =
            generate_poster_with_source(&state2, &resolved, badges, &cross_ids, &settings, image_size).await?;
        Ok((bytes, rd, cache::ImageType::Poster, cross_ids))
    });
}

fn trigger_fanart_background_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    cache_suffix: &str,
    settings: &PosterSettings,
    fanart_kind: FanartImageKind,
    image_size: Option<ImageSize>,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    let cross_id = Some((id_type, cache_suffix.to_string()));
    spawn_background_refresh(state, cache_key, cache_path, cross_id, async move {
        let fanart = state2
            .fanart
            .as_ref()
            .ok_or_else(|| AppError::Other("FANART_API_KEY not configured".into()))?;

        let kind: ImageKind = fanart_kind.into();
        let fanart_lang = match kind {
            ImageKind::Backdrop => "",
            _ => &settings.fanart_lang,
        };
        let fanart_textless = matches!(kind, ImageKind::Poster) && settings.fanart_textless;

        let (resolved, ratings_result, cross_ids) =
            resolve_with_ratings(&state2, id_type, &id_value).await?;
        {
            let id_key = format!("{}/{id_value}", id_type.as_str());
            let sources = ratings::available_sources_string(&ratings_result.badges);
            upsert_available_ratings_cached(&state2, &id_key, &sources, cross_ids.release_date.as_deref()).await;
        }
        let type_ratings_limit = match fanart_kind {
            FanartImageKind::Logo => settings.logo_ratings_limit,
            FanartImageKind::Backdrop => settings.backdrop_ratings_limit,
        };
        let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, type_ratings_limit);

        let fanart_result = fetch_fanart_image(
            fanart,
            &state2.tmdb,
            &state2.fanart_cache,
            &resolved,
            fanart_lang,
            fanart_textless,
            kind,
            &state2.config.cache_dir,
            state2.config.external_cache_only,
        )
        .await;

        let image_bytes = fanart_result
            .map(|r| r.bytes)
            .ok_or_else(|| AppError::IdNotFound("no fanart image available".into()))?;

        let image_type: cache::ImageType = fanart_kind.into();

        let type_badge_style = match fanart_kind {
            FanartImageKind::Logo => settings.logo_badge_style.clone(),
            FanartImageKind::Backdrop => settings.backdrop_badge_style.clone(),
        };
        let type_label_style = match fanart_kind {
            FanartImageKind::Logo => settings.logo_label_style.clone(),
            FanartImageKind::Backdrop => settings.backdrop_label_style.clone(),
        };

        let resolved_size = resolve_image_size(image_size);
        let (target_width, badge_scale) = match fanart_kind {
            FanartImageKind::Logo => (
                resolved_size.logo_target_width(),
                resolved_size.badge_scale(cache::ImageType::Logo),
            ),
            FanartImageKind::Backdrop => (
                resolved_size.backdrop_target_width(),
                resolved_size.badge_scale(cache::ImageType::Backdrop),
            ),
        };
        let bytes = match fanart_kind {
            FanartImageKind::Logo => generate::generate_logo(image_bytes, badges, state2.font.clone(), type_badge_style, type_label_style, state2.render_semaphore.clone(), target_width, badge_scale).await?,
            FanartImageKind::Backdrop => generate::generate_backdrop(image_bytes, badges, state2.font.clone(), state2.config.poster_quality, type_badge_style, type_label_style, state2.render_semaphore.clone(), target_width, badge_scale).await?,
        };

        Ok((bytes, cross_ids.release_date.clone(), image_type, cross_ids))
    });
}

/// Returns (poster_bytes, release_date, fanart_match_tier, cross_id_info)
///
/// Accepts pre-fetched `ResolvedId`, badges, and `CrossIdInfo` so that callers
/// who already resolved/fetched ratings (for cache key construction) don't
/// duplicate that work.
async fn generate_poster_with_source(
    state: &AppState,
    resolved: &id::ResolvedId,
    badges: Vec<ratings::RatingBadge>,
    cross_ids: &CrossIdInfo,
    settings: &PosterSettings,
    image_size: Option<ImageSize>,
) -> Result<(Vec<u8>, Option<String>, Option<PosterMatch>, CrossIdInfo), AppError> {
    let poster_path = resolved
        .poster_path
        .as_deref()
        .ok_or_else(|| {
            let id_desc = resolved.imdb_id.as_deref()
                .unwrap_or_else(|| "unknown");
            AppError::IdNotFound(format!("no poster available for {id_desc} / tmdb:{} (TMDB has no poster_path)", resolved.tmdb_id))
        })?;

    // Try to fetch poster bytes from fanart.tv if configured
    let fanart_result = if &*settings.poster_source == SOURCE_FANART || settings.lang_override {
        if let Some(ref fanart) = state.fanart {
            fetch_fanart_image(
                fanart,
                &state.tmdb,
                &state.fanart_cache,
                resolved,
                &settings.fanart_lang,
                settings.fanart_textless,
                ImageKind::Poster,
                &state.config.cache_dir,
                state.config.external_cache_only,
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

    let resolved_size = resolve_image_size(image_size);
    let target_width = resolved_size.poster_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Poster);
    let tmdb_size: Arc<str> = resolved_size.tmdb_size().into();

    let bytes = generate::generate_poster(generate::PosterParams {
        poster_path,
        badges: &badges,
        tmdb: &state.tmdb,
        font: &state.font,
        quality: state.config.poster_quality,
        cache_dir: &state.config.cache_dir,
        poster_stale_secs: state.config.poster_stale_secs,
        poster_bytes_override: fanart_bytes,
        poster_position: settings.poster_position.clone(),
        badge_style: settings.poster_badge_style.clone(),
        label_style: settings.poster_label_style.clone(),
        badge_direction: settings.poster_badge_direction.clone(),
        render_semaphore: state.render_semaphore.clone(),
        target_width,
        badge_scale,
        tmdb_size,
        external_cache_only: state.config.external_cache_only,
    })
    .await?;

    Ok((bytes, cross_ids.release_date.clone(), match_tier, cross_ids.clone()))
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
    external_cache_only: bool,
) -> Option<FanartResult> {
    let images = fetch_fanart_images(fanart, tmdb, cache, resolved).await?;
    let candidates = select_images_for_kind(&images, kind);

    let (selected, match_tier) = FanartClient::select_image(candidates, lang, textless)?;
    let url = selected.url.clone();
    let fanart_id = selected.id.clone();

    // Try to serve from base fanart cache
    let ext = kind.ext();
    let base_path = cache::base_fanart_path(cache_dir, &fanart_id, ext).ok()?;

    let bytes = match cache::read(&base_path, 0).await {
        Some(entry) => entry.bytes,
        None => {
            match fanart.fetch_poster_bytes(&url).await {
                Ok(fresh) => {
                    if !external_cache_only {
                        let _ = cache::write(&base_path, &fresh).await;
                    }
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
    image_size: Option<ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    let fanart = state
        .fanart
        .as_ref()
        .ok_or_else(|| AppError::Other("FANART_API_KEY not configured".into()))?;

    let kind: ImageKind = fanart_kind.into();
    let id_type = IdType::parse(id_type_str)?;
    let id_value = kind.strip_ext(id_value_raw);
    cache::validate_id_value(id_value)?;
    let kind_prefix = kind.kind_prefix();
    let label = kind.label();

    // Use per-type rating limit for logos/backdrops
    let type_ratings_limit = match fanart_kind {
        FanartImageKind::Logo => settings.logo_ratings_limit,
        FanartImageKind::Backdrop => settings.backdrop_ratings_limit,
    };
    let type_badge_style = match fanart_kind {
        FanartImageKind::Logo => &settings.logo_badge_style,
        FanartImageKind::Backdrop => &settings.backdrop_badge_style,
    };
    let type_label_style = match fanart_kind {
        FanartImageKind::Logo => &settings.logo_label_style,
        FanartImageKind::Backdrop => &settings.backdrop_label_style,
    };

    // Backdrops are language-agnostic (no text) — skip lang/textless entirely.
    // Logos ARE the text — textless makes no sense, only lang matters.
    let fanart_lang = match kind {
        ImageKind::Backdrop => "",
        _ => &settings.fanart_lang,
    };
    let fanart_textless = matches!(kind, ImageKind::Poster) && settings.fanart_textless;

    // Check negative cache — skip generation if we already know there's nothing
    let neg_textless_key = format!("{id_type_str}/{id_value}{kind_prefix}_f_tl_neg");
    let textless_known_missing = fanart_textless
        && state.fanart_negative.get(&neg_textless_key).await.is_some();

    let neg_lang_key = format!("{id_type_str}/{id_value}{kind_prefix}_f_{}_neg", fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&neg_lang_key).await.is_some();

    if lang_known_missing && (!fanart_textless || textless_known_missing) {
        return Err(AppError::IdNotFound(format!("no {label} available").into()));
    }

    let variant = match kind {
        ImageKind::Backdrop => kind_prefix.to_string(),
        ImageKind::Logo => format!("{kind_prefix}_f_{fanart_lang}"),
        ImageKind::Poster => format!("{kind_prefix}_f_{fanart_lang}{}", if fanart_textless { "_tl" } else { "" }),
    };
    let image_type = match fanart_kind {
        FanartImageKind::Logo => cache::ImageType::Logo,
        FanartImageKind::Backdrop => cache::ImageType::Backdrop,
    };

    let id_key = format!("{id_type_str}/{id_value}");

    // Fast path: try to reconstruct the cache key from SQLite-stored available
    // sources, avoiding external API calls on cache hits.
    if let Some(available) = read_available_ratings_cached(state, &id_key).await {
        let ratings_suffix = ratings::badges_suffix_from_available(&available, &settings.ratings_order, type_ratings_limit);
        let suffix = settings_cache_suffix_with_ratings(settings, kind, image_size, &ratings_suffix);
        let cache_key = format!("{id_type_str}/{id_value}{variant}{suffix}");
        let cache_path_base = format!("{id_value}{variant}{suffix}");
        let cache_path = cache::typed_cache_path(&state.config.cache_dir, image_type, id_type_str, &cache_path_base)?;

        let fanart_cache_suffix: Arc<str> = format!("{variant}{suffix}").into();
        {
            let id_value = id_value.to_string();
            let fanart_cache_suffix = fanart_cache_suffix.clone();
            let settings = settings.clone();
            if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
                trigger_fanart_background_refresh(s, k, p, id_type, &id_value, &fanart_cache_suffix, &settings, fanart_kind, image_size);
            }).await? {
                let release_date = cache::read_meta_db(&state.db, &cache_key).await;
                return Ok((bytes, release_date));
            }
        }
    }

    // Slow path: resolve ID and fetch ratings
    let (resolved, ratings_result, cross_ids) =
        resolve_with_ratings(state, id_type, id_value).await?;

    // Persist available sources for future fast-path lookups (always write,
    // even with external_cache_only — this is an optimization index, not a
    // disk cache, and the fast path depends on it).
    {
        let sources = ratings::available_sources_string(&ratings_result.badges);
        upsert_available_ratings_cached(state, &id_key, &sources, cross_ids.release_date.as_deref()).await;
    }

    let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, type_ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);

    let suffix = settings_cache_suffix_with_ratings(settings, kind, image_size, &ratings_suffix);
    let cache_key = format!("{id_type_str}/{id_value}{variant}{suffix}");
    let cache_path_base = format!("{id_value}{variant}{suffix}");
    let cache_path = cache::typed_cache_path(&state.config.cache_dir, image_type, id_type_str, &cache_path_base)?;

    // Check caches (memory → filesystem) — may hit if SQLite was stale but
    // the correct cache entry already exists under the updated key.
    let fanart_cache_suffix: Arc<str> = format!("{variant}{suffix}").into();
    {
        let id_value = id_value.to_string();
        let fanart_cache_suffix = fanart_cache_suffix.clone();
        let settings = settings.clone();
        if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
            trigger_fanart_background_refresh(s, k, p, id_type, &id_value, &fanart_cache_suffix, &settings, fanart_kind, image_size);
        }).await? {
            return Ok((bytes, cross_ids.release_date));
        }
    }

    // Request coalescing — concurrent requests for the same logo/backdrop share one generation
    struct FanartGenCtx {
        state: AppState,
        cache_key: String,
        cache_path: std::path::PathBuf,
        fanart_cache_suffix: Arc<str>,
        fanart: FanartClient,
        neg_textless_key: String,
        neg_lang_key: String,
        type_badge_style: Arc<str>,
        type_label_style: Arc<str>,
        label: &'static str,
        resolved: id::ResolvedId,
        badges: Vec<ratings::RatingBadge>,
        cross_ids: CrossIdInfo,
    }
    let ctx = FanartGenCtx {
        state: state.clone(),
        cache_key: cache_key.clone(),
        cache_path: cache_path.clone(),
        fanart_cache_suffix: fanart_cache_suffix.clone(),
        fanart: fanart.clone(),
        neg_textless_key,
        neg_lang_key,
        type_badge_style: type_badge_style.clone(),
        type_label_style: type_label_style.clone(),
        label,
        resolved: resolved.clone(),
        badges,
        cross_ids: cross_ids.clone(),
    };
    let bytes: Bytes = state
        .poster_inflight
        .try_get_with(cache_key.clone(), async move {
            let ctx = ctx;

            let fanart_result = fetch_fanart_image(
                &ctx.fanart,
                &ctx.state.tmdb,
                &ctx.state.fanart_cache,
                &ctx.resolved,
                fanart_lang,
                fanart_textless,
                kind,
                &ctx.state.config.cache_dir,
                ctx.state.config.external_cache_only,
            )
            .await;

            let image_bytes = match fanart_result {
                Some(r) => {
                    if fanart_textless && r.match_tier == PosterMatch::Language {
                        ctx.state.fanart_negative.insert(ctx.neg_textless_key.clone(), ()).await;
                    }
                    r.bytes
                }
                None => {
                    if fanart_textless {
                        ctx.state.fanart_negative.insert(ctx.neg_textless_key, ()).await;
                    }
                    ctx.state.fanart_negative.insert(ctx.neg_lang_key, ()).await;
                    return Err(AppError::IdNotFound(format!("no {} available", ctx.label).into()));
                }
            };

            let resolved_size = resolve_image_size(image_size);
            let (target_width, badge_scale) = match fanart_kind {
                FanartImageKind::Logo => (
                    resolved_size.logo_target_width(),
                    resolved_size.badge_scale(cache::ImageType::Logo),
                ),
                FanartImageKind::Backdrop => (
                    resolved_size.backdrop_target_width(),
                    resolved_size.badge_scale(cache::ImageType::Backdrop),
                ),
            };
            let bytes = match fanart_kind {
                FanartImageKind::Logo => generate::generate_logo(image_bytes, ctx.badges, ctx.state.font.clone(), ctx.type_badge_style, ctx.type_label_style, ctx.state.render_semaphore.clone(), target_width, badge_scale).await?,
                FanartImageKind::Backdrop => generate::generate_backdrop(image_bytes, ctx.badges, ctx.state.font.clone(), ctx.state.config.poster_quality, ctx.type_badge_style, ctx.type_label_style, ctx.state.render_semaphore.clone(), target_width, badge_scale).await?,
            };

            if !ctx.state.config.external_cache_only {
                let _ = cache::write(&ctx.cache_path, &bytes).await;
            }
            let _ = cache::upsert_meta_db(&ctx.state.db, &ctx.cache_key, ctx.cross_ids.release_date.as_deref(), image_type).await;
            let bytes = Bytes::from(bytes);
            spawn_cross_id_cache(&ctx.state, ctx.cross_ids, id_type, ctx.fanart_cache_suffix.to_string(), image_type, bytes.clone());
            Ok::<_, AppError>(bytes)
        })
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;

    state
        .poster_mem_cache
        .insert(cache_key, MemCacheEntry { bytes: bytes.clone(), last_checked: Instant::now() })
        .await;
    Ok((bytes, cross_ids.release_date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_kind_prefix() {
        assert_eq!(ImageKind::Poster.kind_prefix(), "");
        assert_eq!(ImageKind::Logo.kind_prefix(), "_l");
        assert_eq!(ImageKind::Backdrop.kind_prefix(), "_b");
    }

    #[test]
    fn image_kind_file_ext() {
        assert_eq!(ImageKind::Poster.ext(), "jpg");
        assert_eq!(ImageKind::Logo.ext(), "png");
        assert_eq!(ImageKind::Backdrop.ext(), "jpg");
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

    #[test]
    fn badge_direction_cache_suffix_values() {
        assert_eq!(badge_direction_cache_suffix("h"), ".dh");
        assert_eq!(badge_direction_cache_suffix("v"), ".dv");
        assert_eq!(badge_direction_cache_suffix("d"), ".dd");
    }

    #[test]
    fn cross_id_info_merges_resolved_and_ratings() {
        let resolved = id::ResolvedId {
            imdb_id: Some("tt1234567".into()),
            tmdb_id: 100,
            tvdb_id: None,
            media_type: MediaType::Movie,
            poster_path: None,
            release_date: Some("2020-01-01".into()),
        };
        let ratings = ratings::RatingsResult {
            badges: vec![],
            tmdb_id: Some(100),
            tvdb_id: Some(999),
            imdb_id: Some("tt1234567".into()),
        };
        let info = CrossIdInfo::from_resolved(&resolved, &ratings);
        assert_eq!(info.imdb_id.as_deref(), Some("tt1234567"));
        assert_eq!(info.tmdb_id, 100);
        // tvdb_id backfilled from ratings when resolved has None
        assert_eq!(info.tvdb_id, Some(999));
        assert_eq!(info.release_date.as_deref(), Some("2020-01-01"));
    }

    #[test]
    fn cross_id_info_resolved_takes_precedence() {
        let resolved = id::ResolvedId {
            imdb_id: Some("tt1111111".into()),
            tmdb_id: 200,
            tvdb_id: Some(500),
            media_type: MediaType::Tv,
            poster_path: None,
            release_date: None,
        };
        let ratings = ratings::RatingsResult {
            badges: vec![],
            tmdb_id: Some(200),
            tvdb_id: Some(999),
            imdb_id: Some("tt2222222".into()),
        };
        let info = CrossIdInfo::from_resolved(&resolved, &ratings);
        // Resolved values take precedence over ratings
        assert_eq!(info.imdb_id.as_deref(), Some("tt1111111"));
        assert_eq!(info.tvdb_id, Some(500));
    }

    #[test]
    fn settings_hash_deterministic() {
        let s = PosterSettings::default();
        let h1 = settings_hash(&s, ImageKind::Poster, None);
        let h2 = settings_hash(&s, ImageKind::Poster, None);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32); // 16 bytes = 32 hex chars
    }

    #[test]
    fn settings_hash_differs_by_kind() {
        let s = PosterSettings::default();
        let poster = settings_hash(&s, ImageKind::Poster, None);
        let logo = settings_hash(&s, ImageKind::Logo, None);
        let backdrop = settings_hash(&s, ImageKind::Backdrop, None);
        assert_ne!(poster, logo);
        assert_ne!(poster, backdrop);
        assert_ne!(logo, backdrop);
    }

    #[test]
    fn settings_hash_differs_by_settings() {
        let s1 = PosterSettings::default();
        let mut s2 = PosterSettings::default();
        s2.ratings_limit = 5;
        assert_ne!(
            settings_hash(&s1, ImageKind::Poster, None),
            settings_hash(&s2, ImageKind::Poster, None)
        );
    }

    #[test]
    fn settings_hash_same_for_equivalent_settings() {
        let mut s1 = PosterSettings::default();
        let mut s2 = PosterSettings::default();
        // Different is_default flag shouldn't affect hash (it's metadata)
        s1.is_default = true;
        s2.is_default = false;
        assert_eq!(
            settings_hash(&s1, ImageKind::Poster, None),
            settings_hash(&s2, ImageKind::Poster, None)
        );
    }

    #[test]
    fn settings_hash_includes_lang_override() {
        let mut s1 = PosterSettings::default();
        let mut s2 = PosterSettings::default();
        s1.fanart_lang = "de".into();
        s1.lang_override = false;
        s2.fanart_lang = "de".into();
        s2.lang_override = true;
        assert_ne!(
            settings_hash(&s1, ImageKind::Poster, None),
            settings_hash(&s2, ImageKind::Poster, None)
        );
    }

    #[test]
    fn image_size_cache_suffix_values() {
        use crate::services::db::ImageSize;
        assert_eq!(image_size_cache_suffix(None), ".zm");
        assert_eq!(image_size_cache_suffix(Some(ImageSize::Small)), ".zs");
        assert_eq!(image_size_cache_suffix(Some(ImageSize::Medium)), ".zm");
        assert_eq!(image_size_cache_suffix(Some(ImageSize::Large)), ".zl");
        assert_eq!(image_size_cache_suffix(Some(ImageSize::VeryLarge)), ".zvl");
    }

    #[test]
    fn cross_id_info_backfills_imdb_from_ratings() {
        let resolved = id::ResolvedId {
            imdb_id: None,
            tmdb_id: 300,
            tvdb_id: None,
            media_type: MediaType::Movie,
            poster_path: None,
            release_date: None,
        };
        let ratings = ratings::RatingsResult {
            badges: vec![],
            tmdb_id: None,
            tvdb_id: Some(777),
            imdb_id: Some("tt9999999".into()),
        };
        let info = CrossIdInfo::from_resolved(&resolved, &ratings);
        // Both backfilled from ratings
        assert_eq!(info.imdb_id.as_deref(), Some("tt9999999"));
        assert_eq!(info.tvdb_id, Some(777));
    }

    #[test]
    fn settings_cache_suffix_poster_includes_all_parts() {
        let s = PosterSettings::default();
        let suffix = settings_cache_suffix(&s, ImageKind::Poster, None);
        // Should contain ratings, position, badge style, label style, direction, and size suffixes
        assert!(suffix.contains(".p"), "missing position suffix");
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".d"), "missing badge direction suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_logo_no_position_or_direction() {
        let s = PosterSettings::default();
        let suffix = settings_cache_suffix(&s, ImageKind::Logo, None);
        // Logos don't have position or direction
        assert!(!suffix.contains(".p"), "logo should not have position suffix");
        assert!(!suffix.contains(".d"), "logo should not have direction suffix");
        // But should have badge style, label style, and size
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_backdrop_no_position_or_direction() {
        let s = PosterSettings::default();
        let suffix = settings_cache_suffix(&s, ImageKind::Backdrop, None);
        assert!(!suffix.contains(".p"), "backdrop should not have position suffix");
        assert!(!suffix.contains(".d"), "backdrop should not have direction suffix");
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_uses_per_kind_settings() {
        let mut s = PosterSettings::default();
        s.poster_badge_style = "h".into();
        s.logo_badge_style = "v".into();
        s.backdrop_badge_style = "v".into();
        let poster = settings_cache_suffix(&s, ImageKind::Poster, None);
        let logo = settings_cache_suffix(&s, ImageKind::Logo, None);
        assert!(poster.contains(".sh"), "poster should use poster_badge_style");
        assert!(logo.contains(".sv"), "logo should use logo_badge_style");
    }

    #[test]
    fn settings_cache_suffix_uses_per_kind_ratings_limit() {
        let mut s = PosterSettings::default();
        s.ratings_limit = 3;
        s.logo_ratings_limit = 5;
        s.backdrop_ratings_limit = 2;
        let poster = settings_cache_suffix(&s, ImageKind::Poster, None);
        let logo = settings_cache_suffix(&s, ImageKind::Logo, None);
        let backdrop = settings_cache_suffix(&s, ImageKind::Backdrop, None);
        // Different ratings limits should produce different suffixes
        assert_ne!(poster, logo);
        assert_ne!(logo, backdrop);
    }

    #[test]
    fn settings_cache_suffix_varies_with_image_size() {
        let s = PosterSettings::default();
        let medium = settings_cache_suffix(&s, ImageKind::Poster, None);
        let large = settings_cache_suffix(&s, ImageKind::Poster, Some(ImageSize::Large));
        assert_ne!(medium, large);
        assert!(medium.ends_with(".zm"));
        assert!(large.ends_with(".zl"));
    }

    #[test]
    fn settings_cache_suffix_ignores_source_fields() {
        let mut s1 = PosterSettings::default();
        let mut s2 = PosterSettings::default();
        // These fields are handled by code path / variant, not suffix
        s1.poster_source = "t".into();
        s2.poster_source = "f".into();
        s1.fanart_lang = "en".into();
        s2.fanart_lang = "de".into();
        s1.fanart_textless = false;
        s2.fanart_textless = true;
        s1.lang_override = false;
        s2.lang_override = true;
        assert_eq!(
            settings_cache_suffix(&s1, ImageKind::Poster, None),
            settings_cache_suffix(&s2, ImageKind::Poster, None)
        );
    }

    #[test]
    fn cache_suffix_differs_by_actual_badges() {
        // Two movies with the same settings but different available rating sources
        // should produce different cache keys when using settings_cache_suffix_with_ratings.
        let s = PosterSettings::default();

        let suffix_imdb_rt = settings_cache_suffix_with_ratings(&s, ImageKind::Poster, None, "@ir");
        let suffix_imdb_rt_lb = settings_cache_suffix_with_ratings(&s, ImageKind::Poster, None, "@irl");
        let suffix_none = settings_cache_suffix_with_ratings(&s, ImageKind::Poster, None, "@");

        assert_ne!(suffix_imdb_rt, suffix_imdb_rt_lb, "different badge sets must produce different suffixes");
        assert_ne!(suffix_imdb_rt, suffix_none, "badges vs no badges must differ");
        assert_ne!(suffix_imdb_rt_lb, suffix_none);

        // Same badges should produce the same suffix
        let suffix_imdb_rt_2 = settings_cache_suffix_with_ratings(&s, ImageKind::Poster, None, "@ir");
        assert_eq!(suffix_imdb_rt, suffix_imdb_rt_2);
    }
}
