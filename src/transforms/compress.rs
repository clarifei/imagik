use image::{DynamicImage, RgbaImage};

/// calculates the final webp quality value based on user options.
///
/// - if lossless is enabled, returns 100.0
/// - otherwise uses the specified quality or defaults to 85
pub fn calculate_webp_quality(webp_quality: Option<u32>, webp_lossless: bool) -> f32 {
    if webp_lossless {
        100.0
    } else {
        webp_quality.unwrap_or(85) as f32
    }
}

/// encodes a DynamicImage to webp format.
///
/// quality is 0-100, where:
/// - 100 = maximum quality, largest file
/// - 0 = minimum quality, smallest file (usually looks terrible)
/// - 85 = default sweet spot (good quality, reasonable size)
pub fn encode_to_webp(
    img: &DynamicImage,
    quality: f32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
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

/// encodes RGBA image directly to webp.
///
/// optimization: skips dynamicimage wrapper, encodes raw rgba bytes directly.
pub fn encode_rgba_image_to_webp(
    img: &RgbaImage,
    quality: f32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    encode_rgba_to_webp(img.as_raw(), img.width(), img.height(), quality)
}

fn encode_rgba_to_webp(
    rgba: &[u8],
    width: u32,
    height: u32,
    quality: f32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let encoder = webp::Encoder::from_rgba(rgba, width, height);
    let encoded = encoder.encode(quality);
    Ok(encoded.to_vec())
}

fn encode_rgb_to_webp(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: f32,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let encoder = webp::Encoder::from_rgb(rgb, width, height);
    let encoded = encoder.encode(quality);
    Ok(encoded.to_vec())
}
