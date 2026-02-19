//! generic parser utilities to reduce code duplication across the codebase.

/// parses an integer within an inclusive range.
/// returns none if parsing fails or value is out of range.
pub fn parse_i32_range(value: &str, min: i32, max: i32) -> Option<i32> {
    value.parse::<i32>().ok().filter(|&v| v >= min && v <= max)
}

/// parses a float within an inclusive range.
/// returns none if parsing fails or value is out of range.
pub fn parse_f32_range(value: &str, min: f32, max: f32) -> Option<f32> {
    value.parse::<f32>().ok().filter(|&v| v >= min && v <= max)
}

/// parses a positive float (greater than 0).
/// returns none if parsing fails or value is not positive.
pub fn parse_positive_f32(value: &str) -> Option<f32> {
    value.parse::<f32>().ok().filter(|&v| v > 0.0)
}

/// normalizes a hue value to 0-359 range using modulo arithmetic.
pub fn normalize_hue(value: i32) -> i32 {
    value.rem_euclid(360)
}

/// parses a boolean flag from various string representations.
/// accepts: "1", "true", "yes", "on" (case-insensitive)
pub fn parse_flag(value: &str) -> bool {
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}
