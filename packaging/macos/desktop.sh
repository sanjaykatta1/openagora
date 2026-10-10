#!/usr/bin/env bash
# Called inside ci-build.sh's temporary-keychain lifetime.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
arch=${1:?Usage: desktop.sh arm64|x64}
case "$arch" in arm64|x64) ;; *) echo 'Unsupported desktop architecture' >&2; exit 1 ;; esac
version=$(python3 "$ROOT/packaging/version.py")
cd "$ROOT/desktop"
notarize=${MACOS_NOTARIZE:-true}
case "$notarize" in true|false) ;; *) echo 'MACOS_NOTARIZE must be true or false' >&2; exit 1 ;; esac
signed=false
if [[ -n ${APPLE_TEAM_ID:-} ]]; then
  signed=true
  export CSC_IDENTITY_AUTO_DISCOVERY=true
  args=(-c.forceCodeSigning=true -c.mac.hardenedRuntime=true -c.dmg.sign=true)
  if [[ "$notarize" == true ]]; then
    export APPLE_API_KEY=${NOTARY_KEY_PATH:?Missing notarization key path}
    export APPLE_API_KEY_ID=${NOTARY_KEY_ID:?Missing notarization key ID}
    export APPLE_API_ISSUER=${NOTARY_ISSUER_ID:?Missing notarization issuer}
    args+=(-c.mac.notarize=true)
  else
    echo 'Signed PR desktop build: notarization is deferred to release/rehearsal builds.'
    args+=(-c.mac.notarize=false)
  fi
else
  echo 'Building UNSIGNED desktop app: Apple signing secrets are absent.'
  export CSC_IDENTITY_AUTO_DISCOVERY=false
  args=(-c.mac.identity=null -c.mac.hardenedRuntime=false -c.mac.notarize=false -c.dmg.sign=false)
fi
npm run build
npx --no-install electron-builder --mac "--$arch" --publish never "${args[@]}"
app=release/mac/OpenAgora.app
[[ "$arch" != arm64 ]] || app=release/mac-arm64/OpenAgora.app
binary="$app/Contents/Resources/bin/openagora"
[[ "$("$binary" --version)" == "openagora $version" ]]
"$binary" catalog
dmg="release/OpenAgora-$version-mac-$arch.dmg"
if [[ "$signed" == true ]]; then
  codesign --verify --deep --strict --verbose=2 "$app"
  codesign --verify --strict --verbose=2 "$binary"
  codesign --verify --strict --verbose=2 "$dmg"
  if [[ "$notarize" == true ]]; then
    xcrun stapler validate "$app"
    spctl --assess --type execute -vv "$app"
  fi
fi
zip="release/OpenAgora-$version-mac-$arch.zip"
[[ -f "$zip" && -f release/latest-mac.yml ]]
python3 "$ROOT/packaging/checksums.py" "$dmg"
