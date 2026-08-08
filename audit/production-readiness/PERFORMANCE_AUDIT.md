# Performance Audit

## Fresh Measurements

- XSP/1 加解密：约 135,520 ops/s、155.09 MiB/s，1200-byte packet，50,000 iterations。
- Relay 内核路径：约 88,596 packets/s、18.25 MiB/s，平均转发延迟 3 µs，最大 76 µs。
- namespace Direct RTT：平均 0.664 ms，p95 1.070 ms。
- namespace Relay RTT：平均 0.943 ms，p95 0.879 ms；30 samples/path。
- 两个 release Agent：平均 RSS 8.34 MiB/agent，单核 idle CPU 约 0.60%，15 FDs、3 threads。
- Controller 100/500/1000 节点测试通过；1000 节点累计注册约 92.0 s，1000 节点 Console snapshot 约 315 ms。

## Limitations

- 所有吞吐/RTT 均为单机或 namespace，不是 WAN。
- 未测 Relay concurrent sessions、WebSocket 1000 长连接、生产 DB load、丢包恢复、CPU/Mbps 和长期内存。
- 没有基于当前服务器设置 warning threshold、hard limit 和 admission limit。
- 首次包装器漏传报告变量的失败证据保留；直接固定 QA 容器重跑后 JSON 生成成功。

## Result

`PARTIAL`。这些数据适合开发基线，不足以确定正式公网容量。

## Final Remediation Reassessment

本轮修复没有重新执行 WAN、容量上限或长期性能测试，也不把 CI 时间当作性能证据。状态保持 `PARTIAL`。
