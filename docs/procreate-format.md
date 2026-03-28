# The Procreate File Format

This document describes the `.procreate` file format as understood through reverse engineering, and explains how this library parses it. It is accurate as of Procreate 5.x.

---

## Overview

A `.procreate` file is a **ZIP archive** (standard ZIP, not compressed store — each entry may itself be compressed). It contains:

| Path | Description |
|------|-------------|
| `Document.archive` | NSKeyedArchiver binary plist — all document metadata and layer definitions |
| `{uuid}/` | One directory per layer, named by the layer's UUID |
| `{uuid}/{row}~{col}.lz4` | Tile image data for that layer |

---

## Document.archive — NSKeyedArchiver

`Document.archive` is a **binary property list** using Apple's `NSKeyedArchiver` serialization format. Its top-level structure is:

```
{
  "$version": 100000,
  "$archiver": "NSKeyedArchiver",
  "$top":     { "root": UID(1) },
  "$objects":  [ "$null", <root object>, <object 2>, ... ]
}
```

All real data lives in `$objects`. Object 0 is always the string `"$null"` (the nil sentinel). Object 1 is always the root document object. Objects reference each other via **UID values** — a UID is just an index into the `$objects` array.

### Parsing strategy

1. Read the binary plist using any plist library (e.g., the Rust `plist` crate).
2. Extract the `$objects` array.
3. Start from index 1 (the root).
4. For any value that is a UID, dereference it by looking up `$objects[uid]`.
5. If a dereferenced value is the string `"$null"`, treat it as absent/nil.

### Root object fields

The root object is a dictionary. Key fields:

| Key | Type | Notes |
|-----|------|-------|
| `name` | String | Document name (no `.procreate` extension) |
| `size` | String | Canvas dimensions (see below) |
| `SilicaDocumentArchiveDPIKey` | Real | DPI, typically 72 or 132 |
| `colorProfile` | UID → dict | Color profile object; contains `SiColorProfileArchiveICCNameKey` |
| `backgroundColor` | UID → Data | 16 bytes: four LE f32s (R, G, B, A), values 0.0–1.0 |
| `backgroundHidden` | Boolean | Whether the background is hidden |
| `layers` | UID → NSArray | Display-ordered layer list (top layer first) |
| `strokeCount` | Integer | Total number of strokes |
| `animation` | UID → dict | Present only if the document uses Procreate Animation |

#### Canvas size string

The `size` field is serialized as a **CGSize string**: `{height, width}` — note that height comes **first**, contrary to the standard `{width, height}` convention used in most CGSize documentation. For example, a 1920×1080 canvas is stored as `{1080, 1920}`.

Parsing: strip the curly braces, split on `,`, parse the first value as height and the second as width.

#### NSArray objects

Layer lists (and other collections) are stored as NSKeyedArchiver-serialized `NSArray` objects. They appear in `$objects` as dictionaries with an `NS.objects` key whose value is an array of UIDs pointing to the array's elements.

```
{
  "NS.objects": [ UID(5), UID(7), UID(9), ... ]
}
```

Resolve each UID to get the actual element.

### Layer object fields

Each element of the `layers` array is a layer dictionary:

| Key | Type | Notes |
|-----|------|-------|
| `UUID` | String | UUID string, e.g. `"B95533D8-..."` — also the ZIP folder name |
| `name` | String | User-visible layer name |
| `opacity` | Real | 0.0–1.0 |
| `hidden` | Boolean | `true` means hidden (note: inverted from `visible`) |
| `locked` | Boolean | |
| `preserve` | Boolean | Lock transparency (preserve alpha) |
| `clipped` | Boolean | Clip to layer below |
| `blend` | Integer | Blend mode code (see table below) |
| `type` | Integer | Layer type: 0 = normal, 1 = composite/group, 2 = mask |
| `sizeWidth` | Real | Layer width in pixels |
| `sizeHeight` | Real | Layer height in pixels |
| `contentsRect` | UID → Data | Optional 32-byte blob: four LE f64s (x, y, w, h) — bounding rect of actual painted content |
| `transform` | UID → Data | Optional 128-byte blob: sixteen LE f64s — 4×4 row-major transform matrix |

#### Blend mode codes

| Code | Blend mode |
|------|-----------|
| 0 | Normal |
| 1 | Multiply |
| 2 | Screen |
| 3 | Add (Linear Dodge) |
| 4 | Overlay |
| 5 | Soft Light |
| 6 | Hard Light |
| 7 | Color Dodge |
| 8 | Color Burn |
| 9 | Darken |
| 10 | Lighten |
| 11 | Difference |
| 12 | Exclusion |
| 13 | Hue |
| 14 | Saturation |
| 15 | Color |
| 16 | Luminosity |

#### Animation settings

If the document is animated, the root's `animation` key resolves to a dictionary:

| Key | Type | Notes |
|-----|------|-------|
| `frameRate` | Integer | Frames per second |
| `onionSkinCount` | Integer | Number of onion skin frames |
| `onionSkinOpacity` | Real | 0.0–1.0 |
| `playbackMode` | Integer | 0 = loop, 1 = ping-pong, 2 = one-shot |

---

## Layer tile data

Each layer's pixel data is stored as a grid of **256×256 pixel tiles** inside the ZIP. The tiles for a layer with UUID `B95533D8-...` live at paths like:

```
B95533D8-.../0~0.lz4
B95533D8-.../0~1.lz4
B95533D8-.../1~0.lz4
...
```

### Tile filename format

Filenames follow the pattern `{row}~{col}.lz4`:

- **row** — the y-tile index, counting from **0 at the top** of the canvas downward.
- **col** — the x-tile index, counting from **0 at the left** rightward.

A tile at `row=r, col=c` covers canvas pixels `x = [c×256, (c+1)×256)`, `y = [r×256, (r+1)×256)`. Tiles at the right and bottom edges may cover fewer than 256 pixels if the canvas dimensions are not multiples of 256 — simply clamp to the canvas boundary when blitting.

Tiles that contain only transparent pixels may be absent from the ZIP entirely; treat any missing tile as fully transparent.

### bv41 container format

Each `.lz4` file is **not** a raw LZ4 stream — it is a Procreate-specific container format called **bv41**, made up of one or more sequential chunks. Each chunk has a 12-byte header:

```
bytes 0–3:   magic "bv41"  (4 ASCII bytes)
bytes 4–7:   uncompressed size of this chunk  (LE uint32)
bytes 8–11:  compressed size of this chunk    (LE uint32)
bytes 12…:   LZ4 block data (compressed_size bytes)
```

After the last real chunk, a sentinel chunk with magic `"bv4$"` (or any non-`"bv41"` 4 bytes) marks end-of-stream — stop parsing when you see it or when fewer than 12 bytes remain.

The chunks use **dependent (chained) LZ4 blocks**: match offsets in a later chunk may refer back into bytes decompressed by earlier chunks. All chunks must therefore be decompressed into a **single shared output buffer** in order, using the already-written bytes as the external dictionary for each new chunk. Do not reset the output buffer between chunks.

In practice, a fully-painted 256×256 tile decompresses to exactly 262,144 bytes (256 × 256 × 4 bytes/pixel), split across four chunks of 65,536 uncompressed bytes each.

### Pixel format

Decompressed tile data is **column-major RGBA** with premultiplied alpha:

- Pixels are stored **column by column**: all 256 pixels of column 0 (x=0, y=0..255) come first, then column 1, etc.
- Within each column, pixels are ordered **top-to-bottom** (y=0 first).
- Each pixel is 4 bytes: R, G, B, A.
- Alpha is **premultiplied**: `stored_R = actual_R × (A/255)`. Convert to straight alpha before saving as PNG or passing to compositing software.

To read pixel (x, y) from the decompressed buffer:

```
byte_index = (x × 256 + y) × 4
```

### Un-premultiplying alpha

To convert from premultiplied to straight alpha:

```
if A > 0:
    R = clamp(round(R / (A / 255)), 0, 255)
    G = clamp(round(G / (A / 255)), 0, 255)
    B = clamp(round(B / (A / 255)), 0, 255)
```

Pixels where A = 0 are fully transparent; leave their RGB values as-is.

---

## Stitching a layer image

To reconstruct the full RGBA image for a layer:

1. Create a canvas-sized RGBA buffer, initialized to transparent (all zeros).
2. Enumerate all ZIP entries matching `{uuid}/*.lz4`.
3. For each tile:
   a. Parse the filename to get `(row, col)`.
   b. Compute `x_offset = col × 256`, `y_offset = row × 256`.
   c. Skip the tile if `x_offset ≥ canvas_width` or `y_offset ≥ canvas_height`.
   d. Read and decompress the `.lz4` data using the bv41 decoder.
   e. Un-premultiply alpha.
   f. Blit the tile into the canvas at `(x_offset, y_offset)`, clamping to canvas bounds.
4. Save or return the canvas buffer.

---

## Known quirks and gotchas

| Issue | Detail |
|-------|--------|
| **Size string is height-first** | `{height, width}`, not `{width, height}` |
| **Tile data is column-major** | `x` varies in the outer dimension, `y` in the inner — the opposite of standard raster (row-major) order |
| **bv41 chunks are chained** | LZ4 match offsets cross chunk boundaries; decompress all chunks into one buffer |
| **Layer order** | The `layers` array in `Document.archive` is in **display order** (index 0 = topmost layer in the UI). For compositing, process from last to first (bottom to top). |
| **Absent tiles** | Fully transparent tiles are often omitted from the ZIP. Treat missing tiles as transparent. |
| **`hidden` is inverted** | The field is `hidden`, not `visible`. A layer with `hidden = false` is visible. |
| **`contentsRect` coordinates** | Pixel coordinates within the full canvas, not relative to the tile grid. |
