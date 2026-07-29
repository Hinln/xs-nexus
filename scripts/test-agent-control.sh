#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

if [[ -z "${XS_TEST_DATABASE_URL:-}" ]]; then
    if [[ ! -r /etc/xs-nexus/controller.env ]]; then
        printf 'XS_TEST_DATABASE_URL is required when /etc/xs-nexus/controller.env is unavailable\n' >&2
        exit 2
    fi
    XS_TEST_DATABASE_URL=$(python3 - <<'PY'
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

values = {}
for raw_line in Path("/etc/xs-nexus/controller.env").read_text().splitlines():
    line = raw_line.strip()
    if line and not line.startswith("#") and "=" in line:
        name, value = line.split("=", 1)
        values[name] = value

parsed = urlsplit(values["DATABASE_URL"])
userinfo = parsed.netloc.rsplit("@", 1)[0] + "@" if "@" in parsed.netloc else ""
port = f":{parsed.port}" if parsed.port is not None else ""
print(urlunsplit((parsed.scheme, f"{userinfo}127.0.0.1{port}", parsed.path, parsed.query, parsed.fragment)))
PY
)
    export XS_TEST_DATABASE_URL
fi

export XS_TEST_AGENT_DATABASE_SCHEMA="${XS_TEST_AGENT_DATABASE_SCHEMA:-xs_nexus_agent_test}"
cd "$ROOT_DIR"
cargo test -p xs-agent --test control_plane -- --test-threads=1
