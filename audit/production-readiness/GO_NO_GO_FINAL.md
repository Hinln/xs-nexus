# Final Production Readiness Decision

## Decision

`NO_GO`

本结论是修复后最终结论，不替代或改写首轮只读审计证据。当前版本不得作为正式生产、Release Candidate、企业公网 VPN、正式 Windows/NAS 客户端或已完成安全认证的产品发布。

## Audited Revisions

| Layer | Revision / Identity | Final Observation |
|---|---|---|
| GitHub `main` | `8745b5804312587534c1e91980dfb11720952ed1` | 未包含本轮修复 |
| Production checkout | `8745b5804312587534c1e91980dfb11720952ed1` | 工作树干净，但不是当前运行镜像的构建 revision |
| Production Controller/Relay/Console | OCI revision `ff9551d322067c934d2ac7d55a62af8896660bb3` | 运行且健康，但不是 main 或修复版本 |
| Remediation branch | `8532eb6992389568643f8a501055c6acc72395ab` | CI 全部通过，尚未合并、签名、标记或部署 |
| Final CI | [run 31270487478](https://github.com/Hinln/xs-nexus/actions/runs/31270487478) | `baseline`、`console-real-e2e`、`protocol-fuzz`、`image-reproducibility` 全部成功 |

因此 GitHub main/生产 checkout、实际运行部署和修复版本仍是三个不同状态，尚无正式发布 provenance 把它们闭环为一个可重现生产发布。

## Remediation Completed

- 固定 Rust `1.93.0`、Windows target、CI actions、Ubuntu runner、PostgreSQL service digest 和产品基础镜像引用。
- `cargo audit`、`cargo deny`、npm audit、源 SBOM、许可证策略、Windows target check、独立实现检查和严格 ShellCheck 在最终 baseline 中通过。
- 源 SBOM 与锁文件重新一致：Cargo 325、npm 110、总计 435 个依赖组件。
- 新增可执行协议 fuzz targets；最终 30 秒 CI fuzz 运行通过并保留 corpus、日志和 artifact。
- 新增真实 PostgreSQL + Controller + Vite + Playwright 管理 Console E2E；未使用 `page.route`，验证登录、全部管理页、刷新和注销。
- 五个最终 OCI 镜像各自执行两次无缓存构建并逐字节一致；Console 和 Edge 的易变 Alpine 包操作已隔离或规范化。
- 生产主机安装五分钟只读健康守卫，覆盖磁盘、四个容器、Controller/Console HTTP 和加密备份年龄；正常与失败注入均有证据。
- 最终仓库、CI artifacts 和生产复核文件的秘密扫描通过；没有删除、重建或修改 `1panel-network` 或其他 1Panel 资源。

## Hard Gate Summary

| Result | Count | Gates |
|---|---:|---|
| PASS | 0 | 无 |
| FAIL | 7 | 01、02、13、14、16、23、25 |
| BLOCKED_EXTERNAL | 5 | 03、05、11、12、17 |
| PARTIAL | 10 | 04、06、08、09、15、18、19、20、21、24 |
| SIMULATED_ONLY | 2 | 07、10 |
| UNKNOWN | 1 | 22 |

Gate 02 和 Gate 16 同时包含外部操作条件，但其当前生产状态本身不符合要求，因此仍记为 `FAIL`，不能仅用 `BLOCKED_EXTERNAL` 降低严重性。

## Remaining Production Blockers

1. 已披露的生产、数据库、NAS、Console、Controller、Enrollment 和恢复凭据没有全量轮换及旧值拒绝证据。
2. SSH 仍允许 root/password，INPUT policy 仍为 accept，1Panel 管理入口、补丁、重启和生产最小防火墙未完成。
3. 无正式离线发布/恢复密钥仪式、双人控制、轮换、撤销、备份和独立公钥认证。
4. 无独立第三方协议、密码学、API、Relay、Agent、Windows、供应链安全审计及 retest。
5. 计划域名的应用路径仍返回 CDN 525；当前公网 Console 路由未形成正式入口。
6. PostgreSQL 应用仍使用 bootstrap superuser。该角色不能安全降权；需要新建非 bootstrap 角色、迁移 ownership/grants、轮换 secret 并重新部署。
7. 真实 Windows 在线生命周期、真实 NAS、真实不同公网 NAT/Relay/Subnet Router 仍未执行。
8. 备份副本仍在同一主机，没有自动异地备份、离线 identity 和全新服务器恢复演练。
9. 生产只有本地健康守卫，没有外部通知、on-call、TLS/证书监控和正式告警平台；根分区仍为 87%。
10. 当前修复版本没有 24 小时 soak、签名 tag、正式发布、生产部署、升级/回滚和独立工程师全链路演练。

## Final Evidence

- 远端证据根：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z`
- 最终 CI 证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-20260808T162300Z/ci-final-8532eb6`
- 生产复核：`ci-final-8532eb6/production-final.txt`，SHA-256 `13e0b6c717b8fb52840dd75c6449f47a09fd783c6d847a04ec4872756d5a6df9`
- 最终 artifact IDs：Console E2E `9025475853`、protocol fuzz `9025496390`、image reproducibility `9025655997`
- 生产复核时间：`2026-08-08T18:12:05Z`；四容器健康，`1panel-network` ID 仍为 `7df70648b96ab2d6e5e178cce4e5892d655e7b451dd111f90f42ae86e3757ac0`，子网仍为 `172.18.0.0/16`。

## Residual Risk

`CRITICAL`

内部工程门禁已有实质改善，但正式范围仍同时包含公网入口、自研加密协议、特权网络 Agent、Windows、NAS 和生产数据。凭据、主机边界、正式密钥、DB 最小权限、真实平台、异地恢复、独立安全审计、版本部署一致性和长期运营均未闭环，任何 `GO` 或 `CONDITIONAL_GO` 都缺乏依据。
