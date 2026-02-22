/// calculates perceptual luminance using rec. 601 coefficients.
///
/// weights: 0.299*r + 0.587*g + 0.114*b
///
/// this matches how human eyes actually perceive brightness
/// (green contributes most, blue least).
#[allow(
    clippy::missing_const_for_fn,
    reason = "`From` trait is not yet stable in const context (see rust-lang/rust#143874)."
)]
pub fn luminance(red: u8, green: u8, blue: u8) -> u32 {
    ((u32::from(red) * 299) + (u32::from(green) * 587) + (u32::from(blue) * 114)) / 1000
}

/// converts rgb to hsv color space.
///
/// returns (hue: 0-360, saturation: 0-1, value: 0-1)
pub fn rgb_to_hsv(red: u8, green: u8, blue: u8) -> (f32, f32, f32) {
    let red = f32::from(red) / 255.0;
    let green = f32::from(green) / 255.0;
    let blue = f32::from(blue) / 255.0;

    let max_channel = red.max(green).max(blue);
    let min_channel = red.min(green).min(blue);
    let delta = max_channel - min_channel;
    let epsilon = f32::EPSILON;

    let hue = if delta <= epsilon {
        0.0
    } else if (max_channel - red).abs() <= epsilon {
        60.0 * (((green - blue) / delta) % 6.0)
    } else if (max_channel - green).abs() <= epsilon {
        60.0 * (((blue - red) / delta) + 2.0)
    } else {
        60.0 * (((red - green) / delta) + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };

    let saturation = if max_channel <= epsilon {
        0.0
    } else {
        delta / max_channel
    };
    let value = max_channel;

    (hue, saturation, value)
}

/// converts hsv back to rgb.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "HSV conversion emits clamped `[0,255]` channel values before narrowing to `u8`."
)]
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let chroma = value * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let match_value = value - chroma;

    let (red, green, blue) = if hue < 60.0 {
        (chroma, second, 0.0)
    } else if hue < 120.0 {
        (second, chroma, 0.0)
    } else if hue < 180.0 {
        (0.0, chroma, second)
    } else if hue < 240.0 {
        (0.0, second, chroma)
    } else if hue < 300.0 {
        (second, 0.0, chroma)
    } else {
        (chroma, 0.0, second)
    };

    (
        ((red + match_value) * 255.0).clamp(0.0, 255.0) as u8,
        ((green + match_value) * 255.0).clamp(0.0, 255.0) as u8,
        ((blue + match_value) * 255.0).clamp(0.0, 255.0) as u8,
    )
}
