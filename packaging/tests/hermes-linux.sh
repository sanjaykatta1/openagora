#!/usr/bin/env bash
# Exercises the current official Hermes release without accounts or model calls.
set -euo pipefail
[[ -f /.dockerenv && $(id -u) != 0 ]] || { echo 'Run as an unprivileged user in a disposable container.' >&2; exit 1; }
: "${OPENAGORA_BIN:?Set OPENAGORA_BIN to the engine to test}"
: "${OPENAGORA_SMOKE_OUTPUT:?Set OPENAGORA_SMOKE_OUTPUT to a writable evidence directory}"
export OPENAGORA_HOME
OPENAGORA_HOME=$(mktemp -d /tmp/openagora-hermes.XXXXXX)
mkdir -p "$OPENAGORA_SMOKE_OUTPUT"
trap '"$OPENAGORA_BIN" stop hermes >/dev/null 2>&1 || true' EXIT
"$OPENAGORA_BIN" install hermes --yes 2>&1 | tee "$OPENAGORA_SMOKE_OUTPUT/install.log"
cp "$OPENAGORA_HOME/state/hermes.toml" "$OPENAGORA_SMOKE_OUTPUT/installed.toml"
"$OPENAGORA_BIN" sunday setup
check_dashboard() {
  "$OPENAGORA_BIN" start hermes
  "$OPENAGORA_BIN" sunday status --json > "$OPENAGORA_SMOKE_OUTPUT/$1.json"
  python3 - "$OPENAGORA_SMOKE_OUTPUT/$1.json" <<'PY'
import json, pathlib, sys, urllib.request
status = json.loads(pathlib.Path(sys.argv[1]).read_text())
assert status['setup'] == 'ready' and status['version_ok'], status
assert status['dashboard'] == 'running', status
url = status['url']
assert url.startswith('http://127.0.0.1:'), url
with urllib.request.urlopen(url, timeout=15) as response:
    assert response.status == 200, response.status
print('Real Hermes dashboard is ready:', status['version'])
PY
}
check_dashboard first
for cycle in 1 2; do
  "$OPENAGORA_BIN" uninstall hermes --yes
  test ! -e "$OPENAGORA_HOME/state/hermes.profile.toml"
  "$OPENAGORA_BIN" sunday setup --fresh
  check_dashboard "recreated-$cycle"
done
"$OPENAGORA_BIN" stop hermes
printf 'Real Hermes install, dashboard, removal and repeated recreation passed.\n'
