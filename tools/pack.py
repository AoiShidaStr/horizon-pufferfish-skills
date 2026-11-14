#!/usr/bin/env python3
"""Pack mods/pufferfish_skills into `target/pufferfish_skills.hnmod` and `.hmod`.

The format is docs/api/mod-files.md's: a zip whose top-level entries are the native manifest and
the bundle folder. A `.hmod` holding native entries is read as a native mod too (`kind_of` asks
`horizon_mods::index::native::holds` first), so both files are the same bytes: the `.hnmod` is the
code-only file older launchers know, the `.hmod` the combined one. `python tools/pack.py [--out dir]`
writes `dir/pufferfish_skills.hnmod` and `dir/pufferfish_skills.hmod` (default `target`).
"""

import argparse
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
ID = "pufferfish_skills"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default="target")
    args = parser.parse_args()
    manifest_path = ROOT / "horizon-native.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest["id"] != ID:
        raise SystemExit(f"horizon-native.json names {manifest['id']}, not {ID}")
    bundle = ROOT / "mixins" / ID
    if not (bundle / "horizon-mixin.toml").is_file():
        raise SystemExit(f"{bundle} holds no horizon-mixin.toml")
    out_dir = ROOT / args.out
    out_dir.mkdir(parents=True, exist_ok=True)
    entries = [(manifest_path.relative_to(ROOT), manifest_path)]
    for path in sorted(bundle.rglob("*")):
        if path.is_file():
            entries.append((path.relative_to(ROOT), path))
    for extension in ("hnmod", "hmod"):
        out = out_dir / f"{ID}.{extension}"
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, path in entries:
                archive.write(path, str(name).replace("\\", "/"))
        print(f"{out} ({out.stat().st_size} bytes, {len(entries)} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
