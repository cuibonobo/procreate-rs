use clap::{Parser, Subcommand};
use procreate::export::export_layers;
use procreate::{ExportOptions, ProcreateDocument};
use std::path::PathBuf;

/// Unpack a .procreate file into layer PNGs and a manifest.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Path to the .procreate file
    input: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Export all layers as PNGs alongside a manifest.json
    Export {
        /// Directory to write layers into [default: <input stem>]
        output_dir: Option<PathBuf>,
    },
    /// Print document metadata without rasterizing any layers
    Info,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Export { output_dir: None }) {
        Command::Info => {
            let doc = ProcreateDocument::from_path(&cli.input)?;
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
        }

        Command::Export { output_dir } => {
            let output_dir = output_dir.unwrap_or_else(|| cli.input.with_extension(""));

            println!(
                "Unpacking {} → {}",
                cli.input.display(),
                output_dir.display()
            );

            let options = ExportOptions {
                visible_only: false,
                skip_special_layers: true,
                ..Default::default()
            };

            let manifest_path = export_layers(&cli.input, &output_dir, &options)?;
            println!("Manifest written to: {}", manifest_path.display());
        }
    }

    Ok(())
}
