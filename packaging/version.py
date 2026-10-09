#!/usr/bin/env python3
"""Read and validate the one version used by every native installer."""
import pathlib
import re
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
version = tomllib.loads((ROOT / 'engine/Cargo.toml').read_text())['package']['version']
if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
    raise SystemExit('Native installers require a numeric major.minor.patch Cargo version')
major, minor, patch = map(int, version.split('.'))
if major > 255 or minor > 255 or patch > 65535:
    raise SystemExit('Cargo version exceeds Windows Installer version limits')
print(version)
