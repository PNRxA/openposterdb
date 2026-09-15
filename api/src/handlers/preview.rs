use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, Rgba, RgbaImage};
use std::sync::{Arc, LazyLock};

use crate::cache;
use crate::error::AppError;
use crate::handlers::image::ImageQuery;
use crate::image::generate;
use crate::image::serve;
use crate::services::db::{self, BadgeAppearance, BadgeDirection, BadgeStyle, BadgePosition, RenderSettings};
#[cfg(test)]
use crate::services::db::{BadgeSize, LabelStyle};
use crate::services::ratings::{self, RatingBadge, RatingSource};
use crate::AppState;

/// A 500x750 dark gray gradient poster, computed once.
static SAMPLE_POSTER_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let width = 500u32;
    let height = 750u32;
    let img = RgbaImage::from_fn(width, height, |_x, y| {
        // Dark gradient from #2a2a2a at top to #1a1a1a at bottom
        let t = y as f32 / height as f32;
        let v = (42.0 - t * 16.0) as u8;
        Rgba([v, v, v, 255])
    });
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .expect("PNG encoding should not fail");
    buf
});

/// A 500x200 sample logo (white text-like shape on transparent background).
static SAMPLE_LOGO_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let width = 400u32;
    let height = 120u32;
    let img = RgbaImage::from_fn(width, height, |x, y| {
        // Simple rounded rectangle shape to simulate a logo
        let margin = 8u32;
        if x >= margin && x < width - margin && y >= margin && y < height - margin {
            Rgba([220, 220, 220, 240])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .expect("PNG encoding should not fail");
    buf
});

/// A 1280x720 dark gradient backdrop, computed once.
static SAMPLE_BACKDROP_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let width = 1280u32;
    let height = 720u32;
    let img = RgbaImage::from_fn(width, height, |x, _y| {
        // Horizontal gradient from #1a1a2a (left) to #2a1a1a (right)
        let t = x as f32 / width as f32;
        let r = (26.0 + t * 16.0) as u8;
        let b = (42.0 - t * 16.0) as u8;
        Rgba([r, 26, b, 255])
    });
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .expect("PNG encoding should not fail");
    buf
});

fn sample_badges() -> Vec<RatingBadge> {
    vec![
        RatingBadge { source: RatingSource::Imdb, value: "10.0".into() },
        RatingBadge { source: RatingSource::Tmdb, value: "100%".into() },
        RatingBadge { source: RatingSource::Rt, value: "100%".into() },
        RatingBadge { source: RatingSource::RtAudience, value: "100%".into() },
        RatingBadge { source: RatingSource::Metacritic, value: "100".into() },
        RatingBadge { source: RatingSource::Trakt, value: "100%".into() },
        RatingBadge { source: RatingSource::Letterboxd, value: "5.0".into() },
        RatingBadge { source: RatingSource::Mal, value: "10.00".into() },
        RatingBadge { source: RatingSource::Mdblist, value: "100".into() },
        RatingBadge { source: RatingSource::Ebert, value: "4.0".into() },
    ]
}

const PREVIEW_POSTER_RATINGS_LIMIT: i32 = 3;
const PREVIEW_LOGO_BACKDROP_RATINGS_LIMIT: i32 = 5;

/// Build a `RenderSettings` populated with the given preview parameters for
/// the specified image kind. Only the fields relevant to `kind` are set;
/// the rest use defaults. This allows preview handlers to delegate cache key
/// construction to `serve::settings_cache_suffix_with_ratings`, keeping cache
/// key format in sync with the main image-serving path.
#[allow(clippy::too_many_arguments)]
fn preview_render_settings(
    kind: cache::ImageType,
    badge_style: BadgeStyle,
    label_style: db::LabelStyle,
    badge_size: db::BadgeSize,
    position: BadgePosition,
    badge_direction: BadgeDirection,
    appearance: BadgeAppearance,
    ratings_limit: i32,
    ratings_order: &str,
    ratings_exclude: &str,
) -> RenderSettings {
    let mut s = RenderSettings::default();
    s.ratings_order = std::sync::Arc::from(ratings_order);
    s.ratings_exclude = std::sync::Arc::from(ratings_exclude);
    match kind {
        cache::ImageType::Poster => {
            s.ratings_limit = ratings_limit;
            s.poster_badge_style = badge_style;
            s.poster_label_style = label_style;
            s.poster_badge_size = badge_size;
            s.poster_position = position;
            s.poster_badge_direction = badge_direction;
            s.poster_badge_shape = appearance.shape;
            s.poster_badge_background = appearance.background;
        }
        cache::ImageType::Logo => {
            s.logo_ratings_limit = ratings_limit;
            s.logo_badge_style = badge_style;
            s.logo_label_style = label_style;
            s.logo_badge_size = badge_size;
            s.logo_badge_shape = appearance.shape;
            s.logo_badge_background = appearance.background;
        }
        cache::ImageType::Backdrop => {
            s.backdrop_ratings_limit = ratings_limit;
            s.backdrop_badge_style = badge_style;
            s.backdrop_label_style = label_style;
            s.backdrop_badge_size = badge_size;
            s.backdrop_position = position;
            s.backdrop_badge_direction = badge_direction;
            s.backdrop_badge_shape = appearance.shape;
            s.backdrop_badge_background = appearance.background;
        }
        cache::ImageType::Episode => {
            s.episode_ratings_limit = ratings_limit;
            s.episode_badge_style = badge_style;
            s.episode_label_style = label_style;
            s.episode_badge_size = badge_size;
            s.episode_position = position;
            s.episode_badge_direction = badge_direction;
            s.episode_badge_shape = appearance.shape;
            s.episode_badge_background = appearance.background;
        }
        cache::ImageType::Season => {
            s.season_ratings_limit = ratings_limit;
            s.season_badge_style = badge_style;
            s.season_label_style = label_style;
            s.season_badge_size = badge_size;
            s.season_position = position;
            s.season_badge_direction = badge_direction;
            s.season_badge_shape = appearance.shape;
            s.season_badge_background = appearance.background;
        }
    }
    s
}

/// Build the badge appearance (shape + background) from preview query params,
/// falling back to the server defaults.
fn preview_badge_appearance(query: &ImageQuery) -> BadgeAppearance {
    BadgeAppearance {
        shape: query.badge_shape.unwrap_or_else(db::default_badge_shape),
        background: query.badge_background.unwrap_or_else(db::default_badge_background),
    }
}

/// Parse and validate the optional `imageSize` query parameter for previews.
fn parse_preview_image_size(
    raw: &Option<String>,
    kind: cache::ImageType,
) -> Result<Option<db::ImageSize>, AppError> {
    match raw {
        Some(s) => db::validate_image_size(s, kind).map(Some),
        None => Ok(None),
    }
}

pub async fn preview_poster(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, AppError> {
    let image_size = parse_preview_image_size(&query.image_size, cache::ImageType::Poster)?;
    let badge_size = query.badge_size.unwrap_or(db::default_badge_size());
    let resolved_size = serve::resolve_image_size(image_size);
    let target_width = resolved_size.poster_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Poster) * badge_size.scale_factor();

    let ratings_limit = query.ratings_limit.unwrap_or(PREVIEW_POSTER_RATINGS_LIMIT);
    db::validate_ratings_limit(ratings_limit)?;
    let default_order = db::default_ratings_order();
    let ratings_order = query.ratings_order.as_deref().unwrap_or(&default_order);
    db::validate_ratings_order(ratings_order)?;
    let ratings_exclude = query.ratings_exclude.as_deref().unwrap_or("");
    db::validate_ratings_exclude(ratings_exclude)?;
    let position = query.position.unwrap_or(BadgePosition::BottomCenter);
    let raw_badge_style = query.badge_style.unwrap_or(BadgeStyle::Default);
    let label_style = query.label_style.unwrap_or(db::default_label_style());
    let badge_direction = query.badge_direction.unwrap_or(db::default_poster_badge_direction()).resolve(position);
    let badge_style = raw_badge_style.resolve(badge_direction);
    let badge_appearance = preview_badge_appearance(&query);
    let split = query.split.unwrap_or(false);
    let poster_fit = query.fit.unwrap_or_else(db::default_poster_fit);
    let ratings_suffix = ratings::ratings_cache_suffix(ratings_order, ratings_exclude, ratings_limit);
    let mut preview_settings = preview_render_settings(cache::ImageType::Poster, badge_style, label_style, badge_size, position, badge_direction, badge_appearance, ratings_limit, ratings_order, ratings_exclude);
    preview_settings.poster_badge_split = split;
    preview_settings.poster_fit = poster_fit;
    let suffix = serve::settings_cache_suffix_with_ratings(&preview_settings, cache::ImageType::Poster, image_size, &ratings_suffix);
    let cache_key = format!("preview:{suffix}");
    let cache_path = cache::preview_path(&state.config.cache_dir, cache::ImageType::Poster, &suffix, "jpg")?;

    // 1. Check in-memory cache
    if let Some(cached) = state.preview_cache.get(&cache_key).await {
        return Ok(preview_response(cached));
    }

    // 2. Check filesystem cache (never stale — deterministic output)
    if let Some(entry) = cache::read(&cache_path, 0).await {
        let bytes: bytes::Bytes = entry.bytes.into();
        state.preview_cache.insert(cache_key, bytes.clone()).await;
        return Ok(preview_response(bytes));
    }

    // 3. Render and cache to both layers
    let badges = sample_badges();
    let badges = ratings::apply_rating_preferences(badges, ratings_order, ratings_exclude, ratings_limit);

    let poster_png: &'static Vec<u8> = &SAMPLE_POSTER_PNG;
    let font = state.font.clone();
    let quality = state.config.image_quality;
    let buf = tokio::task::spawn_blocking(move || {
        generate::render_poster_sync(poster_png, &badges, &font, quality, position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, split, poster_fit)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    cache::write(&cache_path, &buf).await?;
    let bytes = bytes::Bytes::from(buf);
    state.preview_cache.insert(cache_key, bytes.clone()).await;

    Ok(preview_response(bytes))
}

/// Season posters render through the 2:3 poster pipeline using the `season_*`
/// badge settings (no split/fit/blur), so the preview reuses the sample poster.
pub async fn preview_season(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, AppError> {
    let image_size = parse_preview_image_size(&query.image_size, cache::ImageType::Season)?;
    let badge_size = query.badge_size.unwrap_or_else(db::default_season_badge_size);
    let resolved_size = serve::resolve_image_size(image_size);
    let target_width = resolved_size.poster_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Season) * badge_size.scale_factor();

    let ratings_limit = query.ratings_limit.unwrap_or_else(db::default_season_ratings_limit);
    db::validate_ratings_limit(ratings_limit)?;
    let default_order = db::default_ratings_order();
    let ratings_order = query.ratings_order.as_deref().unwrap_or(&default_order);
    db::validate_ratings_order(ratings_order)?;
    let ratings_exclude = query.ratings_exclude.as_deref().unwrap_or("");
    db::validate_ratings_exclude(ratings_exclude)?;
    let position = query.position.unwrap_or_else(db::default_season_position);
    let badge_direction = query.badge_direction.unwrap_or_else(db::default_season_badge_direction).resolve(position);
    let badge_style = query.badge_style.unwrap_or_else(db::default_season_badge_style).resolve(badge_direction);
    let label_style = query.label_style.unwrap_or_else(db::default_label_style);
    let badge_appearance = preview_badge_appearance(&query);
    let ratings_suffix = ratings::ratings_cache_suffix(ratings_order, ratings_exclude, ratings_limit);
    let preview_settings = preview_render_settings(cache::ImageType::Season, badge_style, label_style, badge_size, position, badge_direction, badge_appearance, ratings_limit, ratings_order, ratings_exclude);
    let suffix = serve::settings_cache_suffix_with_ratings(&preview_settings, cache::ImageType::Season, image_size, &ratings_suffix);
    let cache_key = format!("preview-season:{suffix}");
    let cache_path = cache::preview_path(&state.config.cache_dir, cache::ImageType::Season, &suffix, "jpg")?;

    if let Some(cached) = state.preview_cache.get(&cache_key).await {
        return Ok(preview_response(cached));
    }

    if let Some(entry) = cache::read(&cache_path, 0).await {
        let bytes: bytes::Bytes = entry.bytes.into();
        state.preview_cache.insert(cache_key, bytes.clone()).await;
        return Ok(preview_response(bytes));
    }

    let badges = sample_badges();
    let badges = ratings::apply_rating_preferences(badges, ratings_order, ratings_exclude, ratings_limit);

    let poster_png: &'static Vec<u8> = &SAMPLE_POSTER_PNG;
    let font = state.font.clone();
    let quality = state.config.image_quality;
    let poster_fit = db::default_poster_fit();
    let buf = tokio::task::spawn_blocking(move || {
        generate::render_poster_sync(poster_png, &badges, &font, quality, position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, false, poster_fit)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    cache::write(&cache_path, &buf).await?;
    let bytes = bytes::Bytes::from(buf);
    state.preview_cache.insert(cache_key, bytes.clone()).await;

    Ok(preview_response(bytes))
}

pub async fn preview_logo(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, AppError> {
    let image_size = parse_preview_image_size(&query.image_size, cache::ImageType::Logo)?;
    let badge_size = query.badge_size.unwrap_or(db::default_badge_size());
    let resolved_size = serve::resolve_image_size(image_size);
    let target_width = resolved_size.logo_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Logo) * badge_size.scale_factor();

    let ratings_limit = query.ratings_limit.unwrap_or(PREVIEW_LOGO_BACKDROP_RATINGS_LIMIT);
    db::validate_ratings_limit(ratings_limit)?;
    let default_order = db::default_ratings_order();
    let ratings_order = query.ratings_order.as_deref().unwrap_or(&default_order);
    db::validate_ratings_order(ratings_order)?;
    let ratings_exclude = query.ratings_exclude.as_deref().unwrap_or("");
    db::validate_ratings_exclude(ratings_exclude)?;
    let badge_style = query.badge_style.unwrap_or(BadgeStyle::Horizontal).resolve(BadgeDirection::Vertical);
    let label_style = query.label_style.unwrap_or(db::default_label_style());
    let badge_appearance = preview_badge_appearance(&query);
    let ratings_suffix = ratings::ratings_cache_suffix(ratings_order, ratings_exclude, ratings_limit);
    let preview_settings = preview_render_settings(cache::ImageType::Logo, badge_style, label_style, badge_size, BadgePosition::BottomCenter, BadgeDirection::Horizontal, badge_appearance, ratings_limit, ratings_order, ratings_exclude);
    let suffix = serve::settings_cache_suffix_with_ratings(&preview_settings, cache::ImageType::Logo, image_size, &ratings_suffix);
    let cache_key = format!("preview-logo:{suffix}");
    let cache_path = cache::preview_path(&state.config.cache_dir, cache::ImageType::Logo, &suffix, "png")?;

    if let Some(cached) = state.preview_cache.get(&cache_key).await {
        return Ok(preview_png_response(cached));
    }

    if let Some(entry) = cache::read(&cache_path, 0).await {
        let bytes: bytes::Bytes = entry.bytes.into();
        state.preview_cache.insert(cache_key, bytes.clone()).await;
        return Ok(preview_png_response(bytes));
    }

    let badges = sample_badges();
    let badges = ratings::apply_rating_preferences(badges, ratings_order, ratings_exclude, ratings_limit);

    let logo_png: &'static Vec<u8> = &SAMPLE_LOGO_PNG;
    let font = state.font.clone();

    let buf = tokio::task::spawn_blocking(move || {
        generate::render_logo_sync(logo_png, &badges, &font, badge_style, label_style, badge_appearance, target_width, badge_scale)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    cache::write(&cache_path, &buf).await?;
    let bytes = bytes::Bytes::from(buf);
    state.preview_cache.insert(cache_key, bytes.clone()).await;

    Ok(preview_png_response(bytes))
}

pub async fn preview_backdrop(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, AppError> {
    let image_size = parse_preview_image_size(&query.image_size, cache::ImageType::Backdrop)?;
    let badge_size = query.badge_size.unwrap_or(db::default_badge_size());
    let resolved_size = serve::resolve_image_size(image_size);
    let target_width = resolved_size.backdrop_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Backdrop) * badge_size.scale_factor();

    let ratings_limit = query.ratings_limit.unwrap_or(PREVIEW_LOGO_BACKDROP_RATINGS_LIMIT);
    db::validate_ratings_limit(ratings_limit)?;
    let default_order = db::default_ratings_order();
    let ratings_order = query.ratings_order.as_deref().unwrap_or(&default_order);
    db::validate_ratings_order(ratings_order)?;
    let ratings_exclude = query.ratings_exclude.as_deref().unwrap_or("");
    db::validate_ratings_exclude(ratings_exclude)?;
    let position = query.position.unwrap_or(db::default_backdrop_position());
    let badge_direction = query.badge_direction.unwrap_or(db::default_backdrop_badge_direction()).resolve(position);
    let badge_style = query.badge_style.unwrap_or(BadgeStyle::Vertical).resolve(badge_direction);
    let label_style = query.label_style.unwrap_or(db::default_label_style());
    let badge_appearance = preview_badge_appearance(&query);
    let edge_inset_x = db::clamp_edge_inset(query.edge_inset_x.unwrap_or(0));
    let edge_inset_y = db::clamp_edge_inset(query.edge_inset_y.unwrap_or(0));
    let ratings_suffix = ratings::ratings_cache_suffix(ratings_order, ratings_exclude, ratings_limit);
    let mut preview_settings = preview_render_settings(cache::ImageType::Backdrop, badge_style, label_style, badge_size, position, badge_direction, badge_appearance, ratings_limit, ratings_order, ratings_exclude);
    preview_settings.backdrop_edge_inset_x = edge_inset_x;
    preview_settings.backdrop_edge_inset_y = edge_inset_y;
    let suffix = serve::settings_cache_suffix_with_ratings(&preview_settings, cache::ImageType::Backdrop, image_size, &ratings_suffix);
    let cache_key = format!("preview-backdrop:{suffix}");
    let cache_path = cache::preview_path(&state.config.cache_dir, cache::ImageType::Backdrop, &suffix, "jpg")?;

    if let Some(cached) = state.preview_cache.get(&cache_key).await {
        return Ok(preview_response(cached));
    }

    if let Some(entry) = cache::read(&cache_path, 0).await {
        let bytes: bytes::Bytes = entry.bytes.into();
        state.preview_cache.insert(cache_key, bytes.clone()).await;
        return Ok(preview_response(bytes));
    }

    let badges = sample_badges();
    let badges = ratings::apply_rating_preferences(badges, ratings_order, ratings_exclude, ratings_limit);

    let backdrop_png: &'static Vec<u8> = &SAMPLE_BACKDROP_PNG;
    let font = state.font.clone();
    let quality = state.config.image_quality;

    let buf = tokio::task::spawn_blocking(move || {
        generate::render_backdrop_sync(backdrop_png, &badges, &font, quality, position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, edge_inset_x, edge_inset_y)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    cache::write(&cache_path, &buf).await?;
    let bytes = bytes::Bytes::from(buf);
    state.preview_cache.insert(cache_key, bytes.clone()).await;

    Ok(preview_response(bytes))
}

/// A 780x439 dark gradient episode still (16:9, matching default episode_target_width).
static SAMPLE_EPISODE_PNG: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let width = 780u32;
    let height = 439u32;
    let img = RgbaImage::from_fn(width, height, |x, y| {
        // Diagonal gradient with a blue-ish tint
        let tx = x as f32 / width as f32;
        let ty = y as f32 / height as f32;
        let r = (20.0 + tx * 12.0) as u8;
        let g = (22.0 + ty * 10.0) as u8;
        let b = (35.0 + tx * 15.0) as u8;
        Rgba([r, g, b, 255])
    });
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(img.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .expect("PNG encoding should not fail");
    buf
});

pub async fn preview_episode(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageQuery>,
) -> Result<Response, AppError> {
    let image_size = parse_preview_image_size(&query.image_size, cache::ImageType::Episode)?;
    let badge_size = query.badge_size.unwrap_or(db::BadgeSize::Large);
    let resolved_size = serve::resolve_image_size(image_size);
    let target_width = resolved_size.episode_target_width();
    let badge_scale = resolved_size.badge_scale(cache::ImageType::Episode) * badge_size.scale_factor();

    let ratings_limit = query.ratings_limit.unwrap_or(db::default_episode_ratings_limit());
    db::validate_ratings_limit(ratings_limit)?;
    let default_order = db::default_ratings_order();
    let ratings_order = query.ratings_order.as_deref().unwrap_or(&default_order);
    db::validate_ratings_order(ratings_order)?;
    let ratings_exclude = query.ratings_exclude.as_deref().unwrap_or("");
    db::validate_ratings_exclude(ratings_exclude)?;
    let position = query.position.unwrap_or(db::default_episode_position());
    let badge_direction = query.badge_direction.unwrap_or(db::default_episode_badge_direction()).resolve(position);
    let badge_style = query.badge_style.unwrap_or(db::default_episode_badge_style()).resolve(badge_direction);
    let label_style = query.label_style.unwrap_or(db::default_label_style());
    let badge_appearance = preview_badge_appearance(&query);
    let blur = query.blur.unwrap_or(false);
    let ratings_suffix = ratings::ratings_cache_suffix(ratings_order, ratings_exclude, ratings_limit);
    let mut preview_settings = preview_render_settings(cache::ImageType::Episode, badge_style, label_style, badge_size, position, badge_direction, badge_appearance, ratings_limit, ratings_order, ratings_exclude);
    preview_settings.episode_blur = blur;
    let suffix = serve::settings_cache_suffix_with_ratings(&preview_settings, cache::ImageType::Episode, image_size, &ratings_suffix);
    let cache_key = format!("preview-episode:{suffix}");
    let cache_path = cache::preview_path(&state.config.cache_dir, cache::ImageType::Episode, &suffix, "jpg")?;

    if let Some(cached) = state.preview_cache.get(&cache_key).await {
        return Ok(preview_response(cached));
    }

    if let Some(entry) = cache::read(&cache_path, 0).await {
        let bytes: bytes::Bytes = entry.bytes.into();
        state.preview_cache.insert(cache_key, bytes.clone()).await;
        return Ok(preview_response(bytes));
    }

    let badges = sample_badges();
    let badges = ratings::apply_rating_preferences(badges, ratings_order, ratings_exclude, ratings_limit);

    let episode_png: &'static Vec<u8> = &SAMPLE_EPISODE_PNG;
    let font = state.font.clone();
    let quality = state.config.image_quality;

    let buf = tokio::task::spawn_blocking(move || {
        generate::render_episode_sync(episode_png, &badges, &font, quality, position, badge_style, label_style, badge_appearance, badge_direction, target_width, badge_scale, badge_size, blur)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    cache::write(&cache_path, &buf).await?;
    let bytes = bytes::Bytes::from(buf);
    state.preview_cache.insert(cache_key, bytes.clone()).await;

    Ok(preview_response(bytes))
}

fn preview_response(bytes: bytes::Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "public, max-age=60"),
        ],
        bytes,
    )
        .into_response()
}

fn preview_png_response(bytes: bytes::Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=60"),
        ],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_poster_png_is_valid() {
        let png = &*SAMPLE_POSTER_PNG;
        assert!(!png.is_empty());
        // PNG magic bytes
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        // Should decode to 500x750
        let img = image::load_from_memory(png).expect("valid PNG");
        assert_eq!(img.width(), 500);
        assert_eq!(img.height(), 750);
    }

    #[test]
    fn sample_badges_returns_all_sources() {
        let badges = sample_badges();
        // Every selectable rating source must have a sample badge, or it can
        // never appear in the settings preview regardless of order/limit.
        assert_eq!(badges.len(), RatingSource::all_keys().len());

        let sources: Vec<_> = badges.iter().map(|b| b.source).collect();
        for key in RatingSource::all_keys() {
            let src = RatingSource::from_key(key).unwrap();
            assert!(sources.contains(&src), "sample_badges missing {key}");
        }
    }

    #[test]
    fn sample_poster_renders_with_badges() {
        let font = ab_glyph::FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let badges = sample_badges();
        let result = generate::render_poster_sync(&SAMPLE_POSTER_PNG, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, db::PosterFit::Native);
        let buf = result.expect("rendering should succeed");
        // Valid JPEG
        assert_eq!(buf[0], 0xFF);
        assert_eq!(buf[1], 0xD8);
        assert!(buf.len() > 1000);
    }

    #[test]
    fn sample_poster_renders_with_no_badges() {
        let font = ab_glyph::FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = generate::render_poster_sync(&SAMPLE_POSTER_PNG, &[], &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeAppearance::default(), BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium, false, db::PosterFit::Native);
        let buf = result.expect("rendering should succeed");
        assert_eq!(buf[0], 0xFF);
        assert_eq!(buf[1], 0xD8);
    }

    #[test]
    fn preview_default_ratings_limits_match_handler_defaults() {
        assert_eq!(PREVIEW_POSTER_RATINGS_LIMIT, db::default_ratings_limit());
        assert_eq!(PREVIEW_LOGO_BACKDROP_RATINGS_LIMIT, db::default_logo_backdrop_ratings_limit());
    }

    #[test]
    fn image_query_defaults() {
        // Simulate what axum does with no query params — serde defaults apply
        let query: ImageQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.ratings_limit, None);
        assert_eq!(query.ratings_order, None);
        assert_eq!(query.badge_style, None);
        assert_eq!(query.label_style, None);
        assert_eq!(query.badge_direction, None);
        assert!(query.image_size.is_none());
    }

    #[test]
    fn image_query_custom_values() {
        let query: ImageQuery =
            serde_json::from_str(r#"{"ratings_limit":5,"ratings_order":"imdb,rt"}"#).unwrap();
        assert_eq!(query.ratings_limit, Some(5));
        assert_eq!(query.ratings_order.as_deref(), Some("imdb,rt"));
    }
}
