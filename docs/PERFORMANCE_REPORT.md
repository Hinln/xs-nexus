# XS Nexus 性能与稳定性报告

状态：`COMPLETE_FOR_CURRENT_LINUX_BASELINE`  
报告日期：2026-08-10  
适用范围：当前 Linux 测试服务器、GitHub hosted Linux、loopback/namespace 网络与真实外部 PostgreSQL。

本报告记录可复现的工程基线，不是公网容量承诺。24 小时 Linux 容器稳定性已经完成；Windows 实机、运营商网络和 NAS 门禁仍未完成，因此不得据此描述为生产就绪。

## 1. 测试原则

- 使用 release profile 测量密码学和 Relay 热路径；debug profile 结果不作为性能结论。
- Controller 规模测试经过真实 Router、认证、Enrollment、签名配置、IPAM 和 PostgreSQL，不使用批量数据库插入代替注册。
- Direct/Relay RTT 使用两个真实 Agent、TUN、network namespace、XSP/1 和业务 ICMP；不使用独立 UDP echo 代替产品路径。
- 保留全部样本和最大尖峰，不裁剪异常值。
- 短时资源校准不能替代 24 小时稳定性。
- Agent RTT 与空闲资源脚本强制构建并运行 release profile，同时把配置二进制路径和两个进程的 `/proc/<pid>/exe` 写入证据；路径不一致时测试失败。此前 debug profile 的约 8.5% 单核 CPU 与约 16 MiB RSS 仅用于发现基准配置错误，不作为产品性能结论。

## 2. 当前结果

| 项目 | 结果 | 证据 |
|---|---:|---|
| XSP/1 1200 字节 seal+open | 161,314 往返/s；184.61 MiB/s | `/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z` |
| UDP Relay 216 字节帧 | 87,822 包/s；18.09 MiB/s | `/srv/xs-nexus/artifacts/qa/relay-throughput-20260731T211903Z` |
| Relay 内部转发延迟 | 平均 4 µs；最大 161 µs | 同上 |
| UDP Relay 持续 5,000,000 个 216 字节帧 | 71,212.14 包/s；14.67 MiB/s；70.213 秒；零 Relay 丢弃 | GitHub Actions run `31358498444`，artifact `9051561308` |
| 持续 Relay 内部转发延迟 | 平均 5 µs；最大 150 µs；最终队列 0 包/0 字节 | 同上 |
| Controller 100 节点注册 | 39.18/s | `/srv/xs-nexus/artifacts/qa/controller-scale-20260731T210210Z` |
| Controller 500 节点增量注册 | 18.20/s | 同上 |
| Controller 1000 节点增量注册 | 8.66/s | 同上 |
| 100/500/1000 节点 Console 快照 | 50.2 / 130.9 / 192.6 ms | 同上 |
| PostgreSQL 节点计数查询 | 1.70–2.72 ms | 同上 |
| Direct RTT，30 样本 | 平均 0.91 ms；p50 0.90 ms；p95 1.13 ms；最大 1.18 ms | `/srv/xs-nexus/artifacts/qa/agent-rtt-20260731T221036Z` |
| Relay RTT，30 样本 | 平均 1.16 ms；p50 1.13 ms；p95 1.45 ms；最大 1.45 ms | 同上 |
| Relay RTT 增量 | 平均 +0.25 ms；p95 +0.32 ms | 同上 |
| Linux Agent 空闲资源（每实例） | 平均 0.55% 单核 CPU；7.95 MiB RSS；9 线程；15 FD | 同上 |
| Controller/Relay/Console 24 小时稳定性 | 每服务 1420 次采样；恰好一次受控 PID 转换；无自动重启 | `/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T212242Z` 与修正回归 `runtime-stability-20260802T061521Z` |

### 2.1 Gate 08 持续 Relay 基线

精确 revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` 的 release-profile 测试使用真实 UDP socket、两份 Controller 签名凭证和短期 Relay Lease。发送端以最多 64 帧窗口保留真实转发路径；每 400,000 帧由两个节点在相同端点、相同身份下用唯一签名请求续租，并在新 Lease 中重置独立序列。这模拟 Agent 必须遵守的短期 Lease 行为，不延长生产限制或关闭过期检查。

测试要求精确转发全部 5,000,000 帧，Relay `packets_dropped=0`，最终全局队列 packet/byte gauge 均为 0，并保持至少 10,000 包/s。最终报告为 71,212.14 包/s、14.67 MiB/s、平均内部转发延迟 5 µs、最大 150 µs。该结果只建立 hosted 单进程 loopback 安全下限；它不包含公网链路丢包、多地域、云侧 DDoS、水平扩展或小时/天级连接风暴。

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

状态：`PASSED_WITH_CORRECTED_RESTART_GATE`。

启动参数：

```bash
XS_STABILITY_DURATION_SECONDS=86400 \
XS_STABILITY_SAMPLE_INTERVAL_SECONDS=60 \
make test-docker-deployment
```

长测从 2026-07-31 21:22:42 UTC 运行至 2026-08-01 21:21:41 UTC。`resources.csv` 包含 4261 行（表头加三服务各 1420 个样本）。结果：

- Controller RSS 6.35–9.41 MiB、线程 10–11、FD 15–17、日志 1676→2514 字节；
- Relay RSS 2.61–5.12 MiB、线程固定 10、FD 固定 14、日志 346→1040 字节；
- Console RSS 24.48–31.92 MiB、线程固定 10、FD 161–163、日志 3416→14423 字节；
- 每个服务恰好观察到重启前后两个 PID，`RestartCount` 全部样本均为 0；未发现无界 FD/线程增长或额外自动重启；
- 测试结束后项目测试容器和 schema 清理完成，既有 `1panel-network`、默认路由、nftables 和 1Panel 服务保持不变。

旧脚本最后错误要求手动 `docker restart` 增加 Docker 的自动重启计数，因此在完整采样和三次健康恢复之后以错误断言退出。`XS-2026-0004` 修复门禁为：重启输出必须精确等于目标完整容器 ID、每服务恰好两个 PID、`RestartCount` 必须保持基线。修正后的真实 60 秒部署回归 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z` 通过，三服务各 11 个样本并验证同一组重启与清理不变量。长样本与修正语义合并构成当前 Linux 24 小时验收证据；没有把旧脚本的非零退出隐藏为成功。

## 5. 尚未覆盖

- Windows Agent 空闲 CPU/内存和 Windows 驱动数据面；
- NAS 真实设备；
- 真实公网、运营商 NAT、跨地域 Relay 容量和 RTT；
- 多核/多实例水平扩展、小时/天级连接风暴和第三方容量复核；
- 正式 RC 构建与签名后的性能复验。
