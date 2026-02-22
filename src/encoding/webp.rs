use image::{DynamicImage, RgbaImage};

/// calculates the final webp quality value based on user options.
///
/// - if lossless is enabled, returns 100.0
/// - otherwise uses the specified quality or defaults to 85
pub fn calculate_webp_quality(webp_quality: Option<u32>, webp_lossless: bool) -> f32 {
    if webp_lossless {
        100.0
    } else {
        webp_quality.map_or(85.0, |value| {
            let clamped = u8::try_from(value).unwrap_or(u8::MAX).min(100);
            f32::from(clamped)
        })
    }
}

/// encodes `DynamicImage` to webp format with automatic format detection.
///
/// handles multiple source formats:
/// - `ImageRgb8`: encodes directly as rgb
/// - `ImageRgba8`: encodes directly as rgba
/// - other formats: converts to rgba8 then encodes
///
/// quality: 0-100, where 100=lossless (if requested), 85=default.
pub fn encode_to_webp(img: &DynamicImage, quality: f32) -> Vec<u8> {
    match img {
        DynamicImage::ImageRgb8(rgb) => {
            encode_rgb_to_webp(rgb.as_raw(), img.width(), img.height(), quality)
        }
        DynamicImage::ImageRgba8(rgba) => {
            encode_rgba_to_webp(rgba.as_raw(), img.width(), img.height(), quality)
        }
        _ => {
            let converted = img.to_rgba8();
            encode_rgba_to_webp(&converted, img.width(), img.height(), quality)
        }
    }
}

/// encodes RGBA image directly to `webp`.
///
/// optimization: skips `DynamicImage` wrapper, encodes raw RGBA bytes directly.
pub fn encode_rgba_image_to_webp(img: &RgbaImage, quality: f32) -> Vec<u8> {
    encode_rgba_to_webp(img.as_raw(), img.width(), img.height(), quality)
}

fn encode_rgba_to_webp(rgba: &[u8], width: u32, height: u32, quality: f32) -> Vec<u8> {
    let encoder = webp::Encoder::from_rgba(rgba, width, height);
    let webp_data = encoder.encode(quality);
    webp_data.to_vec()
}

fn encode_rgb_to_webp(rgb: &[u8], width: u32, height: u32, quality: f32) -> Vec<u8> {
    let encoder = webp::Encoder::from_rgb(rgb, width, height);
    let webp_data = encoder.encode(quality);
    webp_data.to_vec()
}
