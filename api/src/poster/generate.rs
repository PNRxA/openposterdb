use ab_glyph::FontArc;
use image::codecs::jpeg::JpegEncoder;
use image::{imageops, DynamicImage, RgbaImage};
use crate::cache;
use crate::error::AppError;
use crate::poster::badge;
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
    /// Whether to normalize the poster width (e.g. for fanart sources with larger images).
    pub normalize_width: bool,
    /// Badge position: "bottom-center" (default), "top-center", "left", "right",
    /// "top-left", "top-right", "bottom-left", "bottom-right"
    pub poster_position: String,
    pub badge_style: String,
    pub label_style: String,
    pub badge_direction: String,
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
        normalize_width,
        poster_position,
        badge_style,
        label_style,
        badge_direction,
    } = params;

    let poster_bytes = if let Some(bytes) = poster_bytes_override {
        bytes
    } else {
        // Fetch base poster from TMDB, using cache
        let poster_cache = cache::base_poster_path(cache_dir, poster_path)?;
        if let Some(entry) = cache::read(&poster_cache, poster_stale_secs).await {
            if entry.is_stale {
                let bytes = tmdb.fetch_poster_bytes(poster_path).await?;
                cache::write(&poster_cache, &bytes).await?;
                bytes
            } else {
                entry.bytes
            }
        } else {
            let bytes = tmdb.fetch_poster_bytes(poster_path).await?;
            cache::write(&poster_cache, &bytes).await?;
            bytes
        }
    };

    // Move CPU-bound image processing to a blocking thread
    let badges = badges.to_vec();
    let font = font.clone();
    let buf = tokio::task::spawn_blocking(move || {
        render_poster_sync(&poster_bytes, &badges, &font, quality, normalize_width, &poster_position, &badge_style, &label_style, &badge_direction)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    Ok(buf)
}

/// Overlay badges in a vertical column, positioned according to `poster_position`.
fn overlay_vertical_stack(canvas: &mut RgbaImage, badge_images: &[RgbaImage], poster_position: &str) {
    let total_badge_height: u32 = badge_images.iter().map(|b| b.height()).sum::<u32>()
        + BADGE_VERT_SPACING * (badge_images.len() as u32).saturating_sub(1);
    let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);

    // Vertical anchor
    let start_y = if poster_position.starts_with("top") {
        BADGE_TOP_MARGIN
    } else if poster_position.starts_with("bottom") {
        canvas.height().saturating_sub(total_badge_height + BADGE_BOTTOM_MARGIN)
    } else {
        // "left", "right" — vertically centered
        (canvas.height().saturating_sub(total_badge_height)) / 2
    };

    // Horizontal anchor
    let is_left = poster_position.ends_with("left") || poster_position == "left";
    let is_right = poster_position.ends_with("right") || poster_position == "right";

    let base_x = if is_left {
        BADGE_SIDE_MARGIN
    } else if is_right {
        canvas.width().saturating_sub(max_badge_width + BADGE_SIDE_MARGIN)
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
        y += badge_img.height() + BADGE_VERT_SPACING;
    }
}

/// Overlay badges in horizontal rows, positioned according to `poster_position`.
fn overlay_horizontal_rows(canvas: &mut RgbaImage, badge_images: &[RgbaImage], poster_position: &str, max_per_row: usize) {
    let rows: Vec<&[RgbaImage]> = badge_images.chunks(max_per_row).collect();
    let badge_height = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
    let total_height = badge_height * rows.len() as u32
        + BADGE_ROW_SPACING * (rows.len() as u32).saturating_sub(1);

    // Vertical anchor
    let base_y = if poster_position.starts_with("top") {
        BADGE_TOP_MARGIN
    } else if poster_position.starts_with("bottom") {
        canvas.height().saturating_sub(total_height + BADGE_BOTTOM_MARGIN)
    } else {
        // "left", "right" — vertically centered
        (canvas.height().saturating_sub(total_height)) / 2
    };

    // Horizontal alignment
    let is_left = poster_position.ends_with("left") || poster_position == "left";
    let is_right = poster_position.ends_with("right") || poster_position == "right";

    for (row_idx, row) in rows.iter().enumerate() {
        let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
            + BADGE_SPACING * (row.len() as u32).saturating_sub(1);
        let y = base_y + row_idx as u32 * (badge_height + BADGE_ROW_SPACING);

        let start_x = if is_left {
            BADGE_SIDE_MARGIN
        } else if is_right {
            canvas.width().saturating_sub(row_width + BADGE_SIDE_MARGIN)
        } else {
            (canvas.width().saturating_sub(row_width)) / 2
        };

        let mut x = start_x;
        for badge_img in *row {
            let by = y + (badge_height.saturating_sub(badge_img.height())) / 2;
            imageops::overlay(canvas, badge_img, x as i64, by as i64);
            x += badge_img.width() + BADGE_SPACING;
        }
    }
}

pub fn render_poster_sync(
    poster_bytes: &[u8],
    badges: &[RatingBadge],
    font: &FontArc,
    quality: u8,
    normalize_width: bool,
    poster_position: &str,
    badge_style: &str,
    label_style: &str,
    badge_direction: &str,
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(poster_bytes)
        .map_err(AppError::Image)?;

    const TARGET_WIDTH: u32 = 500;
    let base = if normalize_width && base.width() > TARGET_WIDTH {
        let scale = TARGET_WIDTH as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(TARGET_WIDTH, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = match badge_style {
            "vertical" => badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style)).collect(),
            _ => badge::render_badges_uniform(badges, font, label_style),
        };

        if badge_direction == "vertical" {
            overlay_vertical_stack(&mut canvas, &badge_images, poster_position);
        } else {
            let max_per_row = if badge_style == "vertical" { MAX_VERT_BADGES_PER_ROW } else { MAX_BADGES_PER_ROW };
            overlay_horizontal_rows(&mut canvas, &badge_images, poster_position, max_per_row);
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
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(logo_bytes).map_err(AppError::Image)?;

    const TARGET_WIDTH: u32 = 500;
    let base = if base.width() > TARGET_WIDTH {
        let scale = TARGET_WIDTH as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(TARGET_WIDTH, target_height, image::imageops::FilterType::Lanczos3)
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

    match badge_style {
        "vertical" => {
            // Vertical badge shapes arranged in rows below the logo
            let badge_images: Vec<RgbaImage> = badges
                .iter()
                .map(|b| badge::render_vertical_badge(b, font, label_style))
                .collect();

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(MAX_VERT_BADGES_PER_ROW).collect();
            let max_badge_height: u32 = badge_images.iter().map(|b| b.height()).max().unwrap_or(0);
            let total_badge_height = max_badge_height * rows.len() as u32
                + LOGO_BADGE_ROW_SPACING * (rows.len() as u32).saturating_sub(1);

            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + LOGO_BADGE_SPACING * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + LOGO_SPACING_BELOW + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            let badges_start_y = logo_img.height() + LOGO_SPACING_BELOW;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + LOGO_BADGE_SPACING * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (max_badge_height + LOGO_BADGE_ROW_SPACING);

                let mut x = start_x;
                for badge_img in *row {
                    let by = y + (max_badge_height.saturating_sub(badge_img.height())) / 2;
                    imageops::overlay(&mut canvas, badge_img, x as i64, by as i64);
                    x += badge_img.width() + LOGO_BADGE_SPACING;
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
            let badge_images = badge::render_badges_uniform(badges, font, label_style);

            let rows: Vec<&[RgbaImage]> = badge_images.chunks(LOGO_MAX_BADGES_PER_ROW).collect();
            let badge_height = badge_images[0].height();
            let total_badge_height =
                badge_height * rows.len() as u32 + LOGO_BADGE_ROW_SPACING * (rows.len() as u32).saturating_sub(1);

            // Compute row widths to determine canvas width
            let max_row_width: u32 = rows
                .iter()
                .map(|row| {
                    row.iter().map(|b| b.width()).sum::<u32>()
                        + LOGO_BADGE_SPACING * (row.len() as u32).saturating_sub(1)
                })
                .max()
                .unwrap_or(0);

            let canvas_width = logo_img.width().max(max_row_width);
            let canvas_height = logo_img.height() + LOGO_SPACING_BELOW + total_badge_height;

            let mut canvas = RgbaImage::new(canvas_width, canvas_height);

            // Center logo at top
            let logo_x = (canvas_width.saturating_sub(logo_img.width())) / 2;
            imageops::overlay(&mut canvas, &logo_img, logo_x as i64, 0);

            // Center badge rows below logo
            let badges_start_y = logo_img.height() + LOGO_SPACING_BELOW;
            for (row_idx, row) in rows.iter().enumerate() {
                let row_width: u32 = row.iter().map(|b| b.width()).sum::<u32>()
                    + LOGO_BADGE_SPACING * (row.len() as u32).saturating_sub(1);
                let start_x = (canvas_width.saturating_sub(row_width)) / 2;
                let y = badges_start_y + row_idx as u32 * (badge_height + LOGO_BADGE_ROW_SPACING);

                let mut x = start_x;
                for badge_img in *row {
                    imageops::overlay(&mut canvas, badge_img, x as i64, y as i64);
                    x += badge_img.width() + LOGO_BADGE_SPACING;
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
    badge_style: String,
    label_style: String,
) -> Result<Vec<u8>, AppError> {
    tokio::task::spawn_blocking(move || render_logo_sync(&logo_bytes, &badges, &font, &badge_style, &label_style))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
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
) -> Result<Vec<u8>, AppError> {
    let base = image::load_from_memory(backdrop_bytes).map_err(AppError::Image)?;

    const TARGET_WIDTH: u32 = 1280;
    let base = if base.width() > TARGET_WIDTH {
        let scale = TARGET_WIDTH as f64 / base.width() as f64;
        let target_height = (base.height() as f64 * scale).round() as u32;
        base.resize_exact(TARGET_WIDTH, target_height, image::imageops::FilterType::Lanczos3)
    } else {
        base
    };

    let mut canvas: RgbaImage = base.to_rgba8();

    if !badges.is_empty() {
        let badge_images: Vec<RgbaImage> = match badge_style {
            "horizontal" => badge::render_badges_uniform(badges, font, label_style),
            _ => badges.iter().map(|b| badge::render_vertical_badge(b, font, label_style)).collect(),
        };

        let max_badge_width: u32 = badge_images.iter().map(|b| b.width()).max().unwrap_or(0);
        let x = canvas.width().saturating_sub(max_badge_width + BACKDROP_MARGIN);
        let mut y = BACKDROP_MARGIN;

        for badge_img in &badge_images {
            let bx = x + max_badge_width.saturating_sub(badge_img.width());
            imageops::overlay(&mut canvas, badge_img, bx as i64, y as i64);
            y += badge_img.height() + BACKDROP_BADGE_SPACING;
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
    badge_style: String,
    label_style: String,
) -> Result<Vec<u8>, AppError> {
    tokio::task::spawn_blocking(move || render_backdrop_sync(&backdrop_bytes, &badges, &font, quality, &badge_style, &label_style))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
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

        let result = render_poster_sync(&png_bytes, &[], &font, 85, false, "bottom-center", "horizontal", "text", "horizontal").unwrap();
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

        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "bottom-center", "horizontal", "text", "horizontal").unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_poster_invalid_image_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_poster_sync(b"not an image", &[], &font, 85, false, "bottom-center", "horizontal", "text", "horizontal");
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
        let result = render_logo_sync(&png, &[], &font, "horizontal", "text").unwrap();
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
        let result = render_logo_sync(&png, &badges, &font, "horizontal", "text").unwrap();
        assert!(!result.is_empty());
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a logo wider than TARGET_WIDTH (500)
        let png = test_png(1000, 200);
        let result = render_logo_sync(&png, &[], &font, "horizontal", "text").unwrap();
        assert!(!result.is_empty());
        // Verify the output is valid PNG and was produced (implicitly downscaled)
        assert_eq!(&result[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_logo_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_logo_sync(b"not an image", &[], &font, "horizontal", "text");
        assert!(result.is_err());
    }

    #[test]
    fn render_backdrop_no_badges() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let png = test_png(640, 360);
        let result = render_backdrop_sync(&png, &[], &font, 85, "vertical", "text").unwrap();
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
        let result = render_backdrop_sync(&png, &badges, &font, 85, "vertical", "text").unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_downscales_wide_image() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        // Create a backdrop wider than TARGET_WIDTH (1280)
        let png = test_png(2560, 1440);
        let result = render_backdrop_sync(&png, &[], &font, 85, "vertical", "text").unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }

    #[test]
    fn render_backdrop_invalid_bytes() {
        let font = FontArc::try_from_slice(crate::FONT_BYTES).unwrap();
        let result = render_backdrop_sync(b"not an image", &[], &font, 85, "vertical", "text");
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "top-center", "horizontal", "text", "horizontal").unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "left", "horizontal", "text", "horizontal").unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "right", "horizontal", "text", "horizontal").unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "bottom-center", "horizontal", "icon", "horizontal").unwrap();
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
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "bottom-center", "horizontal", "text", "vertical").unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at top-left corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "top-left", "horizontal", "text", "vertical").unwrap();
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);

        // vertical direction at bottom-right corner
        let result = render_poster_sync(&png_bytes, &badges, &font, 85, false, "bottom-right", "horizontal", "text", "vertical").unwrap();
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
        let result = render_logo_sync(&png, &badges, &font, "horizontal", "icon").unwrap();
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
        let result = render_backdrop_sync(&png, &badges, &font, 85, "vertical", "icon").unwrap();
        assert!(!result.is_empty());
        assert_eq!(result[0], 0xFF);
        assert_eq!(result[1], 0xD8);
    }
}
