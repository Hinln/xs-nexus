# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-30 11:35 UTC  
当前 Git 提交：M2.3 完成检查点（以本文件所在提交为准）  
当前总状态：`ACTIVE_AUTONOMOUS_DEVELOPMENT`
当前里程碑：`M3.1 路由、ACL 与 IPAM 完整化`

---

## 已完成

- M0.1 系统、资源、网络、防火墙、Docker、1Panel、服务和工具链基线；
- 隔离 namespace 中的真实 TUN、nftables 和 capability 验证；
- 外部 Secret 文件和 Git 忽略保护；
- Rust workspace、Controller、Relay、Agent、CLI 最小组件；
- React/Vite 控制台 workspace；
- 统一 Make、CI、秘密扫描、组件集成和网络能力脚本；
- M0.1 完整验证和 Git 检查点 `c519e34`；
- M0.2 clean-room、总体架构、威胁模型、安全假设、XSP/1 字节级协议与测试向量；
- M0.2 证据 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log` 和 Git 检查点 `86f8cd9`；
- M1.1 PostgreSQL schema、迁移、稳定 IPAM、一次性 Enrollment Token、节点凭证、配置签名、严格 API 和 WebSocket challenge；
- M1.1 证据 `/srv/xs-nexus/artifacts/qa/m1.1-20260729T105009Z/validate-m11.log` 和 Git 检查点 `fe1f9db`；
- M1.2 Agent 严格配置、本地 Ed25519 身份、`0700`/`0600` 权限、原子状态持久化和中间检查点 `c4e8e44`；
- Agent 与真实 Controller/PostgreSQL enrollment、challenge 认证和配置同步集成测试；
- 非持久 Linux TUN FD 与 Netlink MTU、`/32` 地址、接口启用和地址池路由；
- 默认路由保护、同名接口拒绝、shutdown、Drop 和 stale manifest 恢复；
- 严格只读 Unix IPC、`xs status`、`xs peers`、`xs diagnostics` 和 JSON 输出；
- 最小权限 systemd 单元和 `PrivateNetwork=yes` transient service 生命周期验证；
- M1.2 全量证据 `/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`；
- XSP/1 四消息 typestate 握手、Ed25519 身份验证、X25519、HKDF-SHA-256、ChaCha20-Poly1305、双向 Finish 和方向密钥；
- 96 字节认证数据头、IPv4 源/目标绑定、1024 位重放窗口、严格 Key Epoch、旧 Epoch 退休、协议向量和 Fuzz seed corpus；
- Agent 签名 Peer 目录、UDP 会话、握手重试与冲突决议、有界队列、TUN/UDP 双向循环和严格 endpoint 绑定；
- 单方向确认式自动 Key Epoch，生产阈值为 `2^20` 个数据包或 1 小时，旧 Epoch 保留 30 秒；
- 两个隔离 Linux namespace 中的双向 ICMP/TCP/UDP、密文抓包、Tag 篡改、重放、伪造源地址和 Controller 中断容错；
- M1.3 全量证据 `/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`；
- 固定长度认证 `XSD/1` UDP 地址发现，Request 334 字节、Response 188 字节，绑定活动凭证、节点身份、时间、观察端点和精确请求哈希；
- Controller 每来源发现限速、活动节点数据库检查、无效请求静默丢弃、签名候选持久化、幂等重试和 generation 冲突拒绝；
- Agent Netlink IPv4/IPv6 候选收集、同数据面 UDP socket 映射发现、10 分钟生命周期、4 分钟刷新和最多 16 个候选；
- 节点签名候选通过认证 WebSocket 交换，动态 Controller 签名配置在不丢弃已建立会话的情况下应用；
- 多候选握手有界回退、AEAD PathChallenge/PathResponse、更高优先级路径晋升，以及 CLI/IPC 候选、活动端点和路径原因可观测性；
- 隔离 namespace 验证首选候选确实被尝试后回退、发现请求复用数据面源端口、认证路径晋升和双向 ICMP 连续性；
- M2.1 全量证据 `/srv/xs-nexus/artifacts/qa/m2.1-20260729T175243Z`。
- 双方主动认证 ClientHello 调度、同时握手冲突决议、每进程/每 Tick/每 Peer 资源上限和失败指数退避；
- 已建立 XSP/1 会话的 AEAD Keepalive，生产 15 秒、特权网络测试 1 秒，保持 NAT 映射而不发送明文探测；
- 未知 UDP 来源的 NAT rebinding 只有在 Network/Node/Session Header 预筛选及 AEAD、Epoch、序列、重放验证成功后晋升；
- 同 LAN、Full-cone 类、Restricted、Port-restricted、双端 NAT、公网 IP 重绑定、对称 NAT 无 Direct、UDP 封锁和解封恢复的隔离 namespace/nftables 矩阵；
- M2.1 候选优先级、`handshake_fallback` 和 AEAD PathChallenge/PathResponse 回归保持通过；
- M2.2 全量证据 `/srv/xs-nexus/artifacts/qa/m2.2-20260730T092547Z`。
- 独立 `XSR/1` Relay 协议：352 字节节点注册、168 字节 Relay 签名 Lease、104 字节路由 envelope，以及最多 1396 字节逐字节不变的端到端 XSP/1 密文；
- Relay 活动凭证与节点签名认证、来源端点和 Lease 绑定、每 Lease sequence 重放窗口、空闲/租约清理、并发/包速率/字节速率/队列上限和无匿名转发；
- Controller 严格权限 Relay catalog、签名配置下发、优先级和重复端点拒绝，以及 Agent 双 Relay 注册、续租、健康探测和候选接入；
- Direct 失败后自动 `relay_fallback`、主 Relay 故障后 `relay_failover`，以及 Direct 恢复后的 AEAD `authenticated_path_probe` 回切；
- Relay 链路抓包验证不包含原始虚拟 IP 包和业务明文标记，伪造外层来源被认证丢弃；
- UDP 单次发送失败按路径不可达处理，不再终止 Agent；仅候选路由身份变化才重置回退，候选过期时间刷新不会破坏正在进行的握手；
- M2.3 全量证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260730T113304Z`。

## 当前工作点

- M2.3 已完成全部计划内实现和验证，宿主机无测试 TUN、namespace、bridge、nftables、默认路由、Docker 网络或 `1panel-network` 变化；
- Relay 已具备认证、限速、队列、健康检查和基础错误/字节指标；全局验收中的延迟与丢包指标仍未勾选，不在本里程碑伪造完成；
- M3.1 开始默认拒绝 ACL、节点/组/标签、协议/端口匹配、双端执行、策略签名与 IPAM 冲突完整化。

## 下一步

1. 读取 M3.1 验收与现有配置/数据面结构，定义默认拒绝策略模型和稳定编码；
2. 完成节点、组、标签、TCP、UDP、ICMP 与端口规则的 Controller 持久化和签名下发；
3. 在发送端与接收端同时执行策略，并绑定节点身份、虚拟源 IP 和策略版本；
4. 完成 `100.88.0.0/16`、活动地址、地址池和重叠子网冲突检查；
5. 建立 ACL 正负、离线旧策略和 IPAM 并发回归矩阵。

## 下一条准确命令

```bash
sed -n '315,370p' EXECUTION_PLAN.md
rg -n -C 8 'ACL|IPAM|policy|策略|路由' \
  docs/ARCHITECTURE.md docs/THREAT_MODEL.md crates/core apps/controller apps/agent
```

## 最近测试

- 时间：2026-07-30 11:35 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`./scripts/validate-m23.sh`；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m2.3-20260730T113304Z`；
- 覆盖：M2.2 全部回归、XSR/1 协议与 Relay 单测、真实双 Relay fallback/failover、密文抓包、伪造来源拒绝、Direct 回切、速率限制恢复、ShellCheck、秘密扫描、npm audit，以及 Docker 容器/网络、`1panel-network` 成员、默认路由和 nftables 前后基线。

## 当前失败

无未解决测试失败。

## 外部阻塞

- Windows 驱动真实测试需要 Windows 11 测试 VM、快照和 WDK；
- NAS 接入需要用户在 NAS 本地执行安装；
- 正式驱动签名需要外部签名流程；
- DNS 和生产防火墙变更需要人工批准；
- 现有 PostgreSQL、Redis 公网暴露整改需要用户批准修改 1Panel 或云防火墙，见 `BLK-005`。

## 当前风险

- 自研协议握手、AEAD 数据面、地址发现、认证路径迁移、隔离 NAT 矩阵和 Relay 已实现，但真实运营商网络、Relay 公网容量/延迟/丢包、长期 Fuzz 和独立第三方审计尚未完成；
- Windows 驱动尚未开发，真实设备尚未接入；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器提示需要维护窗口重启；
- 临时凭据后续必须轮换。

## 恢复说明

```bash
cd /srv/xs-nexus
git status --short --branch
GIT_PAGER=cat git log --oneline -10
cat PROGRESS.md
./scripts/validate-m13.sh
./scripts/validate-m21.sh
./scripts/validate-m22.sh
./scripts/validate-m23.sh
sed -n '315,370p' EXECUTION_PLAN.md
rg -n -C 8 'ACL|IPAM|policy|策略|路由' \
  docs/ARCHITECTURE.md docs/THREAT_MODEL.md crates/core apps/controller apps/agent
```

然后读取：

- `AGENTS.md`
- `EXECUTION_PLAN.md`
- 当前目录级 `AGENTS.md`

## 临时服务

无项目临时服务。Controller smoke 和 transient systemd 进程已退出，测试 namespace 已自动清理，未启动 Compose 服务。

## 清理命令

```bash
make clean
```

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
