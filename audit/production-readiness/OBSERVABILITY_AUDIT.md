# Observability Audit

## Available

- Controller、Relay、Console healthcheck；PostgreSQL `pg_isready`。
- Controller/Relay JSON 日志和 Docker health/restart 状态。
- Controller/Relay/Console json-file 10 MiB × 5 轮换。

## Missing

- 无 Prometheus、Grafana、Alertmanager、Loki、OTEL 或等效平台。
- 无 online nodes、Direct/Relay rate、handshake/auth/replay/ACL drops、route/update failure 指标告警。
- 无 CPU、memory、disk、network、certificate expiry 和 backup failure 通知。
- 无 threshold、notification channel、on-call、alert runbook 和审计保留策略闭环。
- PostgreSQL 日志没有轮换配置。

## Production Incident Evidence

审计 clean build 使根分区 100%，Controller/PostgreSQL healthcheck 变为 unhealthy；没有告警阻止或提前通知。删除本轮生成 target 后无重启恢复。该事件直接证明 Gate 20 失败。

## Result

`FAIL`。有日志和 health 不等于可运营。

## Final Remediation Reassessment

- 已安装并启用 `xs-nexus-healthcheck.timer`，每五分钟检查根分区、四个精确容器、Controller/Console HTTP 和最新 age 加密备份年龄。
- 正常运行和阈值失败注入均通过；2026-08-08T18:12:05Z 独立执行结果为 success，磁盘 87% 被记录为 warning。
- 正式 TLS target 因 DNS/CDN 门禁未配置；没有外部通知、on-call、证书到期、集中指标/日志和正式告警保留闭环。

最终结果：Gate 20 从 `FAIL` 改善为 `PARTIAL`。
