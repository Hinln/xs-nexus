# Production Readiness Decision

> 本文件冻结首轮只读结论。修复后的最终权威结论见 `GO_NO_GO_FINAL.md`。

## Decision

`NO_GO`

## Scope

本结论覆盖 XS Nexus 当前声明范围：Linux Agent、Windows 客户端、NAS、Subnet Router、Controller、Relay、Web Console、互联网暴露入口、升级与灾难恢复。它不是仅限实验室或封闭内测的结论。

审计冻结点：GitHub `main` 为 `8745b5804312587534c1e91980dfb11720952ed1`，生产部署源码为 `ff9551d322067c934d2ac7d55a62af8896660bb3`，首轮证据根目录为 `/srv/xs-nexus-qa/artifacts/production-readiness-audit-20260808T110559Z`。

## Hard Gate Summary

- PASS：0
- FAIL：11（01、02、04、13、14、16、19、20、23、24、25）
- BLOCKED_EXTERNAL：5（03、05、11、12、17）
- UNKNOWN：1（22）
- PARTIAL：6（06、08、09、15、18、21）
- SIMULATED_ONLY：2（07、10）

## Blocking Findings

1. **P1 / Security High**：生产登录凭据曾在开发沟通中披露，未获得旧凭据失效和全量轮换证据；SSH 仍允许 root 和密码认证。
2. **P1 / Security High**：不存在正式离线发布密钥、轮换、撤销和恢复仪式；现有证据只证明测试密钥流程。
3. **P1 / Security High**：没有独立第三方协议、密码学、Agent 权限、Relay 滥用和 Windows 驱动审计。
4. **P1**：生产镜像不能按完整 image ID 可复现；构建使用可变基础镜像和在线包操作，Git 无发布 tag，提交未签名，GitHub CI 当前失败。
5. **P1 / Security High**：PostgreSQL 应用角色具有 superuser、createdb、createrole、replication 和 bypassrls 权限。
6. **P1**：计划域名 `vpn.xiashikeji.cn` 仍返回 CDN 525；当前可用域名未公开 Console，计划生产入口未闭环。
7. **P1**：真实 Windows 在线生命周期、真实 NAS 和真实不同公网 NAT/Relay 测试未完成。
8. **P1**：备份副本是同机 loop 设备，不是异地故障域；无自动备份调度、全新服务器恢复和正式密钥恢复证据。
9. **P1**：生产缺少告警、通知、on-call、证书到期、备份失败和容量监控；审计构建曾填满根分区并使 Controller/PostgreSQL 健康检查短暂失败。
10. **P1**：源 SBOM 许可证快照与 `package-lock.json` 漂移；`cargo audit`/`cargo deny` 未清零，协议没有可执行 fuzz harness。

## Evidence Confidence

`HIGH`

该等级表示有充分原始证据支持 `NO_GO`，不表示所有未知项已经验证。外部门禁缺失、计划域名 525、主机硬化、数据库权限、供应链失败和不可复现镜像均已直接重测。

## What Is Safe Today

- 在隔离 namespace 和受控实验环境继续 Linux 功能、ACL、Relay、Subnet Router、安装与升级测试。
- 保持当前 RC 仅用于受控审计和开发验证，不新增正式生产承诺或高价值业务流量。
- 使用当前加密备份做同机完整性检查，但不得称为异地灾备。

## What Is NOT Safe Today

- 不得宣布正式生产、Release Candidate、企业公网 VPN 或密码学安全认证。
- 不得向 Windows、NAS 或 Subnet Router 正式用户发布。
- 不得把 namespace NAT、Mock Playwright、测试签名或测试密钥当作实机/正式证据。
- 不得在未轮换凭据、未完成主机最小暴露和独立审计前扩大公网访问。

## Minimum Path to GO

1. 修复全部内部 P1/Security High：可复现构建、CI 固定、SBOM、依赖处置、协议 fuzz、数据库最小权限和磁盘/日志保护。
2. 人工轮换全部已披露凭据并禁用 root/密码 SSH，完成正式密钥仪式。
3. 修复计划域名 TLS/CDN/Console 路由和最小生产防火墙。
4. 完成真实 Windows、真实 NAS、真实不同公网 NAT/Relay/Subnet Router 全矩阵。
5. 建立异地备份、自动调度并在全新服务器完成恢复演练。
6. 建立告警、通知、容量阈值和至少 24 小时当前版本 soak。
7. 由独立第三方完成安全审计、修复和 retest。
8. 从干净环境完成签名发布、部署、升级、回滚、备份、恢复和卸载演练。

## Estimated Residual Risk

`CRITICAL`

正式范围包含公网、自研加密链路、特权网络 Agent、Windows 和 NAS，但凭据、正式密钥、第三方审计、真实平台、异地恢复和运营监控均未闭环。

## Subsequent Gate 19 V2 Evidence (2026-08-12)

本文件首轮只读结论保持冻结。后续 remediation v2 在 exact revision `5505893710ab1d15e06495603dff08bf5c1e035f`、run `31529393933`、job `93905489516` 和 artifact `9116327161` 完成真实 PostgreSQL/Controller/production Console/Chromium、无 API 拦截、六视口 132 图、权限/并发/故障/破坏性操作和 139/139 证据完整性矩阵。该内部子矩阵为 `PASS`，但计划域名严格 origin TLS/SNI、CDN、正式公网 Console/API/WebSocket、当前分支部署及其他生产硬门禁仍未完成，因此 Gate 19 仍为 `PARTIAL`，整体决策仍严格为 `NO_GO`。
