use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

use crate::services::ratings::RatingBadge;

const BADGE_HEIGHT: u32 = 28;
const BADGE_PADDING_H: u32 = 8;
const BADGE_RADIUS: u32 = 6;
const FONT_SIZE: f32 = 16.0;
const LABEL_FONT_SIZE: f32 = 12.0;

pub fn render_badge(badge: &RatingBadge, font: &FontArc) -> RgbaImage {
    let scale = PxScale::from(FONT_SIZE);
    let label_scale = PxScale::from(LABEL_FONT_SIZE);
    let scaled_font = font.as_scaled(scale);
    let label_scaled_font = font.as_scaled(label_scale);

    let label = badge.source.label();
    let value = &badge.value;

    let label_width = text_width(label, &label_scaled_font);
    let value_width = text_width(value, &scaled_font);
    let total_width = label_width + value_width + BADGE_PADDING_H * 3 + BADGE_PADDING_H / 2 + 2; // extra half-padding on right

    let mut img = RgbaImage::new(total_width, BADGE_HEIGHT);

    // Draw label background (darker)
    let dark_bg = Rgba([0, 0, 0, 200]);
    draw_rounded_rect(&mut img, 0, 0, label_width + BADGE_PADDING_H * 2, BADGE_HEIGHT, BADGE_RADIUS, dark_bg);

    // Draw value background (colored)
    let value_x = label_width + BADGE_PADDING_H * 2;
    draw_rounded_rect(&mut img, value_x, 0, total_width - value_x, BADGE_HEIGHT, BADGE_RADIUS, badge.source.color());

    // Overdraw the inner corners to make a clean join
    // Right side of label section
    draw_filled_rect_mut(
        &mut img,
        Rect::at((label_width + BADGE_PADDING_H) as i32, 0).of_size(BADGE_PADDING_H, BADGE_HEIGHT),
        dark_bg,
    );
    // Left side of value section
    draw_filled_rect_mut(
        &mut img,
        Rect::at(value_x as i32, 0).of_size(BADGE_PADDING_H, BADGE_HEIGHT),
        badge.source.color(),
    );

    // Draw label text
    let label_y = (BADGE_HEIGHT as i32 - LABEL_FONT_SIZE as i32) / 2;
    draw_text_mut(
        &mut img,
        Rgba([255, 255, 255, 255]),
        BADGE_PADDING_H as i32,
        label_y,
        label_scale,
        font,
        label,
    );

    // Draw value text
    let value_y = (BADGE_HEIGHT as i32 - FONT_SIZE as i32) / 2;
    draw_text_mut(
        &mut img,
        Rgba([255, 255, 255, 255]),
        (value_x + BADGE_PADDING_H) as i32,
        value_y,
        scale,
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
