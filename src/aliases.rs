/// all the query params we support, mapped to typed keys.
///
/// accepting multiple aliases for the same param ("w", "width", etc.)
/// without duplicating logic everywhere.
#[derive(Debug, Clone, Copy, PartialEq)]
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
            "w" | "width" => Some(ParamKey::Width),
            "h" | "height" => Some(ParamKey::Height),
            "c" | "crop" => Some(ParamKey::Crop),
            "g" | "gravity" | "pos" => Some(ParamKey::Gravity),
            "ar" | "aspect" | "ratio" => Some(ParamKey::AspectRatio),
            "r" | "rotate" | "rot" => Some(ParamKey::Rotate),
            "b" | "bg" | "background" => Some(ParamKey::Background),
            "blur" | "br" | "bl" => Some(ParamKey::Blur),
            "webp_q" | "webp_quality" | "wq" | "quality" => Some(ParamKey::WebpQuality),
            "webp_lossless" | "wl" | "lossless" => Some(ParamKey::WebpLossless),
            "brightness" | "bright" | "bri" | "brgt" => Some(ParamKey::Brightness),
            "contrast" | "con" | "cnt" => Some(ParamKey::Contrast),
            "hue" | "huerotate" | "hue_rotate" | "hr" => Some(ParamKey::HueRotate),
            "grayscale" | "gray" | "grey" | "gs" => Some(ParamKey::Grayscale),
            "invert" | "inv" | "negative" | "neg" => Some(ParamKey::Invert),
            "saturation" | "sat" | "s" => Some(ParamKey::Saturation),
            "vibrance" | "vib" | "v" => Some(ParamKey::Vibrance),
            "grain" | "gr" | "noise" | "n" => Some(ParamKey::Grain),
            "graingray" | "grgs" | "graygrain" | "grg" => Some(ParamKey::GrainGrayscale),
            "grainthresh" | "grth" | "grain_threshold" => Some(ParamKey::GrainThreshold),
            "ca" | "chromatic" | "aberration" | "chromatic_aberration" => {
                Some(ParamKey::ChromaticAberration)
            }
            "debug" | "dbg" => Some(ParamKey::Debug),
            _ => None,
        }
    }
}
