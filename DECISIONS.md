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

## ADR-022：本地诊断初版使用严格只读 Unix IPC

- 状态：被 ADR-073 扩展
- 日期：2026-07-29
- 背景：运维 CLI 需要读取 Agent 状态，但不应获得节点私钥、凭证、签名材料或修改运行时的能力。
- 决策：Agent 在权限为 `0700` 的运行目录创建 `0600` Unix socket；协议使用有界长度前缀 JSON、拒绝未知字段，并限制单请求大小、单响应大小和并发连接。M1.2 仅提供状态、Peers 和脱敏诊断三类只读请求。
- 原因：把本地可观测性与控制权分离，并为 CLI 提供稳定、可测试的最小接口。
- 代价：后续有运行时效果的操作必须设计独立授权、有界执行和明确响应，不能无约束扩展初版只读请求；该要求由 ADR-073 的认证探测和控制面重连落实。
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
- 安全影响：Secret 文件权限、归档大小/SHA-256/格式、目标 schema 和 PostgreSQL 主版本均失败关闭；后续静态加密、复制和保留边界由 ADR-071 完成。

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

- 状态：已由 ADR-076 替代（2026-08-04）
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
- 原因：保守分类防止在驱动可能已提交后复用 sequence；同步所有权与当前驱动一次完成模型一致，并把未来审计面限制为设备枚举、handle、六个 ABI IOCTL 和独立 identity query。
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

## ADR-054：Win32 FFI 使用独立 no_std crate 和可计数 unsafe

- 状态：接受
- 日期：2026-07-31
- 背景：Agent 全局禁止 unsafe；开发服务器没有 Windows SDK/WDK，完整 Agent 的 TLS 依赖需要 SDK C 头，但 Cargo 能从匹配 `rust-src` 为 MSVC target 构建 core/alloc。继续等待 VM 会让最小 FFI 长期无编译证据，直接放宽 Agent lint 又会扩大审计面。
- 决策：新增 `xs-windows-transport` workspace crate，使用 `no_std + alloc` 和 target-specific `windows-sys 0.61.2`。crate 默认 deny unsafe，只在 `platform.rs` 允许可由源码门禁精确计数的块，包围 Configuration Manager 查询、`CreateFileW`、`DeviceIoControl` 与 `CloseHandle`；Agent 只通过安全请求/结果类型和 `XsnetTransport` 适配。identity query 增加第二个 `DeviceIoControl` 后当前固定为六块。MSVC target 只构建 core/alloc/panic_abort，并同时运行 Clippy warnings-as-errors。
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
- 决策：共享请求解析、严格 JSON、长度帧、响应、大小和超时边界保持平台无关；Unix transport 原样迁移到平台模块。Windows 端固定 `\\.\pipe\xs-nexus-agent`，首实例使用抢占门禁，始终拒绝远程客户端，handle 不继承，受保护 DACL 只授予 LocalSystem 和 built-in Administrators。最多 16 个活动处理器并保留第 17 个 OS 实例作为 listener；许可耗尽时停止接收。SDDL 转换、Tokio 原始安全属性创建和释放集中在独立 `xs-windows-local-ipc` crate 的三个可计数 unsafe 块。
- 原因：固定名称和首实例门禁消除配置注入与启动前管道劫持；显式 DACL 不依赖进程默认 token，远程拒绝和有界实例防止跨主机与资源耗尽；平台无关 handler 让既有 Linux 集成测试继续覆盖真实协议行为。
- 代价：Agent 服务器和 `xs-cli` Named Pipe client 已完成源码/交叉编译，但安全存储、Service/SCM、完整链接和运行测试仍未在 Windows 执行；最小 MSVC target check 不能证明 Windows 有效 DACL、连接拒绝、服务停止或并发行为。
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
- 代价：当前已实现隔离 `GetIpForwardTable2`/`FreeMibTable`、精确 Create/Delete 路由、非持久地址 Create/Delete、DAD 查询和有界轮询，以及地址成功后才执行的路由联合事务；DAD 拒绝/超时/查询失败与路由失败都会精确删除地址并显式返回清理失败。可信 manifest 使用 schema 1、严格 JSON、64 KiB 上限、规范 route order 和 exact LUID/address/route ownership；恢复计划只选择系统中仍精确匹配的记录资源，任何外部重叠失败关闭。Windows 平台通过 `xs-windows-private-storage` 读取和 write-through 原子替换 manifest，删除前同样验证父目录/文件 exact protected DACL 和 reparse 边界；恢复会尝试全部精确路由和地址并聚合每个失败。已通过 MSVC target check/Clippy；尚未实现 Agent 启动编排或 Windows 运行，不能描述为真实路由管理。
- 安全影响：默认路由、外部路由重叠、所有权漂移、重复记录、零 LUID 和无界系统快照均在任何系统写入前被拒绝。真实 FFI 必须保持表释放、精确错误映射和 rollback 失败显式上报。

- Agent 接入边界：新增 Windows-only `WindowsNetworkPreparation`，但 runtime 明确禁止引用。准备顺序固定为恢复可信 stale manifest、快照/冲突检查、原子写 Preparing、创建地址并等待 DAD、执行路由事务、原子写 Active；失败后保留 Preparing 供下次恢复。shutdown 只按 Active manifest 精确清理并在全部成功后删除 manifest。interface LUID 必须由已独占打开的同一 xsnet 设备句柄通过版本化 identity query 显式提供，不从接口名或全局枚举猜测。

---

## ADR-060：xsnet 设备身份使用独立版本化查询绑定 authoritative LUID

- 状态：接受
- 日期：2026-07-31
- 背景：路由事务要求可信的非零 interface LUID，但 exact ABI v1 只定义六类带 32 字节消息头的会话/数据消息，除 TX 外成功响应必须为空。把 LUID 塞进 Hello、增加 ABI v1 消息或按接口名/全局适配器枚举匹配都会削弱 ADR-052 或引入可注入、竞态的身份推断。
- 决策：保留 ABI v1 六类消息完全不变，新增独立 `IOCTL_XSNET_QUERY_IDENTITY`。它只接受 identity schema v1 的精确 8 字节请求并返回精确 16 字节响应；版本、长度、reserved、返回长度和非零 LUID 全部严格校验。驱动只从当前 `NETADAPTER` 调用 `NetAdapterGetNetLuid`，查询仍受同一设备 DACL、唯一 present interface、零共享独占 handle 和 requestor 校验约束。Win32 transport 在打开句柄后立即查询并缓存，Agent session 只暴露该同句柄 LUID；Windows 网络准备层的公开 recover/prepare 入口只接受该 session，原始 `u64` LUID 入口保持私有。
- 原因：身份来源与实际 I/O handle、驱动 device context 和 NetAdapterCx adapter 是同一对象链，不需要名称、display string、SetupAPI 属性猜测或任意全局枚举；独立 schema 又不会把设备元数据冒充现有消息 ABI 的成功响应。
- 代价：identity schema 自身需要独立版本管理；新增 IOCTL 和 `NetAdapterGetNetLuid` 尚未经过 WDK 编译与 Windows VM 执行，不能宣称真实 LUID 查询成功，也不能据此启用 runtime。
- 安全影响：错误版本、错误长度、非零 reserved、零 LUID、adapter 未启动、模糊设备路径和查询失败全部失败关闭。源码门禁固定七个 IOCTL、identity codec 负向测试、同句柄查询和六个集中 unsafe 块；禁止退回接口别名或全局匹配。

---

## ADR-061：运行时镜像供应链证据直接从精确 rootfs 和内容 ID 生成

- 状态：接受
- 日期：2026-07-31
- 背景：源码 SBOM 只覆盖 Cargo/npm 锁文件，不能证明四个运行时镜像实际安装了哪些 Debian/Alpine 包、包含哪些许可证材料，也不能把 Dockerfile 与最终镜像内容绑定。仅记录可变 tag 或在线查询结果无法作为可复现发布证据。
- 决策：新增标准库 Python 生成器，要求每个逻辑镜像提供 exact revision label 与仓库内 Dockerfile。工具创建临时容器并导出只读 rootfs，直接解析 dpkg/apk 已安装数据库；每个 subject 固定 Docker `sha256:` image ID、Dockerfile SHA-256、声明 base image、包 PURL、许可证声明和实际 rootfs 中可用的 copyright/license 文件。输出 CycloneDX 1.6、manifest 和 in-toto/SLSA provenance；同一输入必须双生成逐字节一致。
- 原因：不依赖生成期网络、第三方扫描器或容器内可执行工具，能够审计最小运行时镜像本身，并避免 tag 漂移、包管理器命令缺失和容器入口点差异。许可证缺失的 Alpine dot-prefixed generated dependency metapackage 保留为 virtual package，不伪造许可证。
- 代价：最小镜像未必携带每个声明许可证的全文，必须在 RC 前补齐。漏洞扫描使用临时下载的固定 Grype v0.116.1，先校验官方 archive SHA-256，再使用隔离数据库扫描 manifest 中重新核对过 image ID 的本地镜像；这会引入生成期网络和漏洞库时点，报告必须保存工具/数据库状态，不能与确定性 rootfs SBOM 混为一谈。
- 安全影响：错误 revision、非 SHA-256 image ID、缺少 OCI 标签、未知包数据库、非虚拟包缺失许可证、超限 rootfs/文件/包数、路径穿越、重复 PURL、已有输出目录和 Dockerfile 映射漂移全部失败关闭；验证前后比较容器、Docker 网络、`1panel-network`、默认路由和 nftables。漏洞脚本只接受仓库 `artifacts/qa` 内证据，重新比较 tag 当前 ID 与 manifest，固定 scanner archive hash，在私有 staging 完成全部报告后发布；发现项不自动豁免。
## ADR-062：容器健康检查由运行时二进制执行固定本地探测

- 状态：接受
- 日期：2026-07-31
- 背景：Controller 和 Relay 运行时镜像为 Compose 健康检查安装 `curl`，扩大了 Debian 运行时包与漏洞面；健康检查本身只需要访问各容器内固定的 loopback readiness 地址。
- 决策：在 `xs-core` 中提供无额外依赖的有界 HTTP/1.x 探测器；Controller 和 Relay 各自只公开无参数 `healthcheck` 子命令，并固定访问 `127.0.0.1:8080` 或 `127.0.0.1:8081`。探测设置连接、读写超时，限制响应头为 8 KiB，要求完整头部和 HTTP 200。Compose 直接执行二进制，不再安装或调用 `curl`。Alpine 运行时镜像在构建时执行 `apk upgrade --no-cache`，使已发布修复进入精确镜像。
- 原因：固定目标避免配置注入和外部网络探测；有界同步 I/O 足以满足 Docker 健康检查，同时减少运行时依赖和可修复漏洞。
- 代价：探测器不是通用 HTTP 客户端，不支持 TLS、重定向、代理或可配置目标；它只用于容器内部 readiness。
- 安全影响：非 loopback、畸形路径、连接拒绝、超时、非 200、畸形响应和超大响应全部失败关闭。残余基础镜像发现仍需显式处置，不能因当前无修复版本而自动豁免。

---
## ADR-063：Controller 与 Relay 使用固定 digest 的 distroless 运行时

- 状态：接受
- 日期：2026-07-31
- 背景：删除 curl 后，`debian:bookworm-slim` 仍携带 Controller/Relay 不使用的 perl、gzip、ncurses、ACL 等运行时包，并产生大量 Critical/High 扫描发现。
- 决策：Controller 和 Relay 的运行时阶段固定为 `gcr.io/distroless/cc-debian12:nonroot` 的精确 SHA-256 digest，只复制已构建二进制，并继续显式使用 UID/GID 65532。镜像证据生成器支持 distroless 的 `/var/lib/dpkg/status.d` 独立包元数据，保持包身份、许可证材料、镜像 ID、Dockerfile hash 和 revision 的确定性绑定。Console 删除无反向依赖的 curl 包链。
- 原因：保留 glibc、libgcc、CA 和非 root 运行所需材料，同时移除 shell、包管理器和业务不需要的工具链，显著缩小攻击面。
- 代价：容器内不再提供 shell 或通用诊断工具；诊断必须使用应用日志、健康命令、镜像证据或受控临时工具容器。更新 distroless 基础时必须显式评审并更新 digest 断言。
- 安全影响：Critical/High 总数由最初 44/115 降为 2/6；剩余发现仍保持开放，不因 distroless 或无修复版本而自动接受。

---
## ADR-064：Relay 只发布全局有界、无身份内容的传输指标

- 状态：接受
- 日期：2026-07-31
- 背景：Relay 已有部分转发与分类丢弃计数，但缺少接收字节、总丢弃、I/O 错误和延迟，无法满足可观测性验收；按节点或 lease 发布指标会扩大隐私和高基数风险。
- 决策：在既有内部 health listener 的 `/metrics` JSON 中发布全局累计接收/转发包与字节、分类/总丢弃、I/O 错误，以及成功转发的队列等待时长样本数、平均微秒和最大微秒。队列项只额外保存单调入队时间，不保存新的身份字段；计数使用饱和/原子快照语义。
- 原因：这些值可由 Relay 自身权威观察，足以监控容量、背压和错误，同时保持固定字段、低基数和业务内容最小化。
- 代价：延迟只代表 Relay 内部排队到 send 成功，不代表端到端 RTT；丢弃只代表 Relay 可观察的拒绝/失败，不代表公网中途 UDP 丢失。Controller/Console 聚合仍需后续受控采集设计。
- 安全影响：指标不含 payload、节点/网络/lease 身份或端点；Compose 不发布 Relay HTTP 端口到宿主公网。

---
## ADR-065 候选路径就绪必须按方向分别取证

- 日期：2026-07-31
- 状态：接受
- 决策：真实候选路径测试在发送双向业务流量前，必须分别等待两个 Agent 都把对端新地址标记为 `authenticated_path_probe`；单侧收到 PathResponse 只证明该方向的 challenge/response 成功，不作为反方向同时就绪的证据。
- 原因：XSP/1 每个 peer 方向独立维护 pending probe、token、path ID 和重试状态，UDP 业务包也不提供传输层重传。把单向完成当成双向完成会制造时序相关的假失败，也会削弱测试对真实双向路径状态的表达。
- 边界：不延长等待上限、不重试业务断言、不改变产品路径晋升规则；失败诊断保留第一个失败命令和受限临时日志，以便区分状态机错误与测试编排错误。
---

## ADR-066: Windows route ownership requires canonical site-prefix semantics

- Date: 2026-07-31
- Status: Accepted
- Decision: A route returned by IP Helper is project-owned only when its exact key, on-link next hop, fixed metric, protocol, origin, and `SitePrefixLength` all match the canonical project route shape. A matching destination/interface/metric without the same site prefix is foreign and must fail closed.
- Rationale: `MIB_IPFORWARD_ROW2` carries site-prefix semantics separately from destination prefix length. Treating a shape-mismatched row as owned could permit recovery to delete or mutate a route the project did not create.
- Evidence: `./scripts/test-windows-agent-routing.sh`; the source gate, 16 route transaction tests, Windows target check, and cross-target Clippy pass.

---

## ADR-067: Runtime image license evidence is a per-package offline closure

- Date: 2026-07-31
- Status: Accepted
- Decision: Every installed OS package must have one deterministic closure record. Debian packages resolve to exact package copyright material, following only a safe one-component `/usr/share/doc` link. Alpine SPDX expressions resolve to official text files copied from the distribution's `spdx-licenses-text` build package; that package is removed before the final package database is recorded. Public-domain and virtual package cases are explicit states, while non-SPDX nginx declarations require package-specific COPYRIGHT material.
- Rationale: A package-manager declaration or an arbitrary collection of rootfs license files does not prove that the exact installed package set has complete corresponding material. Package-count equality, exact paths and content hashes make omissions and package-set drift detectable offline.
- Security impact: Missing text, wrong SPDX mapping, unsafe paths/links, duplicate package identities, unbound material and closure-count drift fail closed. The validator independently hashes every referenced file.

---

## ADR-068: Stability acceptance must prove restart identity transitions

- Date: 2026-08-01
- Status: Accepted
- Decision: A long-run stability result is insufficient if it only observes health after a restart command. The restart command must return exactly the full target container ID, and the evidence summary must prove exactly one PID transition per service. Docker `RestartCount` must remain fixed at its pre-injection baseline because an operator-requested `docker restart` does not increment the restart-policy counter; any counter change instead proves an additional automatic restart and fails the run.
- Rationale: A restart command can target the wrong container, fail silently, or leave the original process alive. Exact command output and PID cardinality bind the claimed recovery to the intended service, while a stable restart-policy counter excludes hidden crash recovery before or after the injected restart.
- Scope: This hardening affects future runs of `scripts/test-runtime-stability.sh`; the currently running 24-hour run remains separately identified by its recorded start revision and evidence directory.

---

## ADR-069: The offline release key authorizes code; the Controller only schedules it

- Date: 2026-08-02
- Status: Accepted
- Decision: The release private key never enters the Controller. Controller storage is limited to verified immutable manifests, detached signatures, exact HTTPS archive metadata, and rollout policy. Agent staging and the privileged helper each independently verify the offline signature and archive; the helper then reuses the existing installer transaction.
- Rationale: Compromise of the online Controller may change scheduling or withhold an update, but must not be sufficient to authorize arbitrary root code. Re-verification after a private root-owned copy closes the unprivileged staging substitution boundary.
- Trade-off: Operators must distribute the same public key in raw Controller form and PEM Agent form through authenticated channels, and maintain an external signing/rotation ceremony.

---

## ADR-070: Node update channel is Controller-signed configuration state

- Date: 2026-08-02
- Status: Accepted
- Decision: The node's assigned stable/testing/development channel is an optional field in its Controller-signed configuration. Administrative change uses the network configuration version, publishes a new signed configuration, and causes the Agent to report immediately. Runtime reports and directives must match the current database assignment; an authentication-time channel snapshot is not authoritative for a long-lived connection.
- Rationale: A local-only channel cannot be managed consistently, while an unsigned directive could silently widen rollout exposure. Signed configuration preserves the existing monotonic trust chain and allows connected Agents to change channel without reconnecting.
- Compatibility: Configurations without the field decode as `None` and use the local Agent setting as a legacy fallback. Rollout should first deploy an Agent that understands the optional field before the Controller begins emitting it to existing fleets.

---

## ADR-071：数据库备份私钥离线，数据库主机只做流式公钥加密

- 日期：2026-08-02
- 状态：接受
- 决策：数据库主机只保存 age X25519 public recipient。`pg_dump` custom archive 通过完整消费校验后直接流入 age，持久目录不写明文归档；认证加密 manifest 绑定 schema、备份名、密文字节数/hash、recipient Key ID 和创建时间。每份备份自动复制到带 deployment/target marker 的不同文件系统挂载。离线 identity 只在深度认证、保留销毁和恢复时临时只读挂载。
- 原因：把数据库主机或本地备份介质泄露与备份内容保密分离，同时让无私钥日常校验、异地传输和有私钥灾难恢复各自具有明确证据。不同设备号和 marker 防止把同盘目录误报成异地副本。
- 代价：正式部署必须维护离线 identity、独立故障域挂载和恢复演练；副本不可用、marker 不匹配或 identity 不可用时高风险操作失败关闭。
- 安全影响：公开 index/回执不作为真实性依据；深度校验先完整验证 age 密文和认证 manifest，再校验 PostgreSQL 目录。保留销毁只处理早于显式确认时间的已认证备份，保留最小份数并写不可复用墓碑。正式密钥仪式和真实异地主机由 `BLK-007` 跟踪。

---

## ADR-072：可修复的嵌入式加密依赖必须重建，不得豁免

- 日期：2026-08-02
- 状态：接受
- 决策：运行镜像扫描只要发现 age 二进制内嵌依赖存在明确修复版本，就拒绝发布，即使发行版仓库尚未重建该包。db-tools 使用 digest 固定的 Go builder、age `v1.3.1` 精确提交和显式 `golang.org/x/crypto v0.52.0` 构建两个 CLI；构建时断言上游提交和最终模块版本，运行阶段只复制静态二进制与许可证。
- 原因：发行包版本号更新不等于其静态链接模块已修复；对 fixable finding 做“当前不可达”处置会把可消除风险误写成接受风险。固定源码、工具链镜像和模块版本使修复边界可审计，同时避免把编译器、Git 或 Go 模块缓存带入运行镜像。
- 代价：镜像构建需要访问固定 Git 提交和 Go checksum/module 基础设施；上游发布新版本时必须显式更新提交、版本和依赖断言并重跑完整备份生命周期与镜像扫描。
- 安全影响：旧 age 1.2.1 的 18 Critical/32 High 可修复项及 age 1.3.1-r6 剩余的 `GHSA-w879-237q-wc7r` 均未获豁免。最终证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z` 的 `total_fixable_findings` 为空。

---

## ADR-073：本地管理协议使用显式长度帧并只允许两类受限运行时动作

- 日期：2026-08-02
- 状态：接受
- 背景：初版 Unix 客户端通过关闭写半边让 Agent 的 `read_to_end` 取得请求边界；Windows duplex Named Pipe 没有等价、可靠的半关闭语义，服务器等待 EOF、客户端等待响应会死锁。任务书还要求认证 ping 和控制面 reconnect，纯只读三命令边界已经不足。
- 决策：每条连接只处理一个请求和一个响应，两者均以 4 字节无符号大端 JSON 字节长度开头；请求上限 4 KiB、响应上限 512 KiB，零长度、超限、截断、未知字段和响应类型漂移失败关闭。Windows `xs-cli` 使用固定 Named Pipe client，Unix 继续使用 `0600` socket。读取命令无运行时副作用；`ping` 只通过有界 channel 请求已认证 XSP/1 PathChallenge/PathResponse，`reconnect` 只请求立即重建 Controller WebSocket，并有一秒手工冷却、oneshot 结果与非零失败退出。
- 原因：显式帧在 Unix stream 和 Windows byte-mode Named Pipe 上具有相同终止语义，不依赖 EOF；受限动作复用已有身份、AEAD、cooldown 和控制循环，避免 ICMP/raw socket、任意路由修改或无界重试。
- 代价：本地协议是同步版本切换，旧 CLI 与新 Agent 不互通；安装包必须同时升级二者。Windows 源码/交叉检查不能替代 Named Pipe、DACL、SCM 和全命令实机测试。
- 安全影响：客户端和共享 handler 均无 unsafe；服务端固定 DACL/远程拒绝不变。响应不含私钥、凭证、Token 或原始签名配置；ping/reconnect 失败不会降级为明文、修改系统路由或无限重试。

---

## ADR-074：固定域名的一键引导只分发离线签名的精确 Linux 发布集合

- 日期：2026-08-03
- 状态：接受
- 背景：设备接入需要缩减为 `wget -qO- https://vpn.qinwen.co/install | sudo bash`，同时不能把 Enrollment Token 放进 URL、Shell 历史、进程参数或服务日志，也不能让在线 Controller 获得发布签名私钥或提供任意文件下载。
- 决策：Controller 仅在配置了只读 Linux 发布目录时提供 `/install`，并只允许下载一个固定公钥和 `0.1.0` 的 x86_64/aarch64 两组 archive、manifest、detached signature 共七个精确文件。引导脚本编译进 Controller，固定 `https://vpn.qinwen.co/` 与 stable 下载路径，隐藏地从 `/dev/tty` 读取一次性 Token。脚本先校验内置公钥指纹，再验证 Ed25519 清单签名、字段、长度、SHA-256、归档成员类型/allowlist 和包内逐文件哈希，最后调用既有原子安装事务。正式私钥只保存在仓库和服务器之外；服务器仅保存发布公钥、签名与只读产物。
- 原因：单命令体验不应削弱 ADR-069 的离线授权边界。精确路由和文件 allowlist 消除通用静态文件服务器的路径遍历与意外泄露面；交互式 `/dev/tty` 输入在管道安装时仍不进入命令行。重复安装检测现有节点状态并保留身份、配置、签名状态和虚拟 IP。
- 代价：域名、版本和发布文件名均为显式固定值；发布新版本或轮换公钥必须经过代码/产物更新、重新签名和完整回归。当前引导仅支持以 systemd 为 PID 1、glibc、x86_64/aarch64 的 Linux，不声称支持 musl、非 systemd、Windows 或 macOS。
- 安全影响：Controller 启动时拒绝相对路径、符号链接、可被 group/other 写入的目录或文件、缺失/异常大小产物；归档按 64 KiB 分块流式返回并限制 256 MiB。未知文件返回 404，发布目录缺失返回不可用。Controller Docker 构建显式复制 `installers/`，确保嵌入脚本来自同一源码提交。

---

## ADR-075：NetAdapterCx 发送取消只能推进完成边界

- 日期：2026-08-04
- 状态：接受
- 背景：初版取消处理没有归还 ring，设备移除会无限等待；一次修正同时把 packet/fragment 的 Begin/Next 全部改到 End，普通 PnP restart 变快，但 UMDF/Application Verifier 记录 `WUDFVerifierFailure 414`，Windows WER 同时记录 `LKD_0x15E_VRF_Mini_Nbl_Leak_IMAGE_netcxrd.sys`。
- 决策：同步软件 NIC 的 TX 正常路径仍可在完整复制后同步推进 Begin/Next。进入 `EvtPacketQueueCancel` 时，只遍历 packet completion range、标记 `Scratch` 并把 packet `BeginIndex` 推进到 `EndIndex`；禁止在取消回调中伪造 packet `NextIndex`、fragment `NextIndex` 或 fragment `BeginIndex`。RX 继续把全部 packet 标记为 Ignore，并按 NetAdapterCx 合约归还 packet/fragment Begin range。源码门禁独立提取 TX cancellation 实现并拒绝所有禁止的 ownership 写入。
- 原因：TX packet post range 和 fragment ownership 由 NetAdapterCx 根据 NBL/packet 关系管理。取消路径自行改写这些边界虽能结束表面上的 PnP 等待，却破坏框架的 NBL 归还账本并在 verifier teardown 暴露泄漏。只推进完成边界与微软 NetAdapterCx cancellation contract 一致，也保持普通同步完成路径和取消路径的职责分离。
- 证据：测试包 `15.39.27.376` 在 Windows 11 24H2 VM 中通过 standard Driver Verifier 和 UMDF/Application Verifier；三轮 verifier-enabled PnP restart 为 1,443/2,113/877 ms，每轮 SYSTEM Tx/Rx 通过，且新增 WDF dump、NDIS/LiveKernel dump、相关 WER 和错误事件均为 0。完整记录见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。
- 边界：该决定只闭合测试签名驱动的 ring cancellation。它不证明完整 Windows Agent、SCM/Named Pipe/存储、IP Helper/DAD/route、睡眠、生产升级、Windows 10 或正式签名。

---

## ADR-076：Windows 适配器使用 Ethernet 媒体，私有 Agent ABI 保持 raw IPv4

- 日期：2026-08-04
- 状态：接受，替代 ADR-046
- 背景：ADR-046 的 `IF_TYPE_TUNNEL`/IP media 与 `Layer2TypeNull` 是 WDK/VM 前的源码假设。Windows 11 24H2 上实际可构建并绑定 NetAdapterCx UMDF 软件 NIC 的包使用 `netcxrd` filter、Ethernet IF/media 和 NetAdapterCx extension；不能把未运行的 L3 media 草案保留为权威设计。
- 决策：Windows NIC 向 NDIS/TCP-IP 暴露 Ethernet，设置固定本地 locally-administered MAC、接收过滤能力和 Ethernet layout；驱动只在 NetAdapterCx ring 边界验证/剥离或合成固定 14 字节 Ethernet 头，私有 Agent ABI 继续只传规范 raw IPv4。非 IPv4 TX frame 被有界消费并丢弃，不进入 Agent。异常 ring 元数据停止当前推进，但 callback 内禁止同步调用 link-state 变更，避免重入 NetAdapterCx stop/cancel。
- 原因：保持 Windows 11 官方支持的 NetAdapterCx Ethernet 路径，同时不把 ARP、邻居、广播策略或二层协议扩散进 Agent/XSP。专用 SYSTEM harness 已证明确定性 UDP IPv4 能进入 TX、raw IPv4 RX 能注入系统 ring，并在 PnP/Verifier 周期后重复成立。
- 代价：驱动边界增加固定 14 字节复制和 EtherType 检查；当前只接受 IPv4，IPv6/其他 EtherType 被丢弃。固定 peer MAC 只服务首版 point-to-point 三层语义，不声明通用二层交换能力。
- 安全影响：ABI 的 IPv4 长度、IHL、总长度、MTU、批次和容量验证不变；Windows 提交的其他 L2 流量不能穿过私有 ABI。源码门禁固定 Ethernet INF/capability/layout、头部转换以及 queue callback 内无同步断链。

---

## ADR-077: User-approved signed Wintun adapter boundary for Windows enrollment

- Date: 2026-08-04
- Status: Accepted as a Windows client exception; it does not change the clean-room `xsnet` driver scope.
- Decision: Windows release `0.1.0` may ship official Wintun `0.14.1` x64 `wintun.dll` only through the `xs-windows-wintun` adapter boundary. The builder and installer must pin the official archive SHA-256, DLL SHA-256, Authenticode signer subject, exact payload set, and the upstream prebuilt-binary license. The Agent dynamically loads only that regular, absolute, non-reparse DLL, creates an ephemeral L3 adapter/session, and accepts bounded validated IPv4 packets.
- Rationale: The user explicitly chose an already-signed adapter over distributing the still test-signed `xsnet` driver. This permits practical Windows enrollment without claiming that the self-developed driver is production-signed or that Wintun supplies any XS Nexus network protocol.
- Non-goals and boundary: Wintun must not implement or replace controller enrollment, node identity, XSP/1, key agreement, encryption, replay protection, NAT traversal, candidate selection, relay, ACL, IPAM, policy, or routing authorization. `drivers/windows-xsnet` remains independently validated and continues to prohibit Wintun in its driver source.

## ADR-078: Production host uses a repository-external containerized QA toolchain

- Status: Accepted, 2026-08-08.
- Decision: Keep Rust, Node build dependencies and cross headers out of the production host package set. Use a repository-external QA image that exactly matches `rust-toolchain.toml` (initially `xs-nexus/qa-rust:1.93.0`, currently `xs-nexus/qa-rust:1.94.0` under ADR-088) and narrow wrappers for Cargo, rustc, npm and ShellCheck; mount only the isolated QA worktree, bounded caches and `/tmp`. Privileged namespace tests compile first and execute the resulting host binary directly.
- Rationale: This preserves the production host baseline while still running the exact compiler, Clippy, rust-src, AArch64 GCC/libc and ShellCheck gates required by the task book. Direct host execution prevents Docker networking from replacing the network namespace under test.
- Consequence: The QA image and wrappers are operational evidence, not product dependencies or Git artifacts. Any image rebuild must record tool versions and rerun cross-package and full M5.2 validation.

## ADR-079: Authenticated peer traffic and PathResponse are distinct valid path proofs

- Status: Accepted, 2026-08-08.
- Decision: A matching encrypted PathResponse records `authenticated_path_probe` when the pending probe was created to promote a different endpoint. A periodic latency probe to the existing endpoint does not rewrite the prior path reason. If the peer independently promotes first and sends AEAD-valid traffic from the new endpoint, the receiving side may record `authenticated_peer_traffic` as already specified by XSP/1.
- Rationale: Simultaneous probes are not guaranteed. Requiring both peers to finish their own Challenge/Response before accepting authenticated traffic introduces a test-only timing assumption and can misreport a valid, identity-bound path as failure.
- Consequence: Integration tests still require an observed encrypted PathChallenge, at least one completed matching PathResponse, both active endpoints on the new path and bidirectional traffic. No unauthenticated source can change the active path.

## ADR-080: Deployment baselines compare stable identity and semantic firewall state

- Status: Accepted, 2026-08-08.
- Decision: Existing production Compose projects are captured before and after validation rather than required to be absent. Container snapshots use stable ID/name/image/state fields, not elapsed-time `Status` text. After intentional Docker recreation, nftables comparison removes counters/handles and substitutes ephemeral container IP literals with their Docker service names before canonical comparison.
- Rationale: Production validation must detect added, removed, restarted or stopped resources without failing merely because a healthy container's human-readable uptime crossed a display boundary or Docker reassigned service IPs.
- Consequence: Raw before/after Docker, network and nftables snapshots remain in evidence. Canonical equality supplements rather than replaces exact route, network-name, health, OCI revision and service-member checks.
- Residual gate: This exception is not a production-security certification. Full service/SCM, Named Pipe, protected storage, IP Helper/DAD/route recovery, sleep, repeated install/upgrade/rollback, Windows 10, and independent security review require their own evidence before an RC claim.

## ADR-081 RC 镜像标签必须等于发布 Git revision

- 状态：接受
- 日期：2026-08-08
- 背景：生产候选容器曾使用历史提交作为 Docker tag，但镜像 OCI revision 和活动部署记录已经是较新提交。内容本身可运行，然而 tag、标签和部署记录不一致会破坏审计、回滚选择和事故定位。
- 决策：RC 环境除继续要求干净 Git HEAD、`XS_RELEASE_REVISION` 精确等于 HEAD、镜像 OCI revision 精确匹配外，还要求 Controller、Migration、Relay、Console 和 db-tools 的镜像引用都使用精确 40 位发布 revision 作为 tag；digest 引用或任意旧/语义 tag 在当前 RC 编排中失败关闭。dev 环境保留独立测试 tag。
- 原因：使 Git、环境文件、镜像引用、OCI 标签、活动部署记录和回滚证据形成单一可追溯链，避免 mutable/stale tag 掩盖实际运行内容。
- 代价：文档提交若要成为生产工作树 HEAD，也必须重新构建 revision 标签镜像或保留独立部署工作树；不能用旧 tag 快速覆盖。
- 验证：错误 Controller tag 的 RC 预检被拒绝，正确五镜像 tag 通过，且 Docker 容器/网络、`1panel-network`、默认路由和归一化 nftables 不变；证据 `/srv/xs-nexus-qa/artifacts/deploy-tag-guard-20260807T062812Z`。

---

## ADR-082 生产审计修复证据不得自动升级为生产 GO

- 状态：接受
- 日期：2026-08-09
- 决策：生产审计修复必须在独立分支通过固定版本 baseline、真实 Controller/DB Console E2E、可执行协议 fuzz 和五镜像双无缓存复现；每个失败保留原始日志并修复根因。CI PASS 只证明该 revision 的工程门禁，不得替代 main 合并、签名 tag、正式部署、生产 provenance、真实平台、凭据/密钥、DR 和第三方审计。
- 实现：Console E2E 禁止 `page.route` 并使用临时 PostgreSQL/Controller；镜像复现记录 manifest/config/layer inventory，Edge 删除易变 `apk.log`，Console 使用固定 slim runtime 与隔离许可证阶段；生产健康守卫只读运行且不修改 1Panel 资源。
- 结果：revision `8532eb6` 的 run `31270487478` 全部通过，但 GitHub main、生产源码和运行镜像仍是旧 revision，最终发布结论保持 `NO_GO`。

---

## ADR-083 发布身份使用单一编译来源，正式产物只接受有效签名标签

- 状态：接受
- 日期：2026-08-09
- 决策：Rust 二进制、HTTP 版本端点、Console `version.json`、OCI 标签、Linux 包、SBOM、release manifest 和 in-toto/SLSA provenance 必须绑定同一个完整 40 位 Git commit、语义版本、XSP/1 版本与源码 epoch。正式 `make release` 只接受指向当前干净 `HEAD` 的有效加密签名 annotated tag、规范源码仓库和精确提交时间；release manifest 与 SHA256SUMS 使用独立 Ed25519 detached signature，并由离线 verifier 对完整文件集合和 subjects 做等值验证。
- 原因：过去镜像标签和运行时只能给出不完整、分散或不一致的 revision 线索，无法从已部署实例反向证明源码和构建输入。单一身份源与严格标签门禁把“代码声称的版本”和“签名发布的版本”统一为可复现断言。
- 失败策略：未知或非 40 位 commit、脏工作树、lightweight/无效签名标签、标签不指向 `HEAD`、source/epoch 漂移、额外或缺失文件、symlink/path traversal、摘要/subjects/签名漂移全部失败关闭；安装器在激活前再次核对运行时二进制身份。
- 证据：失败 run `31289302264` 保留了 `Cargo.lock --locked` 拒绝；修正 lock 元数据后，精确 revision `fea456b3d6feff36856b1f2066ace8a22b650bce` 的 GitHub Actions run `31289641228` 全部通过。
- 边界：CI 使用测试签名材料验证机制，不创建正式密钥、不完成正式 tag/bundle，不替代人工离线密钥仪式、main 合并、生产部署、运行时反向核验或从 `ff9551d3` 的升级/回滚。Gate 01 继续为 `FAIL`。

---

## ADR-084：PostgreSQL 运行、迁移和对象所有权必须分离

- 状态：接受
- 日期：2026-08-09
- 背景：旧生产 Controller 长期使用 bootstrap 超级用户，且 `serve` 会隐式创建 schema/执行迁移。应用漏洞或配置错误因此可获得建库、建角色和 DDL 能力；恢复后的对象 owner/grant 也可能漂移。
- 决策：生产数据库固定为不可登录 `xs_nexus_owner`、长驻 `xs_nexus_app` 和部署专用 `xs_nexus_migrator` 三类角色。`serve` 只验证当前角色和精确迁移状态，永不迁移；迁移以 migrator 登录并在事务内显式 `SET ROLE owner`。建表、首次 schema 创建和 restore 后均重放显式/default grants；应用角色不得写 `_sqlx_migrations`。
- 原因：把网络暴露应用、部署变更和对象所有权隔离，让 Controller compromise 不能直接扩大到集群管理或任意 DDL，同时仍允许可审计、可回滚的迁移和恢复。
- 失败策略：runtime/migration URL 相同、owner 可登录、角色属性不符、迁移 drift、无效 owner 标识、授权修复失败或 app 负向权限意外成功均失败关闭。数据库恢复必须以 owner 身份执行，并在服务激活前重新迁移/核对 grants。
- 生产变更：每次尝试先验证加密备份和隔离 restore，保留独立 SSH 会话并启用 20 分钟 systemd 回滚。Docker 重建只允许经 container inspect 认证的项目地址/端口规则变化；所有非项目 nftables 规则、默认路由、IP rule、`1panel-network` 和无关 1Panel 容器必须不变。
- 证据：Git commits `02fc54e`、`3e2caed`、`e0fd15d`、`0533727`、`3d93656`；CI run `31294988591`；隔离证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z`；生产证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z`。
- 边界：该决策关闭 Gate 16，不关闭 Gate 02。bootstrap 仍作为受控平台管理身份存在，必须单独轮换并验证旧值拒绝；正式 release、主机防火墙、TLS、真实设备和第三方审计结论均不由此推导。

---

## ADR-085：生产 INPUT 防火墙使用独立 nftables 表，1Panel 仅经受限 SSH 隧道管理

- 状态：接受
- 日期：2026-08-09
- 背景：生产宿主 INPUT policy 长期为 accept，1Panel TCP `188` 直接公网监听；同时 Docker/1Panel 已维护自己的 nftables 表，使用全局 `flush ruleset` 或接管默认规则会破坏受保护资源。
- 决策：XS Nexus 只拥有 `inet xs_nexus_host_guard` 表，INPUT hook priority `-10`、默认 drop；仅放行 loopback、established/related、必要 ICMP/DHCP、限速 SSH TCP `122`、Web TCP `80`/`443` 与 QUIC UDP `443`。不允许公网 TCP `188`，管理员只能用 key-only SSH 的 local forwarding 到 `127.0.0.1:188`/`[::1]:188`；其他转发和 tunnel 全部关闭。
- 安全边界：脚本只增删该表，不调用全局 flush，不修改 Docker/1Panel/UFW/iptables compatibility、默认 route、IP rule、Docker network 或 1Panel 配置。Discovery/Relay UDP `42000`/`42001` 继续由 Docker DNAT/forward 路径承载。
- 变更纪律：任何 SSH/firewall 更新都必须有冻结 baseline、两条保留会话、20 分钟自动回滚、候选语法验证、全新会话和外部端口/认证负向测试；只有 protected services、OpenResty、`1panel-network` 和非项目网络状态全部通过后才能取消回滚。
- 证据：Git commit `ae74783cdf9f75fd90e496fe837e50b744990310`；CI run `31300939362`；生产证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-host-hardening-20260809T071145Z`。
- 残余：该决策关闭 SSH/公网管理暴露和最小 INPUT 子项，不代表 Gate 14 全部通过。安全更新/重启和磁盘压力已由 ADR-086 闭合；外部告警仍需单独完成。

---

## ADR-086：生产内核升级使用 one-shot GRUB、持久旧内核 fallback 与 Boot-ID 外部批准

- 状态：接受
- 日期：2026-08-09
- 背景：主机有 157 个待升级包和新内核；普通远程 reboot 若新内核、SSH、Docker、防火墙或网络失败，可能永久失联。Docker daemon 重启还会重新分配内部地址和重排生成规则，不能以字节差异直接误判，也不能简单忽略。
- 决策：包升级前重打包旧版本、验证备份并启用限时自动降级；用户态全部通过后才取消。内核阶段临时 `GRUB_DEFAULT=saved`，旧内核为持久 fallback，新内核仅 one-shot；开机 watchdog 先验证内部健康，再等待当前 Boot ID 的外部端口/认证批准，超时或回归自动 `grub-set-default` 旧内核并 reboot。批准后移除临时 unit/drop-in/env，恢复原始 `GRUB_DEFAULT=0`。
- nftables 判定：容器 IP 只能映射到 Docker inspect 认证的稳定成员身份；完整 canonical item multiset 必须精确相同；非 allowlist 链顺序必须精确相同；allowlist Docker 链只允许被逐对证明为匹配谓词不相交的规则反转。任何无法证明的顺序变化失败关闭。
- 清理边界：只删除已验证、路径固定、非 symlink/mount、无打开引用且可重建的项目/维护缓存；禁止 global Docker prune 和 apt autoremove。维护专用旧包和敏感配置归档只在新内核内外部回归通过后销毁。
- 证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z`，218 个最终 manifest 条目，无值秘密扫描 0 findings。
- 残余：该决策关闭 Gate 14 的补丁、reboot 和磁盘压力内部项；没有外部通知目标时不得把本地状态或模拟阈值称为真实磁盘告警送达，Gate 14 仍为 `PARTIAL`。

---

## ADR-087：计划域名必须独立验证边缘与源站严格 TLS，并统一经 Console 进入应用

- 状态：接受
- 日期：2026-08-09
- 背景：边缘证书可验证并不证明 CDN 到源站的严格 TLS 成功。计划域名当前在所有功能路径返回 `525`，源站 SNI 失败；现有 public vhost 又把根路径送到只提供 API 的 Controller，导致即使 TLS 修复也无法提供 Console 根页面。
- 决策：生产验收必须分别探测 edge DNS 和 direct origin；两者都以计划域名作为 SNI/Host，使用系统信任或明确批准的 CA、`CERT_REQUIRED`、主机名验证和 TLS 1.2 以上。HTTP 路径必须验证精确状态，WebSocket 必须验证完整 `101`/Upgrade/Accept。计划域名 OpenResty 只把全部路径代理到 Console loopback `127.0.0.1:28081`，由 Console 统一处理 SPA、health、API 与 WebSocket，不直接把根路径指向 Controller。
- 失败策略：证书链、SAN、SNI、协议版本、HTTP 状态、WebSocket accept 任一失败即失败关闭；工具只报告可观察结果，不推断 CDN control-plane strict 模式，也不把模板存在视为部署完成。Flexible SSL、明文回源、禁用验证、忽略证书错误和 Controller 根 upstream 均被禁止。
- 所有权边界：仓库只提供 `scripts/audit-strict-tls.py` 和 `deploy/host/openresty-xs-nexus-vhost.conf.example`。DNS/CDN、证书私钥和 1Panel/OpenResty 站点属于所有者控制面，未经批准不得应用；任何批准后的变更必须保留现有站点、`1panel-network`、防火墙、路由和容器不变量及回滚。
- 证据：Git commit `94ccae3e8b4bf0279336d01db8b1ab53abf15aae`；`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-cdn-tls-readonly-20260809T094531Z`；`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate13-strict-tls-tool-20260809T100334Z`。
- 残余：当前生产没有应用该模板或计划域名证书/CDN 设置，Gate 13 仍为 `FAIL`，Gate 19 仍为 `PARTIAL`，正式修复为 `BLOCKED_EXTERNAL`。

---

## ADR-088：可消除的依赖公告不得豁免，信息类公告采用有期限拓扑复核

- 状态：接受
- 日期：2026-08-09
- 背景：SQLx `0.8.6` 的不可达功能仍把 `rsa` 带入锁文件并触发 `RUSTSEC-2023-0071`；现有脚本对其设置 ignore，且 `cargo-deny` 没有完整许可证策略。不可达不等于依赖不存在，长期 ignore 会掩盖未来 feature 或拓扑漂移。
- 决策：升级 Rust `1.94` 和 SQLx `0.9.0`，从锁文件消除 `rsa` 并删除公告 ignore；动态 SQL 只能通过显式 `sqlx::AssertSqlSafe` 进入执行边界。`cargo audit` 必须以空 ignore 运行，`cargo deny --all-features check` 必须完整执行 advisories、bans、licenses、sources；许可证使用显式 allowlist，不使用通配跳过。
- 信息类边界：`RUSTSEC-2024-0436` 仅声明 transitive `paste 1.0.15` 未维护、没有漏洞 CVSS 或可用 patched version；它通过 `rtnetlink` 路径进入。保持告警可见，最迟 `2026-08-31` 复核，且 lock、上游依赖、feature、公告或生产 revision 任一变化立即提前复核；由 `KI-026` 跟踪替换或正式 disposition。
- 供应链边界：Rust builder 和 CI toolchain 固定到 `1.94` 精确镜像 digest；SBOM 预期计数随真实 lock 更新，不删除组件以制造旧计数。镜像 glibc Critical/High 仍由 `KI-021` 独立跟踪，本决策不把 API 不可达 disposition 伪装成修复。
- 证据：Git commit `3bf861922c8b3cc62c3bfd1617835565fd86fc6b`；GitHub Actions run `31313868529`；clean-checkout 根 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z`；失败证据封存复核根 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z`。
- 残余：`3bf8619` 未部署、未合并 main、不是正式签名 RC。全局 Critical/High 与外部门禁仍开放，因此 Gate 23 为 `FAIL`、Gate 24 为 `PARTIAL`、总体为 `NO_GO`。

---

## ADR-089：不为消除 `paste` 信息告警而维护私有 netlink fork

- 状态：接受；到期自动复核
- 日期：2026-08-10
- 背景：Gate 24 对 `RUSTSEC-2024-0436` 进行了精确依赖与上游复核。锁文件路径是 `xs-agent -> rtnetlink 0.21.0 -> netlink-packet-core 0.8.2 -> paste 1.0.15`；公告为 unmaintained informational，没有 CVSS、已知利用或 patched version。
- 决策：当前继续使用上游 `rtnetlink 0.21.0`，不为隐藏信息告警而 fork/vendor `rtnetlink` 或 `netlink-packet-core`。plain `cargo audit` 保持空 ignore 和原始告警；`cargo-deny` 只保留带原因、于 `2026-08-31` 失效的单一例外。
- 原因：复核时 `rtnetlink` `main` `e7799b6ee24267586e6aadc0e3fb415b4d921dd4` 仍为 `0.21.0`，`netlink-packet-core` `main` `571d8bb5fa1dbaa875e8aede3f214c87f70b955b` 仍直接依赖 `paste = "1"`。私有 fork 会把关键 Linux 网络依赖的维护与安全更新责任转移给项目，却没有修复已证明的漏洞。
- 失败策略：`scripts/check-rust-advisories.sh` 在截止日期后失败关闭；锁文件、路径、feature、公告、上游 manifest、工具链或候选 RC 变化均要求提前重审。上游正常版本移除 `paste` 后优先升级并删除例外，再重跑完整网络与供应链回归。
- 证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z`；`audit/production-readiness-remediation-v2/DEPENDENCY_TOPOLOGY.md`。
- 边界：该决策只完成 `KI-026` 的 P2 有界处置。`KI-021` glibc、正式签名发布、精确 revision 部署、第三方审计和整体 `NO_GO` 均不改变。

---

## ADR-090：CI 第三方 Action 固定到已复核的 Node 24 提交且不持久化令牌

- 状态：接受
- 日期：2026-08-10
- 背景：精确提交 `45dbc196` 的 CI 全部通过，但 GitHub 对 checkout、setup-node 和 upload-artifact 发出 Node 20 弃用告警并强制使用 Node 24。成功退出码不能消除未来 runner 停止兼容旧 action runtime 的风险；checkout 默认持久化 `GITHUB_TOKEN` 也超出只读构建所需权限。
- 决策：三项官方 Action 分别固定到验证有效的 `v7.0.1`/`v7.0.0`/`v7.0.1` 精确提交，保留可审计版本注释；全部 checkout 显式 `persist-credentials: false`。Docker Buildx 继续使用已固定且为 Node 24 的 `v4.2.0` 提交。
- 门禁：所有远程 `uses:` 必须为小写 40 位 commit SHA；四项已复核 Node 24 Action 必须精确匹配 commit 与版本标签；checkout 缺少 `persist-credentials: false` 即失败。正向和五类负向 fixture 在 `security-check` 中执行。
- 证据：预修复 run `31324714609` 保留 Node 20 annotation；实现提交 `e63c59bc7c54b78de94b01885fa9af1b54a31746`、`98ca145c197b1d2af20d7cf5df28008820a25e50`；最终 GitHub Actions run `31325753985` 四项 job 全通过且 annotation 数均为 0。
- 边界：Action 运行时和 token 最小化只加固 CI，不构成正式 release key、签名 RC、部署 provenance 或第三方供应链审计。

---

## ADR-091：握手 Finish 精确重传，加密控制重试使用新序列

- 状态：接受
- 日期：2026-08-10
- 背景：Finish 的固定 nonce 要求网络重传逐字节相同；但已建立数据面的 PathChallenge 和 KeyUpdate 受序列重放窗口保护，逐字节重发会在首次请求已处理而响应丢失时被拒绝，导致路径探测或轮换无法恢复。Server 若发送 ServerFinish 后立即丢弃编码，也无法恢复最终响应丢包。
- 决策：Server 在有界握手重试窗口内按 ClientFinish 摘要缓存精确 ServerFinish，匹配重复请求只返回缓存帧且不授权路径迁移。PathChallenge 和 KeyUpdate 保持逻辑 Path ID/token 或 payload/Epoch 不变，但每次重试通过当前发送器生成新的单调序列与 AEAD 密文。KeyUpdate 重试耗尽、序列耗尽或状态不确定时丢弃会话并完整重握手。
- 兼容性：XSP/1 版本、字段、长度、密钥派生和首个请求编码均不变；旧实现仍可处理新序列重试。测试向量新增 `retry_semantics` 元数据，不改变既有字节向量；Fuzz 状态机和 corpus 覆盖同 payload 的新序列重试与旧密文重放拒绝。
- 验证边界：内部回归只能证明实现行为，不替代独立密码学审计、真实 WAN 丢包矩阵或正式发布门禁；这些 Gate 状态保持原结论，直至各自原始证据完成。

---

## ADR-092：Linux Agent 恢复测试不得依赖或改写正式安装路径

- 状态：接受
- 日期：2026-08-10
- 背景：真实 systemd sandbox 的 `ProtectHome=yes` 会正确阻止从 CI runner home 执行二进制；若为绕过该限制而关闭 sandbox，会使测试失真。旧测试又曾临时依赖 `/usr/local/lib/xs-nexus/current` 完成 unit verify，存在与既有安装冲突的风险。
- 决策：每次恢复测试创建唯一 `/usr/local/lib/xs-nexus-tests/<unit>` 根并复制精确构建产物，保持 `ProtectHome`、设备、capability、namespace 和文件系统限制。正式 unit 复制到临时 root，并通过 `systemd-analyze verify --root` 离线验证；测试永不创建、替换或删除正式 `current` 路径。
- 验证：transient unit 必须以 `Restart=on-failure` 运行，`SIGKILL` 后恰好自动重启一次且 PID 改变；签名状态保留、TUN 只在私有 namespace 重建、隔离链路变化后进程存活，最终 unit/TUN/test root 全部清理。run `31352258781` 和 artifact `9049384061` 通过。
- 边界：GitHub hosted x86_64 的真实 systemd/TUN 证据不替代普通主机 reboot、disk-full、DHCP/VPN 冲突、arm64/NAS 或生产 Agent 部署。Gate 06 保持 `PARTIAL`。

---

## ADR-093：Relay 验签与队列必须同时受每主体和全局硬上限约束

- 状态：接受
- 日期：2026-08-10
- 背景：每来源注册限制只能约束单一来源地址，每 Lease 队列只能约束单一目标。来源地址喷洒仍可在验签前扩展状态和消耗公钥运算，多个合法 Lease 仍可把总队列内存放大到 `lease_count × per_node_limit`。短时 10,000 帧基线也不能证明跨 Lease、重启和短期 Lease 续租行为。
- 决策：Relay 在创建来源预算状态和执行注册签名验证前先消耗一秒窗口的全局注册预算；数据面同时执行每目标 Lease 与全局 packet/byte 队列上限。拒绝必须发生在 replay 和 traffic budget 提交前；enqueue、dequeue、Lease 替换、过期与 cleanup 共同维护精确全局计数和低基数 gauge。全局上限小于一个完整每节点队列时配置失败关闭。
- 容量验证：专项 release-profile 测试固定 5,000,000 个 216 字节认证帧、至少 10,000 包/s、零 Relay 丢弃和零最终队列。两个节点每 400,000 帧使用相同身份与 UDP 端点、唯一签名请求重新注册，取得新 Lease 后重置该 Lease 的独立 sequence；不延长生产 TTL、不关闭过期检查，也不增加注册速率豁免。
- 故障验证：双 Agent/双 Relay namespace 必须证明主 Relay 停止、备用接管、主 Relay 重启、双方重新注册、备用停止后经重启主 Relay 恢复，以及最终认证 Direct 回切；断言依据实际认证端点，不把合法 `relay_fallback`/`relay_failover` 状态命名差异伪报为链路失败。
- 证据：精确 revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e`；GitHub Actions run `31358498444`；Relay job `93362562136`；artifact `9051561308`，归档 digest `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67`，下载后原始相对 SHA-256 清单和无值秘密扫描均通过。
- 边界：该决策只闭合 hosted 单进程资源边界、持续 loopback 转发和受控 Relay 重启。真实公网恶意流量、分布式有效凭据、云侧链路饱和、多地域/多实例容量和小时/天级负载仍需外部环境；Gate 08 保持 `PARTIAL`，总体保持 `NO_GO`。

---

## ADR-094：配置外层版本与 ACL 策略版本必须分别单调

- 状态：接受
- 日期：2026-08-10
- 背景：签名配置同时包含传输/拓扑使用的 `configuration.version` 和授权语义使用的 `policy_version`。只约束外层版本会允许“较高配置版本包裹较低 ACL 策略版本”的已签名回滚，破坏 Agent 最近有效授权状态。
- 决策：Agent 在解析、签名、网络和节点身份校验后、任何状态替换前，分别要求外层配置版本和策略版本不下降；外层同版本异内容继续按 equivocation 失败关闭。策略版本下降即使外层版本更高也拒绝，拒绝不得修改持久状态、路由、Peer 或 ACL。
- Gate 09 证据拓扑：三节点 Direct 矩阵证明 Controller 离线下的 A→B allow、A→C/C→B deny 和双端执行；独立 Relay 与子网 namespace 矩阵证明授权不能通过转发路径绕过；协议单测证明 Node ID 和虚拟源身份绑定。所有路径使用真实 Agent/TUN/XSP/1，不以 Mock 替代。
- 证据：修复提交 `3f27ed9`；最终 revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b`；GitHub Actions run `31360862865`、ACL job `93369332314`、artifact `9052383034`。
- 边界：namespace 证据足以关闭 ACL 实现 Gate 09，但不替代真实 WAN、真实 NAS/subnet router、独立安全审计或生产部署；总体仍为 `NO_GO`。

---

## ADR-095：1Panel 共存证据必须分离当前源码夹具与真实生产生命周期

- 状态：接受
- 日期：2026-08-11
- 背景：真实生产宿主已经有项目升级/回滚和整机重启证据，但当前整改 revision 未部署；仅复用旧生产结果不能证明当前 Compose 边界，仅运行 hosted fixture 又不能证明真实 1Panel、网站、数据库和 SSH 在主机生命周期后的状态。
- 决策：Gate 15 必须由两层证据共同闭合。当前精确 revision 通过静态 source validator、完整插值 Compose JSON 和 CI-only real Docker-daemon restart，证明 external-only ownership、项目 scoped down、未知 sentinel、目标网络 exact ID、稳定网络 inventory、默认路由和 cleanup。真实生产 Gate 14/16 证据独立证明 host reboot、1Panel/OpenResty/SSH 恢复、project restart/upgrade/automatic rollback、数据库边界与生产网络 identity。
- 运行安全：CI fixture 只有在 `GITHUB_ACTIONS=true`、显式 fixture flag 和精确 `1panel-network` 不存在时才运行；资源带当前 run label 且只按 label 清理。真实网络已存在时必须失败关闭，禁止在生产或 1Panel 主机运行。生命周期源码禁止 global prune、network create/remove/connect/disconnect、Docker socket、host networking、privileged、volume delete 和 unscoped Compose down。
- 证据语义：Docker daemon 可重建 hosted runner 的内置 `bridge` ID，因此全局基线比较 name/driver/scope inventory；目标 external network 和 unrelated sentinel 始终要求 exact ID。最终 PASS 只能在显式 cleanup、全局 inventory 和 route 复核后输出，不能先打印成功再依赖 EXIT trap。
- 证据：revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6`；run `31504402285`；job `93822197946`；artifact `9106406005`；`audit/production-readiness-remediation-v2/ONEPANEL_COEXISTENCE.md`；Gate 14/16 生产 evidence roots。
- 边界：该组合足以关闭 Gate 15 的共存范围，但不证明当前 revision 已部署、已签名、可公开访问或整体 Production Ready；Gate 01/13/18/20/22/23/24/25 与外部门禁保持原状态。
