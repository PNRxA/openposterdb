use std::sync::Arc;

use ab_glyph::FontArc;
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, DynamicImage, RgbaImage};
use tokio::sync::Semaphore;

use crate::cache;
use crate::error::AppError;
use crate::poster::badge;
use crate::services::db::STYLE_VERTICAL;
use crate::services::ratings::RatingBadge;
use crate::services::tmdb::TmdbClient;

const BADGE_SPACING: u32 = 10;
const BADGE_BOTTOM_MARGIN: u32 = 10;
const BADGE_TOP_MARGIN: u32 = 20;
const BADGE_SIDE_MARGIN: u32 = 15;
const BADGE_ROW_SPACING: u32 = 7;
const BADGE_VERT_SPACING: u32 = 7;
const MAX_BADGES_PER_ROW: usize = 3;
const MAX_VERT_BADGES_PER_ROW: usize = 5;

pub struct PosterParams<'a> {
    pub poster_path: &'a str,
    pub badges: &'a [RatingBadge],
    pub tmdb: &'a TmdbClient,

    pub font: &'a FontArc,
    pub quality: u8,
    pub cache_dir: &'a str,
    pub poster_stale_secs: u64,
    pub poster_bytes_override: Option<Vec<u8>>,
    /// Badge position: "bc" (default), "tc", "l", "r", "tl", "tr", "bl", "br"
    pub poster_position: Arc<str>,
    pub badge_style: Arc<str>,
    pub label_style: Arc<str>,
    pub badge_direction: Arc<str>,
    pub render_semaphore: Arc<Semaphore>,
    /// Target width for the output image. Defaults to 500 for posters.
    pub target_width: u32,
    /// Badge scale factor (1.0 = default size).
    pub badge_scale: f32,
    /// TMDB CDN size string (e.g. "w500", "w780", "original").
    pub tmdb_size: Arc<str>,
    /// When true, skip writing base poster images to disk (CDN handles caching).
    pub external_cache_only: bool,
}

pub async fn generate_poster(params: PosterParams<'_>) -> Result<Vec<u8>, AppError> {
    let PosterParams {
        poster_path,
        badges,
        tmdb,
        font,
        quality,
        cache_dir,
        poster_stale_secs,
        poster_bytes_override,
        poster_position,
        badge_style,
        label_style,
        badge_direction,
        render_semaphore,
        target_width,
        badge_scale,
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
        if let Some(entry) = cache::read(&poster_cache, poster_stale_secs).await {
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
        render_poster_sync(&poster_bytes, &badges, &font, quality, &poster_position, &badge_style, &label_style, &badge_direction, target_width, badge_scale)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "poster render complete");
    Ok(buf)
}

/// Overlay badges in a vertical column, positioned according to `poster_position`.
fn overlay_vertical_stack(canvas: &mut RgbaImage, badge_images: &[RgbaImage], poster_position: &str, badge_scale: f32) {
    let vert_spacing = (BADGE_VERT_SPACING as f32 * badge_scale).round() as u32;
    let top_margin = (BADGE_TOP_MARGIN as f32 * badge_scale).round() as u32;
    let bottom_margin = (BADGE_BOTTOM_MARGIN as f32 * badge_scale).round() as u32;
    let side_margin = (BADGE_SIDE_MARGIN as f32 * badge_scale).round() as u32;

    let total_badge_height: u32 = badge_images.iter().map(|b| b.height()).sum::<u32>()
        + vert_spacing * (badge_images.len() as u32).saturating_sub(1);
    let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);

    // Vertical anchor
    let start_y = if poster_position.starts_with('t') {
        top_margin
    } else if poster_position.starts_with('b') {
        canvas.height().saturating_sub(total_badge_height + bottom_margin)
    } else {
        // "l", "r" — vertically centered
        (canvas.height().saturating_sub(total_badge_height)) / 2
    };

    // Horizontal anchor
    let is_left = poster_position.ends_with('l');
    let is_right = poster_position.ends_with('r');

    let base_x = if is_left {
        side_margin
    } else if is_right {
        canvas.width().saturating_sub(max_badge_width + side_margin)
    } else {
        // center
        (canvas.width().saturating_sub(max_badge_width)) / 2
    };

    let mut y = start_y;
    for badge_img in badge_images {
        let bx = if is_left {
            base_x
        } else if is_right {
            base_x + max_badge_width.saturating_sub(badge_img.width())
        } else {
            base_x + (max_badge_width.saturating_sub(badge_img.width())) / 2
        };
        imageops::overlay(canvas, badge_img, bx as i64, y as i64);
        y += badge_img.height() + vert_spacing;
    }
}

/// Overlay badges in horizontal rows, positioned according to `poster_position`.
fn overlay_horizontal_rows(canvas: &mut RgbaImage, badge_images: &[RgbaImage], poster_position: &str, max_per_row: usize, badge_scale: f32) {
    let spacing = (BADGE_SPACING as f32 * badge_scale).round() as u32;
    let row_spacing = (BADGE_ROW_SPACING as f32 * badge_scale).round() as u32;
    let top_margin = (BADGE_TOP_MARGIN as f32 * badge_scale).round() as u32;
    let bottom_margin = (BADGE_BOTTOM_MARGIN as f32 * badge_scale).round() as u32;
    let side_margin = (BADGE_SIDE_MARGIN as f32 * badge_scale).round() as u32;

    let rows: Vec<&[RgbaImage]> = badge_images.chunks(max_per_row).collect();
    let badge_height = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
    let total_height = badge_height * rows.len() as u32
        + row_spacing * (rows.len() as u32).saturating_sub(1);

    // Vertical anchor
    let base_y = if poster_position.starts_with('t') {
        top_margin
    } else if poster_position.starts_with('b') {
        canvas.height().saturating_sub(total_height + bottom_margin)
    } else {
        // "l", "r" — vertically centered
        (canvas.height().saturating_sub(total_height)) / 2
    };

    // Horizontal alignment
    let is_left = poster_position.ends_with('l');
    let is_right = poster_position.ends_with('r');

    for (row_idx, row) in rows.iter().enumerate() {
        let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
            + spacing * (row.len() as u32).saturating_sub(1);
        let y = base_y + row_idx as u32 * (badge_height + row_spacing);

        let start_x = if is_left {
            side_margin
        } else if is_right {
            canvas.width().saturating_sub(row_width + side_margin)
        } else {
            (canvas.width().saturating_sub(row_width)) / 2
        };

        let mut x = start_x;
        for badge_img in *row {
            let by = y + (badge_height.saturating_sub(badge_img.height())) / 2;
            imageops::overlay(canvas, badge_img, x as i64, by as i64);
            x += badge_img.width() + spacing;
        }
    }
}

pub fn render_poster_sync(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    poster_position: &str,
    badge_style: &str,
    label_style: &str,
    badge_direction: &str,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(poster_bytes)
        .map_err(AppError::Image)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = match badge_style {
            STYLE_VERTICAL => badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale)).collect(),
            _ => badge::render_badges_uniform(badges, font, label_style, badge_scale),
        };

        if badge_direction == STYLE_VERTICAL {
            overlay_vertical_stack(&mut canvas, &badge_images, poster_position, badge_scale);
        } else {
            let max_per_row = if badge_style == STYLE_VERTICAL { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW };
            overlay_horizontal_rows(&mut canvas, &badge_images, poster_position, max_per_row, badge_scale);
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

/// Generate a 1x1 transparent placeholder JPEG
pub fn placeholder_jpeg() -> Vec<u8> {
    let img = image::RgbImage::from_pixel(1, 1, image::Rgb([0, 0, 0]));
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, 50);
    img.write_with_encoder(encoder).ok();
    buf
}

/// Generate a 1x1 transparent placeholder PNG
pub fn placeholder_png() -> Vec<u8> {
    let img = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        1,
        1,
        image::ExtendedColorType::Rgba8,
    )
    .ok();
    buf
}

const LOGO_BADGE_ROW_SPACING: u32 = 7;
const LOGO_BADGE_SPACING: u32 = 10;
const LOGO_MAX_BADGES_PER_ROW: usize = 3;
const LOGO_SPACING_BELOW: u32 = 15;

pub fn render_logo_sync(
    logo_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    badge_style: &str,
    label_style: &str,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(logo_bytes).map_err(AppError::Image)?;

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

    let logo_badge_spacing = (LOGO_BADGE_SPACING as f32 * badge_scale).round() as u32;
    let logo_badge_row_spacing = (LOGO_BADGE_ROW_SPACING as f32 * badge_scale).round() as u32;
    let logo_spacing_below = (LOGO_SPACING_BELOW as f32 * badge_scale).round() as u32;

    match badge_style {
        "v" => {
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
        }
        _ => {
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
}

pub async fn generate_logo(
    logo_bytes: Vec<u8>,
    badges: Vec<RatingBadge>,
    font: FontArc,
    badge_style: Arc<str>,
    label_style: Arc<str>,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("logo render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || render_logo_sync(&logo_bytes, &badges, &font, &badge_style, &label_style, target_width, badge_scale))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "logo render complete");
    Ok(buf)
}

const BACKDROP_MARGIN: u32 = 20;
const BACKDROP_BADGE_SPACING: u32 = 7;

pub fn render_backdrop_sync(
    backdrop_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    badge_style: &str,
    label_style: &str,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(backdrop_bytes).map_err(AppError::Image)?;

    let base = if base.width() != target_width {
        let scale = target_width as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(target_width, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = match badge_style {
            "h" => badge::render_badges_uniform(badges, font, label_style, badge_scale),
            _ => badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style, badge_scale)).collect(),
        };

        let backdrop_margin = (BACKDROP_MARGIN as f32 * badge_scale).round() as u32;
        let backdrop_badge_spacing = (BACKDROP_BADGE_SPACING as f32 * badge_scale).round() as u32;
        let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);
        let x = canvas.width().saturating_sub(max_badge_width + backdrop_margin);
        let mut y = backdrop_margin;

        for badge_img in &badge_images {
            let bx = x + max_badge_width.saturating_sub(badge_img.width());
            imageops::overlay(&mut canvas, badge_img, bx as i64, y as i64);
            y += badge_img.height() + backdrop_badge_spacing;
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
    badge_style: Arc<str>,
    label_style: Arc<str>,
    render_semaphore: Arc<Semaphore>,
    target_width: u32,
    badge_scale: f32,
) -> Result<Vec<u8>, AppError> {
    if render_semaphore.available_permits() == 0 {
        tracing::debug!("render queue full, waiting for permit");
    }
    let _permit = render_semaphore.acquire().await
        .map_err(|_| AppError::Other("render queue closed".into()))?;
    tracing::debug!("backdrop render started");
    let start = std::time::Instant::now();

    let buf = tokio::task::spawn_blocking(move || render_backdrop_sync(&backdrop_bytes, &badges, &font, quality, &badge_style, &label_style, target_width, badge_scale))
        .await
        .map_err(|e| AppError::Other(e.to_string()))??;

    tracing::debug!(elapsed_ms = start.elapsed().as_millis() as u64, "backdrop render complete");
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_jpeg_produces_valid_jpeg() {
        let bytes = placeholder_jpeg();
        assert!(!bytes.is_empty());
        // JPEG files start with FF D8
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }

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

        let result = render_poster_sync(&png_bytes, &[], &font, 85, "bc", "h", "t", "h", 500, 1.0).unwrap();
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

        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "bc", "h", "t", "h", 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_invalid_image_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(b"not an image", &[], &font, 85, "bc", "h", "t", "h", 500, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn placeholder_png_produces_valid_png() {
        let bytes = placeholder_png();
        assert!(!bytes.is_empty());
        // PNG files start with 0x89 P N G
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
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
        let result = render_logo_sync(&png, &[], &font, "h", "t", 500, 1.0).unwrap();
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
        let result = render_logo_sync(&png, &badges, &font, "h", "t", 500, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a logo wider than TARGET_WIDTH (500)
        let png = test_png(1000, 200);
        let result = render_logo_sync(&png, &[], &font, "h", "t", 500, 1.0).unwrap();
        assert!(!result.is_empty());
        // Verify the output is valid PNG and was produced (implicitly downscaled)
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_logo_sync(b"not an image", &[], &font, "h", "t", 500, 1.0);
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &font, 85, "v", "t", 1280, 1.0).unwrap();
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
        let result = render_backdrop_sync(&png, &badges, &font, 85, "v", "t", 1280, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a backdrop wider than TARGET_WIDTH (1280)
        let png = test_png(2560, 1440);
        let result = render_backdrop_sync(&png, &[], &font, 85, "v", "t", 1280, 1.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_backdrop_sync(b"not an image", &[], &font, 85, "v", "t", 1280, 1.0);
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "tc", "h", "t", "h", 500, 1.0).unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "l", "h", "t", "h", 500, 1.0).unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "r", "h", "t", "h", 500, 1.0).unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "bc", "h", "i", "h", 500, 1.0).unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "bc", "h", "t", "v", 500, 1.0).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at top-left corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "tl", "h", "t", "v", 500, 1.0).unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at bottom-right corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, "br", "h", "t", "v", 500, 1.0).unwrap();
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
        let result = render_logo_sync(&png, &badges, &font, "h", "i", 500, 1.0).unwrap();
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
        let result = render_backdrop_sync(&png, &badges, &font, 85, "v", "i", 1280, 1.0).unwrap();
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

        let result = generate_poster(PosterParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            poster_stale_secs: 3600,
            poster_position: Arc::from("bc"),
            badge_style: Arc::from("h"),
            label_style: Arc::from("t"),
            badge_direction: Arc::from("h"),
            render_semaphore: sem.clone(),
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
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

        let result = generate_poster(PosterParams {
            poster_bytes_override: Some(png),
            poster_path: "",
            badges: &[],
            tmdb: &tmdb,
            font: &font,
            quality: 85,
            cache_dir: "/tmp",
            poster_stale_secs: 3600,
            poster_position: Arc::from("bc"),
            badge_style: Arc::from("h"),
            label_style: Arc::from("t"),
            badge_direction: Arc::from("h"),
            render_semaphore: sem,
            target_width: 500,
            badge_scale: 1.0,
            tmdb_size: Arc::from("w500"),
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

        let result = generate_logo(png, vec![], font, Arc::from("h"), Arc::from("t"), sem.clone(), 500, 1.0).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_logo_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_logo(png, vec![], font, Arc::from("h"), Arc::from("t"), sem, 500, 1.0).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_backdrop_acquires_and_releases_semaphore() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));

        let result = generate_backdrop(png, vec![], font, 85, Arc::from("h"), Arc::from("t"), sem.clone(), 1280, 1.0).await;
        assert!(result.is_ok());
        assert_eq!(sem.available_permits(), 1);
    }

    #[tokio::test]
    async fn generate_backdrop_closed_semaphore_returns_error() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let sem = Arc::new(Semaphore::new(1));
        sem.close();

        let result = generate_backdrop(png, vec![], font, 85, Arc::from("h"), Arc::from("t"), sem, 1280, 1.0).await;
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
            generate_logo(png, vec![], font2, Arc::from("h"), Arc::from("t"), sem2, 500, 1.0).await
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
        let result = render_poster_sync(&png, &[], &font, 85, "bc", "h", "t", "h", 1280, 2.2).unwrap();
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
        let result = render_poster_sync(&png, &badges, &font, 85, "bc", "h", "t", "h", 1280, 2.2).unwrap();
        let img = image::load_from_memory(&result).unwrap();
        assert_eq!(img.width(), 1280);
    }

    #[test]
    fn render_logo_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(200, 80);
        let result = render_logo_sync(&png, &[], &font, "h", "t", 1722, 2.2).unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_backdrop_at_large_target_width() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &font, 85, "v", "t", 3840, 2.0).unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }
}
