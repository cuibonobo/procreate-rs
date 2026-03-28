//! Top-level Procreate document parser.
//!
//! Opens a .procreate ZIP, parses Document.archive, and exposes
//! the layer list and canvas metadata.

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;
use zip::ZipArchive;

use crate::archive::Archive;
use crate::layer::{BlendMode, Layer};
use crate::tile;
use crate::{ProcreateError, Result};

/// Animation settings from the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationSettings {
    pub frame_rate: i64,
    pub onion_skin_count: i64,
    pub onion_skin_opacity: f64,
    /// 0 = loop, 1 = ping-pong, 2 = one-shot (community-confirmed values)
    pub playback_mode: i64,
}

/// The parsed Procreate document.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProcreateDocument {
    pub name: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub dpi: f64,
    pub color_profile: String,
    pub background_color: [f32; 4], // RGBA 0.0–1.0
    pub background_hidden: bool,
    pub layers: Vec<Layer>,
    pub animation: Option<AnimationSettings>,
    /// Total stroke count (useful for progress tracking)
    pub stroke_count: i64,
}

impl ProcreateDocument {
    /// Parse a .procreate file from a filesystem path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        Self::from_reader(file)
    }

    /// Parse a .procreate file from any reader.
    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Self> {
        let mut zip = ZipArchive::new(reader)?;

        // Read Document.archive
        let archive_bytes = {
            let mut entry = zip
                .by_name("Document.archive")
                .map_err(|_| ProcreateError::InvalidDocument("missing Document.archive".into()))?;
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            buf
        };

        let archive = Archive::from_bytes(&archive_bytes)?;
        let root = archive
            .root()
            .ok_or_else(|| ProcreateError::InvalidDocument("empty archive".into()))?;

        // Canvas size
        let size_str = archive
            .get_string(root, "size")
            .ok_or_else(|| ProcreateError::MissingField("size".into()))?;
        let (canvas_width, canvas_height) = Archive::parse_size(size_str).ok_or_else(|| {
            ProcreateError::InvalidDocument(format!("invalid size string: {}", size_str))
        })?;

        // Document name
        let name = archive
            .get_string(root, "name")
            .unwrap_or("Untitled")
            .to_string();

        // DPI
        let dpi = archive
            .get_f64(root, "SilicaDocumentArchiveDPIKey")
            .unwrap_or(72.0);

        // Color profile name
        let color_profile = archive
            .get_optional(root, "colorProfile")
            .and_then(|cp| archive.get_string(cp, "SiColorProfileArchiveICCNameKey"))
            .unwrap_or("sRGB IEC61966-2.1")
            .to_string();

        // Background color (stored as 4×f32 LE bytes)
        let background_color = archive
            .get_optional(root, "backgroundColor")
            .and_then(|v| v.as_data())
            .and_then(Archive::decode_color_f32)
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);

        let background_hidden = archive.get_bool(root, "backgroundHidden").unwrap_or(false);

        // Stroke count
        let stroke_count = archive.get_i64(root, "strokeCount").unwrap_or(0);

        // Animation settings
        let animation = archive
            .get_optional(root, "animation")
            .map(|anim| AnimationSettings {
                frame_rate: archive.get_i64(anim, "frameRate").unwrap_or(12),
                onion_skin_count: archive.get_i64(anim, "onionSkinCount").unwrap_or(0),
                onion_skin_opacity: archive.get_f64(anim, "onionSkinOpacity").unwrap_or(0.5),
                playback_mode: archive.get_i64(anim, "playbackMode").unwrap_or(0),
            });

        // Layer list - stored in "unwrappedLayers" (flat) and "layers" (display order)
        // We use "layers" for display order which matches what Procreate shows
        let layers_obj = archive
            .get_optional(root, "layers")
            .ok_or_else(|| ProcreateError::MissingField("layers".into()))?;

        let layer_values = archive
            .get_array(layers_obj)
            .ok_or_else(|| ProcreateError::InvalidDocument("layers is not an array".into()))?;

        let layers = layer_values
            .iter()
            .map(|lv| parse_layer(&archive, lv))
            .collect::<Result<Vec<_>>>()?;

        Ok(ProcreateDocument {
            name,
            canvas_width,
            canvas_height,
            dpi,
            color_profile,
            background_color,
            background_hidden,
            layers,
            animation,
            stroke_count,
        })
    }

    /// Rasterize a single layer by UUID into an RGBA image.
    ///
    /// The returned image has the same dimensions as the canvas.
    pub fn rasterize_layer<P: AsRef<Path>>(
        &self,
        procreate_path: P,
        uuid: &str,
    ) -> Result<RgbaImage> {
        let file = File::open(procreate_path)?;
        let mut zip = ZipArchive::new(file)?;
        tile::stitch_layer(&mut zip, uuid, self.canvas_width, self.canvas_height)
    }

    /// Rasterize all layers, returning (layer_metadata, image) pairs
    /// in bottom-to-top order (index 0 = bottom).
    pub fn rasterize_all<P: AsRef<Path>>(
        &self,
        procreate_path: P,
    ) -> Result<Vec<(&Layer, RgbaImage)>> {
        let file = File::open(&procreate_path)?;
        let mut zip = ZipArchive::new(file)?;

        self.layers
            .iter()
            .map(|layer| {
                let img = tile::stitch_layer(
                    &mut zip,
                    &layer.uuid,
                    self.canvas_width,
                    self.canvas_height,
                )?;
                Ok((layer, img))
            })
            .collect()
    }

    /// Find a layer by name (case-insensitive, returns first match).
    pub fn layer_by_name(&self, name: &str) -> Option<&Layer> {
        let lower = name.to_lowercase();
        self.layers.iter().find(|l| l.name.to_lowercase() == lower)
    }

    /// Find a layer by UUID.
    pub fn layer_by_uuid(&self, uuid: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.uuid == uuid)
    }
}

fn parse_layer(archive: &Archive, obj: &plist::Value) -> Result<Layer> {
    let uuid = archive
        .get_string(obj, "UUID")
        .ok_or_else(|| ProcreateError::MissingField("layer UUID".into()))?
        .to_string();

    let name = archive
        .get_string(obj, "name")
        .unwrap_or("Layer")
        .to_string();

    let opacity = archive.get_f64(obj, "opacity").unwrap_or(1.0);
    let visible = !archive.get_bool(obj, "hidden").unwrap_or(false);
    let locked = archive.get_bool(obj, "locked").unwrap_or(false);
    let preserve_alpha = archive.get_bool(obj, "preserve").unwrap_or(false);
    let clipped = archive.get_bool(obj, "clipped").unwrap_or(false);
    let layer_type = archive.get_i64(obj, "type").unwrap_or(0);

    let blend_mode = BlendMode::from_i64(archive.get_i64(obj, "blend").unwrap_or(0));

    let width = archive
        .get_f64(obj, "sizeWidth")
        .map(|v| v as u32)
        .unwrap_or(0);
    let height = archive
        .get_f64(obj, "sizeHeight")
        .map(|v| v as u32)
        .unwrap_or(0);

    // contentsRect: optional 32-byte blob → [x, y, w, h]
    let contents_rect = obj
        .as_dictionary()
        .and_then(|d| d.get("contentsRect"))
        .and_then(|v| {
            if v.as_uid().is_some() {
                archive.resolve(v)
            } else {
                Some(v)
            }
        })
        .and_then(|v| v.as_data())
        .and_then(Archive::decode_rect);

    // transform: optional 128-byte blob → [f64; 16]
    let transform = obj
        .as_dictionary()
        .and_then(|d| d.get("transform"))
        .and_then(|v| {
            if v.as_uid().is_some() {
                archive.resolve(v)
            } else {
                Some(v)
            }
        })
        .and_then(|v| v.as_data())
        .and_then(Archive::decode_transform);

    Ok(Layer {
        uuid,
        name,
        opacity,
        visible,
        locked,
        preserve_alpha,
        clipped,
        blend_mode,
        width,
        height,
        contents_rect,
        transform,
        layer_type,
    })
}
