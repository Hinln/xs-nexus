# Production Audit Evidence Index

## Roots

- 首轮证据：`/srv/xs-nexus-qa/artifacts/production-readiness-audit-20260808T110559Z`
- 精确部署源码：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d`
- 成功 M5.2：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d/artifacts/qa/m5.2-20260808T121327Z`
- 实际镜像 SBOM/Grype：`/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d/artifacts/qa/production-runtime-images-20260808T125447Z`

## Version And Runtime

- `METADATA.txt`、`git-freeze.txt`、`git-show-ff9551d.txt`、`git-show-8745b58.txt`
- `git-diff-name-status.txt`、`git-diff-numstat.txt`、`git-diff-stat.txt`
- `running-containers.json`、`runtime-images.json`、`runtime-config.jsonl`
- `runtime-file-sha256.txt`、`runtime-binary-metadata.txt`、`deployment-active-record.txt`
- `clean-no-cache-rebuild.log`、`clean-rebuild-images.txt`、`clean-rebuild-binary-sha256.txt`
- `database-migration-provenance.txt`、`git-signing-release-audit.txt`、`github-audit.txt`

## Security And Supply Chain

- `gitleaks-git.json`、`gitleaks-production-state.json`、`gitleaks-runtime-logs.json`
- `gitleaks-image-controller.json`、`gitleaks-image-relay.json`、`gitleaks-image-console.json`
- `gitleaks-initial-finding-disposition.txt`、`gitleaks-audit-evidence-final.status`
- `cargo-audit.json`、`cargo-deny.txt`、`rust-advisory-paths.txt`、`rsa-target-all.txt`
- `npm-audit.json`、`npm-audit-status.txt`、`npm-ci.txt`
- `runtime-supply-chain.log`（源 SBOM 漂移失败）
- `runtime-image-vulnerability-summary.txt`、`runtime-image-vulnerability-disposition.txt`

## Host And Network

- `host-baseline.txt`、`ssh-audit.txt`、`listeners.txt`、`nftables.json`
- `routes.json`、`rules.json`、`kernel-hardening.txt`、`packages-upgradable.txt`
- `docker-host-security.txt`、`container-hardening.txt`、`services-hardening.txt`
- `1panel-network.json`、`onepanel-audit.txt`、`linux-agent-runtime-audit.txt`
- `disk-pressure-incident.txt`

## Public Edge And Web

- `domain-http-audit.txt`、`tls-vpn.xiashikeji.cn.txt`、`tls-vpn.qinwen.co.txt`
- `tls-protocol-audit.txt`、`external-port-audit.json`、`public-api-security-probes.txt`
- `web-public-runtime-audit.txt`
- `web-console-mock.status`、`web-console-mock/playwright.log`
- `web-console-mock/screenshots.txt`、`web-console-mock/screenshots.sha256`

## Database, Backup And Operations

- `postgresql-runtime-audit.txt`、`backup-runtime-audit.txt`、`backup-runtime-audit-detail.txt`
- `backup-public-verify.txt`、`observability-runtime-audit.txt`
- `log-xs-nexus-rc-*-last200.txt`

## Fresh Performance

- `performance/protocol-throughput-direct/report.json`
- `performance/relay-throughput-direct/report.json`
- `performance/agent-rtt/report.json`
- `performance/controller-scale-direct/report.json`
- 初次包装失败证据保留在相邻 `.status` 和 `test-output.txt`，未删除或改写。

## Historical Evidence Verification

- `prior-evidence-inventory.txt`、`prior-evidence-files.txt`、`prior-evidence-verification.txt`
- `m52-prior-evidence-verification.txt`、`external-evidence-location-search.txt`
- 历史 Windows、24h soak、最终导出和大多数 `/srv/xs-nexus/artifacts/qa` 原始证据不存在，不能用于 PASS。

最终 SHA-256 清单在首轮和终审结束时重新生成；原始失败日志全部保留。
