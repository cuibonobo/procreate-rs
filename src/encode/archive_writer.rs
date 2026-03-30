//! Build a binary NSKeyedArchiver plist (Document.archive) from document metadata.
//!
//! NSKeyedArchiver stores all objects in a flat `$objects` array; everything else
//! is cross-referenced via UID indices. Every dict object requires a `$class` key
//! pointing to a class-descriptor entry — NSKeyedUnarchiver throws without them.
//!
//! Object layout:
//!   [0]  "$null" sentinel
//!   [1]  root document dict            ($class → [8]  SilicaDocument)
//!   [2]  document name string
//!   [3]  canvas size string  ("{height, width}" — height first, Procreate quirk)
//!   [4]  color profile dict            ($class → [11] ValkyrieColorProfile)
//!   [5]  color profile name string
//!   [6]  background color Data (16 bytes: 4× LE f32 RGBA)
//!   [7]  layers NSMutableArray dict    ($class → [12] NSMutableArray)
//!   [8]  class: SilicaDocument         (classes: SilicaDocument, ValkyrieDocument, NSObject)
//!   [9]  class: NSArray                (classes: NSArray, NSObject)
//!   [10] class: SilicaLayer            (classes: SilicaLayer, ValkyrieLayer, NSObject)
//!   [11] class: ValkyrieColorProfile   (classes: ValkyrieColorProfile, NSObject)
//!   [12] class: NSMutableArray         (classes: NSMutableArray, NSArray, NSObject)
//!   [13] unwrappedLayers NSArray dict  ($class → [9] NSArray) — same items as layers
//!   [14] composite SilicaLayer dict    ($class → [10] SilicaLayer)
//!   [15] composite UUID string
//!   [16] composite transform Data      (128 bytes: 4×4 identity matrix)
//!   [17] composite contentsRect Data   (32 bytes: zero CGRect)
//!   For each layer n (0-indexed), at base = 18 + n*5:
//!   [base+0]  layer dict               ($class → [10] SilicaLayer)
//!   [base+1]  UUID string
//!   [base+2]  name string
//!   [base+3]  transform Data  (128 bytes: 4×4 identity matrix as 16× LE f64)
//!   [base+4]  contentsRect Data (32 bytes: zero CGRect as 4× LE f64)

use crate::ProcreateError;
use plist::{Dictionary, Integer, Uid, Value};
use uuid::Uuid;

pub struct LayerSpec {
    pub uuid: String,
    pub name: String,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub preserve_alpha: bool,
    pub clipped: bool,
    pub blend_mode: i64,
    pub layer_type: i64,
    pub width: u32,
    pub height: u32,
}

pub struct DocumentSpec {
    pub name: String,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub dpi: f64,
    pub color_profile: String,
    pub background_color: [f32; 4],
    pub background_hidden: bool,
    pub layers: Vec<LayerSpec>,
}

fn uid(idx: usize) -> Value {
    Value::Uid(Uid::new(idx as u64))
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn dict(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut d = Dictionary::new();
    for (k, v) in pairs {
        d.insert(k.to_string(), v);
    }
    Value::Dictionary(d)
}

fn class_desc(classname: &str, classes: &[&str]) -> Value {
    let mut d = Dictionary::new();
    d.insert(
        "$classname".to_string(),
        Value::String(classname.to_string()),
    );
    d.insert(
        "$classes".to_string(),
        Value::Array(
            classes
                .iter()
                .map(|&s| Value::String(s.to_string()))
                .collect(),
        ),
    );
    Value::Dictionary(d)
}

// Fixed indices for the five class descriptors.
const CLS_SILICA_DOCUMENT: usize = 8;
const CLS_NS_ARRAY: usize = 9;
const CLS_SILICA_LAYER: usize = 10;
const CLS_VALKYRIE_COLOR_PROFILE: usize = 11;
const CLS_NS_MUTABLE_ARRAY: usize = 12;
// Fixed indices for composite + unwrappedLayers (present in all real .procreate files).
const UNWRAPPED_LAYERS_IDX: usize = 13;
const COMPOSITE_LAYER_IDX: usize = 14;
const COMPOSITE_UUID_IDX: usize = 15;
const COMPOSITE_TRANSFORM_IDX: usize = 16;
const COMPOSITE_CONTENTS_RECT_IDX: usize = 17;
const LAYER_BASE: usize = 18;
/// Slots per layer: dict + UUID string + name string + transform bytes + contentsRect bytes
const LAYER_STRIDE: usize = 5;

/// Return the raw ICC profile bytes for well-known Procreate color profile names.
///
/// Procreate requires `SiColorProfileArchiveICCDataKey` to actually apply the color space;
/// the name string alone is just a display label. Without ICC data the app falls back to sRGB.
/// All blobs must be extracted from Procreate-generated files — macOS ColorSync ICC files use
/// incompatible variants that cause Procreate to throw an ObjC exception during archive loading.
fn icc_data_for_profile(name: &str) -> Option<&'static [u8]> {
    match name {
        // All ICC blobs extracted directly from Procreate-generated files — byte-verified.
        // Procreate does not validate the ICC desc tag against the name key; it uses whatever
        // raw bytes are stored in SiColorProfileArchiveICCDataKey.
        "Display P3" => Some(include_bytes!("../icc/display_p3.icc")),
        "sRGB IEC61966-2.1" => Some(include_bytes!("../icc/sRGB_IEC61966-2.1.icc")),
        "sRGB v4 ICC Appearance" => Some(include_bytes!("../icc/sRGB_v4_ICC_Appearance.icc")),
        "sRGB v4 ICC Preference" => Some(include_bytes!("../icc/sRGB_v4_ICC_Preference.icc")),
        "sRGB v4 ICC Preference Display Class" => Some(include_bytes!(
            "../icc/sRGB_v4_ICC_Preference_Display_Class.icc"
        )),
        _ => None,
    }
}

/// 4×4 identity matrix encoded as 16 LE f64 values (128 bytes).
/// Procreate stores layer transforms in this format.
fn identity_transform() -> Vec<u8> {
    let mut b = vec![0u8; 128];
    for i in [0usize, 5, 10, 15] {
        let off = i * 8;
        b[off..off + 8].copy_from_slice(&1.0f64.to_le_bytes());
    }
    b
}

/// Serialize the document metadata into a binary NSKeyedArchiver plist blob.
pub fn build_document_archive(doc: &DocumentSpec) -> crate::Result<Vec<u8>> {
    let n = doc.layers.len();
    let total = LAYER_BASE + n * LAYER_STRIDE;
    let mut objects: Vec<Value> = vec![Value::Boolean(false); total];

    // [0] $null sentinel
    objects[0] = Value::String("$null".to_string());

    // [2] document name string
    objects[2] = Value::String(doc.name.clone());

    // [3] size string — Procreate serialises CGSize as "{height, width}" (height first)
    objects[3] = Value::String(format!("{{{}, {}}}", doc.canvas_height, doc.canvas_width));

    // [5] color profile name string (before the dict that references it)
    objects[5] = Value::String(doc.color_profile.clone());

    // [4] color profile dict.
    // SiColorProfileArchiveICCDataKey must be stored as inline Data (not a UID reference) —
    // Procreate uses decodeBytesForKey:returnedLength: which reads inline bytes only.
    if let Some(icc) = icc_data_for_profile(&doc.color_profile) {
        objects[4] = dict([
            ("$class", uid(CLS_VALKYRIE_COLOR_PROFILE)),
            ("SiColorProfileArchiveICCNameKey", uid(5)),
            ("SiColorProfileArchiveICCDataKey", Value::Data(icc.to_vec())),
        ]);
    } else {
        objects[4] = dict([
            ("$class", uid(CLS_VALKYRIE_COLOR_PROFILE)),
            ("SiColorProfileArchiveICCNameKey", uid(5)),
        ]);
    }

    // [6] background color — 16 bytes: four LE f32s (R, G, B, A)
    let mut bg_bytes = Vec::with_capacity(16);
    for &c in &doc.background_color {
        bg_bytes.extend_from_slice(&c.to_le_bytes());
    }
    objects[6] = Value::Data(bg_bytes);

    // [15] composite UUID string
    let composite_uuid = Uuid::new_v4().to_string().to_uppercase();
    objects[COMPOSITE_UUID_IDX] = Value::String(composite_uuid);

    // [16] composite transform: 4×4 identity matrix
    objects[COMPOSITE_TRANSFORM_IDX] = Value::Data(identity_transform());

    // [17] composite contentsRect: zero CGRect (contentsRectValid=false → Procreate ignores it)
    objects[COMPOSITE_CONTENTS_RECT_IDX] = Value::Data(vec![0u8; 32]);

    // [14] composite SilicaLayer dict — pre-composited cache; Procreate regenerates tiles
    objects[COMPOSITE_LAYER_IDX] = dict([
        ("$class", uid(CLS_SILICA_LAYER)),
        ("UUID", uid(COMPOSITE_UUID_IDX)),
        ("name", uid(0)), // $null
        ("opacity", Value::Real(1.0)),
        ("hidden", Value::Boolean(false)),
        ("locked", Value::Boolean(false)),
        ("preserve", Value::Boolean(false)),
        ("clipped", Value::Boolean(false)),
        ("blend", int(0)),
        ("type", int(0)),
        // Same sizeWidth/sizeHeight transposition as regular layers
        ("sizeWidth", int(doc.canvas_height as i64)),
        ("sizeHeight", int(doc.canvas_width as i64)),
        ("document", uid(1)),
        ("version", int(4)),
        ("transform", uid(COMPOSITE_TRANSFORM_IDX)),
        ("contentsRect", uid(COMPOSITE_CONTENTS_RECT_IDX)),
        ("contentsRectValid", Value::Boolean(false)),
        ("mask", uid(0)),
        ("text", uid(0)),
        ("textPDF", uid(0)),
        ("textureSet", uid(0)),
        ("bundledImagePath", uid(0)),
        ("bundledMaskPath", uid(0)),
        ("bundledVideoPath", uid(0)),
        ("extendedBlend", int(0)),
        ("extendedBlend2", int(0)),
        ("perspectiveAssisted", Value::Boolean(false)),
        ("private", Value::Boolean(false)),
        ("animationHeldLength", int(0)),
    ]);

    // [8–12] class descriptors (confirmed names from reverse-engineering real .procreate files)
    objects[CLS_SILICA_DOCUMENT] = class_desc(
        "SilicaDocument",
        &["SilicaDocument", "ValkyrieDocument", "NSObject"],
    );
    objects[CLS_NS_ARRAY] = class_desc("NSArray", &["NSArray", "NSObject"]);
    objects[CLS_SILICA_LAYER] =
        class_desc("SilicaLayer", &["SilicaLayer", "ValkyrieLayer", "NSObject"]);
    objects[CLS_VALKYRIE_COLOR_PROFILE] = class_desc(
        "ValkyrieColorProfile",
        &["ValkyrieColorProfile", "NSObject"],
    );
    objects[CLS_NS_MUTABLE_ARRAY] =
        class_desc("NSMutableArray", &["NSMutableArray", "NSArray", "NSObject"]);

    // Layer objects: dict, UUID string, name string, transform bytes, contentsRect bytes
    let mut layer_uids: Vec<Value> = Vec::with_capacity(n);
    for (i, layer) in doc.layers.iter().enumerate() {
        let base = LAYER_BASE + i * LAYER_STRIDE;
        let uuid_idx = base + 1;
        let name_idx = base + 2;
        let transform_idx = base + 3;
        let contents_rect_idx = base + 4;

        objects[uuid_idx] = Value::String(layer.uuid.clone());
        objects[name_idx] = Value::String(layer.name.clone());
        // 4×4 identity transform (confirmed format from real .procreate files)
        objects[transform_idx] = Value::Data(identity_transform());
        // Zero CGRect with contentsRectValid=false — tells Procreate to render the full layer
        objects[contents_rect_idx] = Value::Data(vec![0u8; 32]);

        objects[base] = dict([
            ("$class", uid(CLS_SILICA_LAYER)),
            ("UUID", uid(uuid_idx)),
            ("name", uid(name_idx)),
            ("opacity", Value::Real(layer.opacity)),
            ("hidden", Value::Boolean(!layer.visible)),
            ("locked", Value::Boolean(layer.locked)),
            ("preserve", Value::Boolean(layer.preserve_alpha)),
            ("clipped", Value::Boolean(layer.clipped)),
            ("blend", int(layer.blend_mode)),
            ("type", int(layer.layer_type)),
            // Procreate quirk: sizeWidth = canvas height (row count × 256),
            // sizeHeight = canvas width (col count × 256) — same transposition as the
            // "{height, width}" size string. Wrong order → wrong tile grid → Metal crash.
            ("sizeWidth", int(layer.height as i64)),
            ("sizeHeight", int(layer.width as i64)),
            // Back-reference to the parent document (nil → crash in Procreate's render queue)
            ("document", uid(1)),
            ("version", int(4)),
            // 4×4 identity transform matrix (nil → crash in Metal renderer)
            ("transform", uid(transform_idx)),
            // Bounding box of layer content; false = render full layer
            ("contentsRect", uid(contents_rect_idx)),
            ("contentsRectValid", Value::Boolean(false)),
            // Null-valued fields expected by the layer model
            ("mask", uid(0)),
            ("text", uid(0)),
            ("textPDF", uid(0)),
            ("textureSet", uid(0)),
            ("bundledImagePath", uid(0)),
            ("bundledMaskPath", uid(0)),
            ("bundledVideoPath", uid(0)),
            ("extendedBlend", int(0)),
            ("extendedBlend2", int(0)),
            ("perspectiveAssisted", Value::Boolean(false)),
            ("private", Value::Boolean(false)),
            ("animationHeldLength", int(0)),
        ]);

        layer_uids.push(uid(base));
    }

    // [7] NSMutableArray for layers list (Procreate uses NSMutableArray, not NSArray)
    let mut arr = Dictionary::new();
    arr.insert("$class".to_string(), uid(CLS_NS_MUTABLE_ARRAY));
    arr.insert("NS.objects".to_string(), Value::Array(layer_uids.clone()));
    objects[7] = Value::Dictionary(arr);

    // [13] unwrappedLayers NSArray — flat list of layers (same UIDs as layers);
    // Procreate's render pipeline uses this for compositing.
    let mut uw_arr = Dictionary::new();
    uw_arr.insert("$class".to_string(), uid(CLS_NS_ARRAY));
    uw_arr.insert("NS.objects".to_string(), Value::Array(layer_uids));
    objects[UNWRAPPED_LAYERS_IDX] = Value::Dictionary(uw_arr);

    // Point selectedLayer / primaryItem at the first layer (or $null if no layers)
    let first_layer_uid = if n > 0 { uid(LAYER_BASE) } else { uid(0) };

    // [1] root document dict
    objects[1] = dict([
        ("$class", uid(CLS_SILICA_DOCUMENT)),
        ("name", uid(2)),
        ("size", uid(3)),
        ("SilicaDocumentArchiveDPIKey", Value::Real(doc.dpi)),
        ("colorProfile", uid(4)),
        ("backgroundColor", uid(6)),
        ("backgroundHidden", Value::Boolean(doc.background_hidden)),
        ("layers", uid(7)),
        ("strokeCount", int(0)),
        // Required by Procreate's loader
        ("tileSize", int(256)),
        ("version", int(2)),
        ("featureSet", int(1)),
        // Layer selection (nil → potential crash in canvas setup)
        ("selectedLayer", first_layer_uid.clone()),
        ("primaryItem", first_layer_uid),
        // Orientation: 4 = landscape-right (matches Procreate's real files)
        ("orientation", int(4)),
        // Flat ordered list of layers used by the Metal compositor
        ("unwrappedLayers", uid(UNWRAPPED_LAYERS_IDX)),
        // Pre-composited cache layer (Procreate regenerates tiles on first open)
        ("composite", uid(COMPOSITE_LAYER_IDX)),
        // No flip transforms
        ("flippedHorizontally", Value::Boolean(false)),
        ("flippedVertically", Value::Boolean(false)),
    ]);

    // Wrap in the NSKeyedArchiver envelope
    let mut top_ref = Dictionary::new();
    top_ref.insert("root".to_string(), uid(1));

    let top = dict([
        ("$version", int(100000)),
        ("$archiver", Value::String("NSKeyedArchiver".to_string())),
        ("$top", Value::Dictionary(top_ref)),
        ("$objects", Value::Array(objects)),
    ]);

    let mut buf = Vec::new();
    plist::to_writer_binary(&mut buf, &top).map_err(ProcreateError::Plist)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::Archive;
    use plist::Value;

    fn minimal_doc() -> DocumentSpec {
        DocumentSpec {
            name: "Test".to_string(),
            canvas_width: 100,
            canvas_height: 200,
            dpi: 72.0,
            color_profile: "sRGB IEC61966-2.1".to_string(),
            background_color: [1.0, 0.5, 0.0, 1.0],
            background_hidden: false,
            layers: vec![LayerSpec {
                uuid: "AAAAAAAA-0000-0000-0000-000000000000".to_string(),
                name: "Base".to_string(),
                opacity: 1.0,
                visible: true,
                locked: false,
                preserve_alpha: false,
                clipped: false,
                blend_mode: 0,
                layer_type: 0,
                width: 100,
                height: 200,
            }],
        }
    }

    #[test]
    fn roundtrip_canvas_size() {
        let doc = minimal_doc();
        let bytes = build_document_archive(&doc).unwrap();
        let archive = Archive::from_bytes(&bytes).unwrap();
        let root = archive.root().unwrap();
        let size_str = archive.get_string(root, "size").unwrap();
        let (w, h) = Archive::parse_size(size_str).unwrap();
        assert_eq!(w, 100);
        assert_eq!(h, 200);
    }

    #[test]
    fn roundtrip_name_and_dpi() {
        let doc = minimal_doc();
        let bytes = build_document_archive(&doc).unwrap();
        let archive = Archive::from_bytes(&bytes).unwrap();
        let root = archive.root().unwrap();
        assert_eq!(archive.get_string(root, "name"), Some("Test"));
        assert_eq!(
            archive.get_f64(root, "SilicaDocumentArchiveDPIKey"),
            Some(72.0)
        );
    }

    #[test]
    fn roundtrip_layer_metadata() {
        let doc = minimal_doc();
        let bytes = build_document_archive(&doc).unwrap();
        let archive = Archive::from_bytes(&bytes).unwrap();
        let root = archive.root().unwrap();

        let layers_obj = archive.get_optional(root, "layers").unwrap();
        let layers = archive.get_array(layers_obj).unwrap();
        assert_eq!(layers.len(), 1);

        let layer = layers[0];
        assert_eq!(
            archive.get_string(layer, "UUID"),
            Some("AAAAAAAA-0000-0000-0000-000000000000")
        );
        assert_eq!(archive.get_string(layer, "name"), Some("Base"));
        assert_eq!(archive.get_f64(layer, "opacity"), Some(1.0));
        assert_eq!(archive.get_bool(layer, "hidden"), Some(false));
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Find the ValkyrieColorProfile dict in a raw $objects array.
    fn find_color_profile(objects: &[Value]) -> Option<&plist::Dictionary> {
        objects.iter().find_map(|obj| {
            let dict = obj.as_dictionary()?;
            let cls_uid = dict.get("$class")?.as_uid()?;
            let cls = objects.get(cls_uid.get() as usize)?.as_dictionary()?;
            let cls_name = cls.get("$classname")?.as_string()?;
            cls_name.contains("Valkyrie").then_some(dict)
        })
    }

    /// Parse the raw $objects array out of a build_document_archive result.
    fn raw_objects(bytes: &[u8]) -> Vec<Value> {
        let root: Value = plist::from_bytes(bytes).unwrap();
        root.into_dictionary()
            .unwrap()
            .remove("$objects")
            .unwrap()
            .into_array()
            .unwrap()
    }

    // ── ICC storage tests ─────────────────────────────────────────────────────

    /// Regression test for the inline-vs-UID bug: SiColorProfileArchiveICCDataKey
    /// must be stored as a raw Data value directly in the dict, not as a UID
    /// reference into $objects. Procreate calls decodeBytesForKey:returnedLength:
    /// which reads inline bytes only; a UID reference causes an ObjC exception.
    #[test]
    fn icc_data_is_inline_not_uid_reference() {
        let bytes = build_document_archive(&minimal_doc()).unwrap();
        let objects = raw_objects(&bytes);
        let cp = find_color_profile(&objects).expect("ValkyrieColorProfile not found");

        let icc_val = cp
            .get("SiColorProfileArchiveICCDataKey")
            .expect("SiColorProfileArchiveICCDataKey missing for known profile");

        assert!(
            icc_val.as_data().is_some(),
            "ICC data must be inline Data, not a UID — Procreate uses \
             decodeBytesForKey:returnedLength: which does not follow UID references"
        );
        assert!(
            icc_val.as_uid().is_none(),
            "ICC data must not be a UID reference"
        );
    }

    #[test]
    fn known_profiles_embed_icc_data() {
        let known = [
            "Display P3",
            "sRGB IEC61966-2.1",
            "sRGB v4 ICC Appearance",
            "sRGB v4 ICC Preference",
            "sRGB v4 ICC Preference Display Class",
        ];

        for profile in known {
            let mut doc = minimal_doc();
            doc.color_profile = profile.to_string();
            let bytes = build_document_archive(&doc).unwrap();
            let objects = raw_objects(&bytes);
            let cp = find_color_profile(&objects)
                .unwrap_or_else(|| panic!("{profile}: ValkyrieColorProfile not found"));

            let icc_val = cp
                .get("SiColorProfileArchiveICCDataKey")
                .unwrap_or_else(|| panic!("{profile}: SiColorProfileArchiveICCDataKey missing"));

            let data = icc_val
                .as_data()
                .unwrap_or_else(|| panic!("{profile}: ICC data is not inline Data"));

            assert!(!data.is_empty(), "{profile}: ICC data is empty");
        }
    }

    #[test]
    fn unknown_profile_has_no_icc_data_key() {
        let mut doc = minimal_doc();
        doc.color_profile = "Unknown Exotic Profile".to_string();
        let bytes = build_document_archive(&doc).unwrap();
        let objects = raw_objects(&bytes);
        let cp = find_color_profile(&objects).expect("ValkyrieColorProfile not found");

        assert!(
            cp.get("SiColorProfileArchiveICCDataKey").is_none(),
            "unknown profile should not have an ICC data key"
        );
        assert_eq!(
            cp.get("SiColorProfileArchiveICCNameKey")
                .and_then(|v| {
                    // name may be inline or a UID; resolve if needed
                    if let Some(uid) = v.as_uid() {
                        objects
                            .get(uid.get() as usize)?
                            .as_string()
                            .map(|s| s.to_string())
                    } else {
                        v.as_string().map(|s| s.to_string())
                    }
                })
                .as_deref(),
            Some("Unknown Exotic Profile"),
            "profile name should still be stored even when ICC data is unavailable"
        );
    }

    // ── Structural invariant tests ────────────────────────────────────────────

    #[test]
    fn archive_has_composite_layer() {
        let bytes = build_document_archive(&minimal_doc()).unwrap();
        let archive = Archive::from_bytes(&bytes).unwrap();
        let root = archive.root().unwrap();

        let composite = archive
            .get_optional(root, "composite")
            .expect("composite key must be present and non-null");

        // Must be a SilicaLayer dict with a UUID
        assert!(
            archive.get_string(composite, "UUID").is_some(),
            "composite must be a SilicaLayer with a UUID"
        );
    }

    #[test]
    fn archive_has_unwrapped_layers_matching_layers() {
        let bytes = build_document_archive(&minimal_doc()).unwrap();
        let archive = Archive::from_bytes(&bytes).unwrap();
        let root = archive.root().unwrap();

        let layers_obj = archive.get_optional(root, "layers").unwrap();
        let layers = archive.get_array(layers_obj).unwrap();

        let uw_obj = archive
            .get_optional(root, "unwrappedLayers")
            .expect("unwrappedLayers must be present and non-null");
        let uw_layers = archive.get_array(uw_obj).unwrap();

        assert_eq!(
            layers.len(),
            uw_layers.len(),
            "unwrappedLayers must contain the same number of entries as layers"
        );

        for (i, (l, uw)) in layers.iter().zip(uw_layers.iter()).enumerate() {
            let l_uuid = archive.get_string(l, "UUID");
            let uw_uuid = archive.get_string(uw, "UUID");
            assert_eq!(
                l_uuid, uw_uuid,
                "layer[{i}] UUID mismatch between layers and unwrappedLayers"
            );
        }
    }
}
