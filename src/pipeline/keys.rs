/// all the query params we support, mapped to typed keys.
///
/// accepting multiple aliases for the same param ("w", "width", etc.)
/// without duplicating logic everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKey {
    Width,
    Height,
    Crop,
    Gravity,
    AspectRatio,
    Rotate,
    Background,
    Blur,
    WebpQuality,
    WebpLossless,
    Format,
    Brightness,
    Contrast,
    HueRotate,
    Grayscale,
    Invert,
    Saturation,
    Vibrance,
    Grain,
    GrainGrayscale,
    GrainThreshold,
    ChromaticAberration,
    Debug,
}

impl ParamKey {
    /// maps url param keys to our internal enum.
    ///
    /// accepts a bunch of aliases for flexibility. returns none if
    /// the key isn't recognized — parser will skip it.
    pub fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "w" | "width" => Some(Self::Width),
            "h" | "height" => Some(Self::Height),
            "c" | "crop" => Some(Self::Crop),
            "g" | "gravity" | "pos" => Some(Self::Gravity),
            "ar" | "aspect" | "ratio" => Some(Self::AspectRatio),
            "r" | "rotate" | "rot" => Some(Self::Rotate),
            "b" | "bg" | "background" => Some(Self::Background),
            "blur" | "br" | "bl" => Some(Self::Blur),
            "webp_q" | "webp_quality" | "wq" | "quality" => Some(Self::WebpQuality),
            "webp_lossless" | "wl" | "lossless" => Some(Self::WebpLossless),
            "format" | "f" => Some(Self::Format),
            "brightness" | "bright" | "bri" | "brgt" => Some(Self::Brightness),
            "contrast" | "con" | "cnt" => Some(Self::Contrast),
            "hue" | "huerotate" | "hue_rotate" | "hr" => Some(Self::HueRotate),
            "grayscale" | "gray" | "grey" | "gs" => Some(Self::Grayscale),
            "invert" | "inv" | "negative" | "neg" => Some(Self::Invert),
            "saturation" | "sat" | "s" => Some(Self::Saturation),
            "vibrance" | "vib" | "v" => Some(Self::Vibrance),
            "grain" | "gr" | "noise" | "n" => Some(Self::Grain),
            "graingray" | "grgs" | "graygrain" | "grg" => Some(Self::GrainGrayscale),
            "grainthresh" | "grth" | "grain_threshold" => Some(Self::GrainThreshold),
            "ca" | "chromatic" | "aberration" | "chromatic_aberration" => {
                Some(Self::ChromaticAberration)
            }
            "debug" | "dbg" => Some(Self::Debug),
            _ => None,
        }
    }
}
