#!/usr/bin/env python3
"""
compare_procreate.py — diff two .procreate files' Document.archive metadata.

Usage:
    python3 scripts/compare_procreate.py <file_a.procreate> <file_b.procreate>

Prints:
  - Document-level field differences
  - Per-layer field differences (first layer only, then summary)
  - Tile layout for each layer UUID
  - Decoded values for key binary fields (contentsRect, transform, sizeWidth/sizeHeight)
"""

import sys
import zipfile
import plistlib
import struct
import json
from pathlib import Path


def load_archive(path: str):
    with zipfile.ZipFile(path) as z:
        raw = z.read("Document.archive")
        names = z.namelist()
    pl = plistlib.loads(raw)
    return pl["$objects"], names


def resolve(objs, v):
    if isinstance(v, plistlib.UID):
        return objs[v.data]
    return v


def resolve_deep(objs, v, depth=0):
    """Resolve a UID and summarise the result as a comparable string."""
    if depth > 3:
        return "<...>"
    r = resolve(objs, v)
    if isinstance(r, str):
        return r
    if isinstance(r, bool):
        return r
    if isinstance(r, int):
        return r
    if isinstance(r, float):
        return r
    if isinstance(r, bytes):
        if len(r) == 32:
            try:
                return ("CGRect(f64x4)", struct.unpack("<4d", r))
            except Exception:
                pass
        if len(r) == 128:
            try:
                m = struct.unpack("<16d", r)
                diag = (m[0], m[5], m[10], m[15])
                return ("mat4x4_diag", diag)
            except Exception:
                pass
        if len(r) == 16:
            try:
                return ("rgba_f32", struct.unpack("<4f", r))
            except Exception:
                pass
        return f"<bytes[{len(r)}] {r[:8].hex()}...>"
    if isinstance(r, dict):
        cls_uid = r.get("$class")
        if cls_uid is not None:
            cls = resolve(objs, cls_uid)
            cls_name = cls.get("$classname", "?") if isinstance(cls, dict) else str(cls)
        else:
            cls_name = None
        keys = sorted(k for k in r.keys() if k != "$class")
        if cls_name:
            return f"<{cls_name} keys={keys}>"
        return f"<dict keys={keys}>"
    if isinstance(r, list):
        return f"<array[{len(r)}]>"
    return repr(r)


def layer_tiles(names, uuid):
    prefix = uuid + "/"
    tiles = sorted(n[len(prefix):].rstrip(".lz4") for n in names if n.startswith(prefix))
    return tiles


def describe_layer(objs, layer_uid, names):
    layer = resolve(objs, layer_uid)
    if not isinstance(layer, dict):
        return {}
    result = {}
    for k, v in layer.items():
        if k == "$class":
            continue
        result[k] = resolve_deep(objs, v)
    uuid_val = result.get("UUID", "")
    if isinstance(uuid_val, str):
        result["__tiles__"] = layer_tiles(names, uuid_val)
    return result


def describe_doc(objs, names):
    root = objs[1]
    doc = {}
    for k, v in root.items():
        if k == "$class":
            continue
        doc[k] = resolve_deep(objs, v)

    # Expand layers
    layers_ref = root.get("layers")
    if layers_ref is not None:
        arr = resolve(objs, layers_ref)
        if isinstance(arr, dict):
            items = arr.get("NS.objects", [])
            doc["__layers__"] = [describe_layer(objs, lu, names) for lu in items]

    return doc


def diff_dicts(label_a, a, label_b, b, prefix=""):
    all_keys = sorted(set(a.keys()) | set(b.keys()))
    diffs = []
    for k in all_keys:
        if k.startswith("__"):
            continue
        full_key = f"{prefix}{k}"
        if k not in a:
            diffs.append(f"  {full_key}: only in {label_b} → {b[k]!r}")
        elif k not in b:
            diffs.append(f"  {full_key}: only in {label_a} → {a[k]!r}")
        elif a[k] != b[k]:
            diffs.append(f"  {full_key}:")
            diffs.append(f"    {label_a}: {a[k]!r}")
            diffs.append(f"    {label_b}: {b[k]!r}")
    return diffs


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    path_a, path_b = sys.argv[1], sys.argv[2]
    label_a = Path(path_a).name
    label_b = Path(path_b).name

    objs_a, names_a = load_archive(path_a)
    objs_b, names_b = load_archive(path_b)

    doc_a = describe_doc(objs_a, names_a)
    doc_b = describe_doc(objs_b, names_b)

    # --- Document-level diff ---
    print(f"=== Document fields: {label_a} vs {label_b} ===")
    diffs = diff_dicts(label_a, doc_a, label_b, doc_b)
    if diffs:
        for d in diffs:
            print(d)
    else:
        print("  (identical)")

    # --- Layer-level diff ---
    layers_a = doc_a.get("__layers__", [])
    layers_b = doc_b.get("__layers__", [])
    count = max(len(layers_a), len(layers_b))
    print(f"\n=== Layers ({len(layers_a)} in {label_a}, {len(layers_b)} in {label_b}) ===")
    for i in range(count):
        la = layers_a[i] if i < len(layers_a) else {}
        lb = layers_b[i] if i < len(layers_b) else {}
        name_a = la.get("name", "?")
        name_b = lb.get("name", "?")
        header = f"Layer {i}"
        if name_a == name_b:
            header += f": {name_a!r}"
        else:
            header += f": {label_a}={name_a!r} / {label_b}={name_b!r}"
        print(f"\n  {header}")

        layer_diffs = diff_dicts(label_a, la, label_b, lb)
        if layer_diffs:
            for d in layer_diffs:
                print(f"  {d.strip()}")
        else:
            print("    (identical)")

        tiles_a = la.get("__tiles__", [])
        tiles_b = lb.get("__tiles__", [])
        if tiles_a != tiles_b:
            print(f"    tiles {label_a}: {tiles_a}")
            print(f"    tiles {label_b}: {tiles_b}")
        else:
            print(f"    tiles: {tiles_a}")

    # --- Tile UUID summary ---
    lz4_a = sorted(n for n in names_a if n.endswith(".lz4"))
    lz4_b = sorted(n for n in names_b if n.endswith(".lz4"))
    # Normalise: compare by row~col within each UUID bucket, ignoring UUID differences
    def tile_coords(names):
        coords = set()
        for n in names:
            parts = n.split("/")
            if len(parts) == 2:
                coords.add(parts[1])
        return coords
    coords_a = tile_coords(lz4_a)
    coords_b = tile_coords(lz4_b)
    only_a_coords = coords_a - coords_b
    only_b_coords = coords_b - coords_a
    print(f"\n=== Tiles: {len(lz4_a)} in {label_a}, {len(lz4_b)} in {label_b} ===")
    if only_a_coords:
        print(f"  row~col only in {label_a}: {sorted(only_a_coords)}")
    if only_b_coords:
        print(f"  row~col only in {label_b}: {sorted(only_b_coords)}")
    if not only_a_coords and not only_b_coords:
        print("  (same row~col set)")


if __name__ == "__main__":
    main()
