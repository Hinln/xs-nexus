# Network Audit

## Production Baseline

- 公网 TCP：80、122、188、443；公网 UDP：443、42000、42001。
- Controller/Console HTTP 仅绑定 `127.0.0.1:28080/28081`；数据库未发布主机端口。
- IPv4 INPUT policy 为 accept，现有规则主要是大规模 IP deny list，不是最小 allowlist。
- `1panel-network` 为 external bridge `172.18.0.0/16`，Compose 不管理或删除它。

## Linux Agent

- `/dev/net/tun`、namespace 和 nftables 能力存在。
- M5.2 真实运行 TUN lifecycle、systemd crash cleanup、XSP/1、candidate、NAT、Relay、ACL 和 subnet route namespace 测试，前后无 namespace/TUN/默认路由/nftables 漂移。
- 生产服务器未安装 `xs-agent.service`，没有生产 Agent 实机失败矩阵。

## Direct, NAT And Relay

- namespace full-cone、restricted、port-restricted、symmetric、public rebind、UDP blocked recovery 通过。
- Relay 密文、认证、failover 和 Direct 恢复通过。
- 没有真实不同运营商公网、移动热点、IPv6、Wi-Fi/Ethernet 切换或 WAN 丢包/重排证据。

## ACL And Subnet Routing

- namespace 发送端/接收端 ACL、伪造、默认拒绝和 subnet route 流程通过。
- 没有真实 NAS 网关、生产路由传播、reboot/offline/uninstall 证据。

## Result

Linux namespace 基础可靠，但 Gate 06/08/09 仅 PARTIAL，Gate 07/10 为 SIMULATED_ONLY。公网与生产主机网络不满足正式 PASS。

## Final Remediation Reassessment

最终生产复核确认四个项目容器健康、`1panel-network` ID/子网/四成员不变、默认路由仍经 `eth0`，没有执行生产 namespace、TUN、路由或防火墙写入。协议 fuzz 改善输入健壮性，但没有新增真实 WAN、Windows 或 NAS 证据；原 Gate 结果不变。
