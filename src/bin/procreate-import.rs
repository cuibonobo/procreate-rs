//! CLI tool: procreate-import
//!
//! Usage:
//!   procreate-import <manifest.json|folder/>  [output.procreate]
//!
//! If a folder is given, looks for manifest.json inside it.
//! Output path defaults to <folder_name>.procreate in the current directory.

use std::env;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: procreate-import <manifest.json|folder/> [output.procreate]");
        std::process::exit(1);
    }

    let input = PathBuf::from(&args[1]);

    // Resolve the manifest path and a default output name.
    let (manifest_path, default_stem) = if input.is_dir() {
        let m = input.join("manifest.json");
        let stem = input
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        (m, stem)
    } else {
        // Assume it's already a manifest.json (or similar).
        let stem = input
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();
        (input.clone(), stem)
    };

    if !manifest_path.exists() {
        eprintln!("Error: manifest not found at {}", manifest_path.display());
        std::process::exit(1);
    }

    let output_path = if let Some(arg) = args.get(2) {
        PathBuf::from(arg)
    } else {
        PathBuf::from(format!("{}.procreate", default_stem))
    };

    println!(
        "Importing {} → {}",
        manifest_path.display(),
        output_path.display()
    );

    procreate::import::import_from_manifest(&manifest_path, &output_path)?;

    println!("Written: {}", output_path.display());
    Ok(())
}
