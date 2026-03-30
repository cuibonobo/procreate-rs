use clap::Parser;
use procreate::import::import_from_manifest;
use std::path::PathBuf;

/// Pack a manifest and layer PNGs into a .procreate file.
///
/// INPUT may be a folder containing manifest.json, or the path to the
/// manifest.json file directly.  The output path defaults to
/// <folder_name>.procreate in the current directory.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Folder containing manifest.json, or path to manifest.json directly
    input: PathBuf,

    /// Where to write the .procreate file [default: <input stem>.procreate]
    output: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let (manifest_path, default_stem) = if cli.input.is_dir() {
        let m = cli.input.join("manifest.json");
        let stem = cli
            .input
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        (m, stem)
    } else {
        let stem = cli
            .input
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        (cli.input.clone(), stem)
    };

    if !manifest_path.exists() {
        anyhow::bail!("manifest not found at {}", manifest_path.display());
    }

    let output_path = cli
        .output
        .unwrap_or_else(|| PathBuf::from(format!("{}.procreate", default_stem)));

    println!(
        "Packing {} → {}",
        manifest_path.display(),
        output_path.display()
    );

    import_from_manifest(&manifest_path, &output_path)?;

    println!("Written: {}", output_path.display());
    Ok(())
}
