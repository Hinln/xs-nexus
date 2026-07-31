# DECISIONS.md — 技术决策记录

---

## ADR-001：中心控制、分布式数据传输

- 状态：接受
- 决策：Controller 负责身份、配置、IPAM、候选和策略；业务数据优先节点间直连，失败才通过 Relay。
- 原因：降低中心带宽和单点影响。
- 代价：NAT、路径选择和状态同步复杂度增加。

---

## ADR-002：首版使用三层 TUN

- 状态：接受
- 决策：首版仅实现 IPv4 三层虚拟网络。
- 原因：避免二层广播、ARP 和虚拟交换机复杂度。
- 代价：依赖广播发现的应用需要额外方案。

---

## ADR-003：核心使用 Rust，Windows 驱动使用 WDK

- 状态：接受
- 决策：Controller、Relay、Agent 和协议使用 Rust；Windows 虚拟网卡驱动使用 C/C++ 与 WDK/NetAdapterCx。
- 原因：内存安全、跨平台和 Windows 驱动生态。
- 代价：多语言构建和 FFI。

---

## ADR-004：驱动最小化

- 状态：接受
- 决策：Windows 驱动只做虚拟 NIC 和受控 IPC；协议、加密、ACL、NAT 和 Relay 在用户态 Agent。
- 原因：降低内核攻击面和蓝屏风险。
- 代价：用户态/内核态数据交换需要优化。

---

## ADR-005：PostgreSQL 为唯一主关系型数据库

- 状态：接受
- 决策：使用现有 PostgreSQL；Redis 只做短期状态和协调；MySQL 不作为首版依赖。
- 原因：减少数据库复杂度。
- 代价：需为项目创建最小权限数据库用户。

---

## ADR-006：Docker 与宿主机分工

- 状态：接受
- 决策：Controller、Relay、Console 容器化；宿主机 Agent 以 systemd 运行。
- 原因：真实 TUN 和路由更容易正确控制，避免全特权容器。
- 代价：部署包含容器和宿主机两类单元。

---

## ADR-007：地址池暂定 100.88.0.0/16

- 状态：接受但需运行时检查
- 决策：默认池使用 `100.88.0.0/16`。
- 原因：与用户现有局域网分离。
- 风险：位于共享地址空间，可能与运营商或现有 VPN 冲突。
- 缓解：Agent 启动前检测，冲突时拒绝写路由并要求换池。

---

## ADR-008：标准密码原语，独立协议组合

- 状态：接受
- 决策：使用成熟库提供的 Ed25519、X25519、HKDF 和 AEAD，不实现原语；协议格式和状态机独立设计。
- 原因：满足完全自研核心协议同时避免自创算法。
- 风险：协议组合仍需独立安全审计。

---

## ADR-009：真实设备人工门禁

- 状态：接受
- 决策：NAS 和日常 Windows 的首次接入由用户手动执行。
- 原因：避免远程不可逆操作和凭据长期保存。
- 代价：最终实机验收不能完全无人值守。

---

## 新决策模板

```markdown
## ADR-NNN：标题

- 状态：提议 / 接受 / 废弃 / 替代
- 日期：
- 背景：
- 决策：
- 原因：
- 备选：
- 代价：
- 安全影响：
- 迁移：
```

---

## ADR-010：M0.1 使用发行版固定工具链

- 状态：接受
- 日期：2026-07-29
- 背景：临时服务器初始没有 Rust、Node 或本地编译器。
- 决策：使用 Ubuntu 26.04 仓库提供的 Rust 1.93.1、Node 22.22.1 和配套编译工具；前端依赖通过 npm 锁文件固定。
- 原因：避免执行未固定的远程安装脚本，便于审计和恢复。
- 代价：本地 Rust 工具链跟随发行版打包节奏。
- 安全影响：软件来源和安装日志可追溯；未执行全量系统升级。

---

## ADR-011：真实配置位于仓库外

- 状态：接受
- 日期：2026-07-29
- 背景：开发需要临时数据库、Redis、服务器和 NAS 凭据。
- 决策：真实配置只存放在权限为 `0600` 的 `/etc/xs-nexus/controller.env`；仓库 `.env` 仅为被忽略的符号链接。
- 原因：满足凭据隔离要求并支持本地开发工具读取统一配置。
- 代价：部署和测试必须显式准备外部环境文件。
- 安全影响：Git、构建产物和日志不包含真实秘密。

---

## ADR-012：1Panel 网络只做声明式外部引用

- 状态：接受
- 日期：2026-07-29
- 背景：`1panel-network` 已承载现有数据库和 Redis 容器。
- 决策：Compose 只使用 `external: true` 引用该网络；基线验证只解析配置，不启动或连接容器。
- 原因：证明项目配置边界，同时避免改变现有网络成员或 IPAM。
- 代价：真实容器连通性验证推迟到服务实现后，并且只操作项目命名资源。
- 安全影响：M0.1 不改变 1Panel 网络、容器、卷或端口。

---

## ADR-013：XSP/1 v1 固定单一密码套件

- 状态：接受
- 日期：2026-07-29
- 背景：首版套件协商会增加降级攻击面和互操作分支。
- 决策：v1 仅接受 suite `0x0001`，由 Ed25519、X25519、HKDF-SHA-256、ChaCha20-Poly1305 和 SHA-256 组成；未知或额外套件失败关闭。
- 原因：使用成熟标准原语并保持协议状态机可审计。
- 代价：迁移到新套件需要新协议版本或明确的后续扩展。
- 安全影响：禁止静默降级；协议组合仍需独立审计。

---

## ADR-014：XSP/1 不实现自定义分片

- 状态：接受
- 日期：2026-07-29
- 背景：自定义分片、重组和缓存会显著扩大数据面攻击面。
- 决策：首版虚拟接口默认 MTU 为 1280，超过路径预算的内层包由标准网络机制处理，XSP/1 不定义分片帧。
- 原因：简化实现、内存边界和拒绝服务分析。
- 代价：部分路径需要更保守的 MTU 或标准 PMTU 处理。
- 安全影响：减少重组状态耗尽和重叠分片歧义。

---

## ADR-015：控制器失联不终止既有数据会话

- 状态：接受
- 日期：2026-07-29
- 背景：数据面可用性不应依赖 Controller 持续在线。
- 决策：已建立且未过期的对等会话在 Controller 短期失联时继续；新 enrollment、授权变更和新配置获取失败关闭；Relay 永不持有端到端数据面密钥。
- 原因：保持控制面与数据面故障隔离。
- 代价：撤销传播受配置租约和会话有效期限制。
- 安全影响：必须设置有限租约、epoch 和重握手边界，且记录失联状态。

---

## ADR-016：按环境隔离 PostgreSQL schema

- 状态：接受
- 日期：2026-07-29
- 背景：开发服务器提供现有 1Panel PostgreSQL，项目不得重建或修改其公共资源。
- 决策：Controller 只创建和迁移经严格标识符验证的项目 schema；生产开发使用 `xs_nexus`，自动化使用 `xs_nexus_test`。
- 原因：隔离项目对象并允许真实数据库测试，同时不触碰 `public`、其他 schema 或 1Panel 容器配置。
- 代价：连接建立后必须设置并验证 search path；测试只清空项目测试表。
- 安全影响：阻止 schema 标识符注入，并缩小迁移影响范围。

---

## ADR-017：Enrollment 与 IPAM 使用单事务双重并发控制

- 状态：接受
- 日期：2026-07-29
- 背景：一次性 Token 和首个可用地址都可能被并发请求竞争。
- 决策：Token 使用 `FOR UPDATE` 行锁，地址分配使用每网络 transaction advisory lock，并由活动地址部分唯一索引兜底；Token 消费、节点、lease、凭证、配置和审计原子提交。
- 原因：防止重复 Token、重复 IP 和部分注册状态。
- 代价：同一网络 enrollment 串行化，超大规模时需评估吞吐。
- 安全影响：失败统一回滚；无效 Token 使用通用错误避免泄露细节。

---

## ADR-018：凭证与配置使用独立 Ed25519 密钥

- 状态：接受
- 日期：2026-07-29
- 背景：节点凭证与动态配置具有不同用途、轮换和影响范围。
- 决策：使用不同 32 字节 seed 文件；凭证签名固定 200 字节编码，配置签名域分离后的精确紧凑 JSON payload；启动时拒绝密钥复用。
- 原因：限制跨用途签名混淆和单密钥轮换耦合。
- 代价：部署必须保护、备份和轮换两组在线密钥。
- 安全影响：密钥文件权限不宽于 `0600`，临时读取缓冲和管理员 Token 使用内存清零包装。

---

## ADR-019：控制连接使用每连接 challenge

- 状态：接受
- 日期：2026-07-29
- 背景：仅提交长期凭证无法证明节点当前持有私钥，也容易被跨连接重放。
- 决策：WebSocket 建立后发送 32 字节随机 challenge；节点在 10 秒内签名域标签、challenge 和 Node ID，认证后仅同步签名配置。
- 原因：绑定当前连接、节点身份和私钥持有证明，不把业务数据引入控制面。
- 代价：Agent 需要连接状态机、超时和重连逻辑。
- 安全影响：消息/frame 限制 4096 字节，未知、二进制和越序消息失败关闭。

---

## ADR-020：Agent 本地身份与配置采用验证后原子替换

- 状态：接受
- 日期：2026-07-29
- 背景：Agent 必须在进程重启后保持同一节点身份，并且不能因截断写入或未验证的控制面数据破坏最后有效状态。
- 决策：Ed25519 seed 只写入权限为 `0600` 的普通文件，父目录权限限制为 `0700`；enrollment 响应和后续配置必须完成凭证、签名、网络、节点、地址与单调版本验证后，才通过同目录临时文件、`fsync` 和原子重命名替换持久状态。
- 原因：把信任建立、状态完整性和断电恢复放在 TUN 或路由副作用之前。
- 代价：首次注册需要可写的本地状态目录；配置签名密钥轮换必须设计显式迁移协议。
- 安全影响：拒绝符号链接、宽权限私钥、非规范 Base64URL、密钥复用、回滚配置和同版本不同内容。

---

## ADR-021：Linux TUN 使用非持久 FD 与 Netlink 分离管理

- 状态：接受
- 日期：2026-07-29
- 背景：Agent 需要在不调用 `ip`、`route` 或现成 VPN 工具的前提下创建三层设备，并确保崩溃后不残留路由。
- 决策：仅在 Linux 目标引入 `tokio-tun` 创建默认非持久、无 packet-info 的 TUN FD；接口 MTU、`/32` 地址、启用状态和虚拟地址池路由全部由 `rtnetlink` 设置。配置池前缀仅接受 `/8` 到 `/30`，绝不创建默认路由。
- 原因：非持久 FD 在进程退出时由内核移除接口及其关联路由，Netlink 则提供可审计、无需外部命令的精确状态管理。
- 代价：Linux Agent 需要 `CAP_NET_ADMIN`；Windows 路径必须继续使用自研 `xsnet` 驱动，不能复用该依赖。
- 安全影响：启动时拒绝接管同名现有接口；`tokio-tun` 不用于 Windows，项目不引入 `tun-rs` 或 Wintun。

---

## ADR-022：本地诊断使用严格只读 Unix IPC

- 状态：接受
- 日期：2026-07-29
- 背景：运维 CLI 需要读取 Agent 状态，但不应获得节点私钥、凭证、签名材料或修改运行时的能力。
- 决策：Agent 在权限为 `0700` 的运行目录创建 `0600` Unix socket；协议使用有界长度前缀 JSON、拒绝未知字段，并限制单请求大小、单响应大小和并发连接。M1.2 仅提供状态、Peers 和脱敏诊断三类只读请求。
- 原因：把本地可观测性与控制权分离，并为 CLI 提供稳定、可测试的最小接口。
- 代价：后续写操作必须设计独立授权和审计机制，不能直接扩展现有只读请求。
- 安全影响：响应不包含私钥、凭证、控制面挑战、公钥或原始配置；异常客户端不能无限占用内存或连接槽位。

---

## ADR-023：Linux Agent 以最小权限 systemd 服务运行

- 状态：接受
- 日期：2026-07-29
- 背景：TUN 与 Netlink 需要网络管理能力，但 Agent 不应长期以完整 root 权限运行。
- 决策：正式单元使用专用 `xs-nexus` 用户，仅保留 `CAP_NET_ADMIN`，只允许访问 `/dev/net/tun`，并启用 `NoNewPrivileges`、`ProtectSystem=strict`、`ProtectHome`、地址族限制和显式可写目录。测试使用 `PrivateNetwork=yes` 的 transient unit 验证真实生命周期。
- 原因：把必须的网络权限限制在单一能力和单一设备，同时验证 SIGTERM 后接口、路由和 IPC 清理。
- 代价：安装器必须创建专用用户和受控目录；不同发行版的 systemd 属性需要兼容性验证。
- 安全影响：不使用 `privileged`、完整 root 或可写系统目录；单元不会修改 1Panel、默认路由或宿主机防火墙。

---

## ADR-024：XSP/1 Key Epoch 使用单方向确认式轮换

- 状态：接受
- 日期：2026-07-29
- 背景：收发方向具有独立密钥、序列和丢包行为，双向同时切换会把确认丢失放大为会话中断。
- 决策：KeyUpdate 使用当前发送 Epoch 加密；接收方验证后安装该方向的下一个接收 Epoch并返回 KeyUpdateAck，发起方收到匹配确认后才切换发送 Epoch。生产阈值为每方向 `2^20` 个数据包或 1 小时，旧接收 Epoch 保留 30 秒；确认使用有界重传，失败时保持旧发送 Epoch并重新尝试，不降级为明文。
- 原因：避免 ACK 丢失导致双方使用不同发送状态，同时保持方向隔离和可重放拒绝。
- 代价：每个 Peer 需要维护独立的待确认轮换、重试和旧 Epoch 退休状态。
- 安全影响：Epoch 只能严格加一；KeyUpdate payload 绑定 Session ID 和相邻 Epoch；测试特性缩短阈值但默认构建不包含测试参数。

---

## ADR-025：地址映射发现采用固定长度认证 XSD/1

- 状态：接受
- 日期：2026-07-29
- 背景：Agent 需要知道承载 XSP/1 的 UDP socket 在 Controller 侧观察到的公网端点，但禁止引入 STUN/TURN/coturn，也不能形成匿名反射器。
- 决策：定义项目独立的 `XSD1` 固定线格式；334 字节 Request 携带节点凭证、Network/Node/Request ID、时间和节点 Ed25519 签名，188 字节 Response 由 Controller Configuration Signing Key 签名并绑定完整请求 SHA-256。Controller 额外检查数据库活动凭证，并按来源 IP 限制为每分钟 30 次、最多跟踪 1024 个来源。
- 原因：响应严格小于请求，只有活动节点能获得映射结果，且响应不能移植到另一请求。Agent 从数据面 socket 发出请求，因此观察端口可直接作为 XSP/1 候选。
- 代价：发现依赖可用 Controller UDP 端点和最多 300 秒时钟偏差；大量有效节点请求仍需后续分布式限流设计。
- 安全影响：无效请求静默丢弃；不返回详细错误；发现标签与 XSP/1、配置和控制连接标签域分离。

---

## ADR-026：候选使用短期节点签名广告和 Controller 配置再签名

- 状态：接受
- 日期：2026-07-29
- 背景：候选端点由节点本地接口和外部映射共同产生，Controller 不能替节点声明未经证明的地址，也不能允许旧广告覆盖新状态。
- 决策：Agent 最多枚举 12 个本地地址、消费 8 个发现端点并发布最多 16 个候选；优先级为 LAN local、public IPv6、mapped、static、relay。广告默认 10 分钟过期、4 分钟刷新，携带持久化单调 generation，并使用节点身份密钥签名精确 JSON。Controller 验证后持久化并发布新的 Controller 签名配置；完全相同重试幂等，其他非递增 generation 拒绝。
- 原因：节点为自身可达性负责，Controller 负责成员身份、冲突、资源和版本边界，Peer 只消费 Controller 签名结果。
- 代价：配置版本会随候选变化增加；时钟严重错误会阻塞广告；M2.2 仍需处理 NAT 映射保活和打洞时序。
- 安全影响：跨节点端点冲突、重复端点/优先级、Relay 类型、无效 scope、过期和过大广告均失败关闭。

---

## ADR-027：路径切换必须由握手或 AEAD PathChallenge 证明

- 状态：接受
- 日期：2026-07-29
- 背景：UDP 来源地址会因 NAT、接口或路由变化而变化，直接信任新五元组会允许端点劫持；每次变化都完整重握手又会造成不必要中断。
- 决策：初始会话按候选优先级尝试并在有界重试后回退。已建立会话只对优先级高于当前路径的候选发送 AEAD 加密 PathChallenge；PathResponse 必须来自目标端点并匹配 Path ID、8 字节 token、序列和重放窗口。认证握手和普通已认证 Peer 流量也可证明来源端点。配置更新不主动销毁已建立会话。
- 补充：若某来源正在等待主动 PathChallenge 的响应，普通 AEAD 业务流量不得提前晋升该路径，只有匹配的 PathResponse 可以完成本次主动回切并记录 `authenticated_path_probe`。
- 原因：复用现有方向 traffic key 和重放状态即可证明对端持有会话密钥，同时保持数据连续性和路径可观测性。
- 代价：每 Peer 增加待处理 probe、重试、冷却和路径原因状态；路径性能评分、Relay 与 RTT 尚待后续里程碑。
- 安全影响：未认证地址变化不能晋升；错误来源、Path ID、token、Tag 或重放响应全部拒绝。

---

## ADR-028：UDP 打洞采用双方主动认证握手与 AEAD 映射保活

- 状态：接受
- 日期：2026-07-30
- 背景：仅在首个 TUN 业务包到达时握手无法提前建立 NAT 映射；公网映射变化后继续把未知来源全部丢弃又会让已建立会话失去恢复能力。
- 决策：Idle Peer 根据 Controller 签名候选目录主动发起现有四消息 XSP/1 握手，不增加匿名探测协议；每进程最多 32 个主动客户端握手、每维护 Tick 最多启动 8 个、每 Peer 每轮最多 8 个候选，失败后有界指数退避。已建立会话以 AEAD Keepalive 维持映射。未知来源只有在严格 XSP/1 Header 绑定当前 Network、Destination Node、已知 Source Node 和当前 Session ID，并通过 AEAD、Epoch、序列及重放验证后，才作为 `authenticated_peer_traffic` 晋升。
- 原因：双方主动发送能为普通 Full-cone/Restricted/Port-restricted NAT 建立状态，同时复用既有身份、凭证和会话密钥证明，不扩大匿名 UDP 接口。
- 代价：每个 Peer 增加主动调度、失败退避和 Keepalive 状态；对称 NAT 或 UDP 永久封锁仍需 M2.3 Relay；真实运营商网络仍需外部门禁测试。
- 安全影响：未认证来源、伪造 Session ID、错误 Node/Network、Tag 篡改和重放均不能改变活动路径；生产 Keepalive 为 15 秒，测试特性缩短为 1 秒；不会降级到明文、STUN/TURN 或现成 VPN/穿透核心。

---

## ADR-029：Relay 采用独立 XSR/1 短期 Lease envelope

- 状态：接受
- 日期：2026-07-30
- 背景：对称 NAT、UDP 策略或长期打洞失败时需要自研 Relay，但 Relay 不得持有 XSP/1 业务密钥、解密虚拟 IP 包、成为匿名反射器或复制 TURN/现成组网协议。
- 决策：定义项目独立的 `XSR1` v1 固定 UDP 线格式。节点以 Controller 凭证和长期 Ed25519 身份签名 352 字节注册请求；Relay 以独立身份密钥签发最多 300 秒、绑定 Network/Node/Relay/UDP 来源端点的 168 字节短期 Lease。Data envelope 使用 104 字节有限路由头包裹逐字节不变的 XSP/1 datagram；服务端以活动 Lease、来源端点和每 Lease sequence 重放窗口认证转发状态。Register Request、Register Response 和 Keepalive Response 使用三个独立签名域。
- 原因：把 Relay 可见范围限制为路由元数据和端到端密文，复用现有 Controller 信任根和节点身份，同时通过短期状态、目标活动 Lease、限速与等长转发阻止匿名转发和数据放大。
- 代价：Relay 路径最大内层 datagram 降为 1396 字节；外层 Data 不逐包执行昂贵公钥签名，必须严格依赖随机 Lease、来源端点、过期和重放窗口；on-path 攻击者仍可观察元数据或实施受限 DoS；Agent 需要注册续租、多 Relay 故障切换和 Direct 回切状态。
- 安全影响：Relay 没有 XSP/1 traffic key，不能生成目标接受的业务包；Lease ID 使用 CSPRNG、禁止日志和持久化；匿名、错误端点、过期、重放、无目标 Lease、空 payload 和超限报文静默丢弃；真实公网容量与第三方协议审计仍是后续门禁。

---

## ADR-030：UDP 发送失败按路径故障处理而非终止 Agent

- 状态：接受
- 日期：2026-07-30
- 背景：严格防火墙、路由切换或暂时不可达会让 UDP `send_to` 返回 `EPERM`、`ENETUNREACH` 等端点相关错误；若维护循环将其升级为进程级故障，Agent 会在本应回退到 Relay 的场景退出。
- 决策：XSP/1 Direct、Relay 和 Relay 维护报文的单次发送错误记录为“本次未发送”，由现有有界重传、候选回退、Relay 健康和故障切换状态机处理；UDP 接收、本地 socket 创建/绑定、协议状态和持久化错误仍为进程级失败。配置刷新只在候选 kind、endpoint 或 priority 变化时重置未建立会话，单纯过期时间续期不破坏回退进度。
- 原因：UDP 发送错误通常描述目标路径而非整个本地数据面，路径状态机比进程退出更能提供可用性，同时保持真正的本地 socket 和可信状态故障失败关闭。
- 代价：发送错误不会立即使路径永久失效，需等待已有重试和健康窗口；日志和指标必须能区分 Direct、Relay 与 Relay maintenance。
- 安全影响：不降低任何认证、AEAD、重放或来源绑定检查；失败发送不会改为明文，也不会绕过 Relay Lease、候选优先级或路径认证。

---

## ADR-031：ACL 使用规范顺序的首次匹配与双端执行

- 状态：接受
- 日期：2026-07-31
- 背景：节点、组、标签和端口规则需要在 Controller 解释结果与 Agent 实际执行之间保持确定性，同时任何一端失配都不能放行。
- 决策：ACL 规则按优先级降序、同优先级 Rule ID 升序规范化，采用首次匹配；无匹配、未知源或未知目标默认拒绝。发送端在加密前执行，接收端在认证解密后将 Peer 身份绑定到内层虚拟源地址并再次执行。节点、组、标签、协议和端口范围随 Controller 签名配置下发。
- 原因：同一规范输入产生同一决定和解释，避免 Controller、发送端和接收端出现排序或默认值歧义。
- 备选：最后匹配、显式默认规则或只在接收端执行；均会增加策略歧义或单点绕过风险。
- 代价：每次配置替换必须完整验证并编译策略；规则数量和 selector 数设置硬上限。
- 安全影响：无签名、回滚、同版本异内容、非规范规则和无法绑定身份的包失败关闭，并保留最近有效策略。

---

## ADR-032：IPAM 使用活动唯一、吊销冷却和路由冲突前置拒绝

- 状态：接受
- 日期：2026-07-31
- 背景：地址复用过快会让旧会话或缓存把流量交给新节点，重叠地址池或本机系统路由则可能劫持无关流量。
- 决策：每个网络的活动虚拟地址由数据库唯一约束保护；节点吊销后 Lease 进入有界冷却，冷却结束前自动和手动分配都不得复用；新网络地址池不得与现有网络池重叠。Agent 在创建 TUN 和地址池路由前通过 Netlink 检查本机已连接和静态路由，发现重叠即拒绝启动网络副作用。
- 原因：把并发、生命周期和宿主机冲突分别交给数据库约束、事务状态和内核真实路由视图处理。
- 备选：仅依赖应用内存、立即复用地址或覆盖系统路由；均不能在并发、重启或冲突环境中安全工作。
- 代价：管理员需要等待冷却或显式处理冲突；跨网络池变更必须先消除重叠。
- 安全影响：降低旧会话错投和路由劫持风险；子网路由重叠、优先级和网关撤销已由 M3.2 的独立策略与证据处理。

---

## ADR-033：子网发现采用节点建议与管理员审批分离

- 状态：接受
- 日期：2026-07-31
- 背景：Agent 能观察本机直连私网，但自动发布整个 LAN 会扩大访问面并可能泄露或劫持本地网段。
- 决策：Agent 仅通过 Netlink 枚举主路由表中的直连私有 IPv4 网段，排除 loopback、项目 TUN、容器和常见覆盖网络接口，生成带有效期和单调 generation 的节点签名建议。Controller 验证后持久化建议；管理员使用期望配置版本原子替换 enabled/paused 路由，遗漏项进入 revoked，只有 enabled 路由进入 Controller 签名配置。
- 原因：把“本机观察事实”和“网络范围授权”分离，Controller 负责审批、冲突、版本和审计，Agent 不拥有自动扩权能力。
- 代价：建议过期或接口变化后需要重新审批；真实 NAS 审批仍需人工门禁。
- 安全影响：无建议、过期建议、错误节点、重复 scope、默认/保留网段、虚拟池重叠和部分重叠全部失败关闭。

---

## ADR-034：子网网关使用显式转发、项目独占 NAT 表和可恢复清单

- 状态：接受
- 日期：2026-07-31
- 背景：子网网关需要改变路由、IPv4 forwarding 和 nftables，但不得覆盖宿主或 1Panel 资源，崩溃和撤销后必须恢复原状态。
- 决策：客户端路由和 LAN 连接检查使用 Netlink；网关只为项目 TUN 和审批接口保存并设置 forwarding。NAT 使用 `ip xs_nexus_<tun>` 独占表，规则同时匹配项目入接口、审批出接口和目标前缀并执行 masquerade，表、链和规则带 owner marker。manifest 先记录所有权意图，变更时先关闭转发，再替换 NAT，最后按需启用；shutdown、暂停和 stale recovery 只清理已记录的项目资源并恢复原 sysctl。
- 原因：系统网络副作用必须有明确所有者、顺序和逆操作，且生产实现不依赖 `ip`/`nft` 子进程。
- 代价：外部同名 nftables 表会导致失败关闭；每次配置变化需要重新核对当前直连 LAN 路由。
- 安全影响：防止转发范围扩大、NAT 规则漂移、外部表误删和崩溃残留；不修改 `1panel-network` 或 1Panel 规则。

---

## ADR-035：XSP/1 路由包使用显式策略守卫 API

- 状态：接受
- 日期：2026-07-31
- 背景：普通 XSP/1 数据包严格要求内层源/目标等于双方虚拟 IP，子网请求和返回包需要携带获批 LAN 地址，但不能因此全局取消地址绑定。
- 决策：保留 `seal_ipv4`/`open` 的严格虚拟地址绑定，新增结构校验严格但地址由上层策略绑定的 `seal_routed_ipv4`/`open_routed`。Agent 只有在签名子网策略表明本地或 Peer 为网关时使用 routed receive；发送前执行 ACL 和路由归属，解密后再次将认证 Peer 绑定到虚拟源、审批目标或其拥有的子网源。
- 原因：协议层继续认证 Network、Node、Session、Epoch、序列、AAD 和 AEAD，路由授权由掌握签名配置与 ACL 的 Agent 数据面完成。
- 代价：routed API 的调用方承担显式策略前置条件，必须由协议单测和 namespace 绕过测试共同约束。
- 安全影响：普通节点会话不能借 routed API 绕过地址绑定；伪造虚拟源、错误网关、未审批子网和 ACL 拒绝流量均在转发前丢弃。

---

## ADR-036：控制台使用服务端会话、CSRF 与三角色授权

- 状态：接受
- 日期：2026-07-31
- 背景：Bootstrap bearer Token 适合服务端自动化，但不应进入浏览器、本地存储或成为最终用户权限模型。
- 决策：用户密码使用 Argon2id PHC 哈希；浏览器只持有 HttpOnly、SameSite=Strict 会话 Cookie，写操作额外提交内存中的轮换 CSRF Token。角色固定为 administrator、operator、auditor，Controller 对每个 handler 执行权限检查；bearer Token 保留为服务端管理员通道。
- 原因：浏览器无法读取会话 Token，CSRF 与角色检查由可信服务端统一执行，同时保持既有自动化脚本兼容。
- 备选：浏览器保存 API Token、仅前端隐藏操作或 JWT 长期令牌；均增加泄露、撤销或越权风险。
- 代价：需要会话表、登录限速、过期清理、CSRF 轮换和用户管理迁移。
- 安全影响：数据库不保存会话或 CSRF 明文；登录对未知用户执行等价密码工作；注销、禁用和过期会立即使后续授权失败。

---

## ADR-037：控制台对缺失遥测显式不可用而不推断状态

- 状态：接受
- 日期：2026-07-31
- 背景：M4 页面需要展示节点、Relay、流量、延迟和更新状态，但部分 Agent/Relay 遥测尚未实现。
- 决策：管理快照为每项可能缺失的数据返回 `available` 或 `unavailable`、空值和人类可读原因；节点在线只使用当前进程的已认证控制连接。固定数据仅用于 Playwright 拦截夹具，不进入生产路径。
- 原因：运维界面必须区分“已测得为零/离线”和“尚未采集”，避免虚假健康、路径或性能结论。
- 备选：根据候选端点、最近数据库时间或默认常量推断；会制造不可验证的运行事实。
- 代价：在遥测里程碑完成前，部分卡片和页面明确显示未采集或功能不可用。
- 安全影响：降低错误运维决策和虚假可用性风险，快照测试禁止秘密字段及伪造指标。

---

## ADR-038：Linux 发布使用先验签名外部清单和精确归档 allowlist

- 状态：接受
- 日期：2026-07-31
- 背景：若安装器先读取不可信清单、只校验外层哈希或允许归档携带额外类型，攻击者可操纵解析边界、路径或被安装内容。
- 决策：每个目标产物由归档、固定九字段外部清单和 Ed25519 分离签名组成。安装器必须先验证签名再解析清单，随后验证产品、版本、平台、架构、目标、文件名、长度和 SHA-256。解包前后执行固定成员 allowlist、类型检查和精确逐文件哈希；不接受链接、设备、额外成员或未签名安装脚本。
- 原因：把所有会影响解析、平台选择和落盘内容的字段纳入一个小型可审计签名边界，并阻止 tar 元数据扩展安装面。
- 代价：每次新增包内文件都必须同步修改构建器、安装器和测试；安装器本身需通过独立可信渠道获得。
- 安全影响：错误签名、公钥、平台、架构、长度、外层哈希、成员集合、类型和内层内容均失败关闭；首次发布公钥固定防止升级时换根。

---

## ADR-039：Linux 激活采用版本目录、原子链接和验证后显式回滚

- 状态：接受
- 日期：2026-07-31
- 背景：原地覆盖二进制会在写入或服务启动失败时留下混合版本，并可能破坏节点身份、配置和系统网络恢复能力。
- 决策：不可变版本安装到 `/usr/local/lib/xs-nexus/versions/<version>-<target>`，`current` 与 `previous` 通过同文件系统原子符号链接切换。升级前验证当前版本并保存 unit 和活动状态；新服务验证失败恢复全部旧状态。外部安装禁止降级，`rollback` 仅指向仍通过原签名和哈希验证的已安装版本。配置、身份和签名状态独立于版本目录并默认保留。
- 原因：版本内容、运行选择和持久身份分离后，失败恢复具有明确逆操作，重复安装和重装也不会生成新节点身份。
- 代价：需要管理历史版本和磁盘占用；损坏的当前版本会阻止自动升级，需先保存证据并按 runbook 恢复。
- 安全影响：避免混合版本、未验证历史回滚和身份替换；卸载只有在可信 cleanup 成功后继续，失败时恢复先前活动服务。

---

## ADR-040：arm64 使用发行版交叉工具链并构建匹配 Rust 标准库

- 状态：接受
- 日期：2026-07-31
- 背景：当前构建主机为 x86_64，服务器 Rust 工具链没有预装可下载的 arm64 标准库，但 M5.1 必须产出真实 aarch64 ELF，而不能用占位文件冒充。
- 决策：x86_64 使用已安装标准库；aarch64 使用 Ubuntu 的匹配 `rust-src`、`gcc-aarch64-linux-gnu` 和 `libc6-dev-arm64-cross`，通过 `cargo -Z build-std=std,panic_abort` 构建目标标准库并由 `aarch64-linux-gnu-gcc` 链接。构建器用 `readelf` 拒绝错误 Machine 类型。
- 原因：复用发行版可审计工具链，在无外部 rustup 目标下载的环境中仍产生真实目标二进制，并保持应用运行时无新增依赖。
- 代价：构建步骤依赖当前发行版 Rust 源码与交叉 libc 版本；目标 NAS 运行兼容性仍需实机门禁。
- 安全影响：架构和目标写入签名清单且由 ELF 检查支持，防止把主机构建或测试替身误标为 arm64 发布包。

---

## ADR-041：1Panel 部署只引用外部网络并按环境隔离全部命名资源

- 状态：接受
- 日期：2026-07-31
- 背景：开发服务器已有 1Panel 网络和数据库容器，项目必须加入容器 DNS 网络，但不得接管、重建或修改既有资源。
- 决策：Compose 只声明 `external: true` 的 `1panel-network`，不创建数据库服务或默认项目网络；dev/RC 分别使用固定项目名、schema、端口、Secret、备份、状态目录和镜像标签。默认宿主端口只绑定 loopback，公网入口留给人工批准的 TLS/防火墙配置。
- 原因：项目容器可使用既有 DNS，同时所有项目生命周期操作保持可识别、可清理且不越过 1Panel 所有权边界。
- 代价：网络、数据库和公网入口必须由外部管理员预先提供；RC 不能直接复用开发 Secret、schema 或端口。
- 安全影响：预检精确验证网络名称、driver 和子网，拒绝数据库服务/端口；所有容器非 root、只读、无 capability，并从仓库外私有文件读取 Secret。

---

## ADR-042：数据库变更使用迁移前备份、激活门禁和 schema 级可回滚恢复

- 状态：接受
- 日期：2026-07-31
- 背景：容器替换和数据库迁移的失败边界不同，直接 `up` 会让不兼容 schema、坏镜像和不可验证备份同时影响当前服务。
- 决策：Controller 提供独立 `migrate` 命令；部署先用 PostgreSQL 18 客户端备份既有 schema，再迁移，成功后才激活服务。激活失败恢复上一组镜像。备份使用自定义归档与固定清单；恢复要求精确 schema 确认、恢复前安全备份，并在失败时回滚数据库。
- 原因：把数据状态和进程状态分开验证，使迁移失败不替换健康服务、镜像失败不破坏已有 schema、恢复失败仍有明确逆操作。
- 代价：部署和恢复需要私有宿主备份目录、额外磁盘和短暂停止 Controller；破坏性迁移仍需前向兼容分阶段设计。
- 安全影响：Secret 文件权限、归档大小/SHA-256/格式、目标 schema 和 PostgreSQL 主版本均失败关闭；备份静态加密和异机复制仍由 `KI-015` 跟踪。

---

## ADR-043：Windows 10/11 首版采用最小 KMDF NetAdapterCx 和版本化 direct-I/O ABI

- 状态：被 ADR-044 取代
- 日期：2026-07-31
- 背景：任务要求同时支持 Windows 10/11；微软 UMDF NetAdapterCx 仅从 Windows 11 24H2 开始，无法覆盖完整范围。驱动又必须保持最小内核攻击面和严格用户态边界。
- 决策：首版使用独立 KMDF + NetAdapterCx，只实现虚拟 NIC、队列和 LocalSystem Agent IPC。控制请求使用 buffered I/O，包批次使用 direct I/O，禁止 `METHOD_NEITHER`、`FILE_ANY_ACCESS` 和共享可写环。ABI 使用固定小端字节布局、精确长度、非零单调 sequence、规范连续包批次和硬资源上限。
- 原因：KMDF 覆盖目标系统；WDF/NetAdapterCx 提供明确对象和队列生命周期；先验证小型 ABI 可在没有 WDK 时发现整数、长度和规范编码缺陷。
- 代价：仍需 WDK、测试签名和 Windows VM 才能实现并证明实际 NetAdapterCx 收发；direct I/O 可能比共享环有更多请求开销，性能数据出来前不引入更复杂共享内存。
- 安全影响：设备 ACL 仅允许 LocalSystem，单 owner 会话和状态机限制调用顺序；身份、密码学、ACL、NAT、Relay、路由策略和秘密全部留在用户态。

---

## ADR-044：Windows 11 24H2 首版采用 UMDF NetAdapterCx，Windows 10 不作未验证兼容声明

- 状态：接受
- 日期：2026-07-31
- 背景：微软官方版本表说明 Windows 11 24H2 的 UMDF 2.33 / NetAdapterCx 2.5 支持 Ethernet，而 Windows 10 2004 的 NetAdapterCx 2.0 仅支持 MBBCx。任务书要求优先评估 UMDF，`QA_MATRIX.md` 的强制驱动平台是 Windows 11 LTSC 2024，Windows 10 为条件允许时兼容性项目。
- 决策：M6.1 首个可安装实现面向 Windows 11 24H2 x86_64，使用 UMDF 2.33 + NetAdapterCx 2.5 和系统分配数据缓冲区。继续保持版本化 ABI、LocalSystem 设备 ACL、单 owner、buffered 控制请求、direct-I/O 包批次以及全部长度和状态校验。Windows 10 不复用不受支持的 Ethernet NetAdapterCx 组合，也不声称兼容；其独立驱动路径在 `KI-016` 中跟踪。
- 原因：这是与当前正式 VM 门禁、微软支持矩阵和任务书“优先 UMDF”同时一致的最小攻击面路径，驱动故障不会直接在内核地址空间执行。
- 代价：Windows 10 的任务书目标尚未满足；后续可能需要独立 WDK/NDIS 路径或正式调整支持范围，且当前仍需 WDK 与 Windows VM 才能证明 UMDF 构建和收发。
- 安全影响：使用 UMDF 降低首版内核内存破坏风险；INF ACL、IOCTL access 位、requestor/file object、取消、PnP/power 和实际 host 隔离仍必须在 VM 中验证，不能由平台选择代替。

---

## ADR-045：xsnet 首版使用有界 direct-I/O 包队列而非共享可写环

- 状态：接受
- 日期：2026-07-31
- 背景：Agent 与驱动需要交换 NetAdapterCx 包，但共享可写内存会引入跨进程所有权、TOCTOU、取消和长期映射生命周期；当前性能数据不足以证明该复杂度必要。
- 决策：首版在驱动边界使用同步 direct-I/O 请求和每方向最多 64 包的私有固定槽队列。TX 使用 header-only 请求与 framed 输出，RX 使用 framed direct 输入；批次先完整验证规范布局、MTU、协商深度和剩余容量，再原子入队；输出容量不足不消费，出队或 reset 后清零完整槽。NetAdapterCx 系统缓冲区只在 packet queue callback 持有期间复制，不向 Agent 映射。
- 原因：所有权和失败边界可由 WDF request、单 owner 会话、固定资源上限和小型纯 C 模型分别测试，不需要把可变环索引暴露给不可信用户态进程。
- 代价：每包至少一次复制，吞吐可能低于共享环；WDF direct-I/O 双缓冲方向、取消和 ring 接线仍需 WDK/VM 验证，性能不达标时也必须先保留安全边界再评估替代方案。
- 安全影响：拒绝无限积压、部分批次、陈旧 payload 和跨请求共享写入；队列满只对虚拟 NIC 施加背压，不修改物理网络或放宽输入校验。

---

## ADR-046：xsnet 使用 IF_TYPE_TUNNEL 与 Layer2TypeNull 交换规范原始 IPv4 包

- 状态：接受
- 日期：2026-07-31
- 背景：Linux TUN 与现有 Agent 数据面交换 L3 IPv4；若 Windows 驱动伪装 Ethernet，则必须额外实现 MAC、ARP/邻居、二层广播和头部转换，扩大驱动职责并破坏跨平台包语义。
- 决策：INF 保持 `IF_TYPE_TUNNEL`/IP media，Agent ABI 只接受规范原始 IPv4。队列校验版本 4、IHL、IPv4 总长度、协商 MTU 和批次布局；RX ring 使用 `NetPacketLayer2TypeNull`、零 L2 长度和准确 IPv4 header length，TX 不读取 `Ignore` 包的其他只读字段。
- 原因：微软 ring 规则明确允许 RX 的 `Layer2TypeNull`，这与任务书要求的 L3 虚拟接口和 Linux TUN 边界一致，不把 ARP 或 Ethernet 策略塞进最小驱动。
- 代价：Windows TCP/IP 对 `IF_TYPE_TUNNEL`、IP media 与 UMDF NetAdapterCx 2.5 的实际绑定和路由行为仍必须在 Windows 11 VM 验证；若 WDK 或系统拒绝该组合，必须保存证据并重新评估，而不能静默改为伪 Ethernet。
- 安全影响：驱动拒绝 IPv6、截断头、非法 IHL 和总长度不一致包；不可信 Agent 不能通过合法批次描述符注入非规范 L3 payload。

---

## ADR-047：M6.1 只提供快照 VM 测试安装器并外部引用 WDK DevGen

- 状态：接受
- 日期：2026-07-31
- 背景：PnPUtil 可以暂存并更新匹配设备，但不能创建不存在的 ROOT 测试设备；微软 DevGen 能创建/删除测试设备，却明确禁止重新分发和生产使用。
- 决策：仓库只提供测试 VM PowerShell 编排，不携带 DevGen。安装要求 Windows 11 26100+ 管理员、显式测试 signer thumbprint、本机有效 Microsoft-signed WDK DevGen，以及精确 INF/CAT/DLL allowlist；使用 PnPUtil 暂存、DevGen 创建 `Root\XSNET`，成功后保存受限状态。失败和卸载只操作精确设备实例与 `oem#.inf`，并以零残留为成功条件。
- 原因：满足 M6.1/M6.2 测试签名安装准备，同时遵守微软工具许可，不把测试工具或 test-signing 配置伪装成生产安装器。
- 代价：脚本依赖测试 VM 已安装 WDK，不能用于最终用户分发；生产软件设备创建、正式签名、升级和回滚仍需独立实现与门禁。
- 安全影响：拒绝路径扩展、重解析点、额外文件、未知 signer、非 Microsoft DevGen、既有模糊状态和无限等待；脚本不下载代码、不改 BCD、不绕过执行策略。

---

## ADR-048：在 WDK 门禁前用单锁交错模型验证 xsnet teardown 安全不变量

- 状态：接受
- 日期：2026-07-31
- 背景：当前没有 Windows 11 WDK VM，不能真实执行 WDF 取消、PnP 和电源回调；但 file cleanup、Agent crash、queue cancel、睡眠和设备移除共享 owner、link、私有队列与同步请求状态，等待 VM 才检查会让失败状态边界长期不可验证。
- 决策：新增只依赖纯 C session/queue 实现的生命周期 harness，按驱动单一 `WDFWAITLOCK` 临界区把 cleanup、TX/RX cancel、D0 exit、hardware release 和 I/O stop 建模为原子事件，穷举全部 720 种顺序，并分别验证取消胜出、完成胜出、睡眠后重新认证、队列重启和重复 teardown。测试不保留跨事件 request/ring 引用，不改变真实回调代码，也不标记任何 VM 验收项。
- 原因：有限状态交错能在 Linux Release 与 ASan/UBSan 中持续验证 fail-closed、单次完成和幂等不变量，同时明确隔离无法模拟的 WDF 调度、对象引用与 Windows 网络栈。
- 代价：模型依赖当前单锁和同步 I/O 架构；若以后引入挂起请求、多个锁或共享环，必须先重写模型和锁序证明。即使 720 种顺序全部通过，仍不能替代 WDK 编译、Driver Verifier、Agent crash、PnP/power 或睡眠实测。
- 安全影响：teardown 任意顺序都不能保留活动 link、重复完成请求或自动复活旧 owner；真实 WDF 对象生命周期和回调竞态继续由 `BLK-001` 人工门禁阻塞。

---

## ADR-049：Windows Agent 先实现纯 Rust ABI 提交模型，平台 transport 独立门禁

- 状态：接受
- 日期：2026-07-31
- 背景：workspace 全局禁止 `unsafe`，当前 Linux 主机也没有 Windows SDK；直接加入无法编译验证的 Win32 FFI 会绕过既有安全门禁，但 Agent 仍需先固定 sequence、失败恢复和批次边界。
- 决策：先实现无 `unsafe` 的纯 Rust `XsnetClient`，固定 C ABI/IOCTL 向量并把请求分为成功、明确拒绝和结果不确定三类；只有成功提交状态和 sequence，明确拒绝可原 sequence 重试，未知结果必须重开 handle。设备枚举、句柄 ACL 验证和 `DeviceIoControl` 放入后续独立最小 transport，不在当前模块伪实现。
- 原因：协议状态和不可信字节可在现有主机完整测试、Clippy 和真实 Agent 回归中验证，同时把未来 FFI 审计面限制为无策略 transport。
- 代价：当前 Windows Agent 仍不能打开设备或收发包，M6.1/M6.2 和 `ACCEPTANCE.md` K 项不完成；未来 transport 必须证明取消、overlapped/sync 语义和句柄恢复与本模型一致。
- 安全影响：不确定结果绝不猜测驱动 sequence，畸形响应不会提交客户端状态；Win32 `unsafe` 不扩散到协议和数据面逻辑。

---

## ADR-050：Win32 transport 首版使用同步独占句柄并默认不确定失败

- 状态：接受
- 日期：2026-07-31
- 背景：驱动 ABI 是同步 direct-I/O，但泛化 Win32 错误不能可靠证明驱动是否已推进 sequence；当前主机也没有 Windows Rust 标准库，未编译 FFI 会制造虚假进度。
- 决策：先固化安全 `XsnetTransport` 契约与只读请求访问器。首版实际 Win32 transport 必须使用单一无共享同步 handle，不使用 overlapped、共享环或后台重试；Win32 调用失败、取消、移除、异常字节数和模糊状态默认返回 Indeterminate。只有具备权威未提交证明的状态才可返回 Rejected。`unsafe` 必须隔离在独立 target-specific 边界，不能降低 Agent/workspace 全局规则。
- 原因：保守分类防止在驱动可能已提交后复用 sequence；同步所有权与当前驱动一次完成模型一致，并把未来审计面限制为设备枚举、handle 和六个 IOCTL。
- 代价：部分本可安全重试的 OS 错误首版也会重连；没有 Windows 编译环境前只完成契约和规范，不声称可打开设备。
- 安全影响：任何模糊失败和畸形成功响应都会关闭逻辑会话，旧 sequence 不跨 handle；无依据的错误码映射不能绕过该规则。

---

## ADR-051：Windows 测试包与 VM 验收采用显式工具链和不可复用分阶段证据

- 状态：接受
- 日期：2026-07-31
- 背景：当前 Linux 主机无法执行 WDK、测试签名或 Driver Verifier，但等待 VM 后再临时拼接命令会扩大误签名、误装到日常电脑、自动重启丢证和把部分结果误报为验收的风险。
- 决策：测试包构建必须显式传入 Microsoft-signed MSBuild、InfVerif、Inf2Cat 和 SignTool，使用 Release x64、`SignMode=Off`、InfVerif `/w /v`，先嵌入签名 DLL、再生成 `10_GE_X64` catalog、最后签名 catalog，并使用显式 SHA-256 test signer 和全新输出目录。VM 验收拆成 Initialize、Install、EnableVerifier、CollectVerifier、DisableVerifier、Uninstall 六个不可复用阶段；要求 Windows 11 26100+、管理员、可识别虚拟机、同一快照声明和 Verifier 前后人工重启，最终对证据文件生成 SHA-256 清单。
- 原因：工具来源、包内容、设备状态、重启边界和证据完整性都可独立失败关闭；不自动重启使崩溃、网络和 Verifier 结果能在继续前人工保存，阶段目录也不会覆盖首次失败。
- 代价：脚本不能替代快照 API、场景测试或人工确认，执行步骤更多；快照 ID 只是操作员断言，正式 M6.2 仍必须补充互通、睡眠、网络切换、Agent crash、异常 IOCTL、蓝屏和重复安装证据。
- 安全影响：脚本不下载工具、不创建证书、不修改 BCD/信任/测试签名策略、不自动重启、不删除未知设备或驱动包，也不把 CollectVerifier 描述为验收通过；正式签名和生产安装器继续由独立门禁处理。

---

## ADR-052：首版 Windows 只允许 exact ABI v1 和 clean-install 测试替换

- 状态：接受
- 日期：2026-07-31
- 背景：ABI v1 的 32 字节 header 必须先解析，驱动之后才能读取 Hello payload 中的版本范围，因此当前 Hello 不能跨 header version 协商。测试安装器也没有经过 Windows 验证的原子升级或旧包回滚事务。
- 决策：Agent、驱动和安装状态共同固定 ABI v1；Agent 的 header、Hello minimum 和 maximum 都发送 v1，不静默扩大范围。INF `DriverVer` 只表示包身份，构建清单记录四段 driver version、ABI `1..1` 和 IPv4 capability；安装器要求显式期望版本，在 staging 前后核对 INF 与 driver-store，并把 driver version/ABI 写入安装状态 schema 2，未知或旧 schema 失败关闭。测试包替换只允许停止 Agent、关闭 handle、精确卸载后的 clean install，失败依赖快照恢复。
- 原因：把运行时协议兼容、包身份和安装事务分开后，不会因产品版本相近就连接不兼容 ABI，也不会把尚未实现的热升级或降级路径写成可用能力。
- 代价：任何 ABI 变化都需要新的显式 discovery/compatibility 设计和 VM 矩阵；测试包升级步骤较慢，无法证明最终用户无缝升级。
- 安全影响：未知 ABI、错误 INF 版本、staged package 漂移和既有模糊状态都失败关闭；回滚不能选择未记录 `oem#.inf`、未知 signer 或不同 ABI。生产升级与回滚仍为未完成门禁。

---

## ADR-053：源码 SBOM 使用锁文件、固定许可证快照和双格式确定性导出

- 状态：接受
- 日期：2026-07-31
- 背景：`Cargo.lock` 有 crate checksum 但没有许可证，npm lock v3 有 integrity 但没有许可证；依赖本机 `node_modules` 会遗漏非当前平台 optional 包，生成时在线查询注册表又会让 CI 和历史证据随外部状态漂移。
- 决策：Cargo 以 `cargo metadata --locked` 的许可证和 `Cargo.lock` SHA-256 checksum 逐项绑定；npm 使用建立时已和官方注册表 exact version、license、`dist.integrity` 核对的受版本控制快照，并要求其集合与 `package-lock.json` 完全一致。标准库 Python 工具离线生成 CycloneDX 1.6、SPDX 2.3 和输入/输出 manifest，使用稳定排序、输入摘要派生 UUID 与显式 `SOURCE_DATE_EPOCH`。许可证、禁用依赖、输出路径和覆盖行为全部失败关闭。
- 原因：同时覆盖当前主机未安装的可选平台包、保留锁文件内容身份、消除生成期网络依赖，并使依赖变化必须通过可审查的 lock/snapshot diff 和负向测试。
- 代价：npm 依赖变化必须通过独立受控步骤刷新许可证快照；源码 SBOM 不含容器操作系统包、许可证全文或最终镜像摘要，不能直接满足完整 RC 供应链清单。
- 安全影响：未知许可证、快照缺项或多项、checksum/integrity 缺失、禁用产品依赖和新增运行路径引用都会阻断 CI；允许许可证表达式保留原始选择语义，只把非标准斜杠写法规范化为 SPDX `OR`，不把未知条款误标为已批准。

---

## ADR-054：Win32 FFI 使用独立 no_std crate 和五块可计数 unsafe

- 状态：接受
- 日期：2026-07-31
- 背景：Agent 全局禁止 unsafe；开发服务器没有 Windows SDK/WDK，完整 Agent 的 TLS 依赖需要 SDK C 头，但 Cargo 能从匹配 `rust-src` 为 MSVC target 构建 core/alloc。继续等待 VM 会让最小 FFI 长期无编译证据，直接放宽 Agent lint 又会扩大审计面。
- 决策：新增 `xs-windows-transport` workspace crate，使用 `no_std + alloc` 和 target-specific `windows-sys 0.61.2`。crate 默认 deny unsafe，只在 `platform.rs` 允许五个块，分别包围两次 Configuration Manager 查询、`CreateFileW`、`DeviceIoControl` 与 `CloseHandle`；Agent 只通过安全请求/结果类型和 `XsnetTransport` 适配。MSVC target 只构建 core/alloc/panic_abort，并同时运行 Clippy warnings-as-errors。
- 原因：无需伪造 SDK/WDK 即可真实编译 Windows API 绑定和所有 unsafe 路径，同时保持 Agent、协议、密码学、路由和包解析完全安全 Rust；固定块数让新增 unsafe 必须显式修改门禁与评审。
- 代价：最小 crate check 不能证明完整 Agent 链接、Windows loader、设备 ACL、真实 buffer 映射、取消、PnP/power 或驱动行为；`RUSTC_BOOTSTRAP=1 -Z build-std` 只作为当前 Linux 准备门禁，正式构建仍需固定 Windows 工具链。
- 安全影响：接口列表必须恰有一个规范 `\\?\` 路径，handle 读写且零共享、非 overlapped；所有长度先转 u32，输出预初始化，RX 使用自有副本并检测写入；任何 Win32 失败或异常成功都为 Indeterminate，不复用 sequence。

---

## ADR-055：Windows Agent 会话先采用显式单步 I/O，不引入后台轮询重试

- 状态：接受
- 日期：2026-07-31
- 背景：xsnet 驱动当前让空 TX 和满 RX 以失败状态同步完成；首版 Win32 transport 为避免在驱动可能已提交后复用 sequence，把所有失败 `DeviceIoControl` 保守归类为 Indeterminate。若 Agent 在此基础上轮询 TX 或自动重试 RX，健康空闲会话会被毒化，或者需要无证据地把通用 Win32 错误解释为权威未提交。
- 决策：新增安全 Rust `XsnetDeviceSession`，启动前验证 MTU/双向深度并由协商值推导 TX 输出容量，随后各执行一次 Hello、Attach 和 SetLink。每个数据方法只执行一次有界 TX dequeue 或 RX enqueue，不拆批、不循环、不睡眠、不重试；权威 Rejected 保持状态和 sequence 供调用方显式处理，Indeterminate 或畸形成功强制替换 handle。shutdown 只按需执行 LinkDown，再执行一个 Detach；重复 shutdown 仅重复幂等 Detach，Drop 不执行 I/O。适配层在空 TX/满 RX 和取消矩阵经 WDK/VM 验证前不接入 runtime。
- 原因：单步边界可在 Linux fake transport 中完整验证，又不削弱 ADR-050 的 no-commit 证明要求；把“如何等待驱动有包”和“哪些 Win32 状态可安全重试”留给真实 Windows 证据，而不是把猜测固化为后台线程。
- 代价：当前完整 Windows Agent 仍不能自动泵送包，M6.1/M6.2 不完成；VM 阶段必须证明精确错误映射或设计显式唤醒协议，之后再实现有界调度与 Tokio 阻塞边界。
- 安全影响：没有隐藏重试、序列猜测、跨 handle 状态继承、无界 channel 或析构期设备调用；任何不确定完成立即失败关闭，普通物理网络不由该未启用适配层修改。

---

## ADR-056：Windows 本地管理使用固定受限命名管道并隔离安全描述符 FFI

- 状态：接受
- 日期：2026-07-31
- 背景：Agent 本地状态协议和 CLI 只实现 Unix socket，完整 Agent 即使越过 TLS 工具链也会因 `std::os::unix` 与 Tokio Unix 类型无法形成 Windows 源码边界。直接使用 Tokio 默认命名管道安全描述符不能证明仅服务身份和管理员可访问，把原始 `SECURITY_ATTRIBUTES` 放进 Agent 又会破坏 workspace 的全局 unsafe 禁令。
- 决策：共享请求解析、严格 JSON、只读响应、大小和超时边界保持平台无关；Unix transport 原样迁移到平台模块。Windows 端固定 `\\.\pipe\xs-nexus-agent`，首实例使用抢占门禁，始终拒绝远程客户端，handle 不继承，受保护 DACL 只授予 LocalSystem 和 built-in Administrators。最多 16 个活动处理器并保留第 17 个 OS 实例作为 listener；许可耗尽时停止接收。SDDL 转换、Tokio 原始安全属性创建和释放集中在独立 `xs-windows-local-ipc` crate 的三个可计数 unsafe 块。
- 原因：固定名称和首实例门禁消除配置注入与启动前管道劫持；显式 DACL 不依赖进程默认 token，远程拒绝和有界实例防止跨主机与资源耗尽；平台无关 handler 让既有 Linux 集成测试继续覆盖真实协议行为。
- 代价：当前只实现 Agent 服务器边界，Windows CLI、安全存储、Service/SCM、完整链接和运行测试仍未完成；最小 MSVC target check 不能证明 Windows 有效 DACL、连接拒绝、服务停止或并发行为。
- 安全影响：Agent 继续禁止 unsafe，新增原始指针审计面只有三个块；普通用户与远程客户端默认拒绝，未知 endpoint 拒绝启动。M6.1、M6.2 与 `ACCEPTANCE.md` K 项不因源码准备而完成。

---

## ADR-057：Windows 私有状态使用 exact protected DACL 与同目录 write-through 替换

- 状态：接受
- 日期：2026-07-31
- 背景：Agent identity、签名配置、network manifest 和 enrollment token 只按 Unix mode 验证，Windows 无法编译该模块；仅在创建后设置 ACL 或只检查文件 ACL，会留下 reparse 跟随、宽松父目录替换、临时文件暴露和覆盖式 rename 语义差异。
- 决策：共享层只负责 identity、JSON/token 语义、大小和随机临时名；Unix 保留 mode、同目录 rename 和目录 fsync。Windows 文件/目录分别使用固定仅 LocalSystem/Administrators 的 exact protected DACL，目录 ACE 可继承；要求绝对路径与所有现有路径组件非 reparse，读前同时验证直接父目录和文件 ACL、类型与长度。写入先硬化父目录，在同目录 `create_new` 临时文件上设置并回读 ACL，写入后 `sync_all`，已有目标用 `ReplaceFileW`、新目标用不覆盖的 `MoveFileExW`，两者均 write-through，完成后再次验证。
- 原因：exact ACL 字节和 `SE_DACL_PROTECTED` 同时验证可拒绝额外主体与继承漂移；受限父目录和同目录临时文件收紧替换窗口；非覆盖 Move 避免把竞态出现的新目标静默覆盖。
- 代价：当前 Windows crate 只完成源码和交叉编译，未验证 NTFS ACL 规范化、junction/hard-link、ReplaceFile 元数据、杀进程/断电或杀毒软件共享冲突；正式 ProgramData 根、owner 和 token 删除仍需安装器/VM 证明。
- 安全影响：Agent 继续全局禁止 unsafe，Windows 安全描述符与替换 FFI 集中到最小 crate；ACL、路径、长度或替换结果模糊时一律失败关闭。该准备不完成 M6.1/M6.2。

---

## ADR-058：Windows Service 使用固定 SCM 宿主与一次性停止桥接

- 状态：接受
- 日期：2026-07-31
- 背景：Windows Agent 需要在 SCM 下长期运行，但现有 `run` 入口只监听控制台信号。直接把 Win32 callback、全局状态和 Tokio 生命周期写入 Agent 会扩大 unsafe 审计面；从 runtime 自行创建或修改服务又会把安装权限和运行权限混合。STOP 还可能与 START_PENDING/RUNNING 上报并发，若没有串行化会把 STOP_PENDING 覆盖为过期 RUNNING。
- 决策：新增最小 `xs-windows-service` crate，固定 Agent 服务名 `XsNexusAgent` 和 Windows-only `service --config` 入口。进程主路径调用 dispatcher，服务 callback 注册 STOP/SHUTDOWN/INTERROGATE，按 START_PENDING、RUNNING、STOP_PENDING、STOPPED 上报；首个停止以 atomic swap 和 Tokio Notify 一次性唤醒，通过 watch channel 复用现有 `run_agent` shutdown。状态锁串行化启动、停止和最终状态；handler panic 或 Agent 失败报告 service-specific error。四个 unsafe 块只封装 dispatcher、注册和状态 FFI，runtime 禁止 Create/Delete/ChangeServiceConfig。
- 原因：固定名称消除配置注入，复用 Agent shutdown 保持 console/service 行为一致；一次性通知无轮询、sleep 或后台重试，状态锁关闭 STOP/RUNNING 竞争；安装事务、服务身份和运行时生命周期分离后，可由未来签名安装器独立实现与回滚。
- 代价：最小 MSVC target check 不能证明 SCM callback 线程、30 秒 wait hint、LocalSystem token、事件日志、关机顺序或重复启停；完整 Agent 仍因缺少 Windows SDK 无法链接，服务创建、升级和卸载尚未实现。
- 安全影响：Agent 保持无 unsafe；无效名称、重复初始化、锁毒化、dispatcher 失败、panic 和 runtime 失败均失败关闭。当前源码准备不完成 M6.1/M6.2，真实 SCM、LocalSystem、ACL 和卸载零残留继续由 `BLK-001` 门禁。

---

## ADR-059：Windows 路由先建立可验证的精确所有权事务模型

- 状态：接受
- 日期：2026-07-31
- 背景：Windows IP Helper 路由表属于全局主机状态。若只按前缀删除、先删后加、允许默认路由或把任意同接口路由视为项目所有，配置失败可能破坏普通网络；原生表还必须在有界复制后由 `FreeMibTable` 释放。
- 决策：新增隔离 `xs-windows-route-manager` crate。项目路由固定非零 interface LUID、规范 IPv4 `/1..=/30`、on-link `0.0.0.0` 下一跳和 metric 32；拒绝默认、保留和任何非项目系统路由重叠。可信 manifest 必须与系统中的精确 route key 和项目所有权同时一致。reconcile 只生成 additions-first 计划，创建失败按已成功添加的逆序精确补偿，全部添加成功后才删除 manifest 中不再需要的精确旧路由；删除阶段失败只逆序恢复本事务已删除的路由。原始失败与每个补偿/恢复失败必须共同返回；系统表超过 4096 条失败关闭。
- 原因：把所有权、冲突和事务顺序做成纯安全 Rust，可在接触 IP Helper FFI 前穷举失败边界，并阻止后续平台层使用模糊前缀或接口级清理。
- 代价：当前已实现隔离 `GetIpForwardTable2`/`FreeMibTable`、精确 Create/Delete 路由、非持久地址 Create/Delete 和 DAD 查询，并通过 MSVC target check/Clippy；尚未实现 DAD 有界等待、地址与路由联合事务、manifest 持久化或 Windows 执行，不能描述为真实路由管理。
- 安全影响：默认路由、外部路由重叠、所有权漂移、重复记录、零 LUID 和无界系统快照均在任何系统写入前被拒绝。真实 FFI 必须保持表释放、精确错误映射和 rollback 失败显式上报。
