# Production Hard Gates

> 首个表格冻结首轮只读结论。本文末尾的 “Final Remediation Reassessment” 是修复后的最终权威状态。

| Gate | Current | Required | Evidence | Result |
|---|---|---|---|---|
| 01 代码与部署一致性 | main 与 deploy 产品代码一致，运行二进制可重建；镜像 ID 不可复现、无 tag、提交未签名、CI 失败 | 固定版本、digest、可复现完整镜像、通过的发布 CI | `git-*`、`runtime-*`、`clean-*`、`github-audit.txt` | FAIL |
| 02 Secret 与凭据 | 扫描大部分为零；审计证据曾捕获临时 GitHub clone token并已脱敏；生产旧凭据轮换无证据 | 旧凭据失效、全量轮换、root 密码策略合规 | `gitleaks-*`、`ssh-audit.txt`、`security-scan-status.txt` | FAIL |
| 03 正式密钥体系 | 仅测试签名链路；无正式离线仪式、轮换和恢复证据 | 正式 root/update key 生命周期 | `git-signing-release-audit.txt`、现有更新测试 | BLOCKED_EXTERNAL |
| 04 自研协议安全 | 单测、向量、malformed corpus、replay/并发有覆盖；无可执行 fuzz harness | unit/integration/fuzz/sanitizer 和代码审计全覆盖 | M5.2 日志、`source-risk-*` | FAIL |
| 05 第三方安全审计 | 无独立报告、findings 和 retest | 独立协议、密码、API、Relay、Agent、驱动、供应链审计 | `BLOCKERS.md` 与服务器搜索 | BLOCKED_EXTERNAL |
| 06 Linux Agent 网络安全 | namespace TUN、systemd crash cleanup、路由/ACL/Relay 通过；生产无 Agent，实机故障矩阵不全 | reboot、disk full、DHCP、VPN 冲突等真实主机矩阵 | M5.2、`linux-agent-runtime-audit.txt` | PARTIAL |
| 07 Direct/NAT | namespace NAT matrix 通过 | 不同公网、移动热点、IPv6、切网、丢包和迁移 | M5.2 NAT 日志 | SIMULATED_ONLY |
| 08 Relay | namespace auth、密文、failover、Direct 恢复通过 | 恶意公网、重启、丢失、多 Relay 和长期容量 | M5.2 Relay 日志、运行日志 | PARTIAL |
| 09 ACL | namespace 双端 ACL、伪造和默认拒绝测试通过 | 完整 A/B/C、relay/subnet bypass 与控制器断连实证 | M5.2 ACL 日志 | PARTIAL |
| 10 Subnet Router | namespace proposal/apply/revoke/ACL 测试通过 | 真实网关、NAT/pure route、reboot/offline/uninstall | M5.2 subnet 日志 | SIMULATED_ONLY |
| 11 Windows 在线实机 | 历史文档声明无法找到原始导出；当前在线 Agent 生命周期未测试 | Windows 11 完整安装、网络、升级、回滚、Verifier、清理 | `prior-evidence-*`、`WINDOWS_AUDIT.md` | BLOCKED_EXTERNAL |
| 12 真实 NAS | 未执行 | 真实 NAS 普通节点和 subnet router 全矩阵 | `NAS_AUDIT.md` | BLOCKED_EXTERNAL |
| 13 计划域名/TLS/CDN | `vpn.xiashikeji.cn` 应用路径 525；当前域名未公开 Console | 计划域名 API、WS、Console、health 全通且 TLS 正确 | `domain-*`、`tls-*`、`web-public-runtime-audit.txt` | FAIL |
| 14 服务器硬化 | 容器边界较好；SSH root/password 开启、INPUT accept、1Panel 188 公网、待更新、磁盘无保护 | 最小暴露、补丁、密钥 SSH、容量/重启验证 | `ssh-audit.txt`、`listeners.txt`、`nftables.json`、`packages-upgradable.txt` | FAIL |
| 15 1Panel 共存 | external network 正确，M5.2 生命周期不改变网络；未执行 host/1Panel reboot 和正式升级 | 全生命周期不影响 1Panel | `1panel-network.json`、`onepanel-audit.txt`、M5.2 | PARTIAL |
| 16 数据库 | 迁移 SHA 匹配、DB 集成通过；应用角色为 superuser，无异机 restore，Postgres 日志无轮换 | 最小权限、故障/恢复/并发/回滚完整 | `postgresql-runtime-audit.txt`、`database-migration-provenance.txt` | FAIL |
| 17 异地备份/DR | age 备份和同机副本可公开校验；不是异地，无调度和全新服务器恢复 | 真异地、自动化、离线 identity、全新环境恢复 | `backup-*` | BLOCKED_EXTERNAL |
| 18 升级供应链 | 测试签名、篡改/降级/回滚自动化通过；正式 key/revocation ceremony 缺失 | 正式签名供应链和全平台故障矩阵 | M5.2 test-signed-release | PARTIAL |
| 19 Web Console | 本地服务健康；公网 Console 不可用；135 张新截图均为 Mock | 真实 Controller/DB/Redis/Console Playwright | `web-public-runtime-audit.txt`、`web-console-mock.status` | FAIL |
| 20 可观测性 | 有 JSON 日志和 health；无指标平台、告警、通知、on-call、证书/备份监控 | 正式运营可见性和告警闭环 | `observability-runtime-audit.txt`、`disk-pressure-incident.txt` | FAIL |
| 21 性能 | 本机吞吐、namespace RTT、1000 节点控制面已测 | 不同公网、并发 Relay、DB/WebSocket、容量和阈值 | `performance/` | PARTIAL |
| 22 稳定性 Soak | 文档声称历史 24h，但原始证据不存在；当前未重跑 | 当前版本至少 24h 含故障注入 | `prior-evidence-*` | UNKNOWN |
| 23 P0/P1/P2 | P0 未发现；存在多项 P1/Security High | P1=0，Security High=0，P2 有 owner/期限 | `OPEN_FINDINGS.md` | FAIL |
| 24 依赖和供应链 | npm audit 0；Cargo advisory、paste unmaintained、SBOM 漂移、mutable CI/base | audit/deny/SBOM/license/digest 全闭环 | `cargo-*`、`npm-*`、`runtime-image-*` | FAIL |
| 25 部署演练 | M5.2 隔离生命周期通过；未在全新服务器执行正式签名、真实恢复和节点接入 | 另一工程师按文档完成全链路 | M5.2、`RELEASE_REHEARSAL.md` | FAIL |

## Mandatory External Gates

| Gate | Current | Required | Evidence | Result |
|---|---|---|---|---|
| Windows online | 完整在线 Agent 未验证 | 受控 Windows 11 实机矩阵 | `WINDOWS_AUDIT.md` | BLOCKED_EXTERNAL |
| NAS | 未验证 | 真实 NAS 普通节点与子网路由 | `NAS_AUDIT.md` | BLOCKED_EXTERNAL |
| CDN 525 | 仍可复现 | 修复 origin TLS/SNI/反代 | `domain-http-audit.txt` | FAIL |
| formal keys | 无正式仪式 | 离线签名、轮换、撤销、恢复 | `SECURITY_AUDIT.md` | BLOCKED_EXTERNAL |
| offsite restore | 同机 loop 副本 | 独立故障域全新服务器恢复 | `DR_BACKUP_AUDIT.md` | BLOCKED_EXTERNAL |
| credential rotation | 无失效证据 | 全量轮换与旧凭据拒绝 | `SECURITY_AUDIT.md` | BLOCKED_EXTERNAL |
| third-party audit | 无 | 独立报告、修复、retest | `SECURITY_AUDIT.md` | BLOCKED_EXTERNAL |

## Final Remediation Reassessment

| Gate | Remediation-State Evidence | Final Result |
|---|---|---|
| 01 代码与部署一致性 | 修复分支 `8532eb6` 的五镜像可复现且 CI 通过；GitHub main 仍为 `8745b580`，生产仍运行 `ff9551d`，无签名 tag/部署 | FAIL |
| 02 Secret 与凭据 | 仓库、artifact、生产复核文件复扫为零；已披露凭据无全量轮换和旧值拒绝，SSH 仍允许密码/root | FAIL |
| 03 正式密钥体系 | 测试签名链路通过；正式离线 ceremony、rotation、revoke、restore 未发生 | BLOCKED_EXTERNAL |
| 04 自研协议安全 | 可执行 fuzz targets 与 30 秒 CI 运行通过；无 sanitizer 长跑、独立协议/密码学审计 | PARTIAL |
| 05 第三方安全审计 | 无独立报告、findings 和 retest | BLOCKED_EXTERNAL |
| 06 Linux Agent 网络安全 | namespace TUN/systemd/ACL/Relay/Subnet 回归通过；无生产 Agent 和真实主机故障矩阵 | PARTIAL |
| 07 Direct/NAT | 仅 namespace NAT matrix | SIMULATED_ONLY |
| 08 Relay | 认证、密文、failover、Direct 恢复和 fuzz 通过；无恶意公网、长期容量和多地域证据 | PARTIAL |
| 09 ACL | 双端默认拒绝、伪造和 namespace 回归通过；无完整真实拓扑 bypass 证据 | PARTIAL |
| 10 Subnet Router | namespace proposal/apply/revoke/ACL 通过；无真实网关/NAS | SIMULATED_ONLY |
| 11 Windows 在线实机 | 交叉 target 与离线边界通过；完整在线安装、服务、路由、升级、卸载未执行 | BLOCKED_EXTERNAL |
| 12 真实 NAS | arm64 包和 namespace 路由不是 NAS 实机证据 | BLOCKED_EXTERNAL |
| 13 计划域名/TLS/CDN | 计划域名应用路径仍为 525，正式公网 Console 未闭环 | FAIL |
| 14 服务器硬化 | 五分钟本地健康守卫已安装；SSH/root/password、INPUT accept、管理入口、补丁和 87% 磁盘仍未整改 | FAIL |
| 15 1Panel 共存 | `1panel-network` ID、子网和四成员保持不变；未执行 host/1Panel reboot 和正式升级 | PARTIAL |
| 16 数据库 | 集成和迁移通过；bootstrap superuser 不能原地降权，需新角色、grants、secret rotation、redeploy | FAIL |
| 17 异地备份/DR | 加密备份健康守卫通过；副本仍同机，无全新服务器恢复 | BLOCKED_EXTERNAL |
| 18 升级供应链 | 测试签名、篡改、降级和回滚通过；无正式 key ceremony 和真实平台矩阵 | PARTIAL |
| 19 Web Console | revision `5505893` 的真实 PostgreSQL/Controller/production-build/Chromium 七场景、六视口 132 图矩阵通过，无 API 拦截，artifact 139/139 校验；计划域名严格 TLS 与正式公网 Console 仍不可用 | PARTIAL |
| 20 可观测性 | 本地 systemd 五分钟守卫和失败注入通过；无外部通知、on-call、TLS/证书监控和指标平台 | PARTIAL |
| 21 性能 | 本机/namespace/1000 节点基线存在；无 WAN、并发容量和长期负载 | PARTIAL |
| 22 稳定性 Soak | 当前修复版本没有 24 小时原始证据 | UNKNOWN |
| 23 P0/P1/P2 | P0 未发现；凭据、主机、密钥、DB、域名、真实平台、DR、第三方审计等 P1/High 未清零 | FAIL |
| 24 依赖和供应链 | audit/deny/npm/SBOM/许可证/五镜像复现均在修复分支通过；未签名、未部署，生产 glibc 风险仍为限时处置 | PARTIAL |
| 25 部署演练 | CI 和隔离生命周期通过；未由独立工程师在全新服务器完成正式签名、恢复、节点接入和回滚 | FAIL |

最终计数：PASS 0、FAIL 7、BLOCKED_EXTERNAL 5、PARTIAL 10、SIMULATED_ONLY 2、UNKNOWN 1。最终结论见 `GO_NO_GO_FINAL.md`：`NO_GO`。
