# XS Nexus 生产可观测性与告警

`check-production-health.sh` 是宿主机只读入口，实际采集由同目录的 `production_observability.py` 完成。采集器不修改 Docker、网络、防火墙、数据库、备份或 `1panel-network`，只读取受控状态并原子替换最新指标、快照、告警和去重状态。

该实现关闭仓库侧采集、阈值、告警状态机和通知客户端缺口，不代表 Gate 20 已经通过。正式环境至少一次 warning/critical 告警的独立送达、值班确认和恢复关闭仍是 `BLOCKED_EXTERNAL`；本机 journal、CI Webhook 夹具和模拟阈值都不能替代该证据。

## 1. 覆盖范围

每次运行生成低基数 Prometheus 文本和 JSON 快照，覆盖：

- 宿主 CPU、内存、磁盘、inode、文件描述符、任务数、网络累计字节和受限目录日志大小；
- 指定 Docker 容器状态、health、CPU、内存、网络、PID 和容器日志大小；
- 指定 HTTP/HTTPS 健康端点；明文 HTTP 只允许 loopback；
- 最新 age 密文备份年龄、复制回执和深度验证回执；
- 正式 TLS 主机名/证书链验证和剩余有效期；
- Controller、数据库和适用时 Redis 状态；
- 在线节点、遥测完整性、Direct/Relay 观察比例、流量和握手失败；
- 管理/控制认证失败、ACL/重放丢弃、Relay 分类丢弃、路由变更和失败升级。

Controller 数据只来自验签并持久化的 Agent/Relay 聚合报告。`GET /v1/admin/observability` 不返回节点、网络、Relay 标识、名称、地址、端点、载荷、Token、Cookie、hash 或密钥。缺失和陈旧遥测显式告警，不能解释为零故障。

## 2. 安装

要求 Python 3.12、curl、Docker CLI 和 systemd。先在精确发布提交上执行仓库测试：

```bash
make test-production-health EVIDENCE_DIR=/path/outside/repository/observability-test
python3 scripts/check-secrets.py --root /path/outside/repository/observability-test
```

安装两个入口文件和 systemd 单元：

```bash
sudo install -d -m 0755 /usr/local/libexec/xs-nexus /etc/xs-nexus
sudo install -m 0755 scripts/check-production-health.sh \
  /usr/local/libexec/xs-nexus/check-production-health
sudo install -m 0755 scripts/production_observability.py \
  /usr/local/libexec/xs-nexus/production_observability.py
sudo install -m 0644 deploy/systemd/xs-nexus-healthcheck.service \
  /etc/systemd/system/xs-nexus-healthcheck.service
sudo install -m 0644 deploy/systemd/xs-nexus-healthcheck.timer \
  /etc/systemd/system/xs-nexus-healthcheck.timer
sudo install -m 0600 deploy/systemd/monitor.env.example /etc/xs-nexus/monitor.env
```

Controller 令牌和通知令牌必须由部署系统写入仓库外的普通文件，UID 为运行服务的 root、模式 `0600`、不得是链接。令牌不得放进 URL、环境值、命令行、日志或回执。示例配置中的域名、容器名、路径、节点下限和阈值必须逐项替换并复核。

```bash
sudoedit /etc/xs-nexus/monitor.env
sudo systemd-analyze verify \
  /etc/systemd/system/xs-nexus-healthcheck.service \
  /etc/systemd/system/xs-nexus-healthcheck.timer
sudo systemctl daemon-reload
sudo systemctl start xs-nexus-healthcheck.service
sudo systemctl enable --now xs-nexus-healthcheck.timer
```

首次运行必须先查看结果，不能只确认进程退出：

```bash
sudo systemctl status xs-nexus-healthcheck.service --no-pager
sudo journalctl -u xs-nexus-healthcheck.service -n 200 --no-pager
sudo python3 -m json.tool /var/lib/xs-nexus-monitor/snapshot.json >/dev/null
sudo python3 -m json.tool /var/lib/xs-nexus-monitor/alerts.json >/dev/null
sudo sed -n '1,80p' /var/lib/xs-nexus-monitor/metrics.prom
```

## 3. 配置安全边界

- `XS_MONITOR_CONTROLLER_OBSERVABILITY_URL` 必须使用 loopback；采集器不会把 Controller Bearer token 发送到远端主机。
- HTTP health probe 只允许 loopback；远端入口必须使用 HTTPS。probe URL 禁止凭据、query 和 fragment，curl 配置文件被禁用且不跟随重定向。
- Webhook 正式配置只允许 HTTPS，禁止 URL 凭据、query 和 fragment，并强制使用独立 `0600` token 文件。HTTP 仅在显式测试模式下允许。
- `XS_MONITOR_TEST_MODE`、`XS_MONITOR_PROC_ROOT`、`XS_MONITOR_TEST_DISK_USAGE_PERCENT` 和 `XS_MONITOR_ALLOW_HTTP_WEBHOOK_FOR_TESTS` 不得写入生产配置。
- `XS_MONITOR_TLS_TARGETS` 必须至少包含一个正式 SNI 目标；空配置和严格 TLS 失败均为 critical。
- 备份目录、复制回执和深度验证回执缺失、不安全、陈旧或未来时间均为 critical。
- 输出目录和状态目录必须由服务 UID 拥有且无 group/other 权限。systemd 的 `StateDirectory=xs-nexus-monitor` 会创建 `/var/lib/xs-nexus-monitor` 模式 `0700`。

旧配置名 `XS_MONITOR_DISK_WARNING_PERCENT` 和 `XS_MONITOR_DISK_CRITICAL_PERCENT` 保持兼容；其他阈值使用示例中的 `*_WARNING`/`*_CRITICAL`。warning 必须严格小于 critical。

## 4. 备份回执契约

健康守卫不持有 age identity，也不解密备份。备份自动化必须只在真实操作成功后，将以下 root-owned `0600` JSON 原子替换到监控目录。

复制/公开校验成功：

```json
{
  "schema_version": 1,
  "operation": "replica_copy",
  "status": "PASS",
  "completed_at": "2026-08-12T00:00:00Z"
}
```

使用离线 identity 完成深度认证和 `pg_restore --list` 后：

```json
{
  "schema_version": 1,
  "operation": "deep_verification",
  "status": "PASS",
  "completed_at": "2026-08-12T00:00:00Z"
}
```

允许增加无秘密的备份名、目标 ID 和证据摘要字段，但不得写入数据库 URL、identity、recipient 私钥或任何 Token。推荐在调度脚本中使用同一 `if` 分支绑定命令和回执：

```bash
if sudo ./deploy/docker/xs-nexus-stack.sh \
  --env-file /etc/xs-nexus/deployments/rc.compose.env \
  verify-backup BACKUP_NAME; then
  completed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  sudo env COMPLETED_AT="$completed_at" sh -c '
    umask 077
    receipt=$(mktemp /var/lib/xs-nexus-monitor/.backup-copy.XXXXXX)
    printf "{\"schema_version\":1,\"operation\":\"replica_copy\",\"status\":\"PASS\",\"completed_at\":\"%s\"}\n" \
      "$COMPLETED_AT" >"$receipt"
    chmod 0600 "$receipt"
    mv -f -- "$receipt" /var/lib/xs-nexus-monitor/backup-copy-receipt.json
  '
fi
```

深度验证回执必须由执行 `verify-backup-deep` 的受控作业按同一模式写入，且离线 identity 在操作后立即卸载。手工创建回执或仅检查文件存在不算生产证据。

## 5. 告警生命周期

- 首次出现 warning/critical：发送 `firing`；
- 同一 key、同一严重度持续存在：去重，不重复发送；
- 严重度变化：发送新的 `firing`；
- 条件消失：发送 `resolved`；
- 发送失败：事件保留并在下次运行重试，本机产生 `notification.delivery` critical；
- 队列最多 256 个唯一事件，溢出产生 `notification.queue_overflow` critical，而不是静默丢弃；
- Webhook 事件有稳定 ID，接收端必须按 ID 幂等处理；重试可能造成重复送达。

未配置外部 Webhook 时采集器记录 warning 并保留待发送事件，但不会把本机状态称为通知成功。critical 使 oneshot 单元失败；warning 保持退出码 0，供外部平台按指标和事件升级。

## 6. 输出、保留与隐私

默认输出：

- `/var/lib/xs-nexus-monitor/metrics.prom`：最新 Prometheus 文本，模式 `0644`，父目录仍为 `0700`；
- `/var/lib/xs-nexus-monitor/snapshot.json`：最新完整聚合快照，模式 `0600`；
- `/var/lib/xs-nexus-monitor/alerts.json`：当前告警和待发送事件，模式 `0600`；
- `/var/lib/xs-nexus-monitor/state.json`：去重状态，模式 `0600`。

四个文件均原子替换，因此本组件只保留最新状态，不形成无界历史。历史指标、日志和通知留存由独立平台负责；不得为了接入采集器而把整个状态目录改为公开。journal 的容量和保留由宿主 journald 策略约束。Controller 审计事件继续按数据库策略保留，本采集器不删除审计记录。

标签只包含静态配置的容器名、路径或探测目标；不包含节点、网络、Relay、用户或 Token 标识。任何新增指标在上线前都必须复核基数和敏感性。

## 7. 生产门禁验证

仓库测试覆盖健康采集、真实本地 TLS 握手、warning、critical、去重、严重度变化、恢复、容器/HTTP 失败、通知失败、私有文件权限、URL 限制、队列上限、凭据不落盘和原子写入。CI 夹具只证明代码路径，不证明生产送达。

Gate 20 只有在以下证据全部存在时才能从 `PARTIAL` 改为 `PASS`：

1. 精确发布 revision 的 Controller、Relay、数据库、主机、TLS 和备份指标持续采集；
2. 独立于被监控主机的正式目的地收到至少一次 warning 和一次 critical；
3. 文档化 on-call 完成确认、升级和处置；
4. 故障移除后同一事件收到 `resolved` 并关闭；
5. 通知平台、证书和备份监控留存满足正式策略；
6. 证据目录通过 SHA-256 和无值秘密扫描。

在外部目的地、正式域名/证书、正式备份和 on-call 未提供前，必须保持 `BLOCKED_EXTERNAL`，不得以本地或模拟结果替代。
