# Infrastructure Audit

## Positive Controls

- XS Nexus 应用 HTTP 仅 loopback，DB 无 host publish。
- Controller/Relay/Console/PostgreSQL 容器均非 root、非 privileged、drop ALL、no-new-privileges，默认 AppArmor/seccomp 生效。
- Chrony 和自动更新服务运行，无失败 systemd unit。
- `1panel-network` 保持 external `172.18.0.0/16`，M5.2 生命周期未改变网络、默认路由或 nftables。

## Findings

- SSH 允许 root 和密码认证；1Panel 管理端口 188 公网可达。
- INPUT policy accept，没有按业务最小化公网端口。
- 存在 OpenSSH、OpenSSL、libc、sudo、systemd、Docker、nftables 和新内核等待更新，需要维护窗口和 reboot。
- PostgreSQL 容器 json-file 无日志轮换。
- 根分区在审计 clean build 时达到 100%，Controller/PostgreSQL healthcheck 短暂失败；没有磁盘阈值告警或构建隔离容量。
- 主机未发现磁盘加密。

## 1Panel Boundary

没有删除、重建、修改 `1panel-network`、1Panel 数据库或无关网站。正式防火墙、SSH、1Panel 管理入口和系统更新需要人工批准。

## Result

Gate 14 `FAIL`；Gate 15 `PARTIAL`。
