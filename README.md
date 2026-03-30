# procreate-rs

A Rust library and CLI tool for reading and writing [Procreate](https://procreate.com/) `.procreate` files. Exports layer metadata and rasterized PNGs, and imports them back into a valid `.procreate` file.

## Features

- Parse canvas dimensions, DPI, color profile, and background color
- Extract all layer metadata: name, UUID, opacity, visibility, blend mode, type, and more
- Rasterize individual layers or all layers to RGBA PNG images
- Export a JSON manifest alongside the PNGs
- Import a manifest + PNGs back into a `.procreate` file (full round-trip)
- Build `.procreate` files programmatically via `ProcreateDocumentBuilder`
- Handles the bv41 LZ4 tile format, column-major pixel layout, and premultiplied alpha

## Usage

### CLI

```sh
# Export all layers as PNGs + manifest.json into MyFile/
procreate-export MyFile.procreate

# Export to a specific directory
procreate-export MyFile.procreate output/

# Print document metadata without rasterizing
procreate-export MyFile.procreate --info

# Import a manifest folder back into a .procreate file
procreate-import MyFile/
procreate-import MyFile/manifest.json output.procreate
```

The `--info` output looks like:

```
Name:          My Artwork
Canvas:        1920×1080 @ 132 DPI
Color profile: sRGB IEC61966-2.1
Background:    rgba(1.00, 1.00, 1.00, 1.00) hidden=false
Stroke count:  847

Layers (5):
  [0] Sky                          uuid=A1B2C3D4 opacity=1.00 visible=true type=0
  [1] Mountains                    uuid=E5F6A7B8 opacity=0.80 visible=true type=0
  ...
```

### Library

Add to `Cargo.toml`:

```toml
[dependencies]
procreate = { path = "." }
```

Parse a document and rasterize layers:

```rust
use procreate::ProcreateDocument;

let doc = ProcreateDocument::from_path("MyFile.procreate")?;

println!("Canvas: {}×{}", doc.canvas_width, doc.canvas_height);

for layer in &doc.layers {
    println!("Layer: {} ({})", layer.name, layer.uuid);
}

// Rasterize a single layer by UUID
let img = doc.rasterize_layer("MyFile.procreate", &doc.layers[0].uuid)?;
img.save("layer0.png")?;

// Or rasterize all layers at once
let pairs = doc.rasterize_all("MyFile.procreate")?;
for (layer, img) in pairs {
    img.save(format!("{}.png", layer.name))?;
}
```

Use the high-level export function:

```rust
use procreate::export::{export_layers, ExportOptions};

let options = ExportOptions {
    visible_only: true,
    skip_special_layers: true,
    ..Default::default()
};

let manifest_path = export_layers("MyFile.procreate", "output/", &options)?;
```

Import a manifest + PNGs back into a `.procreate` file:

```rust
use procreate::import::import_from_manifest;

import_from_manifest("output/manifest.json", "Rebuilt.procreate")?;
```

Build a `.procreate` file from scratch:

```rust
use procreate::{ProcreateDocumentBuilder, LayerConfig};

let sky = image::open("sky.png")?;
let mountains = image::open("mountains.png")?;

ProcreateDocumentBuilder::new(1920, 1080)
    .name("My Artwork")
    .dpi(132.0)
    .color_profile("Display P3")
    .add_layer(sky, LayerConfig { name: "Sky".to_string(), ..Default::default() })
    .add_layer(mountains, LayerConfig {
        name: "Mountains".to_string(),
        opacity: 0.8,
        ..Default::default()
    })
    .build("MyArtwork.procreate")?;
```

## Output

**Per-layer PNGs** — full canvas size, straight (un-premultiplied) RGBA.

**`manifest.json`** — document and layer metadata:

```json
{
  "name": "My Artwork",
  "canvas_width": 1920,
  "canvas_height": 1080,
  "dpi": 132.0,
  "color_profile": "sRGB IEC61966-2.1",
  "background_color": [1.0, 1.0, 1.0, 1.0],
  "background_hidden": false,
  "animation": null,
  "layers": [
    {
      "uuid": "A1B2C3D4-...",
      "name": "Sky",
      "opacity": 1.0,
      "visible": true,
      "locked": false,
      "preserve_alpha": false,
      "clipped": false,
      "blend_mode": "Normal",
      "layer_type": 0,
      "file": "Sky_A1B2C3D4.png"
    }
  ]
}
```

**`thumbnail.png`** — the fully-composed image thumbnail extracted directly from the `.procreate` archive's `QuickLook/` folder.

## Supported color profiles

The following color profiles are supported for writing (i.e., the correct ICC data is embedded in the output file). Any other string is stored as the name only without embedded ICC data.

| Profile name | Notes |
|---|---|
| `sRGB IEC61966-2.1` | Standard sRGB — Procreate default |
| `Display P3` | Wide-gamut display profile |
| `sRGB v4 ICC Appearance` | sRGB v4 perceptual rendering intent |
| `sRGB v4 ICC Preference` | sRGB v4 perceptual preference intent |
| `sRGB v4 ICC Preference Display Class` | sRGB v4 preference, display class |

## Building

Requires Rust 1.65+.

```sh
cargo build --release
cargo test
```

## Format documentation

See [`docs/procreate-format.md`](docs/procreate-format.md) for a detailed description of the `.procreate` file format, including the NSKeyedArchiver structure, bv41 tile encoding, column-major pixel layout, and known quirks.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `zip` | Read and write the ZIP archive |
| `plist` | Parse and build NSKeyedArchiver binary plists |
| `lz4_flex` | Compress and decompress bv41/LZ4 tile data |
| `image` | Decode and encode PNG layer images |
| `uuid` | Generate layer UUIDs |
| `serde` / `serde_json` | Serialize the JSON manifest |
| `thiserror` / `anyhow` | Error handling |

## License

MIT
