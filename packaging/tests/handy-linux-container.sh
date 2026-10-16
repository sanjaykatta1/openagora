#!/usr/bin/env bash
# Container entry point. Source is mounted read-only; no host home or credentials.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  ca-certificates curl python3 xvfb xauth xdotool x11-apps dbus-x11 \
  libgtk-layer-shell0 libegl1 libgl1 libgles2 libasound2t64 libnss3 libgbm1 \
  libgtk-3-0t64 libayatana-appindicator3-1
useradd -m smoke
mkdir -p /evidence
chown smoke:smoke /evidence
runuser -u smoke -- env OPENAGORA_BIN="${OPENAGORA_BIN:-/src/engine/target/debug/openagora}" \
  OPENAGORA_SMOKE_OUTPUT=/evidence \
  xvfb-run -a dbus-run-session bash /src/packaging/tests/handy-linux.sh
