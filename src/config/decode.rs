//! image decode limits and validation.
//!
//! prevents OOM attacks by enforcing maximum dimensions and allocation limits.
//! limits are configurable via environment variables with sensible defaults.

use crate::config::env::{parse_u64, parse_usize};
use std::sync::OnceLock;

const DEFAULT_MAX_DECODE_PIXELS: u64 = 128_000_000;
const DEFAULT_MAX_DECODE_DIMENSION: u32 = 16_384;
const DEFAULT_MAX_DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;

/// decode limits for image dimensions and memory allocation.
#[derive(Clone, Copy)]
pub struct DecodeLimits {
    /// maximum total pixels (width * height).
    pub pixels: u64,
    /// maximum width or height in pixels.
    pub dimension: u32,
    /// maximum bytes to allocate for decoded image.
    pub alloc_bytes: u64,
}

/// returns the configured decode limits.
///
/// lazily initialized from environment on first call.
pub fn decode_limits() -> DecodeLimits {
    static LIMITS: OnceLock<DecodeLimits> = OnceLock::new();
    *LIMITS.get_or_init(|| {
        let max_dimension = parse_usize(
            "IMAGIK_MAX_DECODE_DIMENSION",
            usize::try_from(DEFAULT_MAX_DECODE_DIMENSION).unwrap_or(usize::MAX),
            1,
        );
        let dimension = u32::try_from(max_dimension).unwrap_or(u32::MAX);

        DecodeLimits {
            pixels: parse_u64("IMAGIK_MAX_DECODE_PIXELS", DEFAULT_MAX_DECODE_PIXELS, 1),
            dimension,
            alloc_bytes: parse_u64(
                "IMAGIK_MAX_DECODE_ALLOC_BYTES",
                DEFAULT_MAX_DECODE_ALLOC_BYTES,
                1024,
            ),
        }
    })
}

/// validates image dimensions against configured limits.
///
/// returns error if dimensions exceed limits or image would require
/// more memory than allowed.
pub fn validate_decode_bounds(width: u32, height: u32, limits: DecodeLimits) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("image has invalid dimensions".to_string());
    }
    if width > limits.dimension || height > limits.dimension {
        return Err(format!(
            "image dimensions exceed configured limit ({}x{})",
            limits.dimension, limits.dimension
        ));
    }

    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.pixels {
        return Err(format!(
            "image pixel count exceeds configured limit ({})",
            limits.pixels
        ));
    }

    let alloc_bytes = pixels.saturating_mul(4);
    if alloc_bytes > limits.alloc_bytes {
        return Err(format!(
            "image decode allocation exceeds configured limit ({} bytes)",
            limits.alloc_bytes
        ));
    }

    Ok(())
}
