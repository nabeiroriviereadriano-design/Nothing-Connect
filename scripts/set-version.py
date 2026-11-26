#!/usr/bin/env python3
"""Change la version partout (Windows, Android, Rust) : python3 scripts/set-version.py 0.2.0"""
import json, pathlib, re, sys

v = sys.argv[1] if len(sys.argv) > 1 else sys.exit("usage : set-version.py X.Y.Z")
if not re.fullmatch(r"\d+\.\d+\.\d+", v):
    sys.exit("format attendu : X.Y.Z")
root = pathlib.Path(__file__).resolve().parent.parent
cargo = root / "Cargo.toml"
cargo.write_text(re.sub(r'(\[workspace\.package\]\nversion = ")[^"]+"', rf'\g<1>{v}"', cargo.read_text()), encoding="utf-8")
pkg = root / "apps/windows/package.json"
data = json.loads(pkg.read_text())
data["version"] = v
pkg.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(f"Version {v} (Android lit la même valeur ; versionCode calculé automatiquement).")
