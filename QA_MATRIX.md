# QA_MATRIX.md — 测试矩阵

测试结果写入 `artifacts/qa/`，每次运行使用独立时间戳目录。

---

## 1. 平台矩阵

| 平台 | 架构 | 目标 |
|---|---|---|
| Linux Server | x86_64 | Controller、Relay、Agent |
| Linux/NAS | arm64 或 x86_64 | Agent |
| Windows 11 LTSC 2024 测试 VM | x86_64 | Driver、Agent、Installer |
| Windows 10 | x86_64 | 兼容性，条件允许时 |
| Chromium | 最新固定版本 | Console |
| Firefox | 最新固定版本 | Console 基础兼容 |
| WebKit | 固定版本 | Console 基础兼容 |

---

## 2. 网络矩阵

- 同一 namespace bridge；
- 同一局域网；
- 两端普通 NAT；
- 端口受限；
- 对称 NAT；
- 一端公网；
- 双端公网；
- IPv6；
- UDP 丢弃；
- 高延迟；
- 丢包 1%、5%、20%；
- 乱序；
- 重复包；
- MTU 1280/1400/1500；
- 公网 IP 变化；
- 接口切换；
- Relay 故障；
- Controller 故障。

每项记录：

- 是否 Direct；
- 是否 Relay；
- 建链时间；
- RTT；
- 丢包；
- 切换时间；
- 恢复时间；
- 错误码；
- 日志路径。

### M2.1 自动化覆盖

- `apps/controller/tests/controller_db.rs` 使用真实 PostgreSQL 和 UDP socket 验证 XSD/1 活动凭证、响应小于请求、篡改静默丢弃、候选签名、generation 幂等和冲突拒绝；
- `scripts/test-agent-control.sh` 验证 Agent 通过认证 WebSocket 发布节点签名候选并应用 Controller 重新签名的动态配置；
- `scripts/test-agent-candidate-fallback.sh` 在两个 namespace 中证明首选候选不可达时确实被尝试，随后按优先级回退并报告 `handshake_fallback`；
- `scripts/test-agent-candidate-path.sh` 验证发现请求复用数据面 UDP 源端口、初始加密会话、更高优先级路径的 AEAD Challenge/Response、`authenticated_path_probe` 和双向业务连续性；
- `scripts/validate-m21.sh` 汇总格式化、Clippy、构建、单测、真实数据库、协议向量、三组 namespace 数据面测试、秘密扫描以及 Docker、`1panel-network`、默认路由和 nftables 前后基线。
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.1-20260729T175243Z`。

### M2.2 自动化覆盖

- `scripts/test-agent-proactive-punch.sh` 在同 LAN namespace 中不注入 TUN 流量，验证双方主动认证握手、并发冲突决议、周期 Keepalive、Direct 路径和双向 ICMP；
- `scripts/test-agent-nat-matrix.sh` 使用一次性 namespace、veth、bridge、nftables DNAT/SNAT 和过滤规则，覆盖 Full-cone 类、Restricted、Port-restricted、双端 NAT、公网 IP 重绑定、对称 NAT 无法直连、UDP 封锁和解封恢复；
- 公网重绑定只有在 Header 绑定当前 Network/Node/Session 且 AEAD 与重放验证通过后晋升为 `authenticated_peer_traffic`；
- 对称 NAT 和 UDP 封锁场景明确验证 Direct 不会错误建立；Relay 回退属于 M2.3，不在 M2.2 伪造成功；
- `scripts/validate-m22.sh` 汇总 M2.1 全部回归、主动打洞/NAT 矩阵、格式化、Clippy、构建、单测、真实 PostgreSQL、秘密扫描以及 Docker、`1panel-network`、默认路由和 nftables 前后基线；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.2-20260730T092547Z`。

### M2.3 自动化覆盖

- `crates/protocol/src/relay.rs` 与 Relay 协议向量覆盖注册、Lease、Data、Keepalive、独立签名域、长度边界、篡改和错误 framing；
- `apps/relay/src/server.rs` 单测使用真实 UDP socket 覆盖活动凭证认证、Lease/来源端点绑定、转发、重放拒绝、Keepalive、包速率限制和限速窗口恢复；
- `scripts/test-agent-relay.sh` 在两个 namespace、两个 Relay 和 nftables Direct 阻断中验证 `relay_fallback`、端到端密文抓包、伪造来源认证丢弃、主 Relay 停止后的 `relay_failover`，以及 Direct 恢复后的 AEAD `authenticated_path_probe` 回切；
- Relay 抓包同时禁止出现原始虚拟 IP 数据包和业务明文标记，Relay 只看到有限路由元数据和逐字节不变的 XSP/1 密文；
- `scripts/validate-m23.sh` 汇总 M2.2 全部回归、Relay 协议/服务/Agent 测试、格式化、Clippy、构建、真实 PostgreSQL、秘密扫描、ShellCheck、npm audit，以及 Docker 容器/网络、`1panel-network` 成员、默认路由和 nftables 前后基线；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.3-20260730T113304Z`。

### M3.1 自动化覆盖

- `crates/core/src/acl.rs` 单测覆盖默认拒绝、节点/组/标签 selector、Allow/Deny、稳定优先级、TCP/UDP/ICMP、端口范围、无效规则和解释原因；
- `apps/agent/src/state.rs` 单测覆盖策略签名、严格版本递增、同版本异内容、低版本、无签名和无效 ACL，失败时验证最近有效状态逐字节不变；
- `apps/controller/tests/controller_db.rs` 使用真实 PostgreSQL 覆盖组和 ACL 原子替换、乐观版本、解释 API、配置签名、活动地址唯一、节点吊销、地址冷却复用和地址池重叠拒绝；
- `apps/agent/tests/tun_lifecycle.rs` 在隔离 namespace 中安装冲突系统路由并验证 Agent 在创建 TUN 前失败关闭，不覆盖宿主路由；
- `scripts/test-agent-acl.sh` 在两个真实 namespace 中覆盖 ICMP/TCP/UDP 允许、端口拒绝、发送端拒绝、接收端拒绝和被拒绝明文不进入目标链路；
- `scripts/validate-m31.sh` 汇总全部既有网络回归、ACL/IPAM 测试、格式化、Clippy、构建、真实 PostgreSQL、秘密扫描、ShellCheck、npm audit，以及 Docker、`1panel-network`、默认路由和 nftables 前后基线；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`。

### M3.2 自动化覆盖

- `crates/core/src/routes.rs` 单测覆盖安全网段、虚拟池冲突、部分重叠、同前缀优先级、网关候选过期和路由解析；
- `apps/controller/tests/controller_db.rs` 使用真实 PostgreSQL 覆盖签名建议持久化、无建议审批拒绝、重叠拒绝、纯路由/NAT 启用、暂停、撤销、配置版本和审计；
- `apps/agent/tests/tun_lifecycle.rs` 在隔离 namespace 中覆盖客户端路由、网关候选过期、纯路由转发、精确 NAT 规则、系统路由冲突、暂停、shutdown 和 stale manifest 恢复；
- `crates/protocol/tests/session.rs` 验证普通虚拟地址会话继续严格绑定，只有显式策略守卫的 routed API 才接受子网内层地址；
- `scripts/test-agent-subnet-route.sh` 使用客户端、网关和 LAN 三个 namespace 覆盖审批前不可达、纯路由和 NAT ICMP/TCP、伪造源拒绝、网关离线撤销与资源清理；
- `scripts/validate-m32.sh` 汇总全部既有回归、M3.2 测试、格式化、Clippy、构建、秘密扫描、ShellCheck、npm audit，以及 Docker、`1panel-network`、默认路由和 nftables 前后基线；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`。

---

## 3. 协议负向测试

- 错误 Magic；
- 不支持版本；
- 错误 Network ID；
- 错误 Source Node；
- 错误 Destination Node；
- 长度短于包头；
- 长度溢出；
- AEAD Tag 错误；
- 旧 Key Epoch；
- 未来 Key Epoch；
- 重放；
- 序列号窗口边界；
- 随机密文；
- 握手乱序；
- 握手重复；
- 签名错误；
- 过期凭证；
- 吊销凭证；
- 降级字段篡改；
- XSD/1 错误长度、类型、保留字段、时间、凭证、节点签名、请求哈希和 Controller 签名；
- 候选广告错误 Network/Node、重复端点/优先级、过期、Relay 类型、低 generation 和同 generation 不同内容；
- PathResponse 错误来源端点、Path ID、token、AEAD Tag 和重放；
- NAT rebinding 的错误 Network、Source Node、Destination Node、Session ID、AEAD Tag 和重放；
- 资源耗尽攻击。

### M1.3 自动化覆盖

- `crates/protocol/tests/session.rs` 覆盖握手乱序、签名/transcript/AAD/Tag 篡改、重放窗口、虚拟源地址和 Epoch 边界；
- `scripts/test-agent-data-plane.sh` 在两个真实 namespace 中覆盖双向 ICMP/TCP/UDP、原始业务包不可见、自动 Key Epoch、Tag 篡改、重放、伪造源地址和 Controller 中断；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`。

---

## 4. 路由与 ACL

- 节点到节点允许；
- 节点到节点拒绝；
- TCP 端口；
- UDP 端口；
- ICMP；
- 多规则优先级；
- 组和标签；
- 旧策略；
- 无签名策略；
- 低版本策略；
- 重叠子网；
- 网关离线；
- 源地址伪造；
- 未审批网段；
- NAT 模式；
- 纯路由模式。

---

## 5. 安装和生命周期

### Linux

- 干净安装；
- 重复安装；
- 无效 Token；
- Token 过期；
- 下载中断；
- 哈希错误；
- 签名错误；
- 服务启动失败；
- 升级；
- 升级失败；
- 回滚；
- 卸载；
- 卸载后重装；
- 重启后恢复。

### Windows

- 干净安装；
- 驱动安装；
- 驱动升级；
- Agent 升级；
- 安装失败回滚；
- Driver Verifier；
- 睡眠唤醒；
- 网卡变化；
- 卸载；
- 卸载后重装；
- 无残留设备和路由。

---

## 6. 控制面

- 数据库断开；
- Redis 断开；
- Controller 重启；
- 多实例；
- WebSocket 断线；
- 配置乱序；
- 节点频繁上线离线；
- Token 并发使用；
- IP 并发分配；
- 节点吊销；
- 审计完整性；
- 权限越权；
- 登录限速；
- 会话过期；
- CSRF/XSS/注入基础检查。

---

## 7. 性能

至少报告：

- Agent 空闲 CPU/内存；
- 加密吞吐；
- Relay 吞吐；
- Direct RTT 增量；
- Relay RTT 增量；
- Controller 注册吞吐；
- 在线节点规模模拟；
- WebSocket 广播；
- 数据库查询；
- 日志写入；
- 24 小时资源曲线。

性能不达标时不得用隐藏采样、关闭安全校验或降低加密强度解决。

---

## 8. 真实设备验收

### NAS

- 主动注册；
- 普通节点；
- Direct；
- Relay；
- 更新；
- 卸载；
- 子网发布；
- ACL；
- 网关离线。

### 本地 Windows

- 仅在测试 VM 通过后；
- 安装 RC；
- 与 NAS 不同公网出口；
- 手机热点；
- UDP 封锁；
- Direct/Relay；
- 睡眠恢复；
- 卸载。
