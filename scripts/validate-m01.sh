#!/usr/bin/env bash
set -euo pipefail

make fmt-check
make lint
make build
make test
make test-network
make security-check
npm audit --audit-level=high

docker compose --profile baseline -f deploy/docker/compose.yaml config \
  | grep -A4 '^networks:' \
  | grep -F 'external: true'
docker network inspect 1panel-network \
  --format '{{json .IPAM.Config}}' \
  | grep -F '"Subnet":"172.18.0.0/16"'
git check-ignore -q .env

if systemctl --failed --no-legend | grep -q '[^[:space:]]'; then
  systemctl --failed --no-pager >&2
  exit 1
fi

if ip link show xs-cap-test >/dev/null 2>&1; then
  printf 'temporary TUN remained on the host\n' >&2
  exit 1
fi

if nft list table inet xs_capability_test >/dev/null 2>&1; then
  printf 'temporary nftables table remained on the host\n' >&2
  exit 1
fi

printf 'M0.1 validation passed\n'
