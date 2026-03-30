use procreate::import::import_from_manifest;
use procreate::{BlendMode, ProcreateDocument};
use std::path::PathBuf;

fn reference_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference_files/parse-reference/manifest.json")
}

fn reference_procreate() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("reference_files/parse-reference.procreate")
}

fn reference_png(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("reference_files/parse-reference")
        .join(filename)
}

fn temp_procreate(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("procreate-rs-test-{name}.procreate"))
}

// ── Metadata round-trip ───────────────────────────────────────────────────────

#[test]
fn import_produces_parseable_file() {
    let out = temp_procreate("import-parseable");
    import_from_manifest(reference_manifest(), &out).unwrap();
    ProcreateDocument::from_path(&out).unwrap();
    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_canvas_dimensions() {
    let out = temp_procreate("import-dimensions");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    assert_eq!(doc.canvas_width, 768);
    assert_eq!(doc.canvas_height, 512);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_name_and_dpi() {
    let out = temp_procreate("import-name-dpi");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    assert_eq!(doc.name, "Parse-reference");
    assert_eq!(doc.dpi, 64.0);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_color_profile() {
    let out = temp_procreate("import-color-profile");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    assert_eq!(doc.color_profile, "Display P3");

    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_background_color() {
    let out = temp_procreate("import-bg-color");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    let [r, g, b, a] = doc.background_color;
    assert!((r - 0.5).abs() < 0.01, "red ~0.5, got {r}");
    assert!((g - 0.5).abs() < 0.01, "green ~0.5, got {g}");
    assert!((b - 0.5).abs() < 0.01, "blue ~0.5, got {b}");
    assert_eq!(a, 1.0);

    let _ = std::fs::remove_file(&out);
}

// ── Layer round-trip ──────────────────────────────────────────────────────────

#[test]
fn import_preserves_layer_count_and_order() {
    let out = temp_procreate("import-layers");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    assert_eq!(doc.layers.len(), 3);
    assert_eq!(doc.layers[0].name, "Green bottom left");
    assert_eq!(doc.layers[1].name, "Blue top right");
    assert_eq!(
        doc.layers[2].name,
        "Red top left, with black notch in top left corner"
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_layer_uuids() {
    let out = temp_procreate("import-uuids");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    assert_eq!(doc.layers[0].uuid, "C25546E3-56D8-4E87-B37E-5988D036F970");
    assert_eq!(doc.layers[1].uuid, "2DBAA061-5FDB-4970-A97B-E3914E3DBFEA");
    assert_eq!(doc.layers[2].uuid, "B95533D8-3E41-4726-A19E-8EB73EDCA59B");

    let _ = std::fs::remove_file(&out);
}

#[test]
fn import_preserves_layer_properties() {
    let out = temp_procreate("import-layer-props");
    import_from_manifest(reference_manifest(), &out).unwrap();
    let doc = ProcreateDocument::from_path(&out).unwrap();

    for layer in &doc.layers {
        assert_eq!(layer.opacity, 1.0, "layer '{}': opacity", layer.name);
        assert!(layer.visible, "layer '{}': visible", layer.name);
        assert!(!layer.locked, "layer '{}': locked", layer.name);
        assert_eq!(
            layer.blend_mode,
            BlendMode::Normal,
            "layer '{}': blend_mode",
            layer.name
        );
    }

    let _ = std::fs::remove_file(&out);
}

// ── Pixel round-trip ──────────────────────────────────────────────────────────

/// Export → import → rasterize: pixel output must match the original reference PNGs.
/// This validates the full encode path: tile splitting, LZ4 compression, ZIP assembly,
/// and archive serialisation.
#[test]
fn import_pixel_roundtrip() {
    let out = temp_procreate("import-pixel-roundtrip");
    import_from_manifest(reference_manifest(), &out).unwrap();

    let doc = ProcreateDocument::from_path(&out).unwrap();

    let cases = [
        (
            "C25546E3-56D8-4E87-B37E-5988D036F970",
            "Green_bottom_left_C25546E3.png",
        ),
        (
            "2DBAA061-5FDB-4970-A97B-E3914E3DBFEA",
            "Blue_top_right_2DBAA061.png",
        ),
        (
            "B95533D8-3E41-4726-A19E-8EB73EDCA59B",
            "Red_top_left__with_black_notch_in_top_left_corner_B95533D8.png",
        ),
    ];

    for (uuid, png_name) in &cases {
        let rasterized = doc.rasterize_layer(&out, uuid).unwrap();
        let reference = image::open(reference_png(png_name))
            .unwrap_or_else(|e| panic!("failed to open reference PNG {png_name}: {e}"))
            .to_rgba8();

        assert_eq!(
            rasterized.dimensions(),
            reference.dimensions(),
            "{png_name}: dimensions mismatch"
        );
        assert_eq!(
            rasterized.as_raw(),
            reference.as_raw(),
            "{png_name}: pixel data does not match reference after import round-trip"
        );
    }

    let _ = std::fs::remove_file(&out);
}

/// Re-importing the original reference .procreate (not the manifest) should produce
/// the same rasterized output as importing from the manifest, confirming that the
/// export→import chain is lossless regardless of the source path.
#[test]
fn rasterize_imported_matches_rasterize_original() {
    let out = temp_procreate("import-vs-original");
    import_from_manifest(reference_manifest(), &out).unwrap();

    let original = ProcreateDocument::from_path(reference_procreate()).unwrap();
    let imported = ProcreateDocument::from_path(&out).unwrap();

    for (orig_layer, imp_layer) in original.layers.iter().zip(imported.layers.iter()) {
        let orig_img = original
            .rasterize_layer(reference_procreate(), &orig_layer.uuid)
            .unwrap();
        let imp_img = imported.rasterize_layer(&out, &imp_layer.uuid).unwrap();

        assert_eq!(
            orig_img.as_raw(),
            imp_img.as_raw(),
            "layer '{}': pixels differ between original and re-imported file",
            orig_layer.name
        );
    }

    let _ = std::fs::remove_file(&out);
}
