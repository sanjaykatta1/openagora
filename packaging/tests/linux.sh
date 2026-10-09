#!/usr/bin/env bash
# Run on a native Linux runner. Every invocation gets a new disposable container.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
image=${1:?Usage: linux.sh image amd64|arm64 package}
arch=${2:?Missing architecture}
package=$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).resolve())' "${3:?Missing package}")
version=$(python3 "$ROOT/packaging/version.py")
docker run --rm --platform "linux/$arch" \
  -e EXPECTED_VERSION="$version" -e EXPECTED_ARCH="$arch" \
  -v "$(dirname "$package"):/packages:ro" \
  -v "$ROOT/packaging/tests/linux-container.sh:/test.sh:ro" \
  "$image" sh /test.sh "/packages/$(basename "$package")"
