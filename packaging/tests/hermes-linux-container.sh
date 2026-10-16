#!/usr/bin/env bash
# Disposable container only; never mount a real home or any credentials here.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  git curl ca-certificates python3 python3-venv build-essential libatomic1 procps libx11-6
useradd -m smoke
mkdir -p /evidence
chown smoke:smoke /evidence
runuser -u smoke -- env OPENAGORA_BIN="${OPENAGORA_BIN:-/src/engine/target/debug/openagora}" \
  OPENAGORA_SMOKE_OUTPUT=/evidence bash /src/packaging/tests/hermes-linux.sh
