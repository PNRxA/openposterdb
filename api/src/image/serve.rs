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
use crate::image::generate;
use crate::services::db::{BadgeDirection, BadgePosition, BadgeSize, BadgeStyle, ImageSize, LabelStyle, RenderSettings};
use crate::services::fanart::{FanartClient, FanartImages, FanartPoster, PosterMatch};
use crate::services::lang::lang_base;
use crate::services::ratings;
use crate::AppState;

/// Threshold (ms) above which requests are logged as slow.
const SLOW_REQUEST_MS: u64 = 2000;

/// Logo or backdrop — image types served via TMDB images API (primary) with
/// fanart.tv fallback, or vice versa when the user prefers fanart.
/// Posters are excluded because they use a separate code path (`handle_inner`) with
/// staleness checks and background refresh that these endpoints don't need.
#[derive(Debug, Clone, Copy)]
pub enum LogoBackdropKind {
    Logo,
    Backdrop,
}

impl From<LogoBackdropKind> for cache::ImageType {
    fn from(k: LogoBackdropKind) -> Self {
        match k {
            LogoBackdropKind::Logo => cache::ImageType::Logo,
            LogoBackdropKind::Backdrop => cache::ImageType::Backdrop,
        }
    }
}

/// Resolved per-type render parameters for logo/backdrop generation.
///
/// For logos, `position` and `badge_direction` are hardcoded (TopRight / Vertical)
/// and not passed to `generate_logo` — only `badge_style`, `label_style`, and
/// `badge_size` are used. They exist here so backdrop callers can read them
/// without a separate code path.
struct LbRenderParams {
    position: BadgePosition,
    badge_direction: BadgeDirection,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_size: BadgeSize,
}

impl LbRenderParams {
    fn from_settings(lb_kind: LogoBackdropKind, settings: &RenderSettings) -> Self {
        let position = match lb_kind {
            LogoBackdropKind::Logo => BadgePosition::TopRight,
            LogoBackdropKind::Backdrop => settings.backdrop_position,
        };
        let badge_direction = match lb_kind {
            LogoBackdropKind::Logo => BadgeDirection::Vertical,
            LogoBackdropKind::Backdrop => settings.backdrop_badge_direction.resolve(position),
        };
        let badge_style = match lb_kind {
            LogoBackdropKind::Logo => settings.logo_badge_style,
            LogoBackdropKind::Backdrop => settings.backdrop_badge_style,
        }
        .resolve(badge_direction);
        let label_style = match lb_kind {
            LogoBackdropKind::Logo => settings.logo_label_style,
            LogoBackdropKind::Backdrop => settings.backdrop_label_style,
        };
        let badge_size = match lb_kind {
            LogoBackdropKind::Logo => settings.logo_badge_size,
            LogoBackdropKind::Backdrop => settings.backdrop_badge_size,
        };
        Self { position, badge_direction, badge_style, label_style, badge_size }
    }
}

/// Returns a cache key suffix for badge position.
pub fn position_cache_suffix(position: &str) -> String {
    format!(".p{position}")
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
/// Exhaustively destructures `RenderSettings` so adding a field without
/// handling it here produces a compile error.
///
/// Uses `ratings_cache_suffix()` to predict the ratings portion from user
/// settings. For cache keys that reflect *actual* rendered badges, use
/// `settings_cache_suffix_with_ratings()` with a pre-computed ratings suffix.
pub fn settings_cache_suffix(
    settings: &RenderSettings,
    kind: cache::ImageType,
    image_size: Option<ImageSize>,
) -> String {
    let ratings_suffix = match kind {
        cache::ImageType::Poster => ratings::ratings_cache_suffix(&settings.ratings_order, settings.ratings_limit),
        cache::ImageType::Logo => ratings::ratings_cache_suffix(&settings.ratings_order, settings.logo_ratings_limit),
        cache::ImageType::Backdrop => ratings::ratings_cache_suffix(&settings.ratings_order, settings.backdrop_ratings_limit),
        cache::ImageType::Episode => ratings::ratings_cache_suffix(&settings.ratings_order, settings.episode_ratings_limit),
    };
    settings_cache_suffix_with_ratings(settings, kind, image_size, &ratings_suffix)
}

/// Build the cache suffix string using a pre-computed ratings suffix.
///
/// This variant accepts the `@xyz` ratings portion directly (e.g. from
/// `badges_cache_suffix()`) so callers can use the *actual* badge sources
/// rather than the predicted ones from user settings.
pub fn settings_cache_suffix_with_ratings(
    settings: &RenderSettings,
    kind: cache::ImageType,
    image_size: Option<ImageSize>,
    ratings_suffix: &str,
) -> String {
    // Exhaustive destructure ensures new fields trigger a compile error here.
    let RenderSettings {
        image_source: _,        // handled by code path selection, not suffix
        lang: _,                // handled by variant string, not suffix
        textless: _,            // handled by variant string, not suffix
        ratings_limit: _,
        ratings_order: _,
        is_default: _,          // metadata, not a render setting
        poster_position: _,
        logo_ratings_limit: _,
        backdrop_ratings_limit: _,
        poster_badge_style: _,
        logo_badge_style: _,
        backdrop_badge_style: _,
        poster_label_style: _,
        logo_label_style: _,
        backdrop_label_style: _,
        poster_badge_direction: _,
        poster_badge_size: _,
        logo_badge_size: _,
        backdrop_badge_size: _,
        backdrop_position: _,
        backdrop_badge_direction: _,
        episode_ratings_limit: _,
        episode_badge_style: _,
        episode_label_style: _,
        episode_badge_size: _,
        episode_position: _,
        episode_badge_direction: _,
        episode_blur: _,
    } = settings;

    let resolved_size = resolve_image_size(image_size);
    let is_suffix = resolved_size.cache_suffix();
    let rs = ratings_suffix;

    match kind {
        cache::ImageType::Poster => {
            let ps = position_cache_suffix(settings.poster_position.as_str());
            let bs = badge_style_cache_suffix(settings.poster_badge_style.as_str());
            let ls = label_style_cache_suffix(settings.poster_label_style.as_str());
            let bd = badge_direction_cache_suffix(settings.poster_badge_direction.as_str());
            let bsz = settings.poster_badge_size.cache_suffix();
            format!("{rs}{ps}{bs}{ls}{bd}{bsz}{is_suffix}")
        }
        cache::ImageType::Logo => {
            let bs = badge_style_cache_suffix(settings.logo_badge_style.as_str());
            let ls = label_style_cache_suffix(settings.logo_label_style.as_str());
            let bsz = settings.logo_badge_size.cache_suffix();
            format!("{rs}{bs}{ls}{bsz}{is_suffix}")
        }
        cache::ImageType::Backdrop => {
            let ps = position_cache_suffix(settings.backdrop_position.as_str());
            let bs = badge_style_cache_suffix(settings.backdrop_badge_style.as_str());
            let ls = label_style_cache_suffix(settings.backdrop_label_style.as_str());
            let bd = badge_direction_cache_suffix(settings.backdrop_badge_direction.as_str());
            let bsz = settings.backdrop_badge_size.cache_suffix();
            format!("{rs}{ps}{bs}{ls}{bd}{bsz}{is_suffix}")
        }
        cache::ImageType::Episode => {
            let ps = position_cache_suffix(settings.episode_position.as_str());
            let bs = badge_style_cache_suffix(settings.episode_badge_style.as_str());
            let ls = label_style_cache_suffix(settings.episode_label_style.as_str());
            let bd = badge_direction_cache_suffix(settings.episode_badge_direction.as_str());
            let bsz = settings.episode_badge_size.cache_suffix();
            let blur = if settings.episode_blur { ".blur" } else { "" };
            format!("{rs}{ps}{bs}{ls}{bd}{bsz}{blur}{is_suffix}")
        }
    }
}

/// Compute a stable 12-hex-char settings hash for CDN content-addressed URLs.
/// Two users with identical effective settings for the same image type produce
/// the same hash, enabling Cloudflare cache deduplication.
///
/// **Important:** Every field that affects rendered output must be included here.
/// When adding new settings, add them to the hash to prevent CDN cache collisions.
pub fn settings_hash(settings: &RenderSettings, kind: cache::ImageType, image_size: Option<ImageSize>) -> String {
    let mut hasher = Sha256::new();

    hasher.update(kind.label().as_bytes());
    hasher.update(b"\0");

    // Render-affecting settings (via exhaustive destructure in settings_cache_suffix)
    hasher.update(settings_cache_suffix(settings, kind, image_size).as_bytes());
    hasher.update(b"\0");

    // Source-selection settings (not in cache suffix because handled by code path/variant)
    hasher.update(settings.image_source.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(settings.lang.as_bytes());
    hasher.update(b"\0");
    hasher.update(if settings.textless { b"1" } else { b"0" });

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

/// Resolve an ID and fetch ratings.
///
/// When `uplift_episodes` is true and the ID resolves to an episode, the
/// episode is transparently re-resolved to its parent series *before*
/// fetching ratings. This avoids wasted episode-level TMDB calls when the
/// caller only needs series-level data (poster, logo, backdrop endpoints).
///
/// When `skip_ratings` is true, the external ratings fetch (OMDB, MDBList,
/// TMDB) is skipped entirely and an empty `RatingsResult` is returned.
/// Use this when the caller's ratings limit is 0, fetching ratings only
/// to discard them wastes rate-limit quota and stalls image delivery.
async fn resolve_with_ratings(
    state: &AppState,
    id_type: IdType,
    id_value: &str,
    uplift_episodes: bool,
    skip_ratings: bool,
) -> Result<(id::ResolvedId, ratings::RatingsResult, CrossIdInfo), AppError> {
    let id_resolve_start = Instant::now();
    let mut resolved = id::resolve(id_type, id_value, &state.tmdb, &state.id_cache).await?;

    // When an episode ID hits a non-episode endpoint, re-resolve to the
    // parent series so ratings and assets are series-level.
    if uplift_episodes && resolved.media_type == MediaType::Episode {
        if let Some(ref ep) = resolved.episode {
            let series_id = id::format_tmdb_id_value(ep.show_tmdb_id, &MediaType::Tv, None);
            resolved = id::resolve(IdType::Tmdb, &series_id, &state.tmdb, &state.id_cache).await?;
        }
    }

    let id_resolve_ms = id_resolve_start.elapsed().as_millis() as u64;

    let ratings_result = if skip_ratings {
        ratings::RatingsResult::default()
    } else {
        ratings::fetch_ratings(
            &resolved,
            &state.tmdb,
            state.omdb.as_ref(),
            state.mdblist.as_ref(),
            state.trakt.as_ref(),
            &state.ratings_cache,
        )
        .await?
    };

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
    episode: Option<id::EpisodeInfo>,
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
            episode: resolved.episode.clone(),
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
            let tmdb_val = format_tmdb_id_value(cross_ids.tmdb_id, &cross_ids.media_type, cross_ids.episode.as_ref());
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

/// Write a freshly rendered image to disk cache, update the meta DB, and
/// distribute to cross-ID cache entries. Returns the final `Bytes`.
///
/// Cache write failures are logged but do not fail the request — the image is
/// already rendered and should still be served to the caller.
async fn post_render_cache(
    state: &AppState,
    rendered: Vec<u8>,
    cache_path: &std::path::Path,
    cache_key: &str,
    release_date: Option<&str>,
    image_type: cache::ImageType,
    cross_ids: CrossIdInfo,
    id_type: IdType,
    cache_suffix: String,
) -> Bytes {
    if !state.config.external_cache_only {
        if let Err(e) = cache::write(cache_path, &rendered).await {
            tracing::warn!(cache_key, error = %e, "failed to write image to disk cache");
        }
    }
    if let Err(e) = cache::upsert_meta_db(&state.db, cache_key, release_date, image_type).await {
        tracing::warn!(cache_key, error = %e, "failed to upsert cache meta DB");
    }
    let bytes = Bytes::from(rendered);
    spawn_cross_id_cache(state, cross_ids, id_type, cache_suffix, image_type, bytes.clone());
    bytes
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
    if let Some(entry) = state.image_mem_cache.get(cache_key).await {
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
                .image_mem_cache
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
            .image_mem_cache
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
    mut settings: RenderSettings,
    image_size: Option<ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    let request_start = Instant::now();
    let id_type = IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);

    // Reject path traversal, null bytes, etc. before any network calls
    cache::validate_id_value(id_value)?;

    // Resolve "default" badge direction and style early, before cache key construction
    settings.poster_badge_direction = settings.poster_badge_direction.resolve(settings.poster_position);
    settings.poster_badge_style = settings.poster_badge_style.resolve(settings.poster_badge_direction);

    let use_fanart = settings.image_source.is_fanart();
    let id_key = format!("{id_type_str}/{id_value}");
    let variant = tmdb_poster_variant(&settings.lang, settings.textless);

    // Fast path (non-fanart): try to reconstruct the cache key from SQLite-stored
    // available sources, avoiding external API calls entirely on cache hits.
    if !use_fanart {
        let fast_path_start = Instant::now();
        // When ratings are disabled the suffix is always "@" regardless of which
        // sources a title actually has, skip the SQLite lookup entirely.
        let fast_path_available = if settings.ratings_limit == 0 {
            Some(String::new())
        } else {
            read_available_ratings_cached(state, &id_key).await
        };
        if let Some(available) = fast_path_available {
            let available_ratings_ms = fast_path_start.elapsed().as_millis() as u64;
            let ratings_suffix = ratings::badges_suffix_from_available(&available, &settings.ratings_order, settings.ratings_limit);
            let suffix = settings_cache_suffix_with_ratings(&settings, cache::ImageType::Poster, image_size, &ratings_suffix);
            let cache_value = format!("{id_value}{variant}{suffix}");
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
    // repeat requests within the 30-min TTL). Episodes are uplifted to their
    // parent series — the poster endpoint returns series-level assets.
    let slow_path_start = Instant::now();
    let resolve_start = Instant::now();
    let skip_ratings = settings.ratings_limit == 0;
    let (resolved, ratings_result, cross_ids) =
        resolve_with_ratings(state, id_type, id_value, true, skip_ratings).await?;
    let resolve_ms = resolve_start.elapsed().as_millis() as u64;

    // Persist available sources for future fast-path lookups (always write,
    // even with external_cache_only — this is an optimization index, not a
    // disk cache, and the fast path depends on it).
    // Skip when ratings are disabled: we have no real source data to store,
    // and writing "" would erase any previously cached good data.
    if !skip_ratings {
        let sources = ratings::available_sources_string(&ratings_result.badges);
        upsert_available_ratings_cached(state, &id_key, &sources, cross_ids.release_date.as_deref()).await;
    }

    // Fanart → TMDB fallback strategy:
    //
    // 1. If the user's image_source is fanart, try fanart first. On hit, return.
    //
    // 2. On fanart miss, fall through to TMDB. The user's badge/position settings
    //    are preserved — only the image source changes.
    if use_fanart {
        if let Some(bytes) = try_fanart_path(state, id_type_str, id_value, id_type, &resolved, &ratings_result, &cross_ids, &settings, image_size).await? {
            return Ok((bytes, cross_ids.release_date));
        }
    }

    // TMDB path (default, or fanart fallback)
    let settings = &settings;

    let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);

    let suffix = settings_cache_suffix_with_ratings(settings, cache::ImageType::Poster, image_size, &ratings_suffix);
    let cache_value = format!("{id_value}{variant}{suffix}");
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
        .image_inflight
        .try_get_with(cache_key.clone(), async move {
            let (rendered, rd, _used_fanart, gen_cross_ids) =
                generate_poster_with_source(&state2, &resolved, badges, &cross_ids, &settings2, image_size, use_fanart).await?;
            let bytes = post_render_cache(&state2, rendered, &cache_path2, &cache_key2, rd.as_deref(), cache::ImageType::Poster, gen_cross_ids, id_type, cache_suffix.to_string()).await;
            Ok::<_, AppError>(bytes)
        })
        .await
        .map_err(|e| match Arc::try_unwrap(e) {
            Ok(app_err) => app_err,
            Err(arc) => AppError::Other(arc.to_string()),
        })?;

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
        .image_mem_cache
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
    settings: &RenderSettings,
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
    settings: &RenderSettings,
    image_size: Option<ImageSize>,
) -> Result<Option<Bytes>, AppError> {
    // Build the list of cache variants to check.
    // When textless is requested but we know it's unavailable (negative cache),
    // skip the textless key and go straight to language.
    let neg_key = format!("{id_type_str}/{id_value}_f_tl_neg");
    let textless_known_missing = settings.textless
        && state.fanart_negative.get(&neg_key).await.is_some();

    let lang_variant = format!("_f_{}", settings.lang);
    let lang_neg_key = format!("{id_type_str}/{id_value}_f_{}_neg", settings.lang);
    let lang_known_missing = state.fanart_negative.get(&lang_neg_key).await.is_some();

    // All fanart variants are known-missing — skip generation and fall through to TMDB
    if lang_known_missing && (!settings.textless || textless_known_missing) {
        return Ok(None);
    }

    let badges = ratings::apply_rating_preferences(ratings_result.badges.clone(), &settings.ratings_order, settings.ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);

    // Compute settings suffix once for all fanart variants
    let suffix = settings_cache_suffix_with_ratings(settings, cache::ImageType::Poster, image_size, &ratings_suffix);

    // Check cached variants (textless first if requested, then language)
    let mut variants_to_check: Vec<String> = Vec::new();
    if settings.textless && !textless_known_missing {
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
    let result = generate_poster_with_source(state, resolved, badges, cross_ids, settings, image_size, false).await;

    match result {
        Ok((bytes, rd, Some(tier), cross_ids)) => {
            if settings.textless && tier == PosterMatch::Language {
                state.fanart_negative.insert(neg_key, ()).await;
            }

            let actual_variant = match tier {
                PosterMatch::Textless => "_f_tl".to_string(),
                PosterMatch::Language => format!("_f_{}", settings.lang),
            };
            let (cache_key, cache_path) =
                fanart_variant_paths(&state.config.cache_dir, id_type_str, id_value, &actual_variant, &suffix)?;
            let fanart_cache_suffix = format!("{actual_variant}{suffix}");
            let bytes = post_render_cache(state, bytes, &cache_path, &cache_key, rd.as_deref(), cache::ImageType::Poster, cross_ids, id_type, fanart_cache_suffix).await;
            state
                .image_mem_cache
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
            if settings.textless {
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
                    .image_mem_cache
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
    settings: &RenderSettings,
    image_size: Option<ImageSize>,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    let cross_id = Some((id_type, cache_suffix.to_string()));
    spawn_background_refresh(state, cache_key, cache_path, cross_id, async move {
        let skip_ratings = settings.ratings_limit == 0;
        let (resolved, ratings_result, cross_ids) =
            resolve_with_ratings(&state2, id_type, &id_value, true, skip_ratings).await?;
        if !skip_ratings {
            let id_key = format!("{}/{id_value}", id_type.as_str());
            let sources = ratings::available_sources_string(&ratings_result.badges);
            upsert_available_ratings_cached(&state2, &id_key, &sources, cross_ids.release_date.as_deref()).await;
        }
        let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.ratings_limit);
        let (bytes, rd, _tier, cross_ids) =
            generate_poster_with_source(&state2, &resolved, badges, &cross_ids, &settings, image_size, false).await?;
        Ok((bytes, rd, cache::ImageType::Poster, cross_ids))
    });
}

fn trigger_logo_backdrop_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    cache_suffix: &str,
    settings: &RenderSettings,
    lb_kind: LogoBackdropKind,
    image_size: Option<ImageSize>,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    let cross_id = Some((id_type, cache_suffix.to_string()));
    spawn_background_refresh(state, cache_key, cache_path, cross_id, async move {
        let fanart = state2.fanart.as_ref();

        let kind: cache::ImageType = lb_kind.into();
        let lang = match kind {
            cache::ImageType::Backdrop => "",
            _ => &settings.lang,
        };
        // Logos/backdrops never use textless
        let textless = false;

        let type_ratings_limit = match lb_kind {
            LogoBackdropKind::Logo => settings.logo_ratings_limit,
            LogoBackdropKind::Backdrop => settings.backdrop_ratings_limit,
        };
        let skip_ratings = type_ratings_limit == 0;
        let (resolved, ratings_result, cross_ids) =
            resolve_with_ratings(&state2, id_type, &id_value, true, skip_ratings).await?;
        if !skip_ratings {
            let id_key = format!("{}/{id_value}", id_type.as_str());
            let sources = ratings::available_sources_string(&ratings_result.badges);
            upsert_available_ratings_cached(&state2, &id_key, &sources, cross_ids.release_date.as_deref()).await;
        }
        let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, type_ratings_limit);

        let label = kind.label();
        let not_found = || AppError::IdNotFound(format!("no {label} available"));

        // Use the same primary/fallback pattern as the generation path
        let fanart_is_primary = settings.image_source.is_fanart() && fanart.is_some();
        let image_bytes = if fanart_is_primary {
            // Fanart primary → TMDB fallback
            let primary = if let Some(fc) = fanart {
                fetch_fanart_image(fc, &state2.tmdb, &state2.fanart_cache, &resolved, lang, textless, kind, &state2.config.cache_dir, state2.config.external_cache_only).await.map(|r| r.bytes)
            } else {
                None
            };
            match primary {
                Some(b) => b,
                None => try_tmdb_logo_backdrop(&state2, &resolved, kind, lang, textless, image_size)
                    .await
                    .ok_or_else(not_found)?,
            }
        } else {
            // TMDB primary → Fanart fallback
            let primary = try_tmdb_logo_backdrop(&state2, &resolved, kind, lang, textless, image_size).await;
            match primary {
                Some(b) => b,
                None => {
                    if let Some(fc) = fanart {
                        fetch_fanart_image(fc, &state2.tmdb, &state2.fanart_cache, &resolved, lang, textless, kind, &state2.config.cache_dir, state2.config.external_cache_only)
                            .await
                            .map(|r| r.bytes)
                            .ok_or_else(not_found)?
                    } else {
                        return Err(not_found());
                    }
                }
            }
        };

        let image_type: cache::ImageType = lb_kind.into();

        let params = LbRenderParams::from_settings(lb_kind, &settings);
        let resolved_size = resolve_image_size(image_size);
        let badge_size_factor = params.badge_size.scale_factor();
        let (target_width, badge_scale) = match lb_kind {
            LogoBackdropKind::Logo => (
                resolved_size.logo_target_width(),
                resolved_size.badge_scale(cache::ImageType::Logo) * badge_size_factor,
            ),
            LogoBackdropKind::Backdrop => (
                resolved_size.backdrop_target_width(),
                resolved_size.badge_scale(cache::ImageType::Backdrop) * badge_size_factor,
            ),
        };
        let bytes = match lb_kind {
            LogoBackdropKind::Logo => generate::generate_logo(image_bytes, badges, state2.font.clone(), params.badge_style, params.label_style, state2.render_semaphore.clone(), target_width, badge_scale).await?,
            LogoBackdropKind::Backdrop => generate::generate_backdrop(image_bytes, badges, state2.font.clone(), state2.config.image_quality, params.position, params.badge_style, params.label_style, params.badge_direction, state2.render_semaphore.clone(), target_width, badge_scale, params.badge_size).await?,
        };

        Ok((bytes, cross_ids.release_date.clone(), image_type, cross_ids))
    });
}

/// Fetch the base image for an episode and render rating badges on top.
///
/// Shared by `handle_episode_inner` (request path) and `trigger_episode_refresh`
/// (background stale-cache refresh) so that sizing, badge-scale, TMDB-fetch, and
/// render logic live in exactly one place.
async fn generate_episode(
    state: &AppState,
    resolved: &id::ResolvedId,
    badges: Vec<ratings::RatingBadge>,
    settings: &RenderSettings,
    image_size: Option<ImageSize>,
) -> Result<Vec<u8>, AppError> {
    let poster_path = resolved
        .poster_path
        .as_deref()
        .ok_or_else(|| AppError::IdNotFound(format!("no image available for episode (tmdb:{})", resolved.tmdb_id)))?;

    let resolved_size = resolve_image_size(image_size);
    let is_episode_still = resolved.episode.as_ref().map_or(false, |ep| ep.still_path.is_some());
    let target_width = if is_episode_still {
        resolved_size.episode_target_width()
    } else {
        resolved_size.poster_target_width()
    };
    let scale_image_type = if is_episode_still { cache::ImageType::Episode } else { cache::ImageType::Poster };
    let badge_scale = resolved_size.badge_scale(scale_image_type) * settings.episode_badge_size.scale_factor();
    let tmdb_size: Arc<str> = resolved_size.tmdb_size().into();

    let image_bytes = if state.config.external_cache_only {
        state.tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?
    } else {
        let poster_cache = cache::base_poster_path(&state.config.cache_dir, poster_path, &tmdb_size)?;
        if let Some(entry) = cache::read(&poster_cache, state.config.image_stale_secs).await {
            entry.bytes
        } else {
            let bytes = state.tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?;
            cache::write(&poster_cache, &bytes).await?;
            bytes
        }
    };

    let font = state.font.clone();
    let quality = state.config.image_quality;
    let position = settings.episode_position;
    let badge_style = settings.episode_badge_style;
    let label_style = settings.episode_label_style;
    let badge_direction = settings.episode_badge_direction;
    let episode_badge_size = settings.episode_badge_size;
    let blur = settings.episode_blur;
    let render_semaphore = state.render_semaphore.clone();

    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    let rendered = tokio::task::spawn_blocking(move || {
        generate::render_episode_sync(&image_bytes, &badges, &font, quality, position, badge_style, label_style, badge_direction, target_width, badge_scale, episode_badge_size, blur)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(rendered)
}

fn trigger_episode_refresh(
    state: &AppState,
    cache_key: &str,
    cache_path: &std::path::Path,
    id_type: IdType,
    id_value: &str,
    cache_suffix: &str,
    settings: &RenderSettings,
    image_size: Option<ImageSize>,
) {
    let state2 = state.clone();
    let id_value = id_value.to_string();
    let settings = settings.clone();
    let cross_id = Some((id_type, cache_suffix.to_string()));
    spawn_background_refresh(state, cache_key, cache_path, cross_id, async move {
        let skip_ratings = settings.episode_ratings_limit == 0;
        let (resolved, ratings_result, cross_ids) =
            resolve_with_ratings(&state2, id_type, &id_value, false, skip_ratings).await?;
        if !skip_ratings {
            let id_key = format!("{}/{id_value}", id_type.as_str());
            let sources = ratings::available_sources_string(&ratings_result.badges);
            upsert_available_ratings_cached(&state2, &id_key, &sources, cross_ids.release_date.as_deref()).await;
        }
        let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.episode_ratings_limit);
        let bytes = generate_episode(&state2, &resolved, badges, &settings, image_size).await?;

        Ok((bytes, cross_ids.release_date.clone(), cache::ImageType::Episode, cross_ids))
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
    settings: &RenderSettings,
    image_size: Option<ImageSize>,
    fanart_already_tried: bool,
) -> Result<(Vec<u8>, Option<String>, Option<PosterMatch>, CrossIdInfo), AppError> {
    let default_poster_path = resolved
        .poster_path
        .as_deref()
        .ok_or_else(|| {
            let id_desc = resolved.imdb_id.as_deref()
                .unwrap_or_else(|| "unknown");
            AppError::IdNotFound(format!("no poster available for {id_desc} / tmdb:{} (TMDB has no poster_path)", resolved.tmdb_id))
        })?;

    // Unified fallback chain for posters:
    //   TMDB preferred: TMDB(lang) → Fanart(lang) → TMDB(default)
    //   Fanart preferred: Fanart(lang) → TMDB(lang) → TMDB(default)
    // The default case (lang=en, textless=false) skips lang-specific lookups
    // entirely — `resolved.poster_path` is already the TMDB(default).
    let is_default = &*settings.lang == "en" && !settings.textless;

    let try_fanart = || async {
        if fanart_already_tried {
            return None;
        }
        if let Some(ref fanart) = state.fanart {
            fetch_fanart_image(
                fanart,
                &state.tmdb,
                &state.fanart_cache,
                resolved,
                &settings.lang,
                settings.textless,
                cache::ImageType::Poster,
                &state.config.cache_dir,
                state.config.external_cache_only,
            )
            .await
        } else {
            None
        }
    };

    let (lang_poster_path, fanart_result) = if settings.image_source.is_fanart() {
        // Fanart(lang) → TMDB(lang) → TMDB(default)
        let fr = try_fanart().await;
        let tp = if fr.is_none() && !is_default {
            resolve_tmdb_poster_path(state, resolved, &settings.lang, settings.textless).await
        } else {
            None
        };
        (tp, fr)
    } else {
        // TMDB(lang) → Fanart(lang) → TMDB(default)
        let tp = if !is_default {
            resolve_tmdb_poster_path(state, resolved, &settings.lang, settings.textless).await
        } else {
            None
        };
        let fr = if tp.is_none() && !is_default {
            try_fanart().await
        } else {
            None
        };
        (tp, fr)
    };
    // When both None → poster_path = default_poster_path → generate_poster fetches TMDB(default)
    let poster_path = lang_poster_path.as_deref().unwrap_or(default_poster_path);
    let match_tier = fanart_result.as_ref().map(|r| r.match_tier);
    let fanart_bytes = fanart_result.map(|r| r.bytes);

    let resolved_size = resolve_image_size(image_size);
    let target_width = resolved_size.poster_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Poster) * settings.poster_badge_size.scale_factor();
    let tmdb_size: Arc<str> = resolved_size.tmdb_size().into();

    let bytes = generate::generate_poster(generate::ImageParams {
        poster_path,
        badges: &badges,
        tmdb: &state.tmdb,
        font: &state.font,
        quality: state.config.image_quality,
        cache_dir: &state.config.cache_dir,
        image_stale_secs: state.config.image_stale_secs,
        poster_bytes_override: fanart_bytes,
        poster_position: settings.poster_position,
        badge_style: settings.poster_badge_style,
        label_style: settings.poster_label_style,
        badge_direction: settings.poster_badge_direction,
        render_semaphore: state.render_semaphore.clone(),
        target_width,
        badge_scale,
        badge_size: settings.poster_badge_size,
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
        // Episode included for exhaustiveness — episodes use `handle_episode_inner`
        // which does not call fanart functions.
        MediaType::Tv | MediaType::Episode => {
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

fn select_images_for_kind(images: &FanartImages, kind: cache::ImageType) -> &[FanartPoster] {
    match kind {
        // Episode included for exhaustiveness — episodes don't use fanart.
        cache::ImageType::Poster | cache::ImageType::Episode => &images.posters,
        cache::ImageType::Logo => &images.logos,
        cache::ImageType::Backdrop => &images.backdrops,
    }
}

async fn fetch_fanart_image(
    fanart: &FanartClient,
    tmdb: &crate::services::tmdb::TmdbClient,
    cache: &moka::future::Cache<String, Arc<FanartImages>>,
    resolved: &id::ResolvedId,
    lang: &str,
    textless: bool,
    kind: cache::ImageType,
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

pub fn image_response(bytes: Bytes, content_type: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                "public, max-age=3600, stale-while-revalidate=86400",
            ),
        ],
        bytes,
    )
        .into_response()
}


/// Build a cache key variant for TMDB poster language/textless combinations.
///
/// The default case (`lang=en, textless=false`) returns an empty string for
/// backward compatibility — existing cache keys are unchanged.
fn tmdb_poster_variant(lang: &str, textless: bool) -> String {
    match (lang == "en", textless) {
        (true, false)  => String::new(),
        (true, true)   => "_t_tl".into(),
        (false, false) => format!("_t_{lang}"),
        (false, true)  => format!("_t_{lang}_tl"),
    }
}

/// Fetch TMDB images metadata (cached) and select the best candidate for the
/// given image type, language, and textless preference.
async fn get_tmdb_images_cached(
    state: &AppState,
    resolved: &id::ResolvedId,
    lang: &str,
) -> Option<std::sync::Arc<crate::services::tmdb::TmdbImagesResponse>> {
    let media_type_str = match resolved.media_type {
        MediaType::Movie => "movie",
        // Episode included for exhaustiveness — episodes are served by
        // `handle_episode_inner`, which does not use TMDB images API lookups.
        MediaType::Tv | MediaType::Episode => "tv",
    };
    let cache_key = format!("{}:{}:{}", media_type_str, resolved.tmdb_id, lang_base(lang));
    let tmdb = state.tmdb.clone();
    let tmdb_id = resolved.tmdb_id;
    let lang_owned = lang.to_string();
    let mt = media_type_str.to_string();
    match state
        .tmdb_images_cache
        .try_get_with(cache_key, async move {
            let imgs = tmdb.get_images(&mt, tmdb_id, &lang_owned).await?;
            Ok::<_, AppError>(std::sync::Arc::new(imgs))
        })
        .await
    {
        Ok(imgs) => Some(imgs),
        Err(e) => {
            tracing::warn!(tmdb_id = resolved.tmdb_id, %e, "TMDB images API request failed");
            None
        }
    }
}

/// Try to resolve a language-specific poster path via TMDB's images API.
///
/// Returns `Some(file_path)` on hit, `None` on miss (caller falls back to
/// `resolved.poster_path`).
async fn resolve_tmdb_poster_path(
    state: &AppState,
    resolved: &id::ResolvedId,
    lang: &str,
    textless: bool,
) -> Option<String> {
    let tmdb_images = get_tmdb_images_cached(state, resolved, lang).await?;
    let selected = crate::services::tmdb::TmdbClient::select_image(&tmdb_images.posters, lang, textless)?;
    Some(selected.file_path.clone())
}

/// Try to fetch a logo or backdrop image from TMDB's images API.
/// Returns `Some(bytes)` on success, `None` on miss.
async fn try_tmdb_logo_backdrop(
    state: &AppState,
    resolved: &id::ResolvedId,
    kind: cache::ImageType,
    lang: &str,
    textless: bool,
    image_size: Option<ImageSize>,
) -> Option<Vec<u8>> {
    let tmdb_images = get_tmdb_images_cached(state, resolved, lang).await?;
    let candidates = match kind {
        cache::ImageType::Logo => &tmdb_images.logos,
        cache::ImageType::Backdrop => &tmdb_images.backdrops,
        // Episode included for exhaustiveness — episodes don't use TMDB images API.
        cache::ImageType::Poster | cache::ImageType::Episode => &tmdb_images.posters,
    };
    let selected = crate::services::tmdb::TmdbClient::select_image(candidates, lang, textless)?;
    let size = resolve_image_size(image_size).tmdb_size();
    state.tmdb.fetch_image_bytes(&selected.file_path, size).await.ok()
}

/// Try to fetch an image from fanart.tv, updating negative caches on miss.
/// Returns `Some(bytes)` on hit, `None` on miss.
async fn try_fanart_with_negative_cache(
    fanart_client: &FanartClient,
    state: &AppState,
    resolved: &id::ResolvedId,
    lang: &str,
    textless: bool,
    kind: cache::ImageType,
    neg_textless_key: &str,
    neg_lang_key: &str,
) -> Option<Vec<u8>> {
    let result = fetch_fanart_image(
        fanart_client,
        &state.tmdb,
        &state.fanart_cache,
        resolved,
        lang,
        textless,
        kind,
        &state.config.cache_dir,
        state.config.external_cache_only,
    ).await;
    match result {
        Some(r) => {
            if textless && r.match_tier == PosterMatch::Language {
                state.fanart_negative.insert(neg_textless_key.to_string(), ()).await;
            }
            Some(r.bytes)
        }
        None => {
            if textless {
                state.fanart_negative.insert(neg_textless_key.to_string(), ()).await;
            }
            state.fanart_negative.insert(neg_lang_key.to_string(), ()).await;
            None
        }
    }
}

/// Serve an episode image with episode-specific settings (position, direction, blur).
/// Uses the poster pipeline for ID resolution and ratings, but renders with episode
/// settings and landscape-aware target width.
pub async fn handle_episode_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_jpg: &str,
    mut settings: RenderSettings,
    image_size: Option<ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    let id_type = id::IdType::parse(id_type_str)?;
    let id_value = id_value_jpg.strip_suffix(".jpg").unwrap_or(id_value_jpg);
    cache::validate_id_value(id_value)?;

    // Resolve episode-specific defaults
    settings.episode_badge_direction = settings.episode_badge_direction.resolve(settings.episode_position);
    settings.episode_badge_style = settings.episode_badge_style.resolve(settings.episode_badge_direction);

    let id_key = format!("{id_type_str}/{id_value}");

    // Fast path: try to reconstruct the cache key from SQLite-stored available
    // sources, avoiding external API calls entirely on cache hits.
    // When ratings are disabled the suffix is always "@", skip the SQLite lookup.
    let fast_path_available = if settings.episode_ratings_limit == 0 {
        Some(String::new())
    } else {
        read_available_ratings_cached(state, &id_key).await
    };
    if let Some(available) = fast_path_available {
        let ratings_suffix = ratings::badges_suffix_from_available(&available, &settings.ratings_order, settings.episode_ratings_limit);
        let suffix = settings_cache_suffix_with_ratings(&settings, cache::ImageType::Episode, image_size, &ratings_suffix);
        let cache_value = format!("{id_value}{suffix}");
        let cache_path = cache::typed_cache_path(&state.config.cache_dir, cache::ImageType::Episode, id_type_str, &cache_value)?;
        let cache_key = format!("{id_type_str}/{cache_value}");

        let cache_suffix: Arc<str> = suffix.into();
        if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
            trigger_episode_refresh(s, k, p, id_type, id_value, &cache_suffix, &settings, image_size);
        }).await? {
            let release_date = cache::read_meta_db(&state.db, &cache_key).await;
            return Ok((bytes, release_date));
        }
    }

    // Slow path: resolve ID and fetch ratings (no episode uplift — this IS the episode endpoint)
    let skip_ratings = settings.episode_ratings_limit == 0;
    let (resolved, ratings_result, cross_ids) =
        resolve_with_ratings(state, id_type, id_value, false, skip_ratings).await?;

    // The episode endpoint only serves episodes — reject movies and series.
    if resolved.media_type != MediaType::Episode {
        return Err(AppError::BadRequest(format!(
            "{id_value} is a {}, not an episode — use the /{}-default/ endpoint instead",
            match resolved.media_type {
                MediaType::Movie => "movie",
                MediaType::Tv => "series",
                MediaType::Episode => unreachable!(),
            },
            match resolved.media_type {
                MediaType::Movie => "poster",
                MediaType::Tv => "poster",
                MediaType::Episode => unreachable!(),
            },
        )));
    }

    // Persist available sources for fast-path lookups.
    if !skip_ratings {
        let sources = ratings::available_sources_string(&ratings_result.badges);
        upsert_available_ratings_cached(state, &id_key, &sources, cross_ids.release_date.as_deref()).await;
    }

    let badges = ratings::apply_rating_preferences(ratings_result.badges, &settings.ratings_order, settings.episode_ratings_limit);
    let ratings_suffix = ratings::badges_cache_suffix(&badges);
    let suffix = settings_cache_suffix_with_ratings(&settings, cache::ImageType::Episode, image_size, &ratings_suffix);
    let cache_value = format!("{id_value}{suffix}");
    let cache_path = cache::typed_cache_path(&state.config.cache_dir, cache::ImageType::Episode, id_type_str, &cache_value)?;
    let cache_key = format!("{id_type_str}/{cache_value}");

    // Check caches (memory → filesystem) — may hit now if fast-path suffix
    // differed from actual badges (e.g. a rating source appeared/disappeared).
    let cache_suffix: Arc<str> = suffix.into();
    let release_date = cross_ids.release_date.clone();
    if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
        trigger_episode_refresh(s, k, p, id_type, id_value, &cache_suffix, &settings, image_size);
    }).await? {
        return Ok((bytes, release_date));
    }

    // Request coalescing — concurrent requests for the same episode share one generation
    let rd = release_date.clone();
    let state2 = state.clone();
    let cache_path2 = cache_path.clone();
    let cache_key2 = cache_key.clone();
    let settings2 = settings.clone();
    let bytes: Bytes = state
        .image_inflight
        .try_get_with(cache_key.clone(), async move {
            let rendered = generate_episode(&state2, &resolved, badges, &settings2, image_size).await?;
            let bytes = post_render_cache(&state2, rendered, &cache_path2, &cache_key2, rd.as_deref(), cache::ImageType::Episode, cross_ids, id_type, cache_suffix.to_string()).await;
            Ok::<_, AppError>(bytes)
        })
        .await
        .map_err(|e| match Arc::try_unwrap(e) {
            Ok(app_err) => app_err,
            Err(arc) => AppError::Other(arc.to_string()),
        })?;
    state
        .image_mem_cache
        .insert(cache_key, MemCacheEntry { bytes: bytes.clone(), last_checked: Instant::now() })
        .await;
    Ok((bytes, release_date))
}

/// Serve a logo or backdrop image. Tries TMDB primary with fanart fallback (default),
/// or fanart primary with TMDB fallback when image_source is Fanart.
/// Handles caching and negative-cache lookups.
pub async fn handle_logo_backdrop_inner(
    state: &AppState,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &RenderSettings,
    lb_kind: LogoBackdropKind,
    image_size: Option<ImageSize>,
) -> Result<(Bytes, Option<String>), AppError> {
    let fanart = state.fanart.as_ref();

    let kind: cache::ImageType = lb_kind.into();
    let id_type = IdType::parse(id_type_str)?;
    let id_value = kind.strip_ext(id_value_raw);
    cache::validate_id_value(id_value)?;
    let kind_prefix = kind.kind_prefix();
    let label = kind.label();

    // Use per-type rating limit for logos/backdrops
    let type_ratings_limit = match lb_kind {
        LogoBackdropKind::Logo => settings.logo_ratings_limit,
        LogoBackdropKind::Backdrop => settings.backdrop_ratings_limit,
    };
    let params = LbRenderParams::from_settings(lb_kind, settings);
    let type_badge_style = params.badge_style;
    let type_label_style = params.label_style;

    // Backdrops are language-agnostic (no text) — skip lang/textless entirely.
    // Logos ARE the text — textless makes no sense, only lang matters.
    let fanart_lang = match kind {
        cache::ImageType::Backdrop => "",
        _ => &settings.lang,
    };
    // Logos/backdrops never use textless — logos ARE text, backdrops have none.
    let fanart_textless = false;

    let neg_textless_key = format!("{id_type_str}/{id_value}{kind_prefix}_f_tl_neg");
    let textless_known_missing = fanart_textless
        && state.fanart_negative.get(&neg_textless_key).await.is_some();

    let neg_lang_key = format!("{id_type_str}/{id_value}{kind_prefix}_f_{}_neg", fanart_lang);
    let lang_known_missing = state.fanart_negative.get(&neg_lang_key).await.is_some();

    // When all fanart variants are known-missing, skip the fanart API call in
    // the generation path. We still try TMDB as a fallback rather than
    // returning 404 immediately.
    let skip_fanart = lang_known_missing && (!fanart_textless || textless_known_missing);

    let source_prefix = if settings.image_source.is_fanart() { "_f" } else { "_t" };
    let variant = match kind {
        cache::ImageType::Backdrop => format!("{kind_prefix}{source_prefix}"),
        cache::ImageType::Logo => format!("{kind_prefix}{source_prefix}_{fanart_lang}"),
        cache::ImageType::Poster | cache::ImageType::Episode => return Err(AppError::Other("handle_logo_backdrop_inner only handles logos and backdrops".into())),
    };
    let image_type: cache::ImageType = lb_kind.into();

    let id_key = format!("{id_type_str}/{id_value}");

    // Fast path: try to reconstruct the cache key from SQLite-stored available
    // sources, avoiding external API calls on cache hits.
    // When ratings are disabled the suffix is always "@", skip the SQLite lookup.
    let fast_path_available = if type_ratings_limit == 0 {
        Some(String::new())
    } else {
        read_available_ratings_cached(state, &id_key).await
    };
    if let Some(available) = fast_path_available {
        let ratings_suffix = ratings::badges_suffix_from_available(&available, &settings.ratings_order, type_ratings_limit);
        let suffix = settings_cache_suffix_with_ratings(settings, kind, image_size, &ratings_suffix);
        let cache_key = format!("{id_type_str}/{id_value}{variant}{suffix}");
        let cache_path_base = format!("{id_value}{variant}{suffix}");
        let cache_path = cache::typed_cache_path(&state.config.cache_dir, image_type, id_type_str, &cache_path_base)?;

        let lb_cache_suffix: Arc<str> = format!("{variant}{suffix}").into();
        {
            let id_value = id_value.to_string();
            let lb_cache_suffix = lb_cache_suffix.clone();
            let settings = settings.clone();
            if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
                trigger_logo_backdrop_refresh(s, k, p, id_type, &id_value, &lb_cache_suffix, &settings, lb_kind, image_size);
            }).await? {
                let release_date = cache::read_meta_db(&state.db, &cache_key).await;
                return Ok((bytes, release_date));
            }
        }
    }

    // Slow path: resolve ID and fetch ratings. Episodes are uplifted to
    // their parent series — logos/backdrops don't apply to episodes.
    let skip_ratings = type_ratings_limit == 0;
    let (resolved, ratings_result, cross_ids) =
        resolve_with_ratings(state, id_type, id_value, true, skip_ratings).await?;

    // Persist available sources for future fast-path lookups (always write,
    // even with external_cache_only — this is an optimization index, not a
    // disk cache, and the fast path depends on it).
    // Skip when ratings are disabled: no real source data to store.
    if !skip_ratings {
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
    let lb_cache_suffix: Arc<str> = format!("{variant}{suffix}").into();
    {
        let id_value = id_value.to_string();
        let lb_cache_suffix = lb_cache_suffix.clone();
        let settings = settings.clone();
        if let Some(bytes) = check_caches(state, &cache_key, &cache_path, |s, k, p| {
            trigger_logo_backdrop_refresh(s, k, p, id_type, &id_value, &lb_cache_suffix, &settings, lb_kind, image_size);
        }).await? {
            return Ok((bytes, cross_ids.release_date));
        }
    }

    // Request coalescing — concurrent requests for the same logo/backdrop share one generation
    struct LbGenCtx {
        state: AppState,
        cache_key: String,
        cache_path: std::path::PathBuf,
        lb_cache_suffix: Arc<str>,
        fanart: Option<FanartClient>,
        image_source_is_fanart: bool,
        skip_fanart: bool,
        neg_textless_key: String,
        neg_lang_key: String,
        type_badge_style: BadgeStyle,
        type_label_style: LabelStyle,
        type_position: BadgePosition,
        type_badge_direction: BadgeDirection,
        type_badge_size: BadgeSize,
        badge_size_factor: f32,
        label: &'static str,
        resolved: id::ResolvedId,
        badges: Vec<ratings::RatingBadge>,
        cross_ids: CrossIdInfo,
    }
    let ctx = LbGenCtx {
        state: state.clone(),
        cache_key: cache_key.clone(),
        cache_path: cache_path.clone(),
        lb_cache_suffix: lb_cache_suffix.clone(),
        fanart: fanart.cloned(),
        image_source_is_fanart: settings.image_source.is_fanart(),
        skip_fanart,
        neg_textless_key,
        neg_lang_key,
        type_badge_style,
        type_label_style,
        type_position: params.position,
        type_badge_direction: params.badge_direction,
        type_badge_size: params.badge_size,
        badge_size_factor: params.badge_size.scale_factor(),
        label,
        resolved: resolved.clone(),
        badges,
        cross_ids: cross_ids.clone(),
    };
    let bytes: Bytes = state
        .image_inflight
        .try_get_with(cache_key.clone(), async move {
            let ctx = ctx;

            // Determine source priority.
            // When skip_fanart is set (all fanart variants negative-cached),
            // avoid the fanart API call but still try TMDB.
            let fanart_is_primary = ctx.image_source_is_fanart && ctx.fanart.is_some();
            let try_fanart = !ctx.skip_fanart;
            let not_found = || AppError::IdNotFound(format!("no {} available", ctx.label));

            // Primary source, then fallback
            let (primary, fallback) = if fanart_is_primary {
                // Fanart preferred — try fanart first (unless skipped), then TMDB fallback
                let primary = if try_fanart {
                    if let Some(ref fc) = ctx.fanart {
                        try_fanart_with_negative_cache(
                            fc, &ctx.state, &ctx.resolved,
                            fanart_lang, fanart_textless, kind,
                            &ctx.neg_textless_key, &ctx.neg_lang_key,
                        ).await
                    } else {
                        None
                    }
                } else {
                    None
                };
                let fallback = if primary.is_none() {
                    try_tmdb_logo_backdrop(&ctx.state, &ctx.resolved, kind, fanart_lang, fanart_textless, image_size).await
                } else {
                    None
                };
                (primary, fallback)
            } else {
                // TMDB preferred — try TMDB first, then fanart fallback (unless skipped)
                let primary = try_tmdb_logo_backdrop(&ctx.state, &ctx.resolved, kind, fanart_lang, fanart_textless, image_size).await;
                let fallback = if primary.is_none() && try_fanart {
                    if let Some(ref fc) = ctx.fanart {
                        try_fanart_with_negative_cache(
                            fc, &ctx.state, &ctx.resolved,
                            fanart_lang, fanart_textless, kind,
                            &ctx.neg_textless_key, &ctx.neg_lang_key,
                        ).await
                    } else {
                        None
                    }
                } else {
                    None
                };
                (primary, fallback)
            };
            let image_bytes = match primary.or(fallback) {
                Some(b) => b,
                // Logos: try TMDB(default=en) as final fallback when the
                // requested language wasn't English (avoids a redundant call)
                None if kind == cache::ImageType::Logo && fanart_lang != "en" => {
                    try_tmdb_logo_backdrop(&ctx.state, &ctx.resolved, kind, "en", false, image_size)
                        .await
                        .ok_or_else(not_found)?
                }
                None => return Err(not_found()),
            };

            let resolved_size = resolve_image_size(image_size);
            let (target_width, badge_scale) = match lb_kind {
                LogoBackdropKind::Logo => (
                    resolved_size.logo_target_width(),
                    resolved_size.badge_scale(cache::ImageType::Logo) * ctx.badge_size_factor,
                ),
                LogoBackdropKind::Backdrop => (
                    resolved_size.backdrop_target_width(),
                    resolved_size.badge_scale(cache::ImageType::Backdrop) * ctx.badge_size_factor,
                ),
            };
            let bytes = match lb_kind {
                LogoBackdropKind::Logo => generate::generate_logo(image_bytes, ctx.badges, ctx.state.font.clone(), ctx.type_badge_style, ctx.type_label_style, ctx.state.render_semaphore.clone(), target_width, badge_scale).await?,
                LogoBackdropKind::Backdrop => generate::generate_backdrop(image_bytes, ctx.badges, ctx.state.font.clone(), ctx.state.config.image_quality, ctx.type_position, ctx.type_badge_style, ctx.type_label_style, ctx.type_badge_direction, ctx.state.render_semaphore.clone(), target_width, badge_scale, ctx.type_badge_size).await?,
            };

            let release_date = ctx.cross_ids.release_date.clone();
            let bytes = post_render_cache(&ctx.state, bytes, &ctx.cache_path, &ctx.cache_key, release_date.as_deref(), image_type, ctx.cross_ids, id_type, ctx.lb_cache_suffix.to_string()).await;
            Ok::<_, AppError>(bytes)
        })
        .await
        .map_err(|e| match Arc::try_unwrap(e) {
            Ok(app_err) => app_err,
            Err(arc) => AppError::Other(arc.to_string()),
        })?;

    state
        .image_mem_cache
        .insert(cache_key, MemCacheEntry { bytes: bytes.clone(), last_checked: Instant::now() })
        .await;
    Ok((bytes, cross_ids.release_date))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::db::{BadgeDirection, BadgePosition, BadgeSize, BadgeStyle, ImageSource, LabelStyle, RenderSettings};

    #[test]
    fn image_kind_prefix() {
        assert_eq!(cache::ImageType::Poster.kind_prefix(), "");
        assert_eq!(cache::ImageType::Logo.kind_prefix(), "_l");
        assert_eq!(cache::ImageType::Backdrop.kind_prefix(), "_b");
    }

    #[test]
    fn image_kind_file_ext() {
        assert_eq!(cache::ImageType::Poster.ext(), "jpg");
        assert_eq!(cache::ImageType::Logo.ext(), "png");
        assert_eq!(cache::ImageType::Backdrop.ext(), "jpg");
    }

    #[test]
    fn image_kind_strip_ext() {
        assert_eq!(cache::ImageType::Poster.strip_ext("tt123.jpg"), "tt123");
        assert_eq!(cache::ImageType::Poster.strip_ext("tt123"), "tt123");
        assert_eq!(cache::ImageType::Logo.strip_ext("tt123.png"), "tt123");
        assert_eq!(cache::ImageType::Logo.strip_ext("tt123"), "tt123");
        assert_eq!(cache::ImageType::Backdrop.strip_ext("tt123.jpg"), "tt123");
        assert_eq!(cache::ImageType::Backdrop.strip_ext("tt123"), "tt123");
        // Wrong extension is not stripped
        assert_eq!(cache::ImageType::Logo.strip_ext("tt123.jpg"), "tt123.jpg");
        assert_eq!(cache::ImageType::Poster.strip_ext("tt123.png"), "tt123.png");
    }

    #[test]
    fn image_kind_label() {
        assert_eq!(cache::ImageType::Poster.label(), "poster");
        assert_eq!(cache::ImageType::Logo.label(), "logo");
        assert_eq!(cache::ImageType::Backdrop.label(), "backdrop");
    }

    #[test]
    fn position_cache_suffix_all_positions() {
        assert_eq!(position_cache_suffix("bc"), ".pbc");
        assert_eq!(position_cache_suffix("tc"), ".ptc");
        assert_eq!(position_cache_suffix("l"), ".pl");
        assert_eq!(position_cache_suffix("r"), ".pr");
        assert_eq!(position_cache_suffix("tl"), ".ptl");
        assert_eq!(position_cache_suffix("tr"), ".ptr");
        assert_eq!(position_cache_suffix("bl"), ".pbl");
        assert_eq!(position_cache_suffix("br"), ".pbr");
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
            episode: None,
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
            episode: None,
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
        let s = RenderSettings::default();
        let h1 = settings_hash(&s, cache::ImageType::Poster, None);
        let h2 = settings_hash(&s, cache::ImageType::Poster, None);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32); // 16 bytes = 32 hex chars
    }

    #[test]
    fn settings_hash_differs_by_kind() {
        let s = RenderSettings::default();
        let poster = settings_hash(&s, cache::ImageType::Poster, None);
        let logo = settings_hash(&s, cache::ImageType::Logo, None);
        let backdrop = settings_hash(&s, cache::ImageType::Backdrop, None);
        assert_ne!(poster, logo);
        assert_ne!(poster, backdrop);
        assert_ne!(logo, backdrop);
    }

    #[test]
    fn settings_hash_differs_by_settings() {
        let s1 = RenderSettings::default();
        let mut s2 = RenderSettings::default();
        s2.ratings_limit = 5;
        assert_ne!(
            settings_hash(&s1, cache::ImageType::Poster, None),
            settings_hash(&s2, cache::ImageType::Poster, None)
        );
    }

    #[test]
    fn settings_hash_same_for_equivalent_settings() {
        let mut s1 = RenderSettings::default();
        let mut s2 = RenderSettings::default();
        // Different is_default flag shouldn't affect hash (it's metadata)
        s1.is_default = true;
        s2.is_default = false;
        assert_eq!(
            settings_hash(&s1, cache::ImageType::Poster, None),
            settings_hash(&s2, cache::ImageType::Poster, None)
        );
    }

    #[test]
    fn settings_hash_differs_by_episode_settings() {
        let s1 = RenderSettings::default();
        let mut s2 = RenderSettings::default();
        s2.episode_blur = true;
        assert_ne!(
            settings_hash(&s1, cache::ImageType::Episode, None),
            settings_hash(&s2, cache::ImageType::Episode, None)
        );

        let mut s3 = RenderSettings::default();
        s3.episode_position = crate::services::db::BadgePosition::BottomCenter;
        assert_ne!(
            settings_hash(&s1, cache::ImageType::Episode, None),
            settings_hash(&s3, cache::ImageType::Episode, None)
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
            episode: None,
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
        let s = RenderSettings::default();
        let suffix = settings_cache_suffix(&s, cache::ImageType::Poster, None);
        // Should contain ratings, position, badge style, label style, direction, and size suffixes
        assert!(suffix.contains(".p"), "missing position suffix");
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".d"), "missing badge direction suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_logo_no_position_or_direction() {
        let s = RenderSettings::default();
        let suffix = settings_cache_suffix(&s, cache::ImageType::Logo, None);
        // Logos don't have position or direction
        assert!(!suffix.contains(".p"), "logo should not have position suffix");
        assert!(!suffix.contains(".d"), "logo should not have direction suffix");
        // But should have badge style, label style, and size
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_backdrop_includes_position_and_direction() {
        let s = RenderSettings::default();
        let suffix = settings_cache_suffix(&s, cache::ImageType::Backdrop, None);
        assert!(suffix.contains(".p"), "backdrop should have position suffix");
        assert!(suffix.contains(".d"), "backdrop should have direction suffix");
        assert!(suffix.contains(".s"), "missing badge style suffix");
        assert!(suffix.contains(".l"), "missing label style suffix");
        assert!(suffix.contains(".z"), "missing image size suffix");
    }

    #[test]
    fn settings_cache_suffix_uses_per_kind_settings() {
        let mut s = RenderSettings::default();
        s.poster_badge_style = BadgeStyle::Horizontal;
        s.logo_badge_style = BadgeStyle::Vertical;
        s.backdrop_badge_style = BadgeStyle::Vertical;
        let poster = settings_cache_suffix(&s, cache::ImageType::Poster, None);
        let logo = settings_cache_suffix(&s, cache::ImageType::Logo, None);
        assert!(poster.contains(".sh"), "poster should use poster_badge_style");
        assert!(logo.contains(".sv"), "logo should use logo_badge_style");
    }

    #[test]
    fn settings_cache_suffix_uses_per_kind_ratings_limit() {
        let mut s = RenderSettings::default();
        s.ratings_limit = 3;
        s.logo_ratings_limit = 5;
        s.backdrop_ratings_limit = 2;
        let poster = settings_cache_suffix(&s, cache::ImageType::Poster, None);
        let logo = settings_cache_suffix(&s, cache::ImageType::Logo, None);
        let backdrop = settings_cache_suffix(&s, cache::ImageType::Backdrop, None);
        // Different ratings limits should produce different suffixes
        assert_ne!(poster, logo);
        assert_ne!(logo, backdrop);
    }

    #[test]
    fn settings_cache_suffix_varies_with_image_size() {
        let s = RenderSettings::default();
        let medium = settings_cache_suffix(&s, cache::ImageType::Poster, None);
        let large = settings_cache_suffix(&s, cache::ImageType::Poster, Some(ImageSize::Large));
        assert_ne!(medium, large);
        assert!(medium.ends_with(".zm"));
        assert!(large.ends_with(".zl"));
    }

    #[test]
    fn settings_cache_suffix_ignores_source_fields() {
        let mut s1 = RenderSettings::default();
        let mut s2 = RenderSettings::default();
        // These fields are handled by code path / variant, not suffix
        s1.image_source = ImageSource::Tmdb;
        s2.image_source = ImageSource::Fanart;
        s1.lang = "en".into();
        s2.lang = "de".into();
        s1.textless = false;
        s2.textless = true;
        assert_eq!(
            settings_cache_suffix(&s1, cache::ImageType::Poster, None),
            settings_cache_suffix(&s2, cache::ImageType::Poster, None)
        );
    }

    #[test]
    fn cache_suffix_differs_by_actual_badges() {
        // Two movies with the same settings but different available rating sources
        // should produce different cache keys when using settings_cache_suffix_with_ratings.
        let s = RenderSettings::default();

        let suffix_imdb_rt = settings_cache_suffix_with_ratings(&s, cache::ImageType::Poster, None, "@ir");
        let suffix_imdb_rt_lb = settings_cache_suffix_with_ratings(&s, cache::ImageType::Poster, None, "@irl");
        let suffix_none = settings_cache_suffix_with_ratings(&s, cache::ImageType::Poster, None, "@");

        assert_ne!(suffix_imdb_rt, suffix_imdb_rt_lb, "different badge sets must produce different suffixes");
        assert_ne!(suffix_imdb_rt, suffix_none, "badges vs no badges must differ");
        assert_ne!(suffix_imdb_rt_lb, suffix_none);

        // Same badges should produce the same suffix
        let suffix_imdb_rt_2 = settings_cache_suffix_with_ratings(&s, cache::ImageType::Poster, None, "@ir");
        assert_eq!(suffix_imdb_rt, suffix_imdb_rt_2);
    }

    // --- LbRenderParams tests ---

    #[test]
    fn lb_render_params_logo_uses_hardcoded_defaults() {
        let settings = RenderSettings {
            logo_badge_style: BadgeStyle::Horizontal,
            logo_label_style: LabelStyle::Icon,
            logo_badge_size: BadgeSize::Large,
            // Backdrop fields should be ignored for logo
            backdrop_position: BadgePosition::BottomLeft,
            backdrop_badge_direction: BadgeDirection::Horizontal,
            backdrop_badge_style: BadgeStyle::Vertical,
            ..RenderSettings::default()
        };
        let p = LbRenderParams::from_settings(LogoBackdropKind::Logo, &settings);
        assert_eq!(p.position, BadgePosition::TopRight);
        assert_eq!(p.badge_direction, BadgeDirection::Vertical);
        assert_eq!(p.badge_style, BadgeStyle::Horizontal);
        assert_eq!(p.label_style, LabelStyle::Icon);
        assert_eq!(p.badge_size, BadgeSize::Large);
    }

    #[test]
    fn lb_render_params_backdrop_reads_settings() {
        let settings = RenderSettings {
            backdrop_position: BadgePosition::BottomCenter,
            backdrop_badge_direction: BadgeDirection::Horizontal,
            backdrop_badge_style: BadgeStyle::Vertical,
            backdrop_label_style: LabelStyle::Official,
            backdrop_badge_size: BadgeSize::ExtraSmall,
            ..RenderSettings::default()
        };
        let p = LbRenderParams::from_settings(LogoBackdropKind::Backdrop, &settings);
        assert_eq!(p.position, BadgePosition::BottomCenter);
        assert_eq!(p.badge_direction, BadgeDirection::Horizontal);
        // badge_style Vertical stays Vertical (not Default), unaffected by direction
        assert_eq!(p.badge_style, BadgeStyle::Vertical);
        assert_eq!(p.label_style, LabelStyle::Official);
        assert_eq!(p.badge_size, BadgeSize::ExtraSmall);
    }

    #[test]
    fn lb_render_params_backdrop_resolves_default_direction() {
        // BottomCenter is center-horizontal → Default direction resolves to Horizontal
        let settings = RenderSettings {
            backdrop_position: BadgePosition::BottomCenter,
            backdrop_badge_direction: BadgeDirection::Default,
            backdrop_badge_style: BadgeStyle::Default,
            ..RenderSettings::default()
        };
        let p = LbRenderParams::from_settings(LogoBackdropKind::Backdrop, &settings);
        assert_eq!(p.badge_direction, BadgeDirection::Horizontal);
        // Default style resolves to match direction (Horizontal)
        assert_eq!(p.badge_style, BadgeStyle::Horizontal);
    }

    #[test]
    fn lb_render_params_backdrop_resolves_default_direction_vertical() {
        // TopRight is not center-horizontal → Default direction resolves to Vertical
        let settings = RenderSettings {
            backdrop_position: BadgePosition::TopRight,
            backdrop_badge_direction: BadgeDirection::Default,
            backdrop_badge_style: BadgeStyle::Default,
            ..RenderSettings::default()
        };
        let p = LbRenderParams::from_settings(LogoBackdropKind::Backdrop, &settings);
        assert_eq!(p.badge_direction, BadgeDirection::Vertical);
        assert_eq!(p.badge_style, BadgeStyle::Vertical);
    }

    #[test]
    fn tmdb_poster_variant_all_combinations() {
        // Default case: no variant (backward compatible)
        assert_eq!(tmdb_poster_variant("en", false), "");
        // English + textless
        assert_eq!(tmdb_poster_variant("en", true), "_t_tl");
        // Non-English, no textless
        assert_eq!(tmdb_poster_variant("de", false), "_t_de");
        // Non-English + textless
        assert_eq!(tmdb_poster_variant("de", true), "_t_de_tl");
        assert_eq!(tmdb_poster_variant("fr", false), "_t_fr");
        assert_eq!(tmdb_poster_variant("ja", true), "_t_ja_tl");
    }
}
