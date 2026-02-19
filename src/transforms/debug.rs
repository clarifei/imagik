use ab_glyph::{FontArc, PxScale};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use std::sync::OnceLock;

// cached font to avoid parsing on every request
static FONT: OnceLock<FontArc> = OnceLock::new();

/// renders a debug overlay showing processing time.
///
/// displays "{ms}ms" in the bottom-right corner with a black background.
/// useful for benchmarking and spotting performance regressions.
///
/// font size scales with image height (2% of height, clamped 12-48px)
/// so it's readable on both thumbnails and full-res images.
///
/// optimization: works directly on rgba to avoid dynamicimage conversion.
pub fn apply_debug_overlay(img: &mut RgbaImage, processing_time_ms: u128) {
    let (width, height) = img.dimensions();

    let text = format!("{}ms", processing_time_ms);

    // get cached font or initialize on first use
    let font = FONT.get_or_init(|| {
        let font_data = include_bytes!("../../assets/CascadiaMono-VariableFont_wght.ttf");
        FontArc::try_from_slice(font_data).expect("font data should be valid")
    });

    let font_size = (height as f32 * 0.02).clamp(12.0, 48.0);
    let scale = PxScale::from(font_size);

    let text_width = text.len() as f32 * font_size * 0.6;
    let text_height = font_size;

    let padding = (font_size * 0.5) as i32;
    let x = width as i32 - text_width as i32 - padding;
    let y = height as i32 - text_height as i32 - padding;

    let bg_padding = 4;
    let bg_x = (x - bg_padding).max(0) as u32;
    let bg_y = (y - bg_padding).max(0) as u32;
    let bg_width = (text_width as u32) + (bg_padding * 2) as u32;
    let bg_height = (text_height as u32) + (bg_padding * 2) as u32;

    let bg_width = bg_width.min(width - bg_x);
    let bg_height = bg_height.min(height - bg_y);

    // fill background (black)
    for by in bg_y..(bg_y + bg_height) {
        for bx in bg_x..(bg_x + bg_width) {
            if let Some(pixel) = img.get_pixel_mut_checked(bx, by) {
                pixel.0 = [0, 0, 0, 255];
            }
        }
    }

    draw_text_mut(img, Rgba([255, 255, 255, 255]), x, y, scale, &font, &text);
}

/// parses debug flag.
/// accepts: 1, true, yes, on (case-insensitive)
pub fn parse_debug(value: &str) -> bool {
    crate::utils::parser::parse_flag(value)
}
