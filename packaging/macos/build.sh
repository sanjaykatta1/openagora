#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
version=$(python3 packaging/version.py)
notarize=${MACOS_NOTARIZE:-true}
case "$notarize" in true|false) ;; *) echo 'MACOS_NOTARIZE must be true or false' >&2; exit 1 ;; esac
out=${2:-dist}
mkdir -p "$out"
out=$(cd "$out" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/openagora-pkg.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/root/usr/local/bin" "$work/resources"
if [[ -n ${1:-} ]]; then
  arm="$1/aarch64-apple-darwin/openagora"
  intel="$1/x86_64-apple-darwin/openagora"
else
  rustup target add --toolchain stable aarch64-apple-darwin x86_64-apple-darwin
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    RUSTC=$(rustup which --toolchain stable rustc) rustup run stable cargo build --manifest-path engine/Cargo.toml --release --locked --target "$target"
  done
  arm=engine/target/aarch64-apple-darwin/release/openagora
  intel=engine/target/x86_64-apple-darwin/release/openagora
fi
binary="$work/root/usr/local/bin/openagora"
lipo -create "$arm" "$intel" -output "$binary"
archs=$(lipo -archs "$binary")
[[ " $archs " == *" arm64 "* && " $archs " == *" x86_64 "* ]] || { echo "Missing universal architecture" >&2; exit 1; }
lipo -info "$binary"
chmod 755 "$binary"
signed=false
if [[ -n ${APPLE_TEAM_ID:-} ]]; then
  # Resolve only identities belonging to the configured team, without printing identifiers.
  identity() {
    python3 - "$1" <<'PY'
import os, re, subprocess, sys
out = subprocess.check_output(['security', 'find-identity', '-v'], text=True)
pattern = r'([0-9A-F]{40}) "Developer ID ' + re.escape(sys.argv[1]) + r': [^\n]+ \(' + re.escape(os.environ['APPLE_TEAM_ID']) + r'\)"'
found = re.findall(pattern, out)
if len(found) != 1:
    raise SystemExit('Expected exactly one valid Developer ID ' + sys.argv[1] + ' identity for APPLE_TEAM_ID')
print(found[0])
PY
  }
  app_identity=$(identity Application)
  installer_identity=$(identity Installer)
  codesign --force --sign "$app_identity" --options runtime --timestamp "$binary"
  codesign --verify --strict --verbose=2 "$binary"
  signed=true
  sign_args=(--sign "$installer_identity" --timestamp)
  if [[ "$notarize" == true && -z ${NOTARY_PROFILE:-} && ( -z ${NOTARY_KEY_PATH:-} || -z ${NOTARY_KEY_ID:-} || -z ${NOTARY_ISSUER_ID:-} ) ]]; then
    echo 'Signing requires NOTARY_PROFILE or NOTARY_KEY_PATH, NOTARY_KEY_ID, NOTARY_ISSUER_ID' >&2
    exit 1
  fi
else
  echo 'Building UNSIGNED macOS package: signing secrets / APPLE_TEAM_ID are absent.'
fi
pkgbuild --root "$work/root" --identifier app.openagora.cli --version "$version" \
  --install-location / "$work/component.pkg"
cp packaging/macos/Welcome.html "$work/resources/Welcome.html"
cp LICENSE "$work/resources/License.txt"
cat > "$work/Distribution.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>OpenAgora $version</title>
  <welcome file="Welcome.html" mime-type="text/html" />
  <license file="License.txt" mime-type="text/plain" />
  <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64" />
  <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true" />
  <choices-outline><line choice="default" /></choices-outline>
  <choice id="default" visible="false"><pkg-ref id="app.openagora.cli" /></choice>
  <pkg-ref id="app.openagora.cli" version="$version" onConclusion="none">component.pkg</pkg-ref>
</installer-gui-script>
XML
pkg="$out/OpenAgora-CLI-$version.pkg"
# Bash 3 (macOS) treats empty arrays as unset under nounset.
if [[ "$signed" == true ]]; then
  productbuild --distribution "$work/Distribution.xml" --resources "$work/resources" \
    --package-path "$work" "${sign_args[@]}" "$pkg"
  pkgutil --check-signature "$pkg"
  if [[ "$notarize" == true ]]; then
    if [[ -n ${NOTARY_PROFILE:-} ]]; then
      xcrun notarytool submit "$pkg" --keychain-profile "$NOTARY_PROFILE" --wait --output-format json > "$work/notary.json"
    else
      xcrun notarytool submit "$pkg" --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" \
        --issuer "$NOTARY_ISSUER_ID" --wait --output-format json > "$work/notary.json"
    fi
    python3 - "$work/notary.json" <<'PY'
import json, sys
result = json.load(open(sys.argv[1]))
if result.get('status') != 'Accepted':
    raise SystemExit('Notarization was not accepted; submission ID: ' + str(result.get('id')))
print('Notarization accepted.')
PY
    xcrun stapler staple "$pkg"
    xcrun stapler validate "$pkg"
    spctl --assess --type install -vv "$pkg"
  else
    echo 'Signed PR test package: notarization is deferred to release/rehearsal builds.'
  fi
else
  productbuild --distribution "$work/Distribution.xml" --resources "$work/resources" \
    --package-path "$work" "$pkg"
fi
python3 packaging/checksums.py "$pkg"
