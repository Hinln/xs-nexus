# Open Production Findings

> 首个表格冻结首轮发现。本文末尾记录修复后的状态变化；未列入变化表的发现保持原状态。

| ID | Severity | Finding | Owner | Status / Exit Evidence |
|---|---|---|---|---|
| PR-001 | P1 / Security High | 已披露凭据无全量轮换与旧值失效证据 | Production Owner | BLOCKED_EXTERNAL；轮换矩阵与拒绝旧凭据日志 |
| PR-002 | P1 / Security High | SSH 允许 root/password，INPUT accept，1Panel 188 公网 | Production Owner | BLOCKED_EXTERNAL；维护窗口后硬化与回归 |
| PR-003 | P1 / Security High | 无正式离线发布/恢复密钥仪式 | Release Owner | BLOCKED_EXTERNAL；ceremony、rotation、revoke、restore |
| PR-004 | P1 / Security High | 无独立第三方安全审计 | Security Owner | BLOCKED_EXTERNAL；报告、findings、修复、retest |
| PR-005 | P1 | 镜像 ID不可复现、CI mutable 且失败、无 tag/签名 | Engineering | OPEN_INTERNAL |
| PR-006 | P1 / Security High | PostgreSQL 应用角色具有 cluster superuser 权限 | Engineering/DBA | OPEN_INTERNAL |
| PR-007 | P1 | 源 SBOM 与 lock 漂移 | Engineering | OPEN_INTERNAL |
| PR-008 | P1 | `cargo audit`/`cargo deny` 未闭环 | Engineering/Security | OPEN_INTERNAL |
| PR-009 | P1 / Security High | XSP/1 无可执行 fuzz harness | Engineering/Security | OPEN_INTERNAL |
| PR-010 | P1 | 计划域名 525、当前公网 Console 路由错误 | Infrastructure Owner | BLOCKED_EXTERNAL |
| PR-011 | P1 | 真实 Windows 在线矩阵缺失 | Windows QA | BLOCKED_EXTERNAL |
| PR-012 | P1 | 真实 NAS 矩阵缺失 | NAS Owner | BLOCKED_EXTERNAL |
| PR-013 | P1 | 真实不同公网 NAT/Relay 矩阵缺失 | Network QA | BLOCKED_EXTERNAL |
| PR-014 | P1 | 同机 loop 副本冒充 offsite，无自动恢复演练 | DR Owner | BLOCKED_EXTERNAL |
| PR-015 | P1 | 无正式监控/告警；磁盘满曾影响健康 | SRE | OPEN_INTERNAL + 外部通知配置 |
| PR-016 | P1 | 当前版本 24h soak 原始证据缺失 | QA/SRE | OPEN_INTERNAL |
| PR-017 | P2 | PostgreSQL 日志没有大小/文件数轮换 | Infrastructure Owner | OPEN；需在 1Panel 管理边界处理 |
| PR-018 | P2 | 公网 API 缺 HSTS 等头并返回详细 422 解析错误 | Engineering/Infrastructure | OPEN_INTERNAL/EXTERNAL |
| PR-019 | Security High | 审计证据曾保存临时 GitHub clone token | Audit | CONTAINED；已脱敏并复扫为零，令牌不进入 Git |

## Counts At First Decision

- P0：0
- P1 / Security High：未清零
- P2：未完成 owner/期限/缓解闭环

因此 Hard Gate 23 为 `FAIL`。

## Final Remediation Status

| ID | Final Status | Evidence / Residual |
|---|---|---|
| PR-005 | PARTIALLY_REMEDIATED | 修复分支 CI 与五镜像双构建复现通过；main、生产、签名 tag 和部署仍未一致 |
| PR-006 | BLOCKED_EXTERNAL / DBA | bootstrap superuser 不可原地降权；需新非 bootstrap 角色、ownership/grants、secret rotation 和 redeploy |
| PR-007 | RESOLVED_INTERNAL | 源 SBOM 与锁文件一致，Cargo 325 + npm 110 = 435，最终 baseline 通过 |
| PR-008 | RESOLVED_INTERNAL | 固定 `cargo audit 0.22.2`、`cargo deny 0.20.2` 与策略门禁在最终 baseline 通过 |
| PR-009 | RESOLVED_INTERNAL | 新增 executable fuzz targets，artifact `9025496390` 通过；独立审计仍由 PR-004 跟踪 |
| PR-015 | PARTIALLY_REMEDIATED | 本地五分钟健康守卫、失败注入、备份年龄和磁盘告警通过；外部通知/on-call/TLS 监控和 87% 磁盘仍开放 |
| PR-019 | CONTAINED | 临时 clone token 未进入 Git/生产；证据脱敏和复扫为零，不能替代全部生产凭据轮换 |

仍未关闭的 P1/Security High 包括 PR-001、PR-002、PR-003、PR-004、PR-006、PR-010、PR-011、PR-012、PR-013、PR-014、PR-015、PR-016，以及 PR-005 的发布残余。P1 和 Security High 未清零，Hard Gate 23 最终仍为 `FAIL`。
