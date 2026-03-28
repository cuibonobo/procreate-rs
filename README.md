# procreate-rs

A Rust library and CLI tool for parsing [Procreate](https://procreate.com/) `.procreate` files. Extracts layer metadata and rasterizes each layer to a full-canvas PNG.

## Features

- Parse canvas dimensions, DPI, color profile, and background color
- Extract all layer metadata: name, UUID, opacity, visibility, blend mode, type, and more
- Rasterize individual layers or all layers to RGBA PNG images
- Export a JSON manifest alongside the PNGs
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

## Building

Requires Rust 1.65+.

```sh
cargo build --release
cargo test
```

A devcontainer is provided (`.devcontainer/`) for environments without a local Rust toolchain.

## Format documentation

See [`docs/procreate-format.md`](docs/procreate-format.md) for a detailed description of the `.procreate` file format, including the NSKeyedArchiver structure, bv41 tile encoding, column-major pixel layout, and known quirks.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `zip` | Read the ZIP archive |
| `plist` | Parse the NSKeyedArchiver binary plist |
| `lz4_flex` | Decompress bv41/LZ4 tile data |
| `image` | Encode PNG output |
| `serde` / `serde_json` | Serialize the JSON manifest |
| `thiserror` / `anyhow` | Error handling |

## License

MIT
