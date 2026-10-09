#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
arch=${1:?Usage: build.sh amd64|arm64 [static-musl-binary] [output-dir]}
case "$arch" in
  amd64) target=x86_64-unknown-linux-musl; native=x86_64 ;;
  arm64) target=aarch64-unknown-linux-musl; native=aarch64 ;;
  *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
esac
command -v nfpm >/dev/null || { echo 'Install nfpm 2.47.0 (see packaging/README.md)' >&2; exit 1; }
export PACKAGE_VERSION PACKAGE_ARCH PACKAGE_BINARY
PACKAGE_VERSION=$(python3 packaging/version.py)
PACKAGE_ARCH=$arch
if [[ $# -lt 2 ]]; then
  rustup target add --toolchain stable "$target"
  rustup run stable cargo build --manifest-path engine/Cargo.toml --locked --release --target "$target"
fi
PACKAGE_BINARY=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${2:-engine/target/$target/release/openagora}")
[[ -f "$PACKAGE_BINARY" ]] || { echo "Missing binary: $PACKAGE_BINARY" >&2; exit 1; }
# Reject accidentally packaging a dynamically linked executable or the wrong CPU.
python3 - "$PACKAGE_BINARY" "$arch" <<'PY'
import struct, sys
with open(sys.argv[1], 'rb') as f:
    h = f.read(64)
    assert h[:6] == b'\x7fELF\x02\x01', 'Expected a little-endian 64-bit ELF binary'
    assert struct.unpack_from('<H', h, 18)[0] == {'amd64': 62, 'arm64': 183}[sys.argv[2]], 'Wrong ELF architecture'
    offset = struct.unpack_from('<Q', h, 32)[0]
    size, count = struct.unpack_from('<HH', h, 54)
    for i in range(count):
        f.seek(offset + i * size)
        assert struct.unpack('<I', f.read(4))[0] != 3, 'Binary has a runtime interpreter; build static musl'
PY
out=${3:-dist}
mkdir -p "$out"
for format in deb rpm archlinux apk; do
  case "$format" in
    deb) name="openagora_${PACKAGE_VERSION}-1_${arch}.deb" ;;
    rpm) name="openagora-${PACKAGE_VERSION}-1.${native}.rpm" ;;
    archlinux) name="openagora-${PACKAGE_VERSION}-1-${native}.pkg.tar.zst" ;;
    apk) name="openagora-${PACKAGE_VERSION}-r1.${native}.apk" ;;
  esac
  nfpm package --config packaging/linux/nfpm.yaml --packager "$format" --target "$out/$name"
  python3 packaging/checksums.py "$out/$name"
done
