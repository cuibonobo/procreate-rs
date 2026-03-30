//! High-level builder for creating `.procreate` files programmatically.
//!
//! # Example
//!
//! ```no_run
//! use procreate::{ProcreateDocumentBuilder, LayerConfig};
//! use image::DynamicImage;
//!
//! let sky: DynamicImage = image::open("sky.png").unwrap();
//! let mountains: DynamicImage = image::open("mountains.png").unwrap();
//!
//! ProcreateDocumentBuilder::new(1920, 1080)
//!     .name("My Artwork")
//!     .dpi(132.0)
//!     .add_layer(sky, LayerConfig { name: "Sky".to_string(), ..Default::default() })
//!     .add_layer(mountains, LayerConfig { name: "Mountains".to_string(), opacity: 0.8, ..Default::default() })
//!     .build("MyArtwork.procreate")
//!     .unwrap();
//! ```

use std::io::{Cursor, Write};
use std::path::Path;

use image::DynamicImage;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::encode::archive_writer::{build_document_archive, DocumentSpec, LayerSpec};
use crate::encode::tile_encoder::split_into_tiles;
use crate::layer::BlendMode;
use crate::{ProcreateError, Result};

/// Per-layer configuration for `ProcreateDocumentBuilder::add_layer`.
#[derive(Debug, Clone)]
pub struct LayerConfig {
    /// Layer display name.
    pub name: String,
    /// Explicit UUID to assign. If `None`, a new UUID v4 is generated automatically.
    pub uuid: Option<String>,
    /// Opacity 0.0–1.0.
    pub opacity: f64,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Whether the layer is locked.
    pub locked: bool,
    /// Lock transparency (preserve alpha).
    pub preserve_alpha: bool,
    /// Clip to the layer below.
    pub clipped: bool,
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// Layer type: 0 = normal, 1 = group, 2 = mask.
    pub layer_type: i64,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            name: "Layer".to_string(),
            uuid: None,
            opacity: 1.0,
            visible: true,
            locked: false,
            preserve_alpha: false,
            clipped: false,
            blend_mode: BlendMode::Normal,
            layer_type: 0,
        }
    }
}

/// Builder for `.procreate` files.
///
/// Layers are added in display order (index 0 = topmost layer in Procreate's UI,
/// matching the order in the exported `manifest.json`).
pub struct ProcreateDocumentBuilder {
    name: String,
    canvas_width: u32,
    canvas_height: u32,
    dpi: f64,
    color_profile: String,
    background_color: [f32; 4],
    background_hidden: bool,
    layers: Vec<(LayerConfig, DynamicImage)>,
}

impl ProcreateDocumentBuilder {
    /// Create a new builder for a canvas of the given pixel dimensions.
    pub fn new(canvas_width: u32, canvas_height: u32) -> Self {
        Self {
            name: "Untitled".to_string(),
            canvas_width,
            canvas_height,
            dpi: 132.0,
            color_profile: "sRGB IEC61966-2.1".to_string(),
            background_color: [1.0, 1.0, 1.0, 1.0],
            background_hidden: false,
            layers: Vec::new(),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn dpi(mut self, dpi: f64) -> Self {
        self.dpi = dpi;
        self
    }

    pub fn color_profile(mut self, profile: impl Into<String>) -> Self {
        self.color_profile = profile.into();
        self
    }

    pub fn background_color(mut self, rgba: [f32; 4]) -> Self {
        self.background_color = rgba;
        self
    }

    pub fn background_hidden(mut self, hidden: bool) -> Self {
        self.background_hidden = hidden;
        self
    }

    /// Add a layer. Layers are stored and written in the order they are added
    /// (index 0 = topmost in Procreate's layer panel).
    pub fn add_layer(mut self, image: DynamicImage, config: LayerConfig) -> Self {
        self.layers.push((config, image));
        self
    }

    /// Encode and write the `.procreate` file to `output_path`.
    pub fn build<P: AsRef<Path>>(self, output_path: P) -> Result<()> {
        let bytes = self.build_to_vec()?;
        std::fs::write(output_path, bytes)?;
        Ok(())
    }

    /// Encode to an in-memory buffer. Useful for testing or piping.
    pub fn build_to_vec(self) -> Result<Vec<u8>> {
        let canvas_w = self.canvas_width;
        let canvas_h = self.canvas_height;

        // Encode each layer's tiles and build the archive layer specs.
        let mut layer_specs: Vec<LayerSpec> = Vec::with_capacity(self.layers.len());
        type TileList = Vec<(u32, u32, Vec<u8>)>;
        let mut layer_tiles: Vec<(String, TileList)> = Vec::with_capacity(self.layers.len());

        for (config, image) in self.layers {
            let uuid = config
                .uuid
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string().to_uppercase());

            let rgba = image.into_rgba8();

            // Resize if the image doesn't match the canvas (simple crop/pad).
            let rgba = if rgba.width() != canvas_w || rgba.height() != canvas_h {
                let mut canvas = image::RgbaImage::new(canvas_w, canvas_h);
                let blit_w = rgba.width().min(canvas_w);
                let blit_h = rgba.height().min(canvas_h);
                for y in 0..blit_h {
                    for x in 0..blit_w {
                        canvas.put_pixel(x, y, *rgba.get_pixel(x, y));
                    }
                }
                canvas
            } else {
                rgba
            };

            let tiles = split_into_tiles(&rgba);

            layer_specs.push(LayerSpec {
                uuid: uuid.clone(),
                name: config.name,
                opacity: config.opacity,
                visible: config.visible,
                locked: config.locked,
                preserve_alpha: config.preserve_alpha,
                clipped: config.clipped,
                blend_mode: config.blend_mode.to_i64(),
                layer_type: config.layer_type,
                width: canvas_w,
                height: canvas_h,
            });

            layer_tiles.push((uuid, tiles));
        }

        // Build Document.archive binary plist.
        let archive_bytes = build_document_archive(&DocumentSpec {
            name: self.name,
            canvas_width: canvas_w,
            canvas_height: canvas_h,
            dpi: self.dpi,
            color_profile: self.color_profile,
            background_color: self.background_color,
            background_hidden: self.background_hidden,
            layers: layer_specs,
        })?;

        // Assemble the ZIP.
        // Tile blobs are already LZ4-compressed so we store them uncompressed in the ZIP.
        let stored = FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        let cursor = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(cursor);

        zip.start_file("Document.archive", stored)
            .map_err(ProcreateError::Zip)?;
        zip.write_all(&archive_bytes)?;

        for (uuid, tiles) in layer_tiles {
            for (row, col, blob) in tiles {
                let path = format!("{}/{}~{}.lz4", uuid, row, col);
                zip.start_file(&path, stored).map_err(ProcreateError::Zip)?;
                zip.write_all(&blob)?;
            }
        }

        let cursor = zip.finish().map_err(ProcreateError::Zip)?;
        Ok(cursor.into_inner())
    }
}
