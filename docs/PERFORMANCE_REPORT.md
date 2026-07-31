# XS Nexus 性能与稳定性报告

状态：`IN_PROGRESS`  
报告日期：2026-07-31  
适用范围：当前 Linux 测试服务器、loopback/namespace 网络与真实外部 PostgreSQL。

本报告记录可复现的工程基线，不是公网容量承诺。Windows 实机、运营商网络、NAS 和 24 小时稳定性完成前，不得据此描述为生产就绪。

## 1. 测试原则

- 使用 release profile 测量密码学和 Relay 热路径；debug profile 结果不作为性能结论。
- Controller 规模测试经过真实 Router、认证、Enrollment、签名配置、IPAM 和 PostgreSQL，不使用批量数据库插入代替注册。
- Direct/Relay RTT 使用两个真实 Agent、TUN、network namespace、XSP/1 和业务 ICMP；不使用独立 UDP echo 代替产品路径。
- 保留全部样本和最大尖峰，不裁剪异常值。
- 短时资源校准不能替代 24 小时稳定性。

## 2. 当前结果

| 项目 | 结果 | 证据 |
|---|---:|---|
| XSP/1 1200 字节 seal+open | 161,314 往返/s；184.61 MiB/s | `/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z` |
| UDP Relay 216 字节帧 | 87,822 包/s；18.09 MiB/s | `/srv/xs-nexus/artifacts/qa/relay-throughput-20260731T211903Z` |
| Relay 内部转发延迟 | 平均 4 µs；最大 161 µs | 同上 |
| Controller 100 节点注册 | 39.18/s | `/srv/xs-nexus/artifacts/qa/controller-scale-20260731T210210Z` |
| Controller 500 节点增量注册 | 18.20/s | 同上 |
| Controller 1000 节点增量注册 | 8.66/s | 同上 |
| 100/500/1000 节点 Console 快照 | 50.2 / 130.9 / 192.6 ms | 同上 |
| PostgreSQL 节点计数查询 | 1.70–2.72 ms | 同上 |
| Direct RTT，30 样本 | 平均 2.87 ms；p50 1.88 ms；p95 2.19 ms；最大 31.31 ms | `/srv/xs-nexus/artifacts/qa/agent-rtt-20260731T214224Z` |
| Relay RTT，30 样本 | 平均 4.38 ms；p50 2.30 ms；p95 27.21 ms；最大 41.83 ms | 同上 |
| Relay RTT 增量 | 平均 +1.51 ms；p95 +25.02 ms | 同上 |

## 3. 运行时短时校准

命令：

```bash
XS_STABILITY_DURATION_SECONDS=30 \
XS_STABILITY_SAMPLE_INTERVAL_SECONDS=5 \
make test-docker-deployment
```

证据：`/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T205004Z`。

Controller、Relay、Console 均完成受控重启和健康恢复，并采集完整容器进程树的 RSS、线程、FD、CPU ticks 与日志大小。Compose 日志上限固定为 `10m × 5`。该结果只证明采样器和故障恢复路径可执行，不证明无长期资源泄漏。

## 4. 24 小时稳定性

状态：`RUNNING`。

启动参数：

```bash
XS_STABILITY_DURATION_SECONDS=86400 \
XS_STABILITY_SAMPLE_INTERVAL_SECONDS=60 \
make test-docker-deployment
```

完成后必须审计：

- 三服务资源曲线、PID 变化、FD 和线程上限；
- 日志增长及轮转上限；
- Controller、Relay、Console 故障恢复；
- Docker 测试容器、网络、`1panel-network`、默认路由和 nftables 清理基线；
- 是否存在持续单调增长或无法解释的尖峰。

在完整证据生成和审计前，`ACCEPTANCE.md` 中 24 小时、资源泄漏、日志增长与最终性能报告保持未勾选。

## 5. 尚未覆盖

- Windows Agent 空闲 CPU/内存和 Windows 驱动数据面；
- NAS 真实设备；
- 真实公网、运营商 NAT、跨地域 Relay 容量和 RTT；
- 多核/多实例水平扩展、长期连接风暴和第三方容量复核；
- 正式 RC 构建与签名后的性能复验。
