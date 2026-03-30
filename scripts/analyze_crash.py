#!/usr/bin/env python3
"""
analyze_crash.py — extract the signal, faulting thread, and key stack frames
from a Procreate iOS crash report (.ips file).

Usage:
    python3 scripts/analyze_crash.py <Procreate-*.ips> [<Procreate-*.ips> ...]

Prints a concise summary of each crash:
  - Timestamp, Procreate version, OS version, device
  - Exception type and signal
  - Faulting thread queue
  - Condensed stack (abort chain → highest-level Procreate frame → first non-Procreate frame)
  - asi messages (e.g. "abort() called")

Pass multiple .ips files to compare crash progressions.
"""

import sys
import json
from pathlib import Path


KNOWN_IMAGES = {
    # imageIndex: short name (filled dynamically from usedImages)
}


def load_ips(path: str) -> dict:
    """
    .ips files contain two JSON documents separated by a newline:
      Line 1: single-line metadata header  {"app_name":..., "bug_type":...}
      Rest:   multi-line crash report JSON (threads, frames, etc.)
    """
    text = Path(path).read_text(errors="replace")
    # Split into two JSON blobs at the first newline that starts a new object
    first_newline = text.find("\n")
    if first_newline != -1:
        remainder = text[first_newline:].strip()
        if remainder.startswith("{"):
            try:
                return json.loads(remainder)
            except json.JSONDecodeError:
                pass
    # Fallback: the whole text might be one JSON object
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        pass
    raise ValueError(f"Could not parse {path} as JSON")


def image_name(used_images: list, index: int) -> str:
    for img in used_images:
        if img.get("imageIndex") == index or img.get("id") == index:
            p = img.get("name") or img.get("path") or ""
            return Path(p).name or p
    return f"image[{index}]"


def summarise_frame(frame: dict, used_images: list) -> str:
    sym = frame.get("symbol", "")
    sym_loc = frame.get("symbolLocation", 0)
    img_idx = frame.get("imageIndex", -1)
    img_off = frame.get("imageOffset", 0)
    img = image_name(used_images, img_idx)

    if sym:
        loc = f"+{sym_loc}" if sym_loc else ""
        return f"{img}  {sym}{loc}"
    else:
        return f"{img}  +{img_off:#x}"


def analyze(path: str):
    crash = load_ips(path)
    used_images = crash.get("usedImages", [])

    # Header metadata
    ts = crash.get("captureTime", "?")
    proc_name = crash.get("procName", "Procreate")
    bundle_info = crash.get("bundleInfo", {})
    version = bundle_info.get("CFBundleShortVersionString", "?")
    build = bundle_info.get("CFBundleVersion", "?")
    os_info = crash.get("osVersion", {})
    os_ver = os_info.get("train", "?") if isinstance(os_info, dict) else str(os_info)
    model = crash.get("modelCode", "?")

    exc = crash.get("exception", {})
    exc_type = exc.get("type", "?")
    exc_signal = exc.get("signal", "?")
    termination = crash.get("termination", {})
    term_indicator = termination.get("indicator", "")

    asi_messages = []
    for lib, msgs in (crash.get("asi") or {}).items():
        for m in (msgs if isinstance(msgs, list) else [msgs]):
            asi_messages.append(f"{lib}: {m}")

    faulting_idx = crash.get("faultingThread", 0)
    threads = crash.get("threads", [])

    print(f"\n{'='*60}")
    print(f"File:      {Path(path).name}")
    print(f"Time:      {ts}")
    print(f"App:       {proc_name} {version} ({build})")
    print(f"OS:        {os_ver}  device={model}")
    print(f"Exception: {exc_type} / {exc_signal}  —  {term_indicator}")
    if asi_messages:
        print(f"Message:   {'; '.join(asi_messages)}")

    # Find faulting thread
    faulting_thread = None
    for t in threads:
        if t.get("id") == faulting_idx or t.get("triggered"):
            faulting_thread = t
            break
    if faulting_thread is None and threads:
        faulting_thread = threads[0]

    if faulting_thread:
        queue = faulting_thread.get("queue", "?")
        frames = faulting_thread.get("frames", [])
        print(f"Thread:    {queue}  ({len(frames)} frames)")
        print("Stack:")
        for i, frame in enumerate(frames):
            summary = summarise_frame(frame, used_images)
            # Highlight Procreate frames vs system frames
            img_idx = frame.get("imageIndex", -1)
            img = image_name(used_images, img_idx)
            marker = "  >>" if "Procreate" in img or img_idx == 4 else "    "
            print(f"  {i:2d}{marker} {summary}")

    # Also show lastExceptionBacktrace if present
    leb = crash.get("lastExceptionBacktrace")
    if leb:
        print("Last exception backtrace:")
        for i, frame in enumerate(leb):
            print(f"  {i:2d}    {summarise_frame(frame, used_images)}")


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    for path in sys.argv[1:]:
        try:
            analyze(path)
        except Exception as e:
            print(f"\n[ERROR] {path}: {e}")

    print()


if __name__ == "__main__":
    main()
