//! Import a `manifest.json + PNGs` folder into a `.procreate` file.
//!
//! This is the round-trip counterpart of `export::export_layers`. Almost all
//! manifest fields are optional; sensible defaults are applied automatically.
//! The only required fields are `canvas_width`, `canvas_height`, and each
//! layer's `file` path.

use std::path::Path;

use serde::Deserialize;

use crate::builder::{LayerConfig, ProcreateDocumentBuilder};
use crate::layer::BlendMode;
use crate::Result;

// ── Default value helpers for serde ─────────────────────────────────────────

fn default_name() -> String {
    "Untitled".to_string()
}
fn default_dpi() -> f64 {
    132.0
}
fn default_color_profile() -> String {
    "sRGB IEC61966-2.1".to_string()
}
fn default_background_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
fn default_opacity() -> f64 {
    1.0
}
fn default_visible() -> bool {
    true
}
fn default_blend_mode() -> String {
    "Normal".to_string()
}

// ── Manifest structs ─────────────────────────────────────────────────────────

/// Deserialised `manifest.json`.
///
/// Only `canvas_width`, `canvas_height`, and `layers[].file` are required;
/// every other field has a sensible default.
#[derive(Deserialize)]
pub struct ImportManifest {
    pub canvas_width: u32,
    pub canvas_height: u32,

    #[serde(default = "default_name")]
    pub name: String,

    #[serde(default = "default_dpi")]
    pub dpi: f64,

    #[serde(default = "default_color_profile")]
    pub color_profile: String,

    #[serde(default = "default_background_color")]
    pub background_color: [f32; 4],

    #[serde(default)]
    pub background_hidden: bool,

    #[serde(default)]
    pub animation: Option<serde_json::Value>, // parsed but not currently written

    pub layers: Vec<ImportLayer>,
}

/// A single layer entry inside `manifest.json`.
#[derive(Deserialize)]
pub struct ImportLayer {
    /// Path to the PNG, relative to the manifest file's directory.
    pub file: String,

    /// Display name. Defaults to the filename stem of `file`.
    pub name: Option<String>,

    /// UUID to preserve. Auto-generated if absent.
    pub uuid: Option<String>,

    #[serde(default = "default_opacity")]
    pub opacity: f64,

    #[serde(default = "default_visible")]
    pub visible: bool,

    #[serde(default)]
    pub locked: bool,

    #[serde(default)]
    pub preserve_alpha: bool,

    #[serde(default)]
    pub clipped: bool,

    #[serde(default = "default_blend_mode")]
    pub blend_mode: String,

    #[serde(default)]
    pub layer_type: i64,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Import a `manifest.json` (and its sibling PNGs) into a `.procreate` file.
///
/// `manifest_path` — path to the `manifest.json` file.
/// `output_path`   — destination `.procreate` path to write.
pub fn import_from_manifest<P, Q>(manifest_path: P, output_path: Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let manifest_path = manifest_path.as_ref();
    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));

    let json = std::fs::read_to_string(manifest_path)?;
    let manifest: ImportManifest = serde_json::from_str(&json)?;

    let mut builder = ProcreateDocumentBuilder::new(manifest.canvas_width, manifest.canvas_height)
        .name(manifest.name)
        .dpi(manifest.dpi)
        .color_profile(manifest.color_profile)
        .background_color(manifest.background_color)
        .background_hidden(manifest.background_hidden);

    for layer in manifest.layers {
        let img_path = manifest_dir.join(&layer.file);
        println!("  Loading: {}", img_path.display());
        let img = image::open(&img_path)?;

        let name = layer.name.unwrap_or_else(|| {
            Path::new(&layer.file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Layer")
                .to_string()
        });

        let config = LayerConfig {
            name,
            uuid: layer.uuid,
            opacity: layer.opacity,
            visible: layer.visible,
            locked: layer.locked,
            preserve_alpha: layer.preserve_alpha,
            clipped: layer.clipped,
            blend_mode: BlendMode::from_name(&layer.blend_mode),
            layer_type: layer.layer_type,
        };

        builder = builder.add_layer(img, config);
    }

    builder.build(output_path)
}
