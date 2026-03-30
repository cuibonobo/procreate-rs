//! CLI tool: procreate-export
//!
//! Usage:
//!   procreate-export <file.procreate> [output_dir]
//!   procreate-export <file.procreate> --info

use procreate::export::export_layers;
use procreate::{ExportOptions, ProcreateDocument};
use std::env;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: procreate-export <file.procreate> [output_dir|--info]");
        std::process::exit(1);
    }

    let input = PathBuf::from(&args[1]);
    let mode = args.get(2).map(String::as_str).unwrap_or("export");

    if mode == "--info" {
        // Print document metadata without rasterizing
        let doc = ProcreateDocument::from_path(&input)?;
        println!("Name:          {}", doc.name);
        println!(
            "Canvas:        {}×{} @ {} DPI",
            doc.canvas_width, doc.canvas_height, doc.dpi
        );
        println!("Color profile: {}", doc.color_profile);
        println!(
            "Background:    rgba({:.2}, {:.2}, {:.2}, {:.2}) hidden={}",
            doc.background_color[0],
            doc.background_color[1],
            doc.background_color[2],
            doc.background_color[3],
            doc.background_hidden
        );
        println!("Stroke count:  {}", doc.stroke_count);

        if let Some(anim) = &doc.animation {
            println!(
                "Animation:     {} fps, mode={}",
                anim.frame_rate, anim.playback_mode
            );
        }

        println!("\nLayers ({}):", doc.layers.len());
        for (i, layer) in doc.layers.iter().enumerate() {
            println!(
                "  [{i}] {:30} uuid={} opacity={:.2} visible={} type={}",
                layer.name,
                &layer.uuid[..8],
                layer.opacity,
                layer.visible,
                layer.layer_type,
            );
        }
    } else {
        // Export all layers as PNGs
        let output_dir = if mode == "export" {
            input.with_extension("")
        } else {
            PathBuf::from(mode)
        };

        println!("Exporting {} → {}", input.display(), output_dir.display());

        let options = ExportOptions {
            visible_only: false,
            skip_special_layers: true,
            ..Default::default()
        };

        let manifest_path = export_layers(&input, &output_dir, &options)?;
        println!("Manifest written to: {}", manifest_path.display());
    }

    Ok(())
}
