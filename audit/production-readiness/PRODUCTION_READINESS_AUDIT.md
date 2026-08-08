# XS Nexus Production Readiness Audit

## Audit Mode

- 审计标准：`XS_Nexus_Production_Readiness_Audit.md`，SHA-256 `46d7f3e5b4891e534292ff91e9be914b466ba3f80435e64b74cec50125aaa48d`。
- 审计视角：独立发布门禁；既有 PASS、verified、complete、secure、production 声明均不直接继承。
- 首轮规则：先采证和冻结结论，再修复；未找到原始证据的历史声明降级为 UNKNOWN。
- 结论：`NO_GO`。

## Version Freeze

- GitHub `main`：`8745b5804312587534c1e91980dfb11720952ed1`。
- 生产源码：`ff9551d322067c934d2ac7d55a62af8896660bb3`。
- 两者产品代码无差异；16 个差异文件均为文档/交接材料。
- 生产运行 Controller、Relay、Console OCI revision 均为 `ff9551d...`。
- 运行二进制与从 `ff9551d...` 干净 checkout 重建的 Controller/Relay 二进制 SHA-256 一致，但完整镜像 ID 不一致。

## Independent Results

- 完整 `validate-m52.sh` 在修正审计数据库编排后通过，证据 `/srv/xs-nexus-qa/audit-worktrees/production-readiness-ff9551d/artifacts/qa/m5.2-20260808T121327Z`。
- 真实生产四镜像 Grype 结果为 Critical 2、High 4、Medium 16、Negligible 24；Critical/High 均为 glibc `wont-fix`，实际二进制不导入被处置 API。
- 源 SBOM 门禁失败：`nanoid@3.3.18` 未进入许可证快照，仍残留 `nanoid@3.3.16`。
- 新鲜性能测试完成本机协议、Relay、namespace RTT 和 1000 节点控制面；没有不同公网或容量上限证据。
- 新鲜 Playwright 12 项测试和 135 张截图通过，但全部 API 由 `page.route` Mock，分类为 `SIMULATED_ONLY`。
- 真实生产计划域名 525、Console 公网不可用、Windows/NAS/异地恢复/第三方审计仍未完成。

## Audit Incident

完整重建与既有 Docker build cache 共同填满根分区，Controller 与 PostgreSQL healthcheck 短暂显示 unhealthy。仅删除本轮可再生 audit worktree `target` 后，无重启恢复健康。该事件证明当前没有可靠磁盘阈值和告警，见 `disk-pressure-incident.txt`。

## Decision Basis

25 个 Hard Gate 没有一个满足正式 PASS 所需的完整证据。功能回归通过不能覆盖凭据、正式密钥、真实平台、灾备、主机硬化、供应链和独立安全审计的硬失败。

## Remediation Outcome

- 最终修复 revision：`8532eb6992389568643f8a501055c6acc72395ab`。
- 最终 GitHub run `31270487478` 的 baseline、真实 Console E2E、protocol fuzz 和五镜像可复现性全部成功。
- 生产只读复核于 `2026-08-08T18:12:05Z` 完成：四容器健康、生产 source `8745b580`、运行 OCI revision `ff9551d`、`1panel-network` 未变化、健康守卫 success、磁盘 87%。
- 修复版本没有合并到 main、签名/tag 或部署；所有真实 Windows、真实 NAS、正式 DNS/防火墙、正式密钥、凭据轮换、DB 新角色、异地恢复和第三方审计继续保持外部门禁。

最终结论见 `GO_NO_GO_FINAL.md`：`NO_GO`，残余风险 `CRITICAL`。
