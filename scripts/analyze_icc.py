#!/usr/bin/env python3
"""
analyze_icc.py — inspect the ICC color profile embedded in one or more .procreate files.

Usage:
    python3 scripts/analyze_icc.py <file.procreate> [<file.procreate> ...]
    python3 scripts/analyze_icc.py --extract <file.procreate> <out.icc>

With --extract: saves the raw ICC bytes from the first .procreate file to <out.icc>.

For each file prints:
  - SiColorProfileArchiveICCNameKey  (the display name string)
  - ICC data size, profile class, and embedded description tag
  - Whether name key matches any known embedded desc string
"""

import sys
import zipfile
import plistlib
from pathlib import Path


def resolve(objs, v):
    if isinstance(v, plistlib.UID):
        return objs[v.data]
    return v


def find_color_profile(path: str):
    """Return (name, icc_bytes_or_None) from a .procreate file."""
    with zipfile.ZipFile(path) as z:
        raw = z.read("Document.archive")
    pl = plistlib.loads(raw)
    objs = pl["$objects"]

    for obj in objs:
        if not isinstance(obj, dict):
            continue
        cls_uid = obj.get("$class")
        if cls_uid is None:
            continue
        cls = resolve(objs, cls_uid)
        if not (isinstance(cls, dict) and "Valkyrie" in cls.get("$classname", "")):
            continue
        name_uid = obj.get("SiColorProfileArchiveICCNameKey")
        data_uid = obj.get("SiColorProfileArchiveICCDataKey")
        name = resolve(objs, name_uid) if name_uid else None
        data = resolve(objs, data_uid) if data_uid else None
        return name, data

    return None, None


def icc_desc(data: bytes) -> str:
    """Extract the human-readable description from an ICC profile."""
    if len(data) < 132:
        return "<too short>"
    tag_count = int.from_bytes(data[128:132], "big")
    for i in range(tag_count):
        off = 132 + i * 12
        if off + 12 > len(data):
            break
        tag_sig = data[off : off + 4].decode("latin1", errors="replace")
        if tag_sig != "desc":
            continue
        tag_offset = int.from_bytes(data[off + 4 : off + 8], "big")
        tag_size = int.from_bytes(data[off + 8 : off + 12], "big")
        if tag_offset + tag_size > len(data):
            return "<truncated>"
        desc_data = data[tag_offset : tag_offset + tag_size]
        type_sig = desc_data[0:4].decode("latin1", errors="replace")
        if type_sig == "mluc":
            # multiLocalizedUnicodeType: UTF-16BE record
            if len(desc_data) < 28:
                return "<mluc short>"
            str_len = int.from_bytes(desc_data[20:24], "big")
            str_off = int.from_bytes(desc_data[24:28], "big")
            start = 16 + str_off
            return desc_data[start : start + str_len].decode("utf-16-be", errors="replace")
        else:
            # old-style textDescriptionType
            str_len = int.from_bytes(desc_data[8:12], "big")
            return desc_data[12 : 12 + str_len].rstrip(b"\x00").decode("latin1", errors="replace")
    return "<no desc tag>"


def analyze(path: str):
    name, data = find_color_profile(path)
    print(f"\n{'='*60}")
    print(f"File:      {Path(path).name}")
    print(f"Name key:  {name!r}")

    if data is None:
        print("ICC data:  NONE (Procreate will use sRGB fallback)")
        return

    profile_class = data[12:16].decode("latin1", errors="replace") if len(data) >= 16 else "?"
    color_space = data[16:20].decode("latin1", errors="replace") if len(data) >= 20 else "?"
    desc = icc_desc(data)

    print(f"ICC data:  {len(data)} bytes")
    print(f"  class:   {profile_class!r}  ({'display' if profile_class == 'mntr' else 'non-display' if profile_class == 'spac' else profile_class})")
    print(f"  space:   {color_space!r}")
    print(f"  desc:    {desc!r}")

    if name and desc == name:
        print(f"  match:   desc == name key ✓")
    elif name:
        print(f"  match:   desc != name key (Procreate ignores this)")


def extract(src: str, dst: str):
    _, data = find_color_profile(src)
    if data is None:
        print(f"No ICC data found in {src}", file=sys.stderr)
        sys.exit(1)
    Path(dst).write_bytes(data)
    print(f"Saved {len(data)} bytes → {dst}")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    if sys.argv[1] == "--extract":
        if len(sys.argv) != 4:
            print("Usage: analyze_icc.py --extract <file.procreate> <out.icc>")
            sys.exit(1)
        extract(sys.argv[2], sys.argv[3])
        return

    for path in sys.argv[1:]:
        try:
            analyze(path)
        except Exception as e:
            print(f"\n[ERROR] {path}: {e}")

    print()


if __name__ == "__main__":
    main()
