#!/usr/bin/env bash
# Keep secrets out of the checkout. Never enable shell tracing in this script.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
builder="$ROOT/packaging/macos/build.sh"
if [[ ${1:-} == --desktop ]]; then
  builder="$ROOT/packaging/macos/desktop.sh"
  shift
fi
notarize=${MACOS_NOTARIZE:-true}
case "$notarize" in true|false) ;; *) echo 'MACOS_NOTARIZE must be true or false' >&2; exit 1 ;; esac
secret_names=(MACOS_CERT_P12_BASE64 MACOS_CERT_PASSWORD APPLE_TEAM_ID NOTARY_KEY_P8_BASE64 NOTARY_KEY_ID NOTARY_ISSUER_ID)
present=0
for name in "${secret_names[@]}"; do
  [[ -z ${!name:-} ]] || present=$((present + 1))
done
if [[ $present -eq 0 ]]; then
  echo 'All macOS signing secrets are absent: building UNSIGNED (expected on forks).'
  [[ -z ${GITHUB_OUTPUT:-} ]] || echo 'signed=false' >> "$GITHUB_OUTPUT"
  [[ -z ${GITHUB_OUTPUT:-} ]] || echo 'notarized=false' >> "$GITHUB_OUTPUT"
  exec "$builder" "$@"
fi
[[ $present -eq 6 ]] || { echo 'Incomplete signing secrets: all six are required.' >&2; exit 1; }
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/openagora-signing.XXXXXX")
export OPENAGORA_SIGNING_TEMP="$work"
keychain="$work/ci.keychain-db"
# Save the original list, then restore it even if import, build or notarization fails.
security list-keychains -d user > "$work/keychains.txt"
cleanup() {
  python3 - "$work/keychains.txt" <<'PY'
import shlex, subprocess, sys
subprocess.run(['security', 'list-keychains', '-d', 'user', '-s', *shlex.split(open(sys.argv[1]).read())], check=False)
PY
  security delete-keychain "$keychain" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT
umask 077
python3 - <<'PY'
import base64, os, pathlib
root = pathlib.Path(os.environ['OPENAGORA_SIGNING_TEMP'])
for env, filename in [('MACOS_CERT_P12_BASE64', 'signing.p12'), ('NOTARY_KEY_P8_BASE64', 'notary.p8')]:
    (root / filename).write_bytes(base64.b64decode(os.environ[env], validate=True))
PY
keychain_password=$(python3 -c 'import secrets; print(secrets.token_hex(32))')
security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
python3 - "$keychain" "$work/keychains.txt" <<'PY'
import shlex, subprocess, sys
subprocess.run(['security', 'list-keychains', '-d', 'user', '-s', sys.argv[1], *shlex.split(open(sys.argv[2]).read())], check=True)
PY
security import "$work/signing.p12" -k "$keychain" -P "$MACOS_CERT_PASSWORD" \
  -T /usr/bin/codesign -T /usr/bin/productbuild -T /usr/bin/pkgbuild >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
rm "$work/signing.p12"
export NOTARY_KEY_PATH="$work/notary.p8"
export CSC_KEYCHAIN="$keychain"
"$builder" "$@"
[[ -z ${GITHUB_OUTPUT:-} ]] || echo 'signed=true' >> "$GITHUB_OUTPUT"
[[ -z ${GITHUB_OUTPUT:-} ]] || echo "notarized=$notarize" >> "$GITHUB_OUTPUT"
