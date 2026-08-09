#!/usr/bin/env bash
set -Eeuo pipefail

CONFIGURATION=${XS_NEXUS_HOST_FIREWALL_CONFIG:-/etc/xs-nexus/host-firewall.nft}
TABLE_FAMILY=inet
TABLE_NAME=xs_nexus_host_guard
RUNTIME_DIRECTORY=/run/xs-nexus-host-firewall
LOCK_FILE=$RUNTIME_DIRECTORY/lock

fail() {
    printf 'xs-nexus-host-firewall: %s\n' "$1" >&2
    exit 1
}

usage() {
    printf 'usage: xs-nexus-host-firewall {apply|verify|status|remove}\n' >&2
    exit 2
}

require_root() {
    [[ $EUID -eq 0 ]] || fail 'root privileges are required'
}

validate_configuration() {
    local mode owner size
    [[ $CONFIGURATION == /* && ! -L $CONFIGURATION && -f $CONFIGURATION ]] \
        || fail 'configuration must be an absolute regular non-symlink file'
    mode=$(stat -c '%a' "$CONFIGURATION")
    owner=$(stat -c '%u' "$CONFIGURATION")
    size=$(stat -c '%s' "$CONFIGURATION")
    [[ $owner == 0 ]] || fail 'configuration must be owned by root'
    (( (8#$mode & 022) == 0 )) || fail 'configuration must not be group or other writable'
    (( size > 0 && size <= 65536 )) || fail 'configuration size is invalid'
}

prepare_runtime() {
    mkdir -p "$RUNTIME_DIRECTORY"
    [[ ! -L $RUNTIME_DIRECTORY && -d $RUNTIME_DIRECTORY ]] \
        || fail 'runtime path is invalid'
    [[ $(stat -c '%u' "$RUNTIME_DIRECTORY") == 0 ]] \
        || fail 'runtime directory must be owned by root'
    chmod 0700 "$RUNTIME_DIRECTORY"
    exec 9>"$LOCK_FILE"
    flock -x 9
}

verify_table() {
    python3 - "$TABLE_FAMILY" "$TABLE_NAME" \
        3< <(nft -j list table "$TABLE_FAMILY" "$TABLE_NAME") <<'PY'
import json
import os
import sys

family = sys.argv[1]
name = sys.argv[2]
document = json.load(os.fdopen(3))
items = document.get("nftables", [])
tables = [item["table"] for item in items if "table" in item]
chains = [item["chain"] for item in items if "chain" in item]
rules = [item["rule"] for item in items if "rule" in item]
assert len(tables) == 1
assert tables[0]["family"] == family
assert tables[0]["name"] == name
input_chains = [chain for chain in chains if chain.get("name") == "input"]
assert len(input_chains) == 1
input_chain = input_chains[0]
assert input_chain["family"] == family
assert input_chain["table"] == name
assert input_chain["type"] == "filter"
assert input_chain["hook"] == "input"
assert input_chain["prio"] == -10
assert input_chain["policy"] == "drop"
comments = {rule.get("comment") for rule in rules}
assert comments == {
    "xs-allow-loopback",
    "xs-drop-invalid",
    "xs-allow-established",
    "xs-allow-icmp4",
    "xs-allow-icmp6",
    "xs-allow-dhcp4",
    "xs-allow-dhcp6",
    "xs-allow-ssh",
    "xs-allow-web-tcp",
    "xs-allow-quic",
    "xs-drop-unapproved",
}
print("host_firewall_verification=pass")
PY
}

apply_table() {
    local batch
    validate_configuration
    prepare_runtime
    batch=$(mktemp "$RUNTIME_DIRECTORY/batch.XXXXXX")
    trap 'rm -f -- "${batch:-}"' EXIT INT TERM
    chmod 0600 "$batch"
    if nft list table "$TABLE_FAMILY" "$TABLE_NAME" >/dev/null 2>&1; then
        printf 'delete table %s %s\n' "$TABLE_FAMILY" "$TABLE_NAME" > "$batch"
    fi
    cat "$CONFIGURATION" >> "$batch"
    nft --check --file "$batch"
    nft --file "$batch"
    rm -f -- "$batch"
    trap - EXIT INT TERM
    verify_table
}

remove_table() {
    prepare_runtime
    if nft list table "$TABLE_FAMILY" "$TABLE_NAME" >/dev/null 2>&1; then
        nft delete table "$TABLE_FAMILY" "$TABLE_NAME"
    fi
    if nft list table "$TABLE_FAMILY" "$TABLE_NAME" >/dev/null 2>&1; then
        fail 'table remained present after removal'
    fi
    printf 'host_firewall_removed=yes\n'
}

require_root
case ${1:-} in
    apply) apply_table ;;
    verify) verify_table ;;
    status) nft list table "$TABLE_FAMILY" "$TABLE_NAME" ;;
    remove) remove_table ;;
    *) usage ;;
esac
