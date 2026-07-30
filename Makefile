SHELL := /usr/bin/env bash
.RECIPEPREFIX := >

.PHONY: setup fmt fmt-check lint build test test-unit test-integration test-controller-db test-agent-control test-agent-systemd test-agent-data-plane test-agent-candidate-fallback test-agent-candidate-path test-agent-candidates test-agent-proactive-punch test-agent-nat-matrix test-agent-nat test-protocol-vectors test-spec test-network test-e2e test-visual security-check validate-m21 validate-m22 release clean

setup:
>npm ci

fmt:
>cargo fmt --all

fmt-check:
>cargo fmt --all -- --check

lint:
>cargo clippy --workspace --all-targets -- -D warnings
>npm run lint

build:
>cargo build --workspace
>npm run build

test: test-unit test-integration test-controller-db test-agent-control test-protocol-vectors test-spec

test-unit:
>cargo test --workspace --lib --bins
>npm test

test-integration:
>./scripts/test-integration.sh

test-controller-db:
>./scripts/test-controller-db.sh

test-agent-control:
>./scripts/test-agent-control.sh

test-agent-systemd:
>./scripts/test-agent-systemd.sh

test-agent-data-plane:
>./scripts/test-agent-data-plane.sh

test-agent-candidate-fallback:
>./scripts/test-agent-candidate-fallback.sh

test-agent-candidate-path:
>./scripts/test-agent-candidate-path.sh

test-agent-candidates: test-agent-candidate-fallback test-agent-candidate-path

test-agent-proactive-punch:
>./scripts/test-agent-proactive-punch.sh

test-agent-nat-matrix:
>./scripts/test-agent-nat-matrix.sh

test-agent-nat: test-agent-proactive-punch test-agent-nat-matrix

test-protocol-vectors:
>./scripts/test-protocol-vectors.sh

test-spec:
>python3 scripts/validate-m02.py

test-network:
>./scripts/test-network-capabilities.sh
>./scripts/test-agent-network.sh
>./scripts/test-agent-systemd.sh

test-e2e:
>@printf 'E2E tests are not implemented before M4.2\n' >&2
>@exit 2

test-visual:
>@printf 'Visual tests are not implemented before M4.2\n' >&2
>@exit 2

security-check:
>python3 scripts/test-secret-scanner.py
>python3 scripts/check-secrets.py --root . --reference-env /etc/xs-nexus/controller.env

validate-m21:
>./scripts/validate-m21.sh

validate-m22:
>./scripts/validate-m22.sh

release:
>@printf 'Release packaging is not implemented before M9.1\n' >&2
>@exit 2

clean:
>cargo clean
>rm -rf apps/console/dist
