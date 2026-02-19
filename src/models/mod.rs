/// gravity determines where the "focus" is when cropping.
///
/// for example, with `Gravity::North` and a crop to smaller dimensions,
/// we keep the top portion of the image and crop from the bottom.
///
/// compass names (north, south, etc.) are from image processing conventions.
/// also accepts "top", "bottom", "left", "right" in the parser for clarity.
#[derive(Debug, Clone, Copy, Default)]
pub enum Gravity {
    #[default]
    Center,
    North,
    South,
    East,
    West,
}

/// crop mode determines how the image is resized to target dimensions.
///
/// - `Fill`: cover the target box, crop excess (default, good for thumbnails)
/// - `Fit`: contain within the box, no cropping (good for showing full image)
/// - `Scale`: stretch to fit (distorts aspect ratio, rarely looks good)
/// - `Pad`: fit within box, pad with background color
/// - `Crop`: same as Fill
#[derive(Debug, Clone, Copy, Default)]
pub enum CropMode {
    #[default]
    Fill,
    Fit,
    Scale,
    Crop,
    Pad,
}
