use crate::models::{CropMode, Gravity};

/// holds all the transformation params from the url query string.
///
/// everything is optional except crop_mode and gravity (which have sensible defaults).
/// using `Option<T>` everywhere lets us distinguish between "not specified"
/// vs "specified as default value" — useful for deciding whether to skip
/// expensive operations.
#[derive(Debug)]
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
    /// note: webp_quality and webp_lossless are NOT included here —
    /// they're encoding options, not transforms.
    pub fn has_transforms(&self) -> bool {
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
    pub fn has_pixel_ops(&self) -> bool {
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
