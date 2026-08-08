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
