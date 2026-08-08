# Release Rehearsal Audit

## Re-executed

- clean checkout：`ff9551d...` 独立 worktree。
- no-cache/pull rebuild：完成；二进制和 Console payload 可复现，完整 image ID 不可复现。
- DB migrate/integration：真实临时 PostgreSQL 18.4 通过。
- namespace Linux enroll/connectivity/ACL/Relay/Subnet Router：通过。
- Linux x86_64/aarch64 测试签名包：构建、签名验证和 ELF 架构通过。
- Docker lifecycle：preflight、build、migrate、deploy、health、backup、tamper rejection、restore、failed migration/activation rollback、down 和 1Panel/network invariants 通过。

## Not Rehearsed As Production

- 正式 secret 注入和已轮换凭据。
- 正式离线签名 artifacts 和 revoke/key rotation。
- 新服务器、真实异地 backup、现存节点/ACL/撤销状态恢复。
- 真实 Windows 和 NAS enrollment、upgrade/rollback/uninstall。
- 计划域名、CDN、生产防火墙和公网不同地域链路。
- 另一名独立工程师按文档全程执行。

## Failures Preserved

前三次 M5.2 尝试分别暴露缺失测试 DB、Docker hostname 对宿主不可解析、部署测试外部环境未覆盖；最终修正审计编排后完整通过。源 SBOM、供应链和磁盘压力失败同样保留，没有删除、跳过或弱化测试。

## Result

`FAIL` for formal production。隔离开发发布生命周期通过，但不等价于正式全新环境发布演练。

## Final Remediation Reassessment

- 修复分支完成 baseline、真实 Console E2E、protocol fuzz 和五镜像可复现性四项 CI，全部成功。
- 该 run 没有部署到生产，也没有正式签名、tag、密钥 ceremony、全新服务器恢复、真实 Windows/NAS/WAN、DNS/防火墙或独立操作员演练。
- 生产仍运行 `ff9551d`，因此 run `31270487478` 是工程门禁证据，不是正式生产发布演练。

最终结果仍为 Gate 25 `FAIL`。
