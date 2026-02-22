use crate::models::{CropMode, Gravity};

/// normalized transformation parameters from query string parsing.
///
/// design: `Option<T>` for all optional params to distinguish "not specified"
/// from "specified as default". enables optimization decisions (e.g., skip
/// expensive operations when not needed).
///
/// defaults:
/// - `crop_mode`: `Fill` (cover dimensions, crop excess)
/// - `gravity`: `Center` (crop from center)
/// - `grain_threshold`: 0.08 (skip grain on near-black pixels)
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Query flags map directly to independent transform toggles and keep parsing/cache signatures explicit."
)]
pub struct Params {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub crop_mode: CropMode,
    pub gravity: Gravity,
    pub aspect: Option<String>,
    pub rotate: Option<i32>,
    pub background: Option<[u8; 4]>,
    pub blur: Option<f32>,
    pub webp_quality: Option<u32>,
    pub webp_lossless: bool,
    pub brightness: Option<i32>,
    pub contrast: Option<f32>,
    pub hue_rotate: Option<i32>,
    pub grayscale: bool,
    pub invert: bool,
    pub saturation: Option<f32>,
    pub vibrance: Option<f32>,
    pub grain: Option<f32>,
    pub grain_grayscale: Option<f32>,
    pub grain_threshold: f32,
    pub chromatic_aberration: Option<f32>,
    pub debug: bool,
}

impl Params {
    pub const DEFAULT_GRAIN_THRESHOLD: f32 = 0.08;

    /// checks if any actual transforms are requested.
    ///
    /// used to skip the heavy processing pipeline when the user just wants
    /// a format conversion (e.g., jpg -> webp with no other changes).
    ///
    /// note: `webp_quality` and `webp_lossless` are NOT included here —
    /// they're encoding options, not transforms.
    pub const fn has_transforms(&self) -> bool {
        self.width.is_some()
            || self.height.is_some()
            || self.aspect.is_some()
            || self.rotate.is_some()
            || self.blur.is_some()
            || self.brightness.is_some()
            || self.contrast.is_some()
            || self.hue_rotate.is_some()
            || self.grayscale
            || self.invert
            || self.saturation.is_some()
            || self.vibrance.is_some()
            || self.grain.is_some()
            || self.grain_grayscale.is_some()
            || self.chromatic_aberration.is_some()
            || self.debug
    }

    /// checks if pixel-level operations are needed (blur, color filters, effects).
    ///
    /// geometric transforms (resize, rotate, crop) operate on coordinates
    /// and don't need rgba conversion. pixel ops work on individual pixels.
    pub const fn has_pixel_ops(&self) -> bool {
        self.blur.is_some()
            || self.brightness.is_some()
            || self.contrast.is_some()
            || self.hue_rotate.is_some()
            || self.saturation.is_some()
            || self.vibrance.is_some()
            || self.grayscale
            || self.invert
            || self.grain.is_some()
            || self.grain_grayscale.is_some()
            || self.chromatic_aberration.is_some()
    }

    /// stable cache signature used for transform result cache keys.
    ///
    /// this keeps cache keys deterministic regardless of query param order
    /// or alias usage (e.g. `w` vs `width`).
    pub fn cache_signature(&self) -> String {
        format!(
            "fmt=webp|w={}|h={}|crop={}|g={}|ar={}|rot={}|bg={}|blur={}|wq={}|wl={}|bri={}|con={}|hue={}|gs={}|inv={}|sat={}|vib={}|gr={}|grg={}|grth={:.4}|ca={}|dbg={}",
            opt_u32(self.width),
            opt_u32(self.height),
            crop_mode_code(self.crop_mode),
            gravity_code(self.gravity),
            opt_str(self.aspect.as_deref()),
            opt_i32(self.rotate),
            opt_color(self.background),
            opt_f32(self.blur),
            opt_u32(self.webp_quality),
            bool_code(self.webp_lossless),
            opt_i32(self.brightness),
            opt_f32(self.contrast),
            opt_i32(self.hue_rotate),
            bool_code(self.grayscale),
            bool_code(self.invert),
            opt_f32(self.saturation),
            opt_f32(self.vibrance),
            opt_f32(self.grain),
            opt_f32(self.grain_grayscale),
            self.grain_threshold,
            opt_f32(self.chromatic_aberration),
            bool_code(self.debug),
        )
    }
}

fn opt_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "-".to_string(), |v| v.to_string())
}

fn opt_i32(value: Option<i32>) -> String {
    value.map_or_else(|| "-".to_string(), |v| v.to_string())
}

fn opt_f32(value: Option<f32>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.4}"))
}

fn opt_str(value: Option<&str>) -> String {
    value.map_or_else(|| "-".to_string(), ToString::to_string)
}

const fn bool_code(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn opt_color(value: Option<[u8; 4]>) -> String {
    value.map_or_else(
        || "-".to_string(),
        |[r, g, b, a]| format!("{r:02x}{g:02x}{b:02x}{a:02x}"),
    )
}

const fn gravity_code(gravity: Gravity) -> &'static str {
    match gravity {
        Gravity::Center => "c",
        Gravity::North => "n",
        Gravity::South => "s",
        Gravity::East => "e",
        Gravity::West => "w",
    }
}

const fn crop_mode_code(crop_mode: CropMode) -> &'static str {
    match crop_mode {
        CropMode::Fill => "fill",
        CropMode::Fit => "fit",
        CropMode::Scale => "scale",
        CropMode::Crop => "crop",
        CropMode::Pad => "pad",
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            crop_mode: CropMode::default(),
            gravity: Gravity::default(),
            aspect: None,
            rotate: None,
            background: None,
            blur: None,
            webp_quality: None,
            webp_lossless: false,
            brightness: None,
            contrast: None,
            hue_rotate: None,
            grayscale: false,
            invert: false,
            saturation: None,
            vibrance: None,
            grain: None,
            grain_grayscale: None,
            grain_threshold: Self::DEFAULT_GRAIN_THRESHOLD,
            chromatic_aberration: None,
            debug: false,
        }
    }
}
