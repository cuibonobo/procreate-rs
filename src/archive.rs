//! Resolves NSKeyedArchiver object graphs from binary plists.
//!
//! NSKeyedArchiver wraps a standard plist with:
//!   $version, $archiver, $top, $objects
//!
//! Objects reference each other via UID values (indices into $objects).
//! This module provides a thin resolver so callers can navigate the
//! graph without manually chasing UIDs.

use plist::Value;
use crate::{ProcreateError, Result};

pub struct Archive {
    objects: Vec<Value>,
}

impl Archive {
    /// Parse a binary plist NSKeyedArchiver blob.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let root: Value = plist::from_bytes(bytes)
            .map_err(ProcreateError::Plist)?;

        let dict = root.as_dictionary()
            .ok_or_else(|| ProcreateError::InvalidDocument("root is not a dict".into()))?;

        let objects = dict
            .get("$objects")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ProcreateError::InvalidDocument("missing $objects".into()))?
            .clone();

        Ok(Self { objects })
    }

    /// Resolve a UID to the object it points to.
    pub fn resolve(&self, uid: &Value) -> Option<&Value> {
        let idx = uid.as_uid()?.get() as usize;
        self.objects.get(idx)
    }

    /// Get the top-level document object (always index 1 in Procreate files).
    pub fn root(&self) -> Option<&Value> {
        self.objects.get(1)
    }

    /// Resolve a key in a dict-type object, following UID references.
    pub fn get<'a>(&'a self, obj: &'a Value, key: &str) -> Option<&'a Value> {
        let dict = obj.as_dictionary()?;
        let val = dict.get(key)?;
        // If the value is a UID, resolve it; otherwise return as-is
        if val.as_uid().is_some() {
            self.resolve(val)
        } else {
            Some(val)
        }
    }

    /// Resolve a UID key, returning None if it resolves to $null.
    pub fn get_optional<'a>(&'a self, obj: &'a Value, key: &str) -> Option<&'a Value> {
        let resolved = self.get(obj, key)?;
        // $null is stored as the string "$null" at index 0
        if resolved.as_string() == Some("$null") {
            None
        } else {
            Some(resolved)
        }
    }

    /// Get a string value, resolving UIDs if necessary.
    pub fn get_string<'a>(&'a self, obj: &'a Value, key: &str) -> Option<&'a str> {
        self.get(obj, key)?.as_string()
    }

    /// Get an f64 value (stored as real or integer in plist).
    pub fn get_f64(&self, obj: &Value, key: &str) -> Option<f64> {
        let v = self.get(obj, key)?;
        v.as_real().or_else(|| v.as_signed_integer().map(|i| i as f64))
    }

    /// Get a bool value.
    pub fn get_bool(&self, obj: &Value, key: &str) -> Option<bool> {
        self.get(obj, key)?.as_boolean()
    }

    /// Get an i64 value.
    pub fn get_i64(&self, obj: &Value, key: &str) -> Option<i64> {
        self.get(obj, key)?.as_signed_integer()
    }

    /// Resolve an NS.objects array, returning the resolved values.
    pub fn get_array<'a>(&'a self, obj: &'a Value) -> Option<Vec<&'a Value>> {
        let dict = obj.as_dictionary()?;
        let ns_objects = dict.get("NS.objects")?.as_array()?;
        Some(
            ns_objects
                .iter()
                .filter_map(|uid| self.resolve(uid))
                .collect(),
        )
    }

    /// Parse a Procreate size string like "{1920, 1080}" into (width, height).
    ///
    /// Procreate serialises CGSize as `{height, width}` (height first).
    pub fn parse_size(s: &str) -> Option<(u32, u32)> {
        let s = s.trim().trim_start_matches('{').trim_end_matches('}');
        let mut parts = s.splitn(2, ',');
        let h: u32 = parts.next()?.trim().parse().ok()?;
        let w: u32 = parts.next()?.trim().parse().ok()?;
        Some((w, h))
    }

    /// Decode a contentsRect binary blob: 4 little-endian f64s (x, y, w, h).
    /// These are pixel coordinates within the full canvas.
    pub fn decode_rect(bytes: &[u8]) -> Option<[f64; 4]> {
        if bytes.len() < 32 {
            return None;
        }
        let x = f64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let y = f64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let w = f64::from_le_bytes(bytes[16..24].try_into().ok()?);
        let h = f64::from_le_bytes(bytes[24..32].try_into().ok()?);
        Some([x, y, w, h])
    }

    /// Decode a transform binary blob: 16 little-endian f64s (4x4 matrix, row-major).
    pub fn decode_transform(bytes: &[u8]) -> Option<[f64; 16]> {
        if bytes.len() < 128 {
            return None;
        }
        let mut out = [0f64; 16];
        for i in 0..16 {
            out[i] = f64::from_le_bytes(bytes[i*8..(i+1)*8].try_into().ok()?);
        }
        Some(out)
    }

    /// Decode a background color blob: 4 little-endian f32s (r, g, b, a).
    pub fn decode_color_f32(bytes: &[u8]) -> Option<[f32; 4]> {
        if bytes.len() < 16 {
            return None;
        }
        let r = f32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let g = f32::from_le_bytes(bytes[4..8].try_into().ok()?);
        let b = f32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let a = f32::from_le_bytes(bytes[12..16].try_into().ok()?);
        Some([r, g, b, a])
    }
}
