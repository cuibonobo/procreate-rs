//! Encode RGBA images into the bv41/LZ4 tile format used by Procreate.
//!
//! This is the write-direction counterpart of `tile.rs`.
//!
//! Each layer is split into 256×256 tiles. For each tile:
//!   1. Pixels are extracted into a 256×256 buffer (row-major, zero-padded at edges).
//!   2. The buffer is transposed from row-major to column-major (Procreate's layout).
//!   3. Alpha is premultiplied.
//!   4. The 262144-byte buffer is split into 4 chunks of 65536 bytes each.
//!   5. Each chunk is compressed with LZ4 using all prior decompressed data as a
//!      back-reference dictionary (chained/dependent blocks).
//!   6. Each chunk is framed as: b"bv41" + u32_le(65536) + u32_le(compressed_len) + data.
//!   7. A b"bv4$" sentinel terminates the tile blob.
//!
//! Fully-transparent tiles are omitted (Procreate treats absent tiles as transparent).

use image::RgbaImage;

pub const TILE_SIZE: u32 = 256;
const TILE_BYTES: usize = (TILE_SIZE * TILE_SIZE * 4) as usize;
/// Procreate splits each 262144-byte tile into 4 chained LZ4 blocks of this size.
const CHUNK_SIZE: usize = 65536;

/// Encode a 256×256 RGBA tile (row-major, straight alpha) into a bv41 blob.
///
/// `pixels` must be exactly TILE_BYTES (262144) bytes.
pub fn encode_tile(pixels: &[u8]) -> Vec<u8> {
    debug_assert_eq!(pixels.len(), TILE_BYTES);

    // Transpose row-major → column-major and premultiply alpha in one pass.
    let mut colmajor = vec![0u8; TILE_BYTES];
    for y in 0..TILE_SIZE as usize {
        for x in 0..TILE_SIZE as usize {
            let src = (y * TILE_SIZE as usize + x) * 4;
            let dst = (x * TILE_SIZE as usize + y) * 4;
            let r = pixels[src];
            let g = pixels[src + 1];
            let b = pixels[src + 2];
            let a = pixels[src + 3];
            let af = a as f32 / 255.0;
            colmajor[dst] = (r as f32 * af).round() as u8;
            colmajor[dst + 1] = (g as f32 * af).round() as u8;
            colmajor[dst + 2] = (b as f32 * af).round() as u8;
            colmajor[dst + 3] = a;
        }
    }

    // Split into 4 chained LZ4 blocks of CHUNK_SIZE bytes each.
    // Each block is compressed using all prior decompressed bytes as a back-reference
    // dictionary, matching Procreate's "dependent blocks" format.
    let mut blob = Vec::with_capacity(4 * (12 + CHUNK_SIZE) + 4);
    for i in 0..(TILE_BYTES / CHUNK_SIZE) {
        let chunk_start = i * CHUNK_SIZE;
        let chunk = &colmajor[chunk_start..chunk_start + CHUNK_SIZE];
        let dict  = &colmajor[..chunk_start]; // all prior decompressed bytes
        let compressed = lz4_flex::block::compress_with_dict(chunk, dict);

        blob.extend_from_slice(b"bv41");
        blob.extend_from_slice(&(CHUNK_SIZE as u32).to_le_bytes());
        blob.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        blob.extend_from_slice(&compressed);
    }
    blob.extend_from_slice(b"bv4$"); // end-of-stream sentinel

    blob
}

/// Split an RGBA image into bv41 tiles, skipping fully-transparent ones.
///
/// Returns `(row, col, blob)` tuples where row/col are tile indices
/// (row=0 is the top of the canvas) matching Procreate's `{row}~{col}.lz4` naming.
pub fn split_into_tiles(img: &RgbaImage) -> Vec<(u32, u32, Vec<u8>)> {
    let (canvas_w, canvas_h) = img.dimensions();
    let cols = canvas_w.div_ceil(TILE_SIZE);
    let rows = canvas_h.div_ceil(TILE_SIZE);
    let raw = img.as_raw(); // row-major RGBA bytes

    let mut tiles = Vec::new();

    for row in 0..rows {
        for col in 0..cols {
            let x_off = col * TILE_SIZE;
            let y_off = row * TILE_SIZE;

            let tile_w = (canvas_w - x_off).min(TILE_SIZE) as usize;
            let tile_h = (canvas_h - y_off).min(TILE_SIZE) as usize;

            // Extract this tile's pixels into a zero-padded 256×256 buffer.
            let mut buf = vec![0u8; TILE_BYTES];
            let mut all_transparent = true;

            for ty in 0..tile_h {
                for tx in 0..tile_w {
                    let sx = x_off as usize + tx;
                    let sy = y_off as usize + ty;
                    let src = (sy * canvas_w as usize + sx) * 4;
                    let dst = (ty * TILE_SIZE as usize + tx) * 4;
                    buf[dst..dst + 4].copy_from_slice(&raw[src..src + 4]);
                    if raw[src + 3] != 0 {
                        all_transparent = false;
                    }
                }
            }

            if all_transparent {
                continue;
            }

            tiles.push((row, col, encode_tile(&buf)));
        }
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{decompress_tile, unpremultiply};

    #[test]
    fn encode_decode_roundtrip_opaque() {
        // Fill a tile with a known opaque color and round-trip through encode → decode.
        let mut pixels = vec![0u8; TILE_BYTES];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = 200; // R
            chunk[1] = 100; // G
            chunk[2] = 50; // B
            chunk[3] = 255; // A (fully opaque)
        }

        let blob = encode_tile(&pixels);
        let mut decoded = decompress_tile(&blob).expect("decompress failed");
        unpremultiply(&mut decoded);

        // Decoded is column-major; read pixel at (x=0, y=0): index 0
        assert_eq!(decoded[0], 200);
        assert_eq!(decoded[1], 100);
        assert_eq!(decoded[2], 50);
        assert_eq!(decoded[3], 255);
    }

    #[test]
    fn encode_decode_roundtrip_semitransparent() {
        // 50% transparent red — premultiplied stored value should be ~128.
        let mut pixels = vec![0u8; TILE_BYTES];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[0] = 255; // R
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = 128; // ~50% alpha
        }

        let blob = encode_tile(&pixels);
        let mut decoded = decompress_tile(&blob).expect("decompress failed");

        // Before un-premultiplication: R should be ~128 (= 255 * 128/255)
        assert_eq!(decoded[3], 128); // alpha unchanged
        assert!(
            decoded[0] <= 130 && decoded[0] >= 126,
            "premultiplied R ≈ 128, got {}",
            decoded[0]
        );

        unpremultiply(&mut decoded);
        // After un-premultiplication: R should recover to 255
        assert_eq!(decoded[0], 255, "un-premultiplied R should be 255");
    }

    #[test]
    fn split_skips_transparent_tiles() {
        // A 512×256 image with only the left half painted.
        let mut img = RgbaImage::new(512, 256);
        for y in 0..256 {
            for x in 0..256 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let tiles = split_into_tiles(&img);
        // Only col=0 tile should be produced; col=1 is transparent.
        assert_eq!(tiles.len(), 1);
        assert_eq!(tiles[0].0, 0); // row
        assert_eq!(tiles[0].1, 0); // col
    }
}
