use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::{imageops, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

use crate::poster::icons;
use crate::services::ratings::RatingBadge;

const BADGE_HEIGHT: u32 = 50;
const BADGE_PADDING_H: u32 = 14;
const BADGE_RADIUS: u32 = 10;
const FONT_SIZE: f32 = 28.0;
const LABEL_FONT_SIZE: f32 = 21.0;
const ICON_HEIGHT: u32 = 48;

/// Compute the width of an icon when scaled to the given target height, preserving aspect ratio.
fn icon_scaled_width(icon: &RgbaImage, target_height: u32) -> u32 {
    if icon.height() == 0 {
        target_height
    } else {
        (icon.width() as f32 * target_height as f32 / icon.height() as f32).ceil() as u32
    }
}

pub fn render_badge(badge: &RatingBadge, font: &FontArc, label_style: &str) -> RgbaImage {
    render_badge_with_widths(badge, font, None, None, label_style)
}

/// Pre-compute scaled fonts for badge rendering (avoids redundant work).
struct BadgeFonts<'a> {
    font: &'a FontArc,
    scale: PxScale,
    label_scale: PxScale,
    scaled: ab_glyph::PxScaleFont<&'a FontArc>,
    label_scaled: ab_glyph::PxScaleFont<&'a FontArc>,
}

impl<'a> BadgeFonts<'a> {
    fn new(font: &'a FontArc) -> Self {
        let scale = PxScale::from(FONT_SIZE);
        let label_scale = PxScale::from(LABEL_FONT_SIZE);
        Self {
            font,
            scale,
            label_scale,
            scaled: font.as_scaled(scale),
            label_scaled: font.as_scaled(label_scale),
        }
    }
}

/// Render all badges with uniform label and value section widths.
pub fn render_badges_uniform(badges: &[RatingBadge], font: &FontArc, label_style: &str) -> Vec<RgbaImage> {
    if badges.is_empty() {
        return vec![];
    }

    let fonts = BadgeFonts::new(font);

    let max_label_width = if label_style == "icon" {
        // For icon mode, use the max icon width (scaled to icon height)
        badges.iter()
            .map(|b| icon_scaled_width(icons::icon_for_source(&b.source), ICON_HEIGHT))
            .max()
            .unwrap_or(ICON_HEIGHT)
    } else {
        badges.iter()
            .map(|b| text_width(b.source.label(), &fonts.label_scaled))
            .max()
            .unwrap_or(0)
    };
    let max_value_width = badges.iter()
        .map(|b| text_width(&b.value, &fonts.scaled))
        .max()
        .unwrap_or(0);

    badges.iter()
        .map(|b| render_badge_inner(b, &fonts, Some(max_label_width), Some(max_value_width), label_style))
        .collect()
}

fn render_badge_with_widths(
    badge: &RatingBadge,
    font: &FontArc,
    uniform_label_width: Option<u32>,
    uniform_value_width: Option<u32>,
    label_style: &str,
) -> RgbaImage {
    let fonts = BadgeFonts::new(font);
    render_badge_inner(badge, &fonts, uniform_label_width, uniform_value_width, label_style)
}

fn render_badge_inner(
    badge: &RatingBadge,
    fonts: &BadgeFonts<'_>,
    uniform_label_width: Option<u32>,
    uniform_value_width: Option<u32>,
    label_style: &str,
) -> RgbaImage {
    let use_icon = label_style == "icon";

    let label = badge.source.label();
    let value = &badge.value;

    let label_width = if use_icon {
        let actual_w = icon_scaled_width(icons::icon_for_source(&badge.source), ICON_HEIGHT);
        uniform_label_width.unwrap_or(actual_w)
    } else {
        uniform_label_width.unwrap_or_else(|| text_width(label, &fonts.label_scaled))
    };
    let value_width = uniform_value_width.unwrap_or_else(|| text_width(value, &fonts.scaled));
    let total_width = label_width + value_width + BADGE_PADDING_H * 3 + BADGE_PADDING_H / 2 + 2;

    let mut img = RgbaImage::new(total_width, BADGE_HEIGHT);

    // Draw label background (colored)
    let dark_bg = Rgba([0, 0, 0, 200]);
    draw_rounded_rect(&mut img, 0, 0, label_width + BADGE_PADDING_H * 2, BADGE_HEIGHT, BADGE_RADIUS, badge.source.color());

    // Draw value background (dark)
    let value_x = label_width + BADGE_PADDING_H * 2;
    draw_rounded_rect(&mut img, value_x, 0, total_width - value_x, BADGE_HEIGHT, BADGE_RADIUS, dark_bg);

    // Overdraw the inner corners to make a clean join
    draw_filled_rect_mut(
        &mut img,
        Rect::at((label_width + BADGE_PADDING_H) as i32, 0).of_size(BADGE_PADDING_H, BADGE_HEIGHT),
        badge.source.color(),
    );
    draw_filled_rect_mut(
        &mut img,
        Rect::at(value_x as i32, 0).of_size(BADGE_PADDING_H, BADGE_HEIGHT),
        dark_bg,
    );

    // Draw label (icon or text, centered within uniform label area)
    if use_icon {
        let icon = icons::icon_for_source(&badge.source);
        let icon_w = icon_scaled_width(icon, ICON_HEIGHT);
        let scaled_icon = imageops::resize(icon, icon_w, ICON_HEIGHT, imageops::FilterType::Lanczos3);
        let ix = BADGE_PADDING_H + (label_width.saturating_sub(icon_w)) / 2;
        let iy = (BADGE_HEIGHT.saturating_sub(ICON_HEIGHT)) / 2;
        imageops::overlay(&mut img, &scaled_icon, ix as i64, iy as i64);
    } else {
        let actual_label_width = text_width(label, &fonts.label_scaled);
        let label_x = BADGE_PADDING_H + (label_width.saturating_sub(actual_label_width)) / 2;
        let label_y = (BADGE_HEIGHT as i32 - LABEL_FONT_SIZE as i32) / 2;
        draw_text_mut(
            &mut img,
            Rgba([255, 255, 255, 255]),
            label_x as i32,
            label_y,
            fonts.label_scale,
            fonts.font,
            label,
        );
    }

    // Draw value text (centered within uniform value area)
    let actual_value_width = text_width(value, &fonts.scaled);
    let value_text_x = value_x + BADGE_PADDING_H + (value_width.saturating_sub(actual_value_width)) / 2;
    let value_y = (BADGE_HEIGHT as i32 - FONT_SIZE as i32) / 2;
    draw_text_mut(
        &mut img,
        Rgba([255, 255, 255, 255]),
        value_text_x as i32,
        value_y,
        fonts.scale,
        fonts.font,
        value,
    );

    img
}

const VERT_BADGE_WIDTH: u32 = 76;
const VERT_BADGE_PADDING_V: u32 = 8;
const VERT_LABEL_FONT_SIZE: f32 = 21.0;
const VERT_VALUE_FONT_SIZE: f32 = 28.0;

/// Render a vertical badge: source label on top, rating value below.
/// Used for left/right poster positions.
pub fn render_vertical_badge(badge: &RatingBadge, font: &FontArc, label_style: &str) -> RgbaImage {
    let use_icon = label_style == "icon";
    let label_scale = PxScale::from(VERT_LABEL_FONT_SIZE);
    let value_scale = PxScale::from(VERT_VALUE_FONT_SIZE);

    let label = badge.source.label();
    let value = &badge.value;

    let label_area_h = if use_icon { ICON_HEIGHT } else { VERT_LABEL_FONT_SIZE as u32 };
    let total_height = VERT_BADGE_PADDING_V
        + label_area_h
        + 4 // gap between label and value
        + VERT_VALUE_FONT_SIZE as u32
        + VERT_BADGE_PADDING_V;

    let mut img = RgbaImage::new(VERT_BADGE_WIDTH, total_height);

    // Draw full background with source color
    draw_rounded_rect(&mut img, 0, 0, VERT_BADGE_WIDTH, total_height, BADGE_RADIUS, badge.source.color());

    // Draw a dark rect for the value area (bottom half)
    let value_area_y = VERT_BADGE_PADDING_V + label_area_h + 2;
    let value_area_h = total_height - value_area_y;
    draw_rounded_rect(
        &mut img,
        0,
        value_area_y,
        VERT_BADGE_WIDTH,
        value_area_h,
        BADGE_RADIUS,
        Rgba([0, 0, 0, 200]),
    );
    // Overdraw the top corners of the dark area to clean the join
    draw_filled_rect_mut(
        &mut img,
        Rect::at(0, value_area_y as i32).of_size(VERT_BADGE_WIDTH, BADGE_RADIUS.min(value_area_h)),
        Rgba([0, 0, 0, 200]),
    );

    // Center label (icon or text) within the colored label area
    if use_icon {
        let icon = icons::icon_for_source(&badge.source);
        let icon_w = icon_scaled_width(icon, ICON_HEIGHT);
        let scaled_icon = imageops::resize(icon, icon_w, ICON_HEIGHT, imageops::FilterType::Lanczos3);
        let ix = (VERT_BADGE_WIDTH.saturating_sub(icon_w)) / 2;
        let iy = (value_area_y.saturating_sub(ICON_HEIGHT)) / 2;
        imageops::overlay(&mut img, &scaled_icon, ix as i64, iy as i64);
    } else {
        let label_scaled = font.as_scaled(label_scale);
        let lw = text_width(label, &label_scaled);
        let label_x = (VERT_BADGE_WIDTH.saturating_sub(lw)) / 2;
        let label_y = (value_area_y.saturating_sub(VERT_LABEL_FONT_SIZE as u32)) / 2;
        draw_text_mut(
            &mut img,
            Rgba([255, 255, 255, 255]),
            label_x as i32,
            label_y as i32,
            label_scale,
            font,
            label,
        );
    }

    // Center value text
    let value_scaled = font.as_scaled(value_scale);
    let vw = text_width(value, &value_scaled);
    let value_x = (VERT_BADGE_WIDTH.saturating_sub(vw)) / 2;
    let value_y = (value_area_y + VERT_BADGE_PADDING_V / 2) as i32;
    draw_text_mut(
        &mut img,
        Rgba([255, 255, 255, 255]),
        value_x as i32,
        value_y,
        value_scale,
        font,
        value,
    );

    img
}

fn text_width(text: &str, font: &ab_glyph::PxScaleFont<&FontArc>) -> u32 {
    let width: f32 = text
        .chars()
        .map(|c| font.h_advance(font.glyph_id(c)))
        .sum();
    width.ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ratings::{RatingBadge, RatingSource};

    fn test_font() -> FontArc {
        FontArc::try_from_slice(crate::FONT_BYTES).unwrap()
    }

    #[test]
    fn render_badge_correct_height() {
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_badge(&badge, &test_font(), "text");
        assert_eq!(img.height(), BADGE_HEIGHT);
        assert!(img.width() > 0);
    }

    #[test]
    fn render_badge_all_sources_produce_valid_images() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_badge(&badge, &font, "text");
            assert_eq!(img.height(), BADGE_HEIGHT, "wrong height for {:?}", source);
            assert!(img.width() > 0, "zero width for {:?}", source);
        }
    }

    #[test]
    fn render_badge_icon_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_badge(&badge, &font, "icon");
            assert_eq!(img.height(), BADGE_HEIGHT, "wrong height for {:?}", source);
            assert!(img.width() > 0, "zero width for {:?}", source);
        }
    }

    #[test]
    fn render_badge_width_scales_with_value_length() {
        let font = test_font();
        let short = RatingBadge {
            source: RatingSource::Imdb,
            value: "5".to_string(),
        };
        let long = RatingBadge {
            source: RatingSource::Imdb,
            value: "100%".to_string(),
        };

        let short_img = render_badge(&short, &font, "text");
        let long_img = render_badge(&long, &font, "text");

        assert!(
            long_img.width() > short_img.width(),
            "longer value should produce wider badge"
        );
    }

    #[test]
    fn render_vertical_badge_correct_dimensions() {
        let badge = RatingBadge {
            source: RatingSource::Imdb,
            value: "8.5".to_string(),
        };
        let img = render_vertical_badge(&badge, &test_font(), "text");
        assert_eq!(img.width(), VERT_BADGE_WIDTH);
        assert!(img.height() > 0);
    }

    #[test]
    fn render_vertical_badge_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_vertical_badge(&badge, &font, "text");
            assert_eq!(img.width(), VERT_BADGE_WIDTH, "wrong width for {:?}", source);
            assert!(img.height() > 0, "zero height for {:?}", source);
        }
    }

    #[test]
    fn render_vertical_badge_icon_all_sources() {
        let font = test_font();
        let sources = [
            RatingSource::Imdb,
            RatingSource::Tmdb,
            RatingSource::Rt,
            RatingSource::RtAudience,
            RatingSource::Metacritic,
            RatingSource::Trakt,
            RatingSource::Letterboxd,
            RatingSource::Mal,
        ];

        for source in sources {
            let badge = RatingBadge {
                source,
                value: "75%".to_string(),
            };
            let img = render_vertical_badge(&badge, &font, "icon");
            assert_eq!(img.width(), VERT_BADGE_WIDTH, "wrong width for {:?}", source);
            assert!(img.height() > 0, "zero height for {:?}", source);
        }
    }

    #[test]
    fn render_badge_empty_value() {
        let font = test_font();
        let badge = RatingBadge {
            source: RatingSource::Tmdb,
            value: String::new(),
        };
        // Should not panic
        let img = render_badge(&badge, &font, "text");
        assert_eq!(img.height(), BADGE_HEIGHT);
    }
}

fn draw_rounded_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, r: u32, color: Rgba<u8>) {
    // Simple approach: draw a filled rect and round corners by drawing circles
    // For simplicity, just draw the filled rect — true rounded rects need more complex logic
    draw_filled_rect_mut(
        img,
        Rect::at(x as i32, y as i32).of_size(w, h),
        color,
    );

    // Clear corners to simulate rounding (set to transparent)
    let transparent = Rgba([0, 0, 0, 0]);
    for dy in 0..r {
        for dx in 0..r {
            let dist_sq = (r - dx) * (r - dx) + (r - dy) * (r - dy);
            if dist_sq > r * r {
                // Top-left
                if x + dx < img.width() && y + dy < img.height() {
                    img.put_pixel(x + dx, y + dy, transparent);
                }
                // Top-right
                let rx = x + w - 1 - dx;
                if rx < img.width() && y + dy < img.height() {
                    img.put_pixel(rx, y + dy, transparent);
                }
                // Bottom-left
                let by = y + h - 1 - dy;
                if x + dx < img.width() && by < img.height() {
                    img.put_pixel(x + dx, by, transparent);
                }
                // Bottom-right
                if rx < img.width() && by < img.height() {
                    img.put_pixel(rx, by, transparent);
                }
            }
        }
    }
}
