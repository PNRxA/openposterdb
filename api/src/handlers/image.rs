use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::cache;
use crate::error::AppError;
use crate::handlers::auth::hash_api_key;
use crate::image::serve;
use crate::services::db;
use crate::services::db::{
    BadgeBackground, BadgeDirection, BadgeShape, BadgeSize, BadgeStyle, LabelStyle, LangIcon, BadgePosition, ImageSource,
    PosterFit, QualityStyle, RenderSettings,
};
use crate::AppState;

pub const FREE_API_KEY: &str = "t0-free-rpdb";

/// OpenAPI-only enum for the `id_type` path parameter.
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum IdTypeParam {
    Imdb,
    Tmdb,
    Tvdb,
}

/// OpenAPI-only enum for the `imageSize` query parameter.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub enum ImageSizeParam {
    #[schema(rename = "small")]
    Small,
    #[schema(rename = "medium")]
    Medium,
    #[schema(rename = "large")]
    Large,
    #[schema(rename = "very-large")]
    VeryLarge,
    #[schema(rename = "verylarge")]
    VeryLargeAlt,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ImageQuery {
    /// Accepted for RPDB compatibility but has no effect. OpenPosterDB uses TMDB as a fallback source instead of returning a 1x1 pixel placeholder.
    #[serde(default)]
    #[param(value_type = Option<String>, example = "true")]
    pub fallback: Option<String>,
    /// Language code for Fanart.tv image selection (e.g. `en`, `de`, `pt-BR`). 2-5 alphanumeric characters or hyphens.
    #[serde(default)]
    #[param(value_type = Option<String>, pattern = r"^[a-zA-Z0-9\-]{2,5}$")]
    pub lang: Option<String>,
    /// Output image size. `small` is only valid for backdrops. Defaults to `medium`.
    #[serde(default, rename = "imageSize")]
    #[param(rename = "imageSize", default = "medium", value_type = Option<ImageSizeParam>)]
    pub image_size: Option<String>,
    /// Maximum number of rating badges to display (0–10).
    #[serde(default)]
    #[param(value_type = Option<i32>)]
    pub ratings_limit: Option<i32>,
    /// Comma-separated rating source keys controlling display order (e.g. `imdb,tmdb,rt`).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub ratings_order: Option<String>,
    /// Comma-separated rating source keys to exclude from display (e.g. `rt,trakt`). Applied before ordering and limiting, so excluded sources free their badge slots for the next preferred source.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub ratings_exclude: Option<String>,
    /// Badge layout style: `h` (horizontal), `v` (vertical), `d` (default).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub badge_style: Option<BadgeStyle>,
    /// Label rendering style: `t` (text), `i` (icon), `o` (official).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub label_style: Option<LabelStyle>,
    /// Badge size: `xs`, `s`, `m`, `l`, `xl`.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub badge_size: Option<BadgeSize>,
    /// Badge stacking direction (poster only): `d` (default), `h` (horizontal), `v` (vertical).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub badge_direction: Option<BadgeDirection>,
    /// Badge corner shape: `r` (rounded), `p` (pill).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub badge_shape: Option<BadgeShape>,
    /// Badge background: `d` (default), `k` (dark), `t` (transparent), `n` (none).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub badge_background: Option<BadgeBackground>,
    /// Badge anchor position: `bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br`.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub position: Option<BadgePosition>,
    /// Image source: `t` (TMDB), `f` (Fanart.tv).
    #[serde(default, alias = "poster_source")]
    #[param(value_type = Option<String>)]
    pub image_source: Option<ImageSource>,
    /// Prefer textless images when available.
    #[serde(default, alias = "fanart_textless")]
    #[param(value_type = Option<bool>)]
    pub textless: Option<bool>,
    /// Apply Gaussian blur to the base image (episode only, for spoiler protection).
    #[serde(default)]
    #[param(value_type = Option<bool>)]
    pub blur: Option<bool>,
    /// Split badges across two opposite sides of the poster (poster only).
    #[serde(default)]
    #[param(value_type = Option<bool>)]
    pub split: Option<bool>,
    /// Poster fit to the 2:3 frame (poster only): `native`, `cover`, `pad`, `blur`.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub fit: Option<PosterFit>,
    /// Distance (0–50, percent of width) to inset backdrop ratings from the
    /// anchored horizontal edge (backdrop only). Ignored for centered positions.
    #[serde(default)]
    #[param(value_type = Option<i32>)]
    pub edge_inset_x: Option<i32>,
    /// Distance (0–50, percent of height) to inset backdrop ratings from the
    /// anchored vertical edge (backdrop only). Ignored for centered positions.
    #[serde(default)]
    #[param(value_type = Option<i32>)]
    pub edge_inset_y: Option<i32>,
    /// Caller-supplied media quality tiers for the quality overlay badge, comma
    /// separated and stackable: any of `4k`, `1080p`, `720p`, `hdr`, `dv`
    /// (e.g. `4k,dv`). There is no quality metadata server-side, so this must be
    /// supplied by the caller. Empty/absent shows no quality badge.
    #[serde(default)]
    #[param(value_type = Option<String>, example = "4k,dv")]
    pub quality: Option<String>,
    /// How the quality badge renders: `text` (chip) or `logo` (brand logo).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub quality_style: Option<QualityStyle>,
    /// Main-language overlay badge: `off`, `flag`, or `text`.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub lang_icon: Option<LangIcon>,
    /// Override the title's detected language for the language badge (ISO 639-1,
    /// e.g. `ja`). When absent, TMDB `original_language` is used.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub lang_code: Option<String>,
    /// Comma-separated languages to hide the language badge for (e.g. `en` to
    /// show it on every title except English ones). Matches the title's main
    /// language.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub lang_exclude: Option<String>,
    /// Anchor position for the quality badge, independent of the ratings and the
    /// language badge: `bc`, `tc`, `l`, `r`, `tl`, `tr`, `bl`, `br` (default `tr`).
    /// Ignored on logos.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub quality_position: Option<BadgePosition>,
    /// Anchor position for the main-language badge, independent of the ratings
    /// and the quality badge (default `tl`). Ignored on logos.
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub lang_position: Option<BadgePosition>,
    /// Layout direction for stacked quality badges: `d` (auto — resolved from
    /// the quality anchor: a column at corner/side positions, a row at
    /// top/bottom-center), `h` (horizontal row), `v` (vertical column).
    #[serde(default)]
    #[param(value_type = Option<String>)]
    pub quality_direction: Option<BadgeDirection>,
}

impl ImageQuery {
    /// Returns `true` if any render-setting override query parameter is present.
    fn has_overrides(&self) -> bool {
        self.ratings_limit.is_some()
            || self.ratings_order.is_some()
            || self.ratings_exclude.is_some()
            || self.badge_style.is_some()
            || self.label_style.is_some()
            || self.badge_size.is_some()
            || self.badge_direction.is_some()
            || self.badge_shape.is_some()
            || self.badge_background.is_some()
            || self.position.is_some()
            || self.image_source.is_some()
            || self.textless.is_some()
            || self.blur.is_some()
            || self.split.is_some()
            || self.fit.is_some()
            || self.edge_inset_x.is_some()
            || self.edge_inset_y.is_some()
            || self.quality.is_some()
            || self.quality_style.is_some()
            || self.lang_icon.is_some()
            || self.lang_code.is_some()
            || self.quality_position.is_some()
            || self.lang_position.is_some()
            || self.quality_direction.is_some()
            || self.lang_exclude.is_some()
    }
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

/// Public, read-only view of the global default render settings the free API
/// key serves with. Mirrors the per-type fields the "Try it out" UI needs so it
/// can show the operator's actual configuration instead of hardcoded frontend
/// defaults. This is a strict subset of the admin `GlobalSettingsResponse` —
/// it deliberately omits operational flags (`fanart_available`,
/// `free_api_key_locked`) so the public endpoint can't leak them.
#[derive(Serialize)]
pub struct FreeKeySettingsResponse {
    pub image_source: ImageSource,
    pub lang: String,
    pub textless: bool,
    pub ratings_limit: i32,
    pub ratings_order: String,
    pub ratings_exclude: String,
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
    pub quality_style: QualityStyle,
    pub poster_lang_icon: LangIcon,
    pub logo_lang_icon: LangIcon,
    pub backdrop_lang_icon: LangIcon,
    pub poster_quality_position: BadgePosition,
    pub backdrop_quality_position: BadgePosition,
    pub poster_lang_position: BadgePosition,
    pub backdrop_lang_position: BadgePosition,
    pub quality_direction: BadgeDirection,
    pub lang_exclude: String,
}

impl From<&RenderSettings> for FreeKeySettingsResponse {
    fn from(s: &RenderSettings) -> Self {
        Self {
            image_source: s.image_source,
            lang: s.lang.to_string(),
            textless: s.textless,
            ratings_limit: s.ratings_limit,
            ratings_order: s.ratings_order.to_string(),
            ratings_exclude: s.ratings_exclude.to_string(),
            poster_position: s.poster_position,
            logo_ratings_limit: s.logo_ratings_limit,
            backdrop_ratings_limit: s.backdrop_ratings_limit,
            poster_badge_style: s.poster_badge_style,
            logo_badge_style: s.logo_badge_style,
            backdrop_badge_style: s.backdrop_badge_style,
            poster_label_style: s.poster_label_style,
            logo_label_style: s.logo_label_style,
            backdrop_label_style: s.backdrop_label_style,
            poster_badge_direction: s.poster_badge_direction,
            poster_badge_split: s.poster_badge_split,
            poster_fit: s.poster_fit,
            poster_badge_size: s.poster_badge_size,
            logo_badge_size: s.logo_badge_size,
            backdrop_badge_size: s.backdrop_badge_size,
            backdrop_position: s.backdrop_position,
            backdrop_badge_direction: s.backdrop_badge_direction,
            backdrop_edge_inset_x: s.backdrop_edge_inset_x,
            backdrop_edge_inset_y: s.backdrop_edge_inset_y,
            episode_ratings_limit: s.episode_ratings_limit,
            episode_badge_style: s.episode_badge_style,
            episode_label_style: s.episode_label_style,
            episode_badge_size: s.episode_badge_size,
            episode_position: s.episode_position,
            episode_badge_direction: s.episode_badge_direction,
            episode_blur: s.episode_blur,
            poster_badge_shape: s.poster_badge_shape,
            logo_badge_shape: s.logo_badge_shape,
            backdrop_badge_shape: s.backdrop_badge_shape,
            episode_badge_shape: s.episode_badge_shape,
            poster_badge_background: s.poster_badge_background,
            logo_badge_background: s.logo_badge_background,
            backdrop_badge_background: s.backdrop_badge_background,
            episode_badge_background: s.episode_badge_background,
            quality_style: s.quality_style,
            poster_lang_icon: s.poster_lang_icon,
            logo_lang_icon: s.logo_lang_icon,
            backdrop_lang_icon: s.backdrop_lang_icon,
            poster_quality_position: s.poster_quality_position,
            backdrop_quality_position: s.backdrop_quality_position,
            poster_lang_position: s.poster_lang_position,
            backdrop_lang_position: s.backdrop_lang_position,
            quality_direction: s.quality_direction,
            lang_exclude: s.lang_exclude.to_string(),
        }
    }
}

/// `GET /api/free-key/settings` — the global default render settings the free
/// API key serves with, so the public "Try it out" UI reflects what the server
/// actually produces. Returns 401 when the free key is disabled (same gating as
/// the free-key image routes). Reads from `global_settings_cache`, so it never
/// touches the database on the hot path.
pub async fn free_key_settings(State(state): State<Arc<AppState>>) -> Response {
    match resolve_free_settings(&state).await {
        Ok(settings) => axum::Json(FreeKeySettingsResponse::from(&*settings)).into_response(),
        Err(resp) => resp,
    }
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
        Ok(_) => axum::Json(serde_json::json!({ "valid": true })).into_response(),
        Err(resp) => resp,
    }
}

/// Apply query-parameter overrides to render settings, returning a new `Arc` only
/// when at least one override is present. Poster-only params are silently ignored
/// on logo/backdrop endpoints.
fn apply_query_overrides(
    settings: Arc<db::RenderSettings>,
    query: &ImageQuery,
    kind: cache::ImageType,
) -> Result<Arc<db::RenderSettings>, Response> {
    if !query.has_overrides() {
        return Ok(settings);
    }

    let mut s = (*settings).clone();

    // -- shared overrides (image-type-aware) --
    if let Some(limit) = query.ratings_limit {
        db::validate_ratings_limit(limit).map_err(|e| e.into_response())?;
        match kind {
            cache::ImageType::Poster => s.ratings_limit = limit,
            cache::ImageType::Logo => s.logo_ratings_limit = limit,
            cache::ImageType::Backdrop => s.backdrop_ratings_limit = limit,
            cache::ImageType::Episode => s.episode_ratings_limit = limit,
        }
    }
    if let Some(ref order) = query.ratings_order {
        db::validate_ratings_order(order).map_err(|e| e.into_response())?;
        s.ratings_order = Arc::from(order.as_str());
    }
    // Exclusion applies to every image type (it is not per-type, unlike ratings_limit).
    if let Some(ref exclude) = query.ratings_exclude {
        db::validate_ratings_exclude(exclude).map_err(|e| e.into_response())?;
        s.ratings_exclude = Arc::from(exclude.as_str());
    }
    if let Some(style) = query.badge_style {
        match kind {
            cache::ImageType::Poster => s.poster_badge_style = style,
            cache::ImageType::Logo => s.logo_badge_style = style,
            cache::ImageType::Backdrop => s.backdrop_badge_style = style,
            cache::ImageType::Episode => s.episode_badge_style = style,
        }
    }
    if let Some(style) = query.label_style {
        match kind {
            cache::ImageType::Poster => s.poster_label_style = style,
            cache::ImageType::Logo => s.logo_label_style = style,
            cache::ImageType::Backdrop => s.backdrop_label_style = style,
            cache::ImageType::Episode => s.episode_label_style = style,
        }
    }
    if let Some(size) = query.badge_size {
        match kind {
            cache::ImageType::Poster => s.poster_badge_size = size,
            cache::ImageType::Logo => s.logo_badge_size = size,
            cache::ImageType::Backdrop => s.backdrop_badge_size = size,
            cache::ImageType::Episode => s.episode_badge_size = size,
        }
    }
    if let Some(shape) = query.badge_shape {
        match kind {
            cache::ImageType::Poster => s.poster_badge_shape = shape,
            cache::ImageType::Logo => s.logo_badge_shape = shape,
            cache::ImageType::Backdrop => s.backdrop_badge_shape = shape,
            cache::ImageType::Episode => s.episode_badge_shape = shape,
        }
    }
    if let Some(background) = query.badge_background {
        match kind {
            cache::ImageType::Poster => s.poster_badge_background = background,
            cache::ImageType::Logo => s.logo_badge_background = background,
            cache::ImageType::Backdrop => s.backdrop_badge_background = background,
            cache::ImageType::Episode => s.episode_badge_background = background,
        }
    }

    // -- position and direction overrides (poster, backdrop, episode) --
    if kind == cache::ImageType::Poster {
        if let Some(dir) = query.badge_direction {
            s.poster_badge_direction = dir;
        }
        if let Some(pos) = query.position {
            s.poster_position = pos;
        }
        if let Some(split) = query.split {
            s.poster_badge_split = split;
        }
        if let Some(fit) = query.fit {
            s.poster_fit = fit;
        }
    }
    if kind == cache::ImageType::Backdrop {
        if let Some(dir) = query.badge_direction {
            s.backdrop_badge_direction = dir;
        }
        if let Some(pos) = query.position {
            s.backdrop_position = pos;
        }
        if let Some(inset) = query.edge_inset_x {
            s.backdrop_edge_inset_x = db::clamp_edge_inset(inset);
        }
        if let Some(inset) = query.edge_inset_y {
            s.backdrop_edge_inset_y = db::clamp_edge_inset(inset);
        }
    }
    if kind == cache::ImageType::Episode {
        if let Some(dir) = query.badge_direction {
            s.episode_badge_direction = dir;
        }
        if let Some(pos) = query.position {
            s.episode_position = pos;
        }
        if let Some(blur) = query.blur {
            s.episode_blur = blur;
        }
    }

    // -- source override (applies to all image types) --
    if let Some(src) = query.image_source {
        s.image_source = src;
    }
    // -- textless is poster-only (logos ARE text, backdrops have none) --
    if kind == cache::ImageType::Poster {
        if let Some(textless) = query.textless {
            s.textless = textless;
        }
    }

    // -- quality + main-language overlay badges (apply to all image types) --
    if let Some(ref q) = query.quality {
        db::validate_quality(q).map_err(|e| e.into_response())?;
        s.quality = Arc::from(q.as_str());
    }
    if let Some(style) = query.quality_style {
        s.quality_style = style;
    }
    // The language badge is configurable per image type; logos/episodes that
    // support it map to their own field, episodes ignore it.
    if let Some(icon) = query.lang_icon {
        match kind {
            cache::ImageType::Poster => s.poster_lang_icon = icon,
            cache::ImageType::Logo => s.logo_lang_icon = icon,
            cache::ImageType::Backdrop => s.backdrop_lang_icon = icon,
            cache::ImageType::Episode => {}
        }
    }
    if let Some(ref code) = query.lang_code {
        db::validate_lang_code(code).map_err(|e| e.into_response())?;
        s.lang_code = Some(Arc::from(code.as_str()));
    }
    // Overlay anchor positions are per-image-type (poster vs backdrop); logos
    // and episodes don't use them, so the override is ignored there.
    if let Some(pos) = query.quality_position {
        match kind {
            cache::ImageType::Poster => s.poster_quality_position = pos,
            cache::ImageType::Backdrop => s.backdrop_quality_position = pos,
            cache::ImageType::Logo | cache::ImageType::Episode => {}
        }
    }
    if let Some(pos) = query.lang_position {
        match kind {
            cache::ImageType::Poster => s.poster_lang_position = pos,
            cache::ImageType::Backdrop => s.backdrop_lang_position = pos,
            cache::ImageType::Logo | cache::ImageType::Episode => {}
        }
    }
    if let Some(dir) = query.quality_direction {
        s.quality_direction = dir;
    }
    if let Some(ref exclude) = query.lang_exclude {
        db::validate_lang_exclude(exclude).map_err(|e| e.into_response())?;
        s.lang_exclude = Arc::from(exclude.as_str());
    }

    Ok(Arc::new(s))
}

/// URL path segment used in CDN redirect URLs for each image type.
fn cdn_route_segment(kind: cache::ImageType) -> &'static str {
    match kind {
        cache::ImageType::Poster => "poster-default",
        cache::ImageType::Logo => "logo-default",
        cache::ImageType::Backdrop => "backdrop-default",
        cache::ImageType::Episode => "episode-default",
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
    if let Some(size) = image_size {
        url.push('?');
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

/// Dispatch image generation to the correct backend (poster vs logo/backdrop).
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
            serve::handle_logo_backdrop_inner(state, id_type_str, id_value_raw, settings, serve::LogoBackdropKind::Logo, image_size).await
        }
        cache::ImageType::Backdrop => {
            serve::handle_logo_backdrop_inner(state, id_type_str, id_value_raw, settings, serve::LogoBackdropKind::Backdrop, image_size).await
        }
        cache::ImageType::Episode => {
            serve::handle_episode_inner(state, id_type_str, id_value_raw, settings.clone(), image_size).await
        }
    }
}

async fn serve_image(
    state: &Arc<AppState>,
    id_type_str: &str,
    id_value_raw: &str,
    settings: &db::RenderSettings,
    kind: cache::ImageType,
    image_size: Option<db::ImageSize>,
) -> Response {
    let content_type = kind.content_type();
    match dispatch_image(state, id_type_str, id_value_raw, settings, kind, image_size).await {
        Ok((bytes, _)) => serve::image_response(bytes, content_type),
        Err(e) => e.into_response(),
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

    let settings = match resolve_settings(&state, api_key).await {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    // Apply ?lang= override directly
    let settings = if let Some(ref lang) = query.lang {
        match db::validate_lang(lang) {
            Ok(()) => {
                let mut s = (*settings).clone();
                s.lang = Arc::from(lang.as_str());
                Arc::new(s)
            }
            Err(e) => return e.into_response(),
        }
    } else {
        settings
    };
    let settings = match apply_query_overrides(settings, &query, kind) {
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
        image_size,
    )
    .await
    {
        return redirect;
    }

    serve_image(&state, id_type_str, id_value_raw, &settings, kind, image_size).await
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
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-`, `series-`, or `episode-` (e.g. `movie-550`, `series-1399`, `episode-1396-S1E1`). For TVDB use the numeric ID. Episode IMDb IDs (e.g. `tt0959621`) and TVDB episode IDs are also supported."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Poster image", content_type = "image/jpeg",
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found."),
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
    description = "Returns a PNG logo image with rating badge overlays for the given media item. Uses TMDB as the default source with Fanart.tv as fallback (or vice versa when configured).",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-`, `series-`, or `episode-` (e.g. `movie-550`, `series-1399`, `episode-1396-S1E1`). For TVDB use the numeric ID. Episode IMDb IDs (e.g. `tt0959621`) and TVDB episode IDs are also supported."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Logo image", content_type = "image/png",
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found."),
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
    description = "Returns a JPEG backdrop image with rating badge overlays for the given media item. Uses TMDB as the default source with Fanart.tv as fallback (or vice versa when configured).",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-`, `series-`, or `episode-` (e.g. `movie-550`, `series-1399`, `episode-1396-S1E1`). For TVDB use the numeric ID. Episode IMDb IDs (e.g. `tt0959621`) and TVDB episode IDs are also supported."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Backdrop image", content_type = "image/jpeg",
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Media item not found."),
    ),
)]
pub async fn backdrop_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    image_handler_inner(state, &api_key, &id_type_str, &id_value, query, cache::ImageType::Backdrop).await
}

#[utoipa::path(
    get,
    path = "/{api_key}/{id_type}/episode-default/{id_value}",
    operation_id = "getEpisode",
    tag = "Images",
    summary = "Get episode",
    description = "Returns a JPEG episode still image with rating badge overlays. Supports optional Gaussian blur for spoiler protection. Falls back to the series poster when no episode still is available.",
    params(
        ("api_key" = String, Path, description = "Your API key (64-character hex string). Use `t0-free-rpdb` as a free public key if enabled on this instance."),
        ("id_type" = IdTypeParam, Path, description = "The type of media ID being used.", example = "imdb"),
        ("id_value" = String, Path, description = "The media ID value. For IMDB use the `tt` prefixed ID (e.g. `tt1234567`). For TMDB prefix with `movie-`, `series-`, or `episode-` (e.g. `movie-550`, `series-1399`, `episode-1396-S1E1`). For TVDB use the numeric ID. Episode IMDb IDs (e.g. `tt0959621`) and TVDB episode IDs are also supported."),
        ImageQuery,
    ),
    responses(
        (status = 200, description = "Episode image", content_type = "image/jpeg",
            headers(("Cache-Control" = String, description = "Cache directive, e.g. `public, max-age=3600, stale-while-revalidate=86400`"))),
        (status = 400, description = "Invalid request — bad ID type, image size, or language format, or non-episode ID used on episode endpoint."),
        (status = 401, description = "Invalid or missing API key."),
        (status = 404, description = "Episode not found."),
    ),
)]
pub async fn episode_handler(
    State(state): State<Arc<AppState>>,
    Path((api_key, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    image_handler_inner(state, &api_key, &id_type_str, &id_value, query, cache::ImageType::Episode).await
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
        Err(e) => cdn_error_response(e),
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

pub async fn cdn_episode_handler(
    State(state): State<Arc<AppState>>,
    Path((settings_hash, id_type_str, id_value)): Path<(String, String, String)>,
    Query(query): Query<ImageQuery>,
) -> Response {
    cdn_handler_inner(state, &settings_hash, &id_type_str, &id_value, query, cache::ImageType::Episode).await
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

    fn empty_query() -> ImageQuery {
        ImageQuery {
            fallback: None,
            lang: None,
            image_size: None,
            ratings_limit: None,
            ratings_order: None,
            ratings_exclude: None,
            badge_style: None,
            label_style: None,
            badge_size: None,
            badge_direction: None,
            badge_shape: None,
            badge_background: None,
            position: None,
            image_source: None,
            textless: None,
            blur: None,
            split: None,
            fit: None,
            edge_inset_x: None,
            edge_inset_y: None,
            quality: None,
            quality_style: None,
            lang_icon: None,
            lang_code: None,
            quality_position: None,
            lang_position: None,
            quality_direction: None,
            lang_exclude: None,
        }
    }

    #[test]
    fn apply_query_overrides_no_overrides_returns_same_arc() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = empty_query();
        let result =
            apply_query_overrides(settings.clone(), &query, cache::ImageType::Poster).unwrap();
        assert!(Arc::ptr_eq(&settings, &result));
    }

    #[test]
    fn apply_query_overrides_poster_maps_correctly() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            ratings_limit: Some(3),
            badge_style: Some(BadgeStyle::Horizontal),
            label_style: Some(LabelStyle::Icon),
            badge_size: Some(BadgeSize::Large),
            badge_direction: Some(BadgeDirection::Horizontal),
            position: Some(BadgePosition::TopLeft),
            image_source: Some(ImageSource::Fanart),
            textless: Some(true),
            ratings_order: Some("imdb,tmdb".into()),
            ratings_exclude: Some("rt".into()),
            split: Some(true),
            fit: Some(PosterFit::Pad),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Poster).unwrap();
        assert_eq!(result.ratings_limit, 3);
        assert_eq!(&*result.ratings_exclude, "rt");
        assert_eq!(result.poster_badge_style, BadgeStyle::Horizontal);
        assert_eq!(result.poster_label_style, LabelStyle::Icon);
        assert_eq!(result.poster_badge_size, BadgeSize::Large);
        assert_eq!(result.poster_badge_direction, BadgeDirection::Horizontal);
        assert_eq!(result.poster_position, BadgePosition::TopLeft);
        assert_eq!(result.image_source, ImageSource::Fanart);
        assert_eq!(result.poster_fit, PosterFit::Pad);
        assert!(result.textless);
        assert!(result.poster_badge_split);
        assert_eq!(&*result.ratings_order, "imdb,tmdb");
    }

    #[test]
    fn apply_query_overrides_logo_maps_correctly_ignores_poster_only() {
        let settings = Arc::new(db::RenderSettings::default());
        let original_position = settings.poster_position;
        let original_direction = settings.poster_badge_direction;

        let query = ImageQuery {
            ratings_limit: Some(2),
            badge_style: Some(BadgeStyle::Vertical),
            label_style: Some(LabelStyle::Text),
            badge_size: Some(BadgeSize::Small),
            badge_direction: Some(BadgeDirection::Horizontal),
            position: Some(BadgePosition::TopRight),
            image_source: Some(ImageSource::Fanart),
            textless: Some(true),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Logo).unwrap();
        // Logo-specific fields applied
        assert_eq!(result.logo_ratings_limit, 2);
        assert_eq!(result.logo_badge_style, BadgeStyle::Vertical);
        assert_eq!(result.logo_label_style, LabelStyle::Text);
        assert_eq!(result.logo_badge_size, BadgeSize::Small);
        // Poster-only fields unchanged
        assert_eq!(result.poster_position, original_position);
        assert_eq!(result.poster_badge_direction, original_direction);
        // Source override applies to all image types
        assert_eq!(result.image_source, ImageSource::Fanart);
        // Textless is poster-only — should remain at default for logo
        assert!(!result.textless);
    }

    #[test]
    fn apply_query_overrides_backdrop_maps_correctly() {
        let settings = Arc::new(db::RenderSettings::default());
        let original_poster_position = settings.poster_position;
        let original_poster_direction = settings.poster_badge_direction;

        let query = ImageQuery {
            ratings_limit: Some(5),
            badge_style: Some(BadgeStyle::Default),
            position: Some(BadgePosition::BottomCenter),
            badge_direction: Some(BadgeDirection::Horizontal),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Backdrop).unwrap();
        assert_eq!(result.backdrop_ratings_limit, 5);
        assert_eq!(result.backdrop_badge_style, BadgeStyle::Default);
        assert_eq!(result.backdrop_position, BadgePosition::BottomCenter);
        assert_eq!(result.backdrop_badge_direction, BadgeDirection::Horizontal);

        // Poster fields unchanged
        assert_eq!(result.poster_position, original_poster_position);
        assert_eq!(result.poster_badge_direction, original_poster_direction);
    }

    #[test]
    fn apply_query_overrides_backdrop_edge_inset() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            edge_inset_x: Some(12),
            edge_inset_y: Some(7),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Backdrop).unwrap();
        assert_eq!(result.backdrop_edge_inset_x, 12);
        assert_eq!(result.backdrop_edge_inset_y, 7);
    }

    #[test]
    fn apply_query_overrides_clamps_out_of_range_edge_inset() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            edge_inset_x: Some(999),
            edge_inset_y: Some(-5),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Backdrop).unwrap();
        assert_eq!(result.backdrop_edge_inset_x, db::MAX_EDGE_INSET);
        assert_eq!(result.backdrop_edge_inset_y, 0);
    }

    #[test]
    fn apply_query_overrides_edge_inset_ignored_for_poster() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            edge_inset_x: Some(20),
            edge_inset_y: Some(20),
            ..empty_query()
        };
        // edge_inset counts as an override (forces a fresh Arc) but only the
        // backdrop path consumes it — posters are unaffected.
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Poster).unwrap();
        assert_eq!(result.backdrop_edge_inset_x, 0);
        assert_eq!(result.backdrop_edge_inset_y, 0);
    }

    #[test]
    fn apply_query_overrides_rejects_invalid_ratings_limit() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            ratings_limit: Some(11),
            ..empty_query()
        };
        let result = apply_query_overrides(settings, &query, cache::ImageType::Poster);
        assert!(result.is_err());
    }

    #[test]
    fn apply_query_overrides_rejects_invalid_ratings_order() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            ratings_order: Some("bogus_source".into()),
            ..empty_query()
        };
        let result = apply_query_overrides(settings, &query, cache::ImageType::Poster);
        assert!(result.is_err());
    }

    #[test]
    fn apply_query_overrides_rejects_invalid_ratings_exclude() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            ratings_exclude: Some("bogus_source".into()),
            ..empty_query()
        };
        let result = apply_query_overrides(settings, &query, cache::ImageType::Poster);
        assert!(result.is_err());
    }

    #[test]
    fn apply_query_overrides_ratings_exclude_applies_to_logo() {
        // Exclusion is not poster-only — it should apply on every image type.
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            ratings_exclude: Some("rt,trakt".into()),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Logo).unwrap();
        assert_eq!(&*result.ratings_exclude, "rt,trakt");
    }

    #[test]
    fn apply_query_overrides_textless_false() {
        let settings = Arc::new(db::RenderSettings::default());
        let query = ImageQuery {
            textless: Some(false),
            ..empty_query()
        };
        let result =
            apply_query_overrides(settings, &query, cache::ImageType::Poster).unwrap();
        assert!(!result.textless);
    }
}
