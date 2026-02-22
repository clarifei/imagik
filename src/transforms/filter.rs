use crate::utils::parser::{normalize_hue, parse_f32_range, parse_i32_range};

/// brightness: -100 (completely black) to 100 (completely white).
pub fn parse_brightness(value: &str) -> Option<i32> {
    parse_i32_range(value, -100, 100)
}

/// contrast: -100 (flat gray) to 100 (harsh contrast).
pub fn parse_contrast(value: &str) -> Option<f32> {
    parse_f32_range(value, -100.0, 100.0)
}

/// hue rotation: any integer, normalized to 0-359 using `rem_euclid`.
pub fn parse_hue_rotate(value: &str) -> Option<i32> {
    value.parse::<i32>().ok().map(normalize_hue)
}

/// saturation: -100 (grayscale) to 100 (oversaturated).
pub fn parse_saturation(value: &str) -> Option<f32> {
    parse_f32_range(value, -100.0, 100.0)
}

/// vibrance: -100 (muted) to 100 (boosted, non-linear).
pub fn parse_vibrance(value: &str) -> Option<f32> {
    parse_f32_range(value, -100.0, 100.0)
}
