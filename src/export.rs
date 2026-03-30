//! Export utilities: write per-layer PNGs and a JSON manifest.

use crate::{ProcreateDocument, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Only export visible layers.
    pub visible_only: bool,
    /// Skip composite/group and mask layers (type != 0).
    pub skip_special_layers: bool,
    /// Un-premultiply alpha before saving (recommended: true).
    pub unpremultiply: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            visible_only: false,
            skip_special_layers: true,
            unpremultiply: true, // tile.rs handles this; flag is informational
        }
    }
}

/// JSON manifest written alongside the exported PNGs.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportManifest {
    pub name: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub dpi: f64,
    pub color_profile: String,
    pub background_color: [f32; 4],
    pub background_hidden: bool,
    pub animation: Option<crate::document::AnimationSettings>,
    pub layers: Vec<ExportedLayer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedLayer {
    pub uuid: String,
    pub name: String,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub preserve_alpha: bool,
    pub clipped: bool,
    pub blend_mode: String,
    pub layer_type: i64,
    /// Relative path to the exported PNG (from the manifest's directory).
    pub file: String,
}

/// Export all layers of a .procreate file to a directory.
///
/// Writes:
///   - `{output_dir}/{layer_name}_{uuid_short}.png` per layer
///   - `{output_dir}/manifest.json` with full metadata
///
/// Returns the path to the manifest file.
pub fn export_layers<P: AsRef<Path>, Q: AsRef<Path>>(
    procreate_path: P,
    output_dir: Q,
    options: &ExportOptions,
) -> Result<PathBuf> {
    let output_dir = output_dir.as_ref();
    fs::create_dir_all(output_dir)?;

    let doc = ProcreateDocument::from_path(&procreate_path)?;
    let mut exported_layers = Vec::new();

    for layer in &doc.layers {
        // Apply filters
        if options.visible_only && !layer.visible {
            continue;
        }
        if options.skip_special_layers && layer.layer_type != 0 {
            continue;
        }

        // Rasterize
        let img = doc.rasterize_layer(&procreate_path, &layer.uuid)?;

        // Build a safe filename: sanitize the layer name + short UUID
        let safe_name = layer
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let uuid_short = &layer.uuid[..8];
        let filename = format!("{}_{}.png", safe_name, uuid_short);
        let file_path = output_dir.join(&filename);

        img.save(&file_path)?;
        println!("  Exported: {}", filename);

        exported_layers.push(ExportedLayer {
            uuid: layer.uuid.clone(),
            name: layer.name.clone(),
            opacity: layer.opacity,
            visible: layer.visible,
            locked: layer.locked,
            preserve_alpha: layer.preserve_alpha,
            clipped: layer.clipped,
            blend_mode: format!("{:?}", layer.blend_mode),
            layer_type: layer.layer_type,
            file: filename,
        });
    }

    // Write manifest
    let manifest = ExportManifest {
        name: doc.name.clone(),
        canvas_width: doc.canvas_width,
        canvas_height: doc.canvas_height,
        dpi: doc.dpi,
        color_profile: doc.color_profile.clone(),
        background_color: doc.background_color,
        background_hidden: doc.background_hidden,
        animation: doc.animation.clone(),
        layers: exported_layers,
    };

    let manifest_path = output_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| crate::ProcreateError::InvalidDocument(e.to_string()))?;
    fs::write(&manifest_path, json)?;

    // Extract the QuickLook thumbnail from the archive
    let zip_file = fs::File::open(&procreate_path)?;
    let mut zip = ZipArchive::new(zip_file)?;
    if let Ok(mut entry) = zip.by_name("QuickLook/Thumbnail.png") {
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        drop(entry);
        fs::write(output_dir.join("thumbnail.png"), &bytes)?;
        println!("  Exported: thumbnail.png");
    }

    Ok(manifest_path)
}
