#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

bash -n "$ROOT_DIR/deploy/host/xs-nexus-host-firewall.sh"
shellcheck "$ROOT_DIR/deploy/host/xs-nexus-host-firewall.sh"
python3 "$ROOT_DIR/scripts/test-host-hardening.py"

echo 'host hardening tests passed'
