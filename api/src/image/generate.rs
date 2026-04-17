use std::sync::Arc;

use ab_glyph::FontArc;
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, DynamicImage, ImageDecoder, ImageReader, Limits, RgbaImage};
use tokio::sync::Semaphore;

use crate::cache;
use crate::error::AppError;
use crate::image::badge;
use crate::services::db::{BadgeDirection, BadgeSize, BadgeStyle, LabelStyle, BadgePosition};
use crate::services::ratings::RatingBadge;
use crate::services::tmdb::TmdbClient;

/// Threshold (ms) above which render is logged as slow.
const SLOW_RENDER_MS: u64 = 2000;

const BADGE_SPACING: u32 = 10;
const BADGE_BOTTOM_MARGIN: u32 = 10;
const BADGE_TOP_MARGIN: u32 = 20;
const BADGE_SIDE_MARGIN: u32 = 15;
const BADGE_ROW_SPACING: u32 = 7;
const BADGE_VERT_SPACING: u32 = 7;
const BACKDROP_SIDE_MARGIN: u32 = 20;
const MAX_BADGES_PER_ROW: usize = 3;
const MAX_VERT_BADGES_PER_ROW: usize = 5;

fn scaled_or_override(base: u32, badge_scale: f32, override_px: i32) -> u32 {
    if override_px >= 0 {
        override_px as u32
    } else {
        (base as f32 * badge_scale).round() as u32
    }
}

pub struct ImageParams<'a> {
    pub poster_path: &'a str,
    pub badges: &'a [RatingBadge],
    pub tmdb: &'a TmdbClient,

    pub font: &'a FontArc,
    pub quality: u8,
    pub cache_dir: &'a str,
    pub image_stale_secs: u64,
    pub poster_bytes_override: Option<Vec<u8>>,
    pub poster_position: BadgePosition,
    pub badge_style: BadgeStyle,
    pub label_style: LabelStyle,
    pub badge_direction: BadgeDirection,
    pub render_semaphore: Arc<Semaphore>,
    /// Target width for the output image. Defaults to 500 for posters.
    pub target_width: u32,
    /// Badge scale factor (1.0 = default size).
    pub badge_scale: f32,
    /// TMDB CDN size string (e.g. "w500", "w780", "original").
    pub tmdb_size: Arc<str>,
    /// Badge size — used to adjust max badges per row.
    pub badge_size: BadgeSize,
    /// Fixed badge gap override in pixels (-1 = auto scaled).
    pub badge_gap_px: i32,
    /// Fixed badge margin override in pixels (-1 = auto scaled).
    pub badge_margin_px: i32,
    /// When true, skip writing base poster images to disk (CDN handles caching).
    pub external_cache_only: bool,
}

pub async fn generate_poster(params: ImageParams<'_>) -> Result<Vec<u8>, AppError> {
    let ImageParams {
        poster_path,
        badges,
        tmdb,
        font,
        quality,
        cache_dir,
        image_stale_secs,
        poster_bytes_override,
        poster_position,
        badge_style,
        label_style,
        badge_direction,
        render_semaphore,
        target_width,
        badge_scale,
        badge_size,
        badge_gap_px,
        badge_margin_px,
        tmdb_size,
        external_cache_only,
    } = params;

    let poster_bytes = if let Some(bytes) = poster_bytes_override {
        bytes
    } else if external_cache_only {
        // No filesystem cache — always fetch from TMDB
        tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?
    } else {
        // Fetch base poster from TMDB, using cache
        let poster_cache = cache::base_poster_path(cache_dir, poster_path, &tmdb_size)?;
        if let Some(entry) = cache::read(&poster_cache, image_stale_secs).await {
            if entry.is_stale {
                // Conditional fetch — send If-Modified-Since to avoid re-downloading unchanged images
                let modified = tokio::fs::metadata(&poster_cache).await.ok()
                    .and_then(|m| m.modified().ok());
                match tmdb.fetch_poster_bytes_conditional(poster_path, &tmdb_size, modified).await? {
                    Some(fresh_bytes) => {
                        cache::write(&poster_cache, &fresh_bytes).await?;
                        fresh_bytes
                    }
                    None => {
                        // 304 Not Modified — touch mtime without rewriting the file
                        if let Ok(f) = tokio::fs::File::open(&poster_cache).await {
                            let _ = f.into_std().await.set_modified(std::time::SystemTime::now());
                        }
                        entry.bytes
                    }
                }
            } else {
                entry.bytes
            }
        } else {
            let bytes = tmdb.fetch_poster_bytes(poster_path, &tmdb_size).await?;
            cache::write(&poster_cache, &bytes).await?;
            bytes
        }
    };

    // Acquire render permit before CPU-bound work — also bounds queue depth
    // so tasks waiting for a blocking thread still count against the limit.
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("poster render started");
    let start = std::time::Instant::now();

    // Move CPU-bound image processing to a blocking thread
    let badges = badges.to_vec();
    let font = font.clone();
    let buf = tokio::task::spawn_blocking(move || {
        render_poster_sync_with_layout(
            &poster_bytes,
            &badges,
            &font,
            quality,
            poster_position,
            badge_style,
            label_style,
            badge_direction,
            target_width,
            badge_scale,
            badge_size,
            badge_gap_px,
            badge_margin_px,
        )
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    let render_ms = start.elapsed().as_millis() as u64;
    tracing::debug!(elapsed_ms = render_ms, "poster render complete");
    if render_ms > SLOW_RENDER_MS {
        tracing::warn!(
            poster_path,
            render_ms,
            "slow generate_poster"
        );
    }
    Ok(buf)
}

/// Overlay badges in a vertical column, positioned according to `position`.
fn overlay_vertical_stack(
    canvas: &mut RgbaImage,
    badge_images: &[RgbaImage],
    position: BadgePosition,
    badge_scale: f32,
    side_margin_base: u32,
    badge_gap_px: i32,
    badge_margin_px: i32,
) {
    let vert_spacing = scaled_or_override(BADGE_VERT_SPACING, badge_scale, badge_gap_px);
    let top_margin = scaled_or_override(BADGE_TOP_MARGIN, badge_scale, badge_margin_px);
    let bottom_margin = scaled_or_override(BADGE_BOTTOM_MARGIN, badge_scale, badge_margin_px);
    let side_margin = scaled_or_override(side_margin_base, badge_scale, badge_margin_px);

    let total_badge_height: u32 = badge_images.iter().map(|b| b.height()).sum::<u32>()
        + vert_spacing * (badge_images.len() as u32).saturating_sub(1);
    let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);
    let canvas_w = canvas.width() as i64;
    let canvas_h = canvas.height() as i64;
    let total_badge_height_i = total_badge_height as i64;
    let max_badge_width_i = max_badge_width as i64;
    let top_margin_i = top_margin as i64;
    let bottom_margin_i = bottom_margin as i64;
    let side_margin_i = side_margin as i64;

    // Vertical anchor
    let start_y = if position.is_top() {
        top_margin_i
    } else if position.is_bottom() {
        canvas_h - total_badge_height_i - bottom_margin_i
    } else {
        // "l", "r" — vertically centered
        (canvas_h - total_badge_height_i) / 2
    };

    // Horizontal anchor
    let is_left = position.is_left();
    let is_right = position.is_right();

    let base_x = if is_left {
        side_margin_i
    } else if is_right {
        canvas_w - max_badge_width_i - side_margin_i
    } else {
        // center
        (canvas_w - max_badge_width_i) / 2
    };

    let mut y = start_y;
    for badge_img in badge_images {
        let badge_w = badge_img.width() as i64;
        let bx = if is_left {
            base_x
        } else if is_right {
            base_x + (max_badge_width_i - badge_w)
        } else {
            base_x + (max_badge_width_i - badge_w) / 2
        };
        imageops::overlay(canvas, badge_img, bx, y);
        y += badge_img.height() as i64 + vert_spacing as i64;
    }
}

/// Overlay badges in horizontal rows, positioned according to `position`.
fn overlay_horizontal_rows(
    canvas: &mut RgbaImage,
    badge_images: &[RgbaImage],
    position: BadgePosition,
    max_per_row: usize,
    badge_scale: f32,
    side_margin_base: u32,
    badge_gap_px: i32,
    badge_margin_px: i32,
) {
    let spacing = scaled_or_override(BADGE_SPACING, badge_scale, badge_gap_px);
    let row_spacing = scaled_or_override(BADGE_ROW_SPACING, badge_scale, badge_gap_px);
    let top_margin = scaled_or_override(BADGE_TOP_MARGIN, badge_scale, badge_margin_px);
    let bottom_margin = scaled_or_override(BADGE_BOTTOM_MARGIN, badge_scale, badge_margin_px);
    let side_margin = scaled_or_override(side_margin_base, badge_scale, badge_margin_px);

    let rows: Vec<&[RgbaImage]> = badge_images.chunks(max_per_row).collect();
    let badge_height = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
    let total_height = badge_height * rows.len() as u32
        + row_spacing * (rows.len() as u32).saturating_sub(1);
    let canvas_w = canvas.width() as i64;
    let canvas_h = canvas.height() as i64;
    let total_height_i = total_height as i64;
    let badge_height_i = badge_height as i64;
    let top_margin_i = top_margin as i64;
    let bottom_margin_i = bottom_margin as i64;
    let side_margin_i = side_margin as i64;

    // Vertical anchor
    let base_y = if position.is_top() {
        top_margin_i
    } else if position.is_bottom() {
        canvas_h - total_height_i - bottom_margin_i
    } else {
        // "l", "r" — vertically centered
        (canvas_h - total_height_i) / 2
    };

    // Horizontal alignment
    let is_left = position.is_left();
    let is_right = position.is_right();

    for (row_idx, row) in rows.iter().enumerate() {
        let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
            + spacing * (row.len() as u32).saturating_sub(1);
        let row_width_i = row_width as i64;
        let y = base_y + row_idx as i64 * (badge_height_i + row_spacing as i64);

        let start_x = if is_left {
            side_margin_i
        } else if is_right {
            canvas_w - row_width_i - side_margin_i
        } else {
            (canvas_w - row_width_i) / 2
        };

        let mut x = start_x;
        for badge_img in *row {
            let by = y + (badge_height_i - badge_img.height() as i64) / 2;
            imageops::overlay(canvas, badge_img, x, by);
            x += badge_img.width() as i64 + spacing as i64;
        }
    }
}

/// Maximum total pixels allowed for a decoded source image (width * height).
/// 8192x8192 = 67M pixels (~256 MB as RGBA) is generous for poster/backdrop art
/// while preventing OOM from crafted images with extreme dimensions.
const MAX_IMAGE_PIXELS: u64 = 8192 * 8192;

/// Decode an image from bytes, rejecting oversized images *before* full decode
/// to avoid OOM from crafted inputs (e.g. PNG bombs).
fn load_image_with_limits(bytes: &[u8]) -> Result<image::DynamicImage, AppError> {
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| AppError::Other(format!("image format detection failed: {e}")))?;

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4); // RGBA = 4 bytes/pixel
    let mut decoder = reader.into_decoder().map_err(AppError::Image)?;
    decoder.set_limits(limits).map_err(AppError::Image)?;

    image::DynamicImage::from_decoder(decoder).map_err(AppError::Image)
}

pub fn render_poster_sync(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    poster_position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
) -> Result<Vec<u8>, AppError> {
    render_poster_sync_with_layout(
        poster_bytes,
        badges,
        font,
        quality,
        poster_position,
        badge_style,
        label_style,
        badge_direction,
        target_width,
        badge_scale,
        badge_size,
        0,
        0,
    )
}

pub fn render_poster_sync_with_layout(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    poster_position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    let base = load_image_with_limits(poster_bytes)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_scale)
        };

        if badge_direction.is_vertical() {
            overlay_vertical_stack(
                &mut canvas,
                &badge_images,
                poster_position,
                badge_scale,
                BADGE_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        } else {
            let max_per_row = match badge_size {
                BadgeSize::Large | BadgeSize::ExtraLarge => {
                    if badge_style == BadgeStyle::Horizontal { 2 } else { 4 }
                }
                _ => {
                    if badge_style.is_vertical() { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW }
                }
            };
            overlay_horizontal_rows(
                &mut canvas,
                &badge_images,
                poster_position,
                max_per_row,
                badge_scale,
                BADGE_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        }
    }

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;

    Ok(buf)
}

const LOGO_BADGE_ROW_SPACING: u32 = 7;
const LOGO_BADGE_SPACING: u32 = 10;
const LOGO_MAX_BADGES_PER_ROW: usize = 3;
const LOGO_SPACING_BELOW: u32 = 15;

pub fn render_logo_sync(
    logo_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    render_logo_sync_with_layout(
        logo_bytes,
        badges,
        font,
        badge_style,
        label_style,
        target_width,
        badge_scale,
        0,
        0,
    )
}

pub fn render_logo_sync_with_layout(
    logo_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    target_width: u32,
    badge_scale: f32,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    let base = load_image_with_limits(logo_bytes)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let logo_img = base.to_rgba8();

    if badges.is_empty() {
        // No badges — just encode the logo as PNG
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            logo_img.as_raw(),
            logo_img.width(),
            logo_img.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        return Ok(buf);
    }

    let logo_badge_spacing = scaled_or_override(LOGO_BADGE_SPACING, badge_scale, badge_gap_px);
    let logo_badge_row_spacing = scaled_or_override(LOGO_BADGE_ROW_SPACING, badge_scale, badge_gap_px);
    let logo_spacing_below = scaled_or_override(LOGO_SPACING_BELOW, badge_scale, badge_margin_px);

    if badge_style.is_vertical() {
        // Vertical badge shapes arranged in rows below the logo
        let badge_images: Vec<RgbaImage> = badges
                .iter()
                .map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale))
                .collect();

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(MAX_VERT_BADGES_PER_ROW).collect();
            let max_badge_height: u32 = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
            let total_badge_height = max_badge_height * rows.len() as u32
                + logo_badge_row_spacing * (rows.len() as u32).saturating_sub(1);

            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + logo_badge_spacing * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + logo_spacing_below + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            let badges_start_y = logo_img.height() + logo_spacing_below;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + logo_badge_spacing * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (max_badge_height + logo_badge_row_spacing);

                let mut x = start_x;
                for badge_img in *row {
                    let by = y + (max_badge_height.saturating_sub(badge_img.height())) / 2;
                    imageops::overlay(&mut canvas, badge_img, x as i64, by as i64);
                    x += badge_img.width() + logo_badge_spacing;
                }
            }

            // Encode as PNG
            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                encoder,
                canvas.as_raw(),
                canvas.width(),
                canvas.height(),
                image::ExtendedColorType::Rgba8,
            )?;
            Ok(buf)
    } else {
        // Horizontal badge images (default) — uniform widths, arranged in rows below the logo
            let badge_images = badge::render_badges_uniform(badges, font, label_style, badge_scale);

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(LOGO_MAX_BADGES_PER_ROW).collect();
            let badge_height = badge_images[0].height();
            let total_badge_height =
                badge_height * rows.len() as u32 + logo_badge_row_spacing * (rows.len() as u32).saturating_sub(1);

            // Compute row widths to determine canvas width
            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + logo_badge_spacing * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + logo_spacing_below + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            // Center logo at top
            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            // Center badge rows below logo
            let badges_start_y = logo_img.height() + logo_spacing_below;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + logo_badge_spacing * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (badge_height + logo_badge_row_spacing);

                let mut x = start_x;
                for badge_img in *row {
                    imageops::overlay(&mut canvas, badge_img, x as i64, y as i64);
                    x += badge_img.width() + logo_badge_spacing;
                }
            }

            // Encode as PNG (preserves transparency)
            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                encoder,
                canvas.as_raw(),
                canvas.width(),
                canvas.height(),
                image::ExtendedColorType::Rgba8,
            )?;
            Ok(buf)
    }
}

pub async fn generate_logo(
    logo_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    font: FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    generate_logo_with_layout(
        logo_bytes,
        badges,
        font,
        badge_style,
        label_style,
        render_semaphore,
        target_width,
        badge_scale,
        0,
        0,
    ).await
}

pub async fn generate_logo_with_layout(
    logo_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    font: FontArc,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("logo render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || {
        render_logo_sync_with_layout(
            &logo_bytes,
            &badges,
            &font,
            badge_style,
            label_style,
            target_width,
            badge_scale,
            badge_gap_px,
            badge_margin_px,
        )
    })
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "logo render complete");
    Ok(buf)
}

pub fn render_backdrop_sync(
    backdrop_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    _badge_size: BadgeSize,
) -> Result<Vec<u8>, AppError> {
    render_backdrop_sync_with_layout(
        backdrop_bytes,
        badges,
        font,
        quality,
        position,
        badge_style,
        label_style,
        badge_direction,
        target_width,
        badge_scale,
        _badge_size,
        0,
        0,
    )
}

pub fn render_backdrop_sync_with_layout(
    backdrop_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    _badge_size: BadgeSize,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    let base = load_image_with_limits(backdrop_bytes)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_scale)
        };

        if badge_direction.is_vertical() {
            overlay_vertical_stack(
                &mut canvas,
                &badge_images,
                position,
                badge_scale,
                BACKDROP_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        } else {
            // Backdrops are wide (16:9) so all badges fit in a single row
            overlay_horizontal_rows(
                &mut canvas,
                &badge_images,
                position,
                badge_images.len(),
                badge_scale,
                BACKDROP_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        }
    }

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;
    Ok(buf)
}

pub async fn generate_backdrop(
    backdrop_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    font: FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
) -> Result<Vec<u8>, AppError> {
    generate_backdrop_with_layout(
        backdrop_bytes,
        badges,
        font,
        quality,
        position,
        badge_style,
        label_style,
        badge_direction,
        render_semaphore,
        target_width,
        badge_scale,
        badge_size,
        0,
        0,
    ).await
}

pub async fn generate_backdrop_with_layout(
    backdrop_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    font: FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("backdrop render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || {
        render_backdrop_sync_with_layout(
            &backdrop_bytes,
            &badges,
            &font,
            quality,
            position,
            badge_style,
            label_style,
            badge_direction,
            target_width,
            badge_scale,
            badge_size,
            badge_gap_px,
            badge_margin_px,
        )
    })
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "backdrop render complete");
    Ok(buf)
}

/// Episode still rendering with configurable position, direction, and optional blur.
/// Reuses the poster badge overlay functions but with episode-specific settings.
pub fn render_episode_sync(
    image_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    blur: bool,
) -> Result<Vec<u8>, AppError> {
    render_episode_sync_with_layout(
        image_bytes,
        badges,
        font,
        quality,
        position,
        badge_style,
        label_style,
        badge_direction,
        target_width,
        badge_scale,
        badge_size,
        blur,
        0,
        0,
    )
}

pub fn render_episode_sync_with_layout(
    image_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    position: BadgePosition,
    badge_style: BadgeStyle,
    label_style: LabelStyle,
    badge_direction: BadgeDirection,
    target_width: u32,
    badge_scale: f32,
    badge_size: BadgeSize,
    blur: bool,
    badge_gap_px: i32,
    badge_margin_px: i32,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(image_bytes)
        .map_err(AppError::Image)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    // Apply blur for spoiler protection (before badge overlay).
    // Downscale → small blur → upscale is much faster than a large-radius Gaussian
    // and visually equivalent for spoiler hiding.
    if blur && canvas.width() >= 8 && canvas.height() >= 8 {
        let (w, h) = (canvas.width(), canvas.height());
        let small = image::imageops::resize(&canvas, w / 8, h / 8, image::imageops::FilterType::Triangle);
        let small = imageproc::filter::gaussian_blur_f32(&small, 3.0);
        canvas = image::imageops::resize(&small, w, h, image::imageops::FilterType::Triangle);
    }

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = if badge_style.is_vertical() {
            badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale)).collect()
        } else {
            badge::render_badges_uniform(badges, font, label_style, badge_scale)
        };

        if badge_direction.is_vertical() {
            overlay_vertical_stack(
                &mut canvas,
                &badge_images,
                position,
                badge_scale,
                BADGE_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        } else {
            let max_per_row = match badge_size {
                BadgeSize::Large | BadgeSize::ExtraLarge => {
                    if badge_style == BadgeStyle::Horizontal { 2 } else { 4 }
                }
                _ => {
                    if badge_style.is_vertical() { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW }
                }
            };
            overlay_horizontal_rows(
                &mut canvas,
                &badge_images,
                position,
                max_per_row,
                badge_scale,
                BADGE_SIDE_MARGIN,
                badge_gap_px,
                badge_margin_px,
            );
        }
    }

    // Encode as JPEG
    let dynamic = DynamicImage::ImageRgba8(canvas);
    let rgb = dynamic.to_rgb8();
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    rgb.write_with_encoder(encoder)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_poster_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a minimal valid PNG in memory
        let img = image::RgbaImage::from_pixel(100, 150, image::Rgba([128, 128, 128, 255]));
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            100,
            150,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let result = render_poster_sync(&png_bytes, &[], &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        // Should be valid JPEG
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let img = image::RgbaImage::from_pixel(500, 750, image::Rgba([128, 128, 128, 255]));
        let mut png_bytes = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            500,
            750,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
            RatingBadge {
                source: RatingSource::Tmdb,
                value: "85%".to_string(),
            },
            RatingBadge {
                source: RatingSource::Rt,
                value: "92%".to_string(),
            },
            RatingBadge {
                source: RatingSource::Metacritic,
                value: "78".to_string(),
            },
        ];

        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_invalid_image_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(b"not an image", &[], &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium);
        assert!(result.is_err());
    }

    /// Helper: create a minimal PNG in memory.
    fn test_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([128, 128, 128, 255]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }

    #[test]
    fn render_logo_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let result = render_logo_sync(&png, &[], &font, BadgeStyle::Horizontal, LabelStyle::Text, 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &font, BadgeStyle::Horizontal, LabelStyle::Text, 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a logo wider than TARGET_WIDTH (500)
        let png = test_png(1000, 200);
        let result = render_logo_sync(&png, &[], &font, BadgeStyle::Horizontal, LabelStyle::Text, 500, 1.0).unwrap();
        assert!(!result.is_empty());
        // Verify the output is valid PNG and was produced (implicitly downscaled)
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_logo_sync(b"not an image", &[], &font, BadgeStyle::Horizontal, LabelStyle::Text, 500, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        // Backdrop outputs JPEG
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a backdrop wider than TARGET_WIDTH (1280)
        let png = test_png(2560, 1440);
        let result = render_backdrop_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_backdrop_sync(b"not an image", &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium);
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_horizontal_bottom_center() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_horizontal_large_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "80%".to_string() },
        ];
        // Large + Horizontal style — all badges in a single row on wide backdrop
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 1280, 1.0, BadgeSize::Large).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_vertical_bottom_left() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::BottomLeft, BadgeStyle::Vertical, LabelStyle::Icon, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    // --- Episode rendering tests ---

    #[test]
    fn render_episode_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let result = render_episode_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_with_badges() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_episode_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_with_blur() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(780, 439);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
        ];
        let result = render_episode_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, true).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1920, 1080);
        let result = render_episode_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_episode_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_episode_sync(b"not an image", &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 780, 1.0, BadgeSize::Large, false);
        assert!(result.is_err());
    }

    #[test]
    fn render_poster_top_center_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::TopCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }



    #[test]
    fn render_poster_left_position_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::Left, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_right_position_produces_valid_jpeg() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge {
                source: RatingSource::Imdb,
                value: "8.5".to_string(),
            },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::Right, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Icon, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_vertical_badge_direction() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        // vertical direction at bottom-center
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at top-left corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::TopLeft, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at bottom-right corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::BottomRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_logo_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &font, BadgeStyle::Horizontal, LabelStyle::Icon, 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_with_icon_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Icon, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png_bytes = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "45%".to_string() },
        ];
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Official, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_logo_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(400, 100);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        let result = render_logo_sync(&png, &badges, &font, BadgeStyle::Horizontal, LabelStyle::Official, 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_with_official_label_style() {
        use crate::services::ratings::{RatingBadge, RatingSource};

        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(1280, 720);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "9.0".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "95%".to_string() },
            RatingBadge { source: RatingSource::RtAudience, value: "40%".to_string() },
        ];
        let result = render_backdrop_sync(&png, &badges, &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Official, BadgeDirection::Vertical, 1280, 1.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    fn dummy_tmdb() -> crate::services::tmdb::TmdbClient {
        crate::services::tmdb::TmdbClient::new("test".to_string(), reqwest::Client::new())
    }

    #[tokio::test]
    async fn generate_poster_acquires_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(100, 150);
        let sem = Arc::new(Semaphore::new(2));
        let tmdb = dummy_tmdb();

        let result = generate_poster(ImageParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            image_stale_secs: 3600,
            poster_position: BadgePosition::BottomCenter,
            badge_style: BadgeStyle::Horizontal,
            label_style: LabelStyle::Text,
            badge_direction: BadgeDirection::Horizontal,
            render_semaphore: sem.clone(),
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
            badge_size: BadgeSize::Medium,
            badge_gap_px: 0,
            badge_margin_px: 0,
            external_cache_only: false,
        })
        .await;

        assert!(result.is_ok());
        // Permit should be released after render
        assert_eq!(sem.available_permits(), 2);
    }

    #[tokio::test]
    async fn generate_poster_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(100, 150);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();
        let tmdb = dummy_tmdb();

        let result = generate_poster(ImageParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            image_stale_secs: 3600,
            poster_position: BadgePosition::BottomCenter,
            badge_style: BadgeStyle::Horizontal,
            label_style: LabelStyle::Text,
            badge_direction: BadgeDirection::Horizontal,
            render_semaphore: sem,
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
            badge_size: BadgeSize::Medium,
            badge_gap_px: 0,
            badge_margin_px: 0,
            external_cache_only: false,
        })
        .await;

        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("render queue closed"), "expected 'render queue closed', got: {err_msg}");
    }

    #[tokio::test]
    async fn generate_logo_acquires_and_releases_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let sem = Arc::new(Semaphore::new(1));

        let result = generate_logo(png, vec![], font, BadgeStyle::Horizontal, LabelStyle::Text, sem.clone(), 500, 1.0).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_logo_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_logo(png, vec![], font, BadgeStyle::Horizontal, LabelStyle::Text, sem, 500, 1.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_backdrop_acquires_and_releases_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));

        let result = generate_backdrop(png, vec![], font, 85, BadgePosition::TopRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Vertical, sem.clone(), 1280, 1.0, BadgeSize::Medium).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_backdrop_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_backdrop(png, vec![], font, 85, BadgePosition::TopRight, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Vertical, sem, 1280, 1.0, BadgeSize::Medium).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn semaphore_bounds_concurrent_logo_renders() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let sem = Arc::new(Semaphore::new(1));

        // Acquire the only permit
        let permit = sem.clone().acquire_owned().await.unwrap();

        let png = test_png(200, 80);
        let sem2 = sem.clone();
        let font2 = font.clone();

        // Spawn a render that should block waiting for the permit
        let handle = tokio::spawn(async move {
            generate_logo(png, vec![], font2, BadgeStyle::Horizontal, LabelStyle::Text, sem2, 500, 1.0).await
        });

        // Give the task a moment to start and block
        tokio::task::yield_now().await;
        assert_eq!(sem.available_permits(), 0, "permit should still be held");

        // Release the permit so the render can proceed
        drop(permit);
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[test]
    fn render_poster_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        // Render at large size (1280 width, ~2.2x badge scale)
        let result = render_poster_sync(&png, &[], &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 1280, 2.2, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
        // Verify the output is wider than a default-size render
        let img = image::load_from_memory(&result).unwrap();
        assert_eq!(img.width(), 1280);
    }

    #[test]
    fn render_poster_with_badges_at_large_scale() {
        use crate::services::ratings::{RatingBadge, RatingSource};
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ];
        let result = render_poster_sync(&png, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 1280, 2.2, BadgeSize::Medium).unwrap();
        let img = image::load_from_memory(&result).unwrap();
        assert_eq!(img.width(), 1280);
    }

    #[test]
    fn render_poster_with_large_badge_size() {
        use crate::services::ratings::{RatingBadge, RatingSource};
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(500, 750);
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];
        // Large badge size with horizontal style — should use max 2 per row
        let result = render_poster_sync(&png, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Large).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // Extra-large badge size with vertical style — should use max 4 per row
        let result = render_poster_sync(&png, &badges, &font, 85, BadgePosition::BottomCenter, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Horizontal, 500, 1.0, BadgeSize::ExtraLarge).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    /// Compute the centroid (center of mass) of non-background pixels in a
    /// rendered poster JPEG. The base image is uniform gray (128,128,128), so
    /// any pixel that deviates significantly is part of a badge overlay.
    /// Returns (centroid_x, centroid_y) as fractions of image dimensions (0.0–1.0).
    fn badge_centroid(jpeg_bytes: &[u8]) -> (f64, f64) {
        let img = image::load_from_memory(jpeg_bytes).unwrap().to_rgba8();
        let (w, h) = (img.width() as f64, img.height() as f64);
        let (mut sx, mut sy, mut count) = (0u64, 0u64, 0u64);
        for (x, y, px) in img.enumerate_pixels() {
            // JPEG compression blurs edges, so use a generous threshold
            let diff = (px[0] as i32 - 128).abs()
                .max((px[1] as i32 - 128).abs())
                .max((px[2] as i32 - 128).abs());
            if diff > 20 {
                sx += x as u64;
                sy += y as u64;
                count += 1;
            }
        }
        assert!(count > 0, "no badge pixels found");
        (sx as f64 / count as f64 / w, sy as f64 / count as f64 / h)
    }

    fn position_test_badges() -> Vec<crate::services::ratings::RatingBadge> {
        use crate::services::ratings::{RatingBadge, RatingSource};
        vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
        ]
    }

    #[test]
    fn badge_position_top_center_places_badges_in_top_region() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::TopCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cy < 0.33, "TopCenter: badge y-centroid {cy:.2} should be in top third");
        assert!(cx > 0.3 && cx < 0.7, "TopCenter: badge x-centroid {cx:.2} should be centered");
    }

    #[test]
    fn badge_position_bottom_center_places_badges_in_bottom_region() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Horizontal, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cy > 0.67, "BottomCenter: badge y-centroid {cy:.2} should be in bottom third");
        assert!(cx > 0.3 && cx < 0.7, "BottomCenter: badge x-centroid {cx:.2} should be centered");
    }

    #[test]
    fn badge_position_bottom_center_stays_centered_when_scaled_row_overflows() {
        use crate::services::ratings::{RatingBadge, RatingSource};
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let badges = vec![
            RatingBadge { source: RatingSource::Imdb, value: "8.5".to_string() },
            RatingBadge { source: RatingSource::Rt, value: "92%".to_string() },
            RatingBadge { source: RatingSource::Tmdb, value: "85%".to_string() },
        ];

        // At 4.0x this row can exceed canvas width; keep center anchoring instead of clamping to x=0.
        let result = render_poster_sync(&test_png(500, 750), &badges, &font, 85,
            BadgePosition::BottomCenter, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Horizontal, 500, 4.0, BadgeSize::Medium).unwrap();
        let (cx, _cy) = badge_centroid(&result);
        assert!(cx > 0.3 && cx < 0.7, "BottomCenter overflow: badge x-centroid {cx:.2} should remain centered");
    }

    #[test]
    fn badge_position_left_places_badges_on_left_side() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::Left, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx < 0.5, "Left: badge x-centroid {cx:.2} should be in left half");
        assert!(cy > 0.25 && cy < 0.75, "Left: badge y-centroid {cy:.2} should be vertically centered");
    }

    #[test]
    fn badge_position_right_places_badges_on_right_side() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::Right, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx > 0.5, "Right: badge x-centroid {cx:.2} should be in right half");
        assert!(cy > 0.25 && cy < 0.75, "Right: badge y-centroid {cy:.2} should be vertically centered");
    }

    #[test]
    fn badge_position_top_left_places_badges_in_top_left_quadrant() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::TopLeft, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx < 0.5, "TopLeft: badge x-centroid {cx:.2} should be in left half");
        assert!(cy < 0.5, "TopLeft: badge y-centroid {cy:.2} should be in top half");
    }

    #[test]
    fn badge_position_bottom_right_places_badges_in_bottom_right_quadrant() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(&test_png(500, 750), &position_test_badges(), &font, 85,
            BadgePosition::BottomRight, BadgeStyle::Horizontal, LabelStyle::Text,
            BadgeDirection::Vertical, 500, 1.0, BadgeSize::Medium).unwrap();
        let (cx, cy) = badge_centroid(&result);
        assert!(cx > 0.5, "BottomRight: badge x-centroid {cx:.2} should be in right half");
        assert!(cy > 0.5, "BottomRight: badge y-centroid {cy:.2} should be in bottom half");
    }

    #[test]
    fn render_logo_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let result = render_logo_sync(&png, &[], &font, BadgeStyle::Horizontal, LabelStyle::Text, 1722, 2.2).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &font, 85, BadgePosition::TopRight, BadgeStyle::Vertical, LabelStyle::Text, BadgeDirection::Vertical, 3840, 2.0, BadgeSize::Medium).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }
}
