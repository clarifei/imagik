use crate::aliases::ParamKey;
use crate::models::{CropMode, Gravity};
use crate::transforms::debug::parse_debug;
use crate::transforms::effect::ca::parse_chromatic;
use crate::transforms::effect::grain::{parse_grain, parse_grain_threshold};
use crate::transforms::filter::{
    parse_brightness, parse_contrast, parse_hue_rotate, parse_saturation, parse_vibrance,
};
use crate::utils::blur::parse_blur;
use crate::utils::parser::parse_flag;

use super::params::Params;

/// parses the url path into transform params.
///
/// expected format: `key1_value1,key2_value2,...`
/// example: `w_500,h_300,c_fill,blur_2.5`
///
/// unknown segments are ignored. known params are validated and can
/// return an error for malformed values.
pub fn parse_params(param_str: &str) -> Result<Params, String> {
    let mut result = Params::default();

    for segment in param_str.split(',') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (key, value) = match trimmed.find('_') {
            Some(pos) => (&trimmed[..pos], &trimmed[pos + 1..]),
            None => continue,
        };

        match ParamKey::from_alias(key) {
            Some(ParamKey::Width) => result.width = Some(parse_dimension(value, "width")?),
            Some(ParamKey::Height) => result.height = Some(parse_dimension(value, "height")?),
            Some(ParamKey::Crop) => result.crop_mode = parse_crop_mode(value),
            Some(ParamKey::Gravity) => result.gravity = parse_gravity(value),
            Some(ParamKey::AspectRatio) => result.aspect = Some(parse_aspect_ratio(value)?),
            Some(ParamKey::Rotate) => result.rotate = Some(parse_rotation(value)?),
            Some(ParamKey::Background) => result.background = Some(parse_hex_color(value)?),
            Some(ParamKey::Blur) => result.blur = parse_blur(value),
            Some(ParamKey::WebpQuality) => result.webp_quality = Some(parse_webp_quality(value)?),
            Some(ParamKey::WebpLossless) => result.webp_lossless = parse_flag(value),
            Some(ParamKey::Brightness) => result.brightness = parse_brightness(value),
            Some(ParamKey::Contrast) => result.contrast = parse_contrast(value),
            Some(ParamKey::HueRotate) => result.hue_rotate = parse_hue_rotate(value),
            Some(ParamKey::Grayscale) => result.grayscale = parse_flag(value),
            Some(ParamKey::Invert) => result.invert = parse_flag(value),
            Some(ParamKey::Saturation) => result.saturation = parse_saturation(value),
            Some(ParamKey::Vibrance) => result.vibrance = parse_vibrance(value),
            Some(ParamKey::Grain) => result.grain = parse_grain(value),
            Some(ParamKey::GrainGrayscale) => result.grain_grayscale = parse_grain(value),
            Some(ParamKey::GrainThreshold) => {
                if let Some(threshold) = parse_grain_threshold(value) {
                    result.grain_threshold = threshold;
                }
            }
            Some(ParamKey::ChromaticAberration) => {
                result.chromatic_aberration = parse_chromatic(value)
            }
            Some(ParamKey::Debug) => result.debug = parse_debug(value),
            None => {}
        }
    }

    Ok(result)
}

/// validates and parses dimension values.
///
/// 8192px max is arbitrary but reasonable — prevents memory explosions
/// from malicious requests like `w_999999999`.
fn parse_dimension(value: &str, name: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(v) if v > 0 && v <= 8192 => Ok(v),
        Ok(_) => Err(format!("{} must be between 1 and 8192", name)),
        Err(_) => Err(format!("invalid {} value: {}", name, value)),
    }
}

/// validates aspect ratio format (must be `W:H`).
///
/// we store it as a string and parse the actual numbers later —
/// lets us fail gracefully if the aspect ratio is bogus.
fn parse_aspect_ratio(value: &str) -> Result<String, String> {
    if !value.contains(':') {
        return Err("aspect ratio must be in format W:H (e.g., 16:9)".to_string());
    }
    Ok(value.to_string())
}

/// only supports right-angle rotations (0, 90, 180, 270).
///
/// arbitrary angles would require transparent backgrounds and
/// way more math — not worth it for 99% of use cases.
fn parse_rotation(value: &str) -> Result<i32, String> {
    match value.parse::<i32>() {
        Ok(v) if [0, 90, 180, 270].contains(&v) => Ok(v),
        Ok(_) => Err("rotation must be 0, 90, 180, or 270".to_string()),
        Err(_) => Err(format!("invalid rotation value: {}", value)),
    }
}

/// parses gravity/position params.
///
/// supports compass directions (n, s, e, w) and descriptive names.
/// defaults to center if unrecognized — most users expect centered crops.
fn parse_gravity(value: &str) -> Gravity {
    match value {
        "n" | "north" | "top" => Gravity::North,
        "s" | "south" | "bottom" => Gravity::South,
        "e" | "east" | "right" => Gravity::East,
        "w" | "west" | "left" => Gravity::West,
        _ => Gravity::Center,
    }
}

/// parses crop mode.
///
/// - `fill`: cover the target dimensions, crop excess
/// - `fit`: contain within dimensions, no cropping
/// - `scale`: stretch to fit (distorts aspect ratio)
/// - `pad`: fit within dimensions, pad with background color
/// - default is `fill` (most common for thumbnails)
fn parse_crop_mode(value: &str) -> CropMode {
    match value {
        "scale" => CropMode::Scale,
        "fit" => CropMode::Fit,
        "crop" => CropMode::Crop,
        "pad" => CropMode::Pad,
        _ => CropMode::Fill,
    }
}

/// parses hex colors with optional alpha.
///
/// supports:
/// - 3 char: `f0a` (becomes `ff00aa`)
/// - 6 char: `ff00aa`
/// - 8 char: `ff00aaff` (with alpha)
///
/// leading # is optional and stripped if present.
fn parse_hex_color(value: &str) -> Result<[u8; 4], String> {
    let hex = value.trim_start_matches('#');

    let (r, g, b, a) = match hex.len() {
        3 => {
            let r = parse_hex_digit(hex.as_bytes()[0])
                .map(|d| d * 16 + d)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let g = parse_hex_digit(hex.as_bytes()[1])
                .map(|d| d * 16 + d)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let b = parse_hex_digit(hex.as_bytes()[2])
                .map(|d| d * 16 + d)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            (r, g, b, 255)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let g = u8::from_str_radix(&hex[2..4], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let b = u8::from_str_radix(&hex[4..6], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let g = u8::from_str_radix(&hex[2..4], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let b = u8::from_str_radix(&hex[4..6], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            let a = u8::from_str_radix(&hex[6..8], 16)
                .map_err(|_| format!("invalid hex color: {}", value))?;
            (r, g, b, a)
        }
        _ => return Err(format!("hex color must be 3, 6, or 8 chars: {}", value)),
    };

    Ok([r, g, b, a])
}

/// webp quality is 0-100, where 100 = best quality, larger file.
///
/// default is 85 in the encoder (good balance of quality/size).
fn parse_webp_quality(value: &str) -> Result<u32, String> {
    match value.parse::<u32>() {
        Ok(v) if v <= 100 => Ok(v),
        Ok(_) => Err("webp_quality must be between 0 and 100".to_string()),
        Err(_) => Err(format!("invalid webp_quality value: {}", value)),
    }
}

/// helper to parse a single hex digit.
fn parse_hex_digit(b: u8) -> Result<u8, ()> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(()),
    }
}
