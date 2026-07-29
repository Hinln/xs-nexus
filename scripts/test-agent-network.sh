#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

if [[ ${1:-} == --inside-namespace ]]; then
    if [[ $(readlink /proc/self/ns/net) == "$XS_HOST_NETWORK_NAMESPACE" ]]; then
        printf 'network namespace isolation failed\n' >&2
        exit 1
    fi
    cd "$ROOT_DIR"
    exec cargo test -p xs-agent --features privileged-network-tests --test tun_lifecycle -- --test-threads=1
fi

if [[ ! -c /dev/net/tun ]]; then
    printf '/dev/net/tun is unavailable\n' >&2
    exit 2
fi
if [[ $(id -u) -ne 0 ]]; then
    printf 'privileged Agent network tests require root\n' >&2
    exit 2
fi
if [[ -e /sys/class/net/xstest0 ]]; then
    printf 'refusing to run while host interface xstest0 exists\n' >&2
    exit 2
fi

host_namespace=$(readlink /proc/self/ns/net)
export XS_HOST_NETWORK_NAMESPACE="$host_namespace"
unshare --net --mount-proc "$ROOT_DIR/scripts/test-agent-network.sh" --inside-namespace

if [[ -e /sys/class/net/xstest0 ]]; then
    printf 'Agent network test leaked xstest0 into the host namespace\n' >&2
    exit 1
fi
printf 'Agent TUN lifecycle namespace tests passed\n'
