#!/usr/bin/env bash
# Run only in a disposable Linux container with an unprivileged desktop session.
# Tests the real catalog, downloaded release, engine, and native window together.
set -euo pipefail
[[ -f /.dockerenv && $(id -u) != 0 ]] || { echo 'Run this smoke test as an unprivileged user in a disposable container.' >&2; exit 1; }
: "${OPENAGORA_BIN:?Set OPENAGORA_BIN to the engine to test}"
: "${OPENAGORA_SMOKE_OUTPUT:?Set OPENAGORA_SMOKE_OUTPUT to a writable evidence directory}"
mkdir -p "$OPENAGORA_SMOKE_OUTPUT"
export OPENAGORA_HOME
OPENAGORA_HOME=$(mktemp -d /tmp/openagora-handy.XXXXXX)
# An apostrophe and spaces must remain literal when passed through the installer.
export OPENAGORA_HOME="$OPENAGORA_HOME/User's App data"
cleanup() {
  "$OPENAGORA_BIN" stop handy >/dev/null 2>&1 || true
  cp "$OPENAGORA_HOME/logs/handy.log" "$OPENAGORA_SMOKE_OUTPUT/handy.log" 2>/dev/null || true
}
trap cleanup EXIT
"$OPENAGORA_BIN" install handy --yes 2>&1 | tee "$OPENAGORA_SMOKE_OUTPUT/install.log"
cp "$OPENAGORA_HOME/state/handy.toml" "$OPENAGORA_SMOKE_OUTPUT/installed.toml"
test -x "$OPENAGORA_HOME/apps/handy/Handy-launcher"
test -x "$OPENAGORA_HOME/apps/handy/Handy.AppImage"
"$OPENAGORA_BIN" start handy
"$OPENAGORA_BIN" open handy
wait_window() {
  local window
  for _ in $(seq 1 100); do
    window=$(xdotool search --onlyvisible --name '^Handy$' 2>/dev/null | head -1 || true)
    if [[ -n "$window" ]]; then printf '%s' "$window"; return 0; fi
    sleep .2
  done
  echo 'Handy did not show its window' >&2
  return 1
}
window=$(wait_window)
# Preserve the runtime context Tauri uses to update/restart the original AppImage.
python3 - "$OPENAGORA_HOME/apps/handy/Handy.AppImage" "$OPENAGORA_SMOKE_OUTPUT" <<'PYENV'
import json, pathlib, sys
expected = sys.argv[1].encode()
for entry in pathlib.Path('/proc').glob('[0-9]*/environ'):
    try:
        env = dict(item.split(b'=', 1) for item in entry.read_bytes().split(b'\0') if b'=' in item)
    except (OSError, ValueError):
        continue
    if env.get(b'APPIMAGE') == expected and env.get(b'APPDIR'):
        assert env.get(b'APPIMAGE_EXTRACT_AND_RUN') == b'1'
        evidence = {k: env[k.encode()].decode() for k in ['APPIMAGE', 'APPDIR', 'APPIMAGE_EXTRACT_AND_RUN']}
        (pathlib.Path(sys.argv[2])/'appimage-environment.json').write_text(json.dumps(evidence))
        break
else:
    raise AssertionError('Handy lost its original AppImage update/restart context')
PYENV
"$OPENAGORA_BIN" catalog --json > "$OPENAGORA_SMOKE_OUTPUT/before.json"
# Hiding and opening again must raise the same running instance.
xdotool windowunmap "$window"
"$OPENAGORA_BIN" open handy
wait_window > "$OPENAGORA_SMOKE_OUTPUT/window-id.txt"
"$OPENAGORA_BIN" catalog --json > "$OPENAGORA_SMOKE_OUTPUT/after.json"
python3 - "$OPENAGORA_SMOKE_OUTPUT" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
def handy(name):
    return next(a for a in json.loads((root/name).read_text()) if a['id'] == 'handy')
before, after = handy('before.json'), handy('after.json')
assert before['pid'] and before['pid'] == after['pid'], (before, after)
assert after['status'] == 'running' and after['can_open_window'], after
PY
# Capture only the test window in this disposable profile.
xwd -silent -id "$window" -out "$OPENAGORA_SMOKE_OUTPUT/window.xwd"
"$OPENAGORA_BIN" stop handy
"$OPENAGORA_BIN" uninstall handy --yes
"$OPENAGORA_BIN" install handy --yes > "$OPENAGORA_SMOKE_OUTPUT/reinstall.log" 2>&1
"$OPENAGORA_BIN" open handy
wait_window >/dev/null
printf 'Handy install, visible window, reopen without restart, remove and reinstall passed.\n'
