#!/usr/bin/env bash
set -euo pipefail

if [[ ! -c /dev/net/tun ]]; then
  printf '/dev/net/tun is unavailable\n' >&2
  exit 1
fi

unshare --net --mount-proc bash -euo pipefail -c '
  ip link set lo up
  ip tuntap add dev xs-cap-test mode tun
  ip address add 100.88.255.254/32 dev xs-cap-test
  ip link set xs-cap-test up
  nft add table inet xs_capability_test
  nft add chain inet xs_capability_test input "{ type filter hook input priority 0; policy accept; }"
  ip -br address show xs-cap-test | grep -F "100.88.255.254/32"
  nft list table inet xs_capability_test | grep -F "table inet xs_capability_test"
'

printf 'isolated TUN, namespace, and nftables checks passed\n'
