use procreate::export::{export_layers, ExportOptions};
use procreate::{BlendMode, ProcreateDocument, ProcreateError};
use std::path::PathBuf;

fn reference_procreate() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("reference_files/parse-reference.procreate")
}

fn reference_png(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("reference_files/parse-reference")
        .join(filename)
}

#[test]
fn document_metadata() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    assert_eq!(doc.name, "Parse-reference");
    assert_eq!(doc.canvas_width, 768);
    assert_eq!(doc.canvas_height, 512);
    assert_eq!(doc.dpi, 64.0);
    assert_eq!(doc.color_profile, "Display P3");
    assert!(!doc.background_hidden);
}

#[test]
fn background_color_is_mid_gray_opaque() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();
    let [r, g, b, a] = doc.background_color;

    // Reference manifest shows ~0.5 gray, fully opaque
    assert!((r - 0.5).abs() < 0.01, "red channel should be ~0.5, got {r}");
    assert!((g - 0.5).abs() < 0.01, "green channel should be ~0.5, got {g}");
    assert!((b - 0.5).abs() < 0.01, "blue channel should be ~0.5, got {b}");
    assert_eq!(a, 1.0);
}

#[test]
fn animation_settings() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();
    let anim = doc.animation.as_ref().expect("animation settings should be present");

    assert_eq!(anim.frame_rate, 15);
    assert_eq!(anim.playback_mode, 1); // 1 = ping-pong
}

#[test]
fn layer_count_and_order() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    assert_eq!(doc.layers.len(), 3);

    // Layers are in bottom-to-top display order (index 0 = bottom)
    assert_eq!(doc.layers[0].name, "Green bottom left");
    assert_eq!(doc.layers[1].name, "Blue top right");
    assert_eq!(doc.layers[2].name, "Red top left, with black notch in top left corner");
}

#[test]
fn layer_uuids() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    assert_eq!(doc.layers[0].uuid, "C25546E3-56D8-4E87-B37E-5988D036F970");
    assert_eq!(doc.layers[1].uuid, "2DBAA061-5FDB-4970-A97B-E3914E3DBFEA");
    assert_eq!(doc.layers[2].uuid, "B95533D8-3E41-4726-A19E-8EB73EDCA59B");
}

#[test]
fn layer_properties() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    for layer in &doc.layers {
        assert_eq!(layer.opacity, 1.0, "layer '{}' should have full opacity", layer.name);
        assert!(layer.visible, "layer '{}' should be visible", layer.name);
        assert!(!layer.locked, "layer '{}' should not be locked", layer.name);
        assert_eq!(layer.blend_mode, BlendMode::Normal, "layer '{}' should use Normal blend mode", layer.name);
        assert_eq!(layer.layer_type, 0, "layer '{}' should be a normal layer type", layer.name);
    }
}

#[test]
fn layer_lookup_by_name_is_case_insensitive() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    assert!(doc.layer_by_name("Green bottom left").is_some());
    assert!(doc.layer_by_name("green bottom left").is_some());
    assert!(doc.layer_by_name("GREEN BOTTOM LEFT").is_some());
    assert!(doc.layer_by_name("nonexistent layer").is_none());
}

#[test]
fn layer_lookup_by_uuid() {
    let doc = ProcreateDocument::from_path(reference_procreate()).unwrap();

    let layer = doc.layer_by_uuid("C25546E3-56D8-4E87-B37E-5988D036F970").unwrap();
    assert_eq!(layer.name, "Green bottom left");

    assert!(doc.layer_by_uuid("00000000-0000-0000-0000-000000000000").is_none());
}

#[test]
fn rasterized_layers_match_reference_pngs() {
    let path = reference_procreate();
    let doc = ProcreateDocument::from_path(&path).unwrap();

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
        let rasterized = doc.rasterize_layer(&path, uuid).unwrap();
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
            "{png_name}: pixel data does not match reference"
        );
    }
}

// --- from_reader ---

#[test]
fn from_reader_parses_identically_to_from_path() {
    let path = reference_procreate();
    let from_path = ProcreateDocument::from_path(&path).unwrap();
    let reader = std::fs::File::open(&path).unwrap();
    let from_reader = ProcreateDocument::from_reader(reader).unwrap();

    assert_eq!(from_path.name, from_reader.name);
    assert_eq!(from_path.canvas_width, from_reader.canvas_width);
    assert_eq!(from_path.canvas_height, from_reader.canvas_height);
    assert_eq!(from_path.layers.len(), from_reader.layers.len());
    for (a, b) in from_path.layers.iter().zip(from_reader.layers.iter()) {
        assert_eq!(a.uuid, b.uuid);
        assert_eq!(a.name, b.name);
    }
}

// --- rasterize_all ---

#[test]
fn rasterize_all_returns_all_layers_with_correct_dimensions() {
    let path = reference_procreate();
    let doc = ProcreateDocument::from_path(&path).unwrap();
    let pairs = doc.rasterize_all(&path).unwrap();

    assert_eq!(pairs.len(), doc.layers.len());
    for (layer, img) in &pairs {
        assert_eq!(
            img.dimensions(),
            (doc.canvas_width, doc.canvas_height),
            "layer '{}' image has wrong dimensions",
            layer.name
        );
    }
}

#[test]
fn rasterize_all_matches_rasterize_layer_per_layer() {
    let path = reference_procreate();
    let doc = ProcreateDocument::from_path(&path).unwrap();
    let all = doc.rasterize_all(&path).unwrap();

    for (layer, img_from_all) in &all {
        let img_single = doc.rasterize_layer(&path, &layer.uuid).unwrap();
        assert_eq!(
            img_from_all.as_raw(),
            img_single.as_raw(),
            "rasterize_all and rasterize_layer differ for '{}'",
            layer.name
        );
    }
}

// --- export_layers ---

fn temp_export_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("procreate-rs-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn export_layers_writes_pngs_and_manifest() {
    let out = temp_export_dir("export-default");
    let manifest_path = export_layers(reference_procreate(), &out, &ExportOptions::default()).unwrap();

    assert!(manifest_path.exists(), "manifest.json was not written");
    assert_eq!(manifest_path, out.join("manifest.json"));
    assert!(out.join("thumbnail.png").exists(), "thumbnail.png was not written");

    // All 3 layers should produce PNG files
    let pngs: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |x| x == "png"))
        .collect();
    // 3 layer PNGs + thumbnail
    assert_eq!(pngs.len(), 4, "expected 4 PNG files (3 layers + thumbnail), got {}", pngs.len());

    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn export_layers_manifest_matches_reference() {
    let out = temp_export_dir("export-manifest");
    export_layers(reference_procreate(), &out, &ExportOptions::default()).unwrap();

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();

    assert_eq!(written["name"], "Parse-reference");
    assert_eq!(written["canvas_width"], 768);
    assert_eq!(written["canvas_height"], 512);
    assert_eq!(written["dpi"], 64.0);
    assert_eq!(written["color_profile"], "Display P3");
    assert_eq!(written["layers"].as_array().unwrap().len(), 3);

    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn export_options_default_values() {
    let opts = ExportOptions::default();
    assert!(!opts.visible_only);
    assert!(opts.skip_special_layers);
    assert!(opts.unpremultiply);
}

// --- error handling ---

#[test]
fn from_path_nonexistent_file_returns_io_error() {
    let result = ProcreateDocument::from_path("/nonexistent/path/file.procreate");
    assert!(matches!(result, Err(ProcreateError::Io(_))));
}

#[test]
fn from_reader_non_zip_returns_zip_error() {
    let junk = std::io::Cursor::new(b"this is not a zip file");
    let result = ProcreateDocument::from_reader(junk);
    assert!(matches!(result, Err(ProcreateError::Zip(_))));
}
