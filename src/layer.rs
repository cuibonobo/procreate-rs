//! Layer metadata parsed from Document.archive.

use serde::{Deserialize, Serialize};

/// A single Procreate layer with all its metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    /// Procreate's UUID for this layer (also the folder name in the ZIP).
    pub uuid: String,
    /// Display name set by the user.
    pub name: String,
    /// Opacity 0.0–1.0.
    pub opacity: f64,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Whether the layer is locked.
    pub locked: bool,
    /// Whether alpha is preserved (lock transparency).
    pub preserve_alpha: bool,
    /// Whether the layer is clipped to the one below.
    pub clipped: bool,
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// Canvas width in pixels (same as document for non-transformed layers).
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Bounding rect of actual content within the canvas [x, y, w, h].
    /// Useful for skipping empty tiles.
    pub contents_rect: Option<[f64; 4]>,
    /// 4×4 transform matrix (row-major). Identity for most layers.
    pub transform: Option<[f64; 16]>,
    /// Layer type: 0 = normal, 1 = composite/group, 2 = mask.
    pub layer_type: i64,
}

/// Procreate blend modes, mapped from their integer codes.
///
/// These correspond to standard Photoshop/CSS blend modes.
/// Values confirmed from community reverse engineering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum BlendMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Add = 3, // Linear Dodge
    Overlay = 4,
    SoftLight = 5,
    HardLight = 6,
    ColorDodge = 7,
    ColorBurn = 8,
    Darken = 9,
    Lighten = 10,
    Difference = 11,
    Exclusion = 12,
    Hue = 13,
    Saturation = 14,
    Color = 15,
    Luminosity = 16,
    Unknown(i64),
}

impl BlendMode {
    pub fn to_i64(self) -> i64 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
            Self::Add => 3,
            Self::Overlay => 4,
            Self::SoftLight => 5,
            Self::HardLight => 6,
            Self::ColorDodge => 7,
            Self::ColorBurn => 8,
            Self::Darken => 9,
            Self::Lighten => 10,
            Self::Difference => 11,
            Self::Exclusion => 12,
            Self::Hue => 13,
            Self::Saturation => 14,
            Self::Color => 15,
            Self::Luminosity => 16,
            Self::Unknown(n) => n,
        }
    }

    /// Parse a blend mode from its debug-format name (as written by the JSON exporter).
    pub fn from_name(s: &str) -> Self {
        match s {
            "Normal" => Self::Normal,
            "Multiply" => Self::Multiply,
            "Screen" => Self::Screen,
            "Add" => Self::Add,
            "Overlay" => Self::Overlay,
            "SoftLight" => Self::SoftLight,
            "HardLight" => Self::HardLight,
            "ColorDodge" => Self::ColorDodge,
            "ColorBurn" => Self::ColorBurn,
            "Darken" => Self::Darken,
            "Lighten" => Self::Lighten,
            "Difference" => Self::Difference,
            "Exclusion" => Self::Exclusion,
            "Hue" => Self::Hue,
            "Saturation" => Self::Saturation,
            "Color" => Self::Color,
            "Luminosity" => Self::Luminosity,
            _ => Self::Normal,
        }
    }

    pub fn from_i64(v: i64) -> Self {
        match v {
            0 => Self::Normal,
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Add,
            4 => Self::Overlay,
            5 => Self::SoftLight,
            6 => Self::HardLight,
            7 => Self::ColorDodge,
            8 => Self::ColorBurn,
            9 => Self::Darken,
            10 => Self::Lighten,
            11 => Self::Difference,
            12 => Self::Exclusion,
            13 => Self::Hue,
            14 => Self::Saturation,
            15 => Self::Color,
            16 => Self::Luminosity,
            n => Self::Unknown(n),
        }
    }

    /// CSS mix-blend-mode string for web rendering.
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Add => "screen", // closest CSS approximation
            Self::Overlay => "overlay",
            Self::SoftLight => "soft-light",
            Self::HardLight => "hard-light",
            Self::ColorDodge => "color-dodge",
            Self::ColorBurn => "color-burn",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
            Self::Unknown(_) => "normal",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_mode_from_i64_known_values() {
        assert_eq!(BlendMode::from_i64(0), BlendMode::Normal);
        assert_eq!(BlendMode::from_i64(1), BlendMode::Multiply);
        assert_eq!(BlendMode::from_i64(2), BlendMode::Screen);
        assert_eq!(BlendMode::from_i64(3), BlendMode::Add);
        assert_eq!(BlendMode::from_i64(16), BlendMode::Luminosity);
    }

    #[test]
    fn blend_mode_from_i64_unknown() {
        assert_eq!(BlendMode::from_i64(99), BlendMode::Unknown(99));
        assert_eq!(BlendMode::from_i64(-1), BlendMode::Unknown(-1));
    }

    #[test]
    fn blend_mode_to_css() {
        assert_eq!(BlendMode::Normal.to_css(), "normal");
        assert_eq!(BlendMode::Multiply.to_css(), "multiply");
        assert_eq!(BlendMode::SoftLight.to_css(), "soft-light");
        assert_eq!(BlendMode::ColorDodge.to_css(), "color-dodge");
        assert_eq!(BlendMode::Unknown(42).to_css(), "normal");
    }
}
