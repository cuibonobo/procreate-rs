//! LZ4 tile decompression and stitching into a full-layer RGBA image.
//!
//! Procreate stores each layer as a grid of 256×256 pixel tiles.
//! Each tile is stored as an LZ4-compressed blob of raw RGBA bytes.
//! The filename encodes position as `{row}~{col}.lz4` where row is the
//! y-tile index (row 0 = top of canvas) and col is the x-tile index.
//!
//! Pixel order within each tile blob is **column-major**: column 0 (x=0)
//! is stored first (all 256 rows, top-to-bottom), then column 1, etc.
//! Each pixel is 4 bytes (R, G, B, A), premultiplied alpha.

use crate::Result;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::io::Read;
use zip::ZipArchive;

pub const TILE_SIZE: u32 = 256;

/// Parse tile coordinates from a filename like "2~4.lz4".
/// Returns (col, row) or None if the name doesn't match.
///
/// Procreate encodes tile positions as `{row}~{col}.lz4` where row is the
/// y-tile index counting from the **bottom** of the canvas (Metal/GPU convention).
/// The caller is responsible for converting to screen-space y coordinates.
pub fn parse_tile_name(name: &str) -> Option<(u32, u32)> {
    let stem = name.strip_suffix(".lz4")?;
    let mut parts = stem.splitn(2, '~');
    let row: u32 = parts.next()?.parse().ok()?;
    let col: u32 = parts.next()?.parse().ok()?;
    Some((col, row))
}

/// Decompress a single LZ4-compressed tile into raw RGBA bytes.
///
/// Procreate stores tiles as one or more "bv41" chunks. Each chunk has a
/// 12-byte header followed by raw LZ4 block data:
///   bytes 0–3:  magic b"bv41"
///   bytes 4–7:  uncompressed size for this chunk (LE u32)
///   bytes 8–11: compressed size for this chunk (LE u32)
///   bytes 12..: LZ4 block data
///
/// Chunks use *dependent* (chained) LZ4 blocks: each chunk may contain
/// match offsets that reference bytes from earlier chunks. All chunks must
/// therefore be decompressed into a single shared output buffer using
/// `lz4_flex::block::decompress_into`, which allows backward references
/// into already-written data.
pub fn decompress_tile(compressed: &[u8]) -> crate::Result<Vec<u8>> {
    const MAGIC: &[u8; 4] = b"bv41";
    const HEADER_LEN: usize = 12;

    let mut output = Vec::with_capacity((TILE_SIZE * TILE_SIZE * 4) as usize);
    let mut pos = 0;

    while pos < compressed.len() {
        // "bv4$" is a sentinel marking end-of-stream; also stop if fewer than
        // HEADER_LEN bytes remain (can't be a valid chunk).
        if compressed.len() - pos < HEADER_LEN || &compressed[pos..pos + 4] != MAGIC {
            break;
        }
        let _uncompressed_size =
            u32::from_le_bytes(compressed[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let compressed_size =
            u32::from_le_bytes(compressed[pos + 8..pos + 12].try_into().unwrap()) as usize;
        pos += HEADER_LEN;

        let chunk = compressed
            .get(pos..pos + compressed_size)
            .ok_or_else(|| crate::ProcreateError::InvalidDocument("bv41 chunk data truncated".into()))?;

        // decompress_with_dict uses `output` (all prior chunks) as the external
        // dictionary so match offsets in this chunk can reference earlier data.
        let decompressed =
            lz4_flex::block::decompress_with_dict(chunk, _uncompressed_size, &output)
                .map_err(|e| {
                    crate::ProcreateError::InvalidDocument(format!(
                        "LZ4 block decompression failed: {e}"
                    ))
                })?;
        output.extend_from_slice(&decompressed);
        pos += compressed_size;
    }

    Ok(output)
}

/// Un-premultiply alpha in-place.
///
/// Procreate stores premultiplied RGBA. Most image editors and the
/// PNG format expect straight (un-premultiplied) alpha.
pub fn unpremultiply(rgba: &mut [u8]) {
    for chunk in rgba.chunks_exact_mut(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.0 {
            chunk[0] = (chunk[0] as f32 / a).round().min(255.0) as u8;
            chunk[1] = (chunk[1] as f32 / a).round().min(255.0) as u8;
            chunk[2] = (chunk[2] as f32 / a).round().min(255.0) as u8;
        }
    }
}

/// Stitch all tiles for a layer UUID into a single RGBA image.
///
/// `canvas_width` and `canvas_height` are the document dimensions.
/// Tiles outside the canvas bounds are ignored.
pub fn stitch_layer<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    uuid: &str,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<RgbaImage> {
    let mut image: RgbaImage = ImageBuffer::new(canvas_width, canvas_height);

    // Collect tile paths for this layer
    let tile_paths: Vec<String> = archive
        .file_names()
        .filter(|name| name.starts_with(&format!("{}/", uuid)) && name.ends_with(".lz4"))
        .map(String::from)
        .collect();

    for path in tile_paths {
        // Extract col~row from the filename portion
        let filename = path.split('/').next_back().unwrap_or("");
        let (col, row) = match parse_tile_name(filename) {
            Some(coords) => coords,
            None => continue,
        };

        // Read and decompress the tile
        let mut zip_file = archive.by_name(&path)?;
        let mut compressed = Vec::new();
        zip_file.read_to_end(&mut compressed)?;
        drop(zip_file);

        let mut raw = decompress_tile(&compressed)?;
        unpremultiply(&mut raw);

        // Calculate pixel offset for this tile
        let x_offset = col * TILE_SIZE;
        let y_offset = row * TILE_SIZE;

        // Skip tiles that start outside the canvas
        if x_offset >= canvas_width || y_offset >= canvas_height {
            continue;
        }

        // Actual tile dimensions (may be smaller at canvas edges)
        let tile_w = (canvas_width - x_offset).min(TILE_SIZE);
        let tile_h = (canvas_height - y_offset).min(TILE_SIZE);

        // Blit tile pixels into the canvas image.
        // Procreate stores tile data column-major: the x (column) index varies
        // in the outer dimension, so the byte layout is column 0 (all rows),
        // then column 1, etc.  We therefore iterate tx in the outer loop so
        // that pixel_idx increases monotonically and the break remains valid.
        for tx in 0..tile_w {
            for ty in 0..tile_h {
                let pixel_idx = ((tx * TILE_SIZE + ty) * 4) as usize;
                if pixel_idx + 3 >= raw.len() {
                    break;
                }
                let r = raw[pixel_idx];
                let g = raw[pixel_idx + 1];
                let b = raw[pixel_idx + 2];
                let a = raw[pixel_idx + 3];
                image.put_pixel(x_offset + tx, y_offset + ty, Rgba([r, g, b, a]));
            }
        }
    }

    Ok(image)
}
