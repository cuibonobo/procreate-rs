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
    Add = 3,        // Linear Dodge
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
    pub fn from_i64(v: i64) -> Self {
        match v {
            0  => Self::Normal,
            1  => Self::Multiply,
            2  => Self::Screen,
            3  => Self::Add,
            4  => Self::Overlay,
            5  => Self::SoftLight,
            6  => Self::HardLight,
            7  => Self::ColorDodge,
            8  => Self::ColorBurn,
            9  => Self::Darken,
            10 => Self::Lighten,
            11 => Self::Difference,
            12 => Self::Exclusion,
            13 => Self::Hue,
            14 => Self::Saturation,
            15 => Self::Color,
            16 => Self::Luminosity,
            n  => Self::Unknown(n),
        }
    }

    /// CSS mix-blend-mode string for web rendering.
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::Normal     => "normal",
            Self::Multiply   => "multiply",
            Self::Screen     => "screen",
            Self::Add        => "screen",   // closest CSS approximation
            Self::Overlay    => "overlay",
            Self::SoftLight  => "soft-light",
            Self::HardLight  => "hard-light",
            Self::ColorDodge => "color-dodge",
            Self::ColorBurn  => "color-burn",
            Self::Darken     => "darken",
            Self::Lighten    => "lighten",
            Self::Difference => "difference",
            Self::Exclusion  => "exclusion",
            Self::Hue        => "hue",
            Self::Saturation => "saturation",
            Self::Color      => "color",
            Self::Luminosity => "luminosity",
            Self::Unknown(_) => "normal",
        }
    }
}
