# KNOWN_ISSUES.md — 已知问题和风险

---

## KI-001 自研协议未完成独立安全审计

- 严重度：高
- 状态：开放
- 说明：内部测试不能替代第三方协议和密码学审计。
- 当前内部证据：Gate 04 已在 revision `875395352afc8a17dafc17b2496d62ce701ee4ae` 通过状态机回归、六目标 AddressSanitizer Fuzz 和完整 CI；该结果不解除本条，Gate 05 仍为 `BLOCKED_EXTERNAL`。
- 影响：在完成独立审计前不得宣称生产级安全。
- 解除：完成外部审计并处理发现。

---

## KI-002 Windows 正式驱动签名未完成

- 严重度：高
- 状态：开放
- 说明：开发阶段可使用测试签名，正式发布需要合法签名和兼容性流程。
- 解除：完成正式签名、安装和升级验证。

---

## KI-003 100.88.0.0/16 可能冲突

- 严重度：中
- 状态：开放
- 缓解：运行时冲突检测和可配置地址池。
- 已完成缓解：Controller 拒绝重叠虚拟地址池；Linux Agent 在创建 TUN 前通过 Netlink 检测本机系统路由重叠并失败关闭，不覆盖现有路由。
- 证据：`/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`。
- 保持开放原因：共享地址空间仍可能与运营商 CGNAT、其他主机上的 VPN 或尚未接入的真实设备冲突，需在真实节点和后续子网路由阶段继续验证。

---

## KI-004 真实 NAS 不可从公网服务器直接访问

- 严重度：预期限制
- 状态：开放
- 说明：必须由 NAS 主动安装和注册。
- 解除：用户执行安装后通过虚拟 IP 管理。

---

## KI-005 真实 Windows 驱动测试依赖外部 VM

- 严重度：高
- 状态：已解除（2026-08-04）
- 历史说明：Linux 服务器不能证明 Windows 驱动实机稳定。
- 解除证据：Windows 11 24H2 快照 VM 完成 WDK build、测试签名、clean install、SYSTEM Tx/Rx、PnP restart、standard Driver Verifier、UMDF/Application Verifier 三轮重复验证和 clean uninstall；见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。
- 剩余边界：完整 Agent、生产安装器/签名、路由/DAD/睡眠和 Windows 10 是独立开放项，不由本问题的解除覆盖。

---

## 新问题模板

```markdown
## KI-NNN 标题

- 严重度：
- 状态：
- 首次发现：
- 影响：
- 复现：
- 临时缓解：
- 根因：
- 计划：
- 解除条件：
```

---

## KI-006 现有 PostgreSQL 和 Redis 端口可从公网到达（已解除）

- 严重度：高
- 状态：已解除
- 首次发现：2026-07-29
- 影响：违反数据库不得暴露公网的最终验收要求，增加凭据猜测和服务漏洞攻击面。
- 复现：从开发服务器外部对 `34.92.139.129` 的 TCP `5432`、`6379` 建立连接成功。
- 证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt`
- 临时缓解：项目不使用公网端口连接数据库，不扩大暴露面，不记录凭据。
- 根因：现有 1Panel 容器在项目开始前已经发布到所有主机接口。
- 已完成：项目迁移到新的生产候选服务器；当前只运行无 host binding 的 `xs-nexus-rc-postgres`，外部 TCP `3306`、`5432`、`6379`、`28080`、`28081` 均不可达，Controller/PostgreSQL 和其余项目容器健康，`1panel-network` 保持外部网络且成员未变。
- 解除证据：`/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z`。
- 解除日期：2026-08-08。
- 解除条件（已满足）：外部探测不可达、项目数据库无宿主映射、容器内连接和项目健康检查通过。

---

## KI-007 开发服务器等待系统重启（已解除）

- 严重度：中
- 状态：已解除
- 首次发现：2026-07-29
- 影响（历史）：已安装内核或系统更新尚未通过重启生效。
- 已完成：用户在维护窗口完成重启；服务器运行 Linux `7.0.0-1008-gcp`，SSH、Docker、现有 1Panel/业务容器、默认网络和 `1panel-network` 均恢复。重启后的 24 小时稳定性样本、修正重启回归、Docker 生命周期及供应链验证继续通过。
- 解除证据：`/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T212242Z`、`/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z`、`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`。
- 解除日期：2026-08-02。
- 解除条件（已满足）：重启后内核、Docker、1Panel、SSH、路由和防火墙验证通过。

---

## KI-008 M1.3 前 TUN 数据包只读取并丢弃

- 严重度：中
- 状态：已解除
- 首次发现：2026-07-29
- 影响：M1.2 已完成接口和生命周期，但尚不能在节点间传输虚拟网络业务流量。
- 复现：启动已注册 Agent 后向项目 TUN 写入数据包，诊断中的接收和丢弃计数增加，不产生明文或未认证 UDP 转发。
- 临时缓解：保持失败关闭，不提供未加密回退路径，也不把业务包送入控制连接。
- 根因：XSP/1 握手、AEAD、抗重放和 TUN/UDP 双向循环属于 M1.3。
- 计划：实现经过测试向量约束的 XSP/1 会话状态机，并在两个隔离 namespace 间完成加密双向验证。
- 解除条件：M1.3 全量验收通过，TUN 数据只经认证加密会话发送，异常和未认证包被拒绝。
- 解除日期：2026-07-29
- 解除证据：`/srv/xs-nexus/artifacts/qa/m1.3-20260729T153126Z`；双向 ICMP/TCP/UDP、密文抓包、自动 Key Epoch、Tag 篡改、重放、伪造源地址和 Controller 端点中断均通过。

---

## KI-009 隔离 NAT 模型不能替代真实公网验证

- 严重度：预期限制
- 状态：开放
- 首次发现：2026-07-29
- 影响：M2.2 的 namespace/nftables 自动化可以证明双方主动认证握手、映射保活、普通受限 NAT、端口受限 NAT、双端 NAT、对称 NAT 失败、公网地址重绑定和 UDP 解封恢复，但不能代表所有运营商 CGNAT、真实公网 IPv6、多出口、防火墙和长期网络抖动。
- 复现：`scripts/test-agent-nat-matrix.sh` 只创建并清理临时 namespace、veth、bridge 和 nftables，不改变宿主机默认路由、1Panel 网络或生产防火墙。
- 临时缓解：M2.2 只声明可重复的隔离 NAT 行为矩阵通过，不宣称所有真实公网环境均已验证。
- 根因：真实运营商与边缘网络条件无法由单台开发服务器完整模拟，且生产防火墙属于人工门禁。
- 已完成缓解：`/srv/xs-nexus/artifacts/qa/m2.2-20260730T092547Z` 已覆盖计划内隔离模型，并验证无网络/防火墙资源残留。
- 计划：在获批外部 Linux 节点、NAS 和 Windows VM 上补充真实公网 IPv4/IPv6、CGNAT、多出口和长时间稳定性验证。
- 解除条件：受控外部节点覆盖上述真实网络条件，证据可重复且不伪造人工门禁结果。

---

## KI-010 Relay 公网容量和延迟/丢包指标尚未验证

- 严重度：预期限制
- 状态：开放
- 首次发现：2026-07-30
- 影响：M2.3 已证明认证、限速、队列、主备切换、密文不可恢复和 Direct 回切，但单机 namespace 测试不能证明真实公网容量、跨地域 RTT、持续丢包、拥塞公平性或长期稳定性。
- 复现：`scripts/test-agent-relay.sh` 使用同一开发服务器上的临时 bridge、namespace 和两个 Relay 进程，验证功能与安全边界，不注入广域网延迟和长期负载。
- 临时缓解：仅把隔离主机上的功能、吞吐、RTT 与可观测性证据作为本地基线，不把它描述为跨地域公网容量结论。
- 根因：真实跨地域节点、容量压测窗口和生产网络观测属于后续性能阶段及人工门禁。
- 已完成缓解：`/srv/xs-nexus/artifacts/qa/m2.3-20260730T113304Z` 已覆盖双 Relay、速率限制恢复和宿主机基线不变；Relay 现把无节点身份、端点或载荷的累计指标用自身目录身份密钥签名推送给 Controller，Controller 保存有界 25 小时窗口并在 Console 展示当前健康、流量、丢弃、错误和转发延迟。
- 计划：本地 Relay 吞吐、内部延迟、分类丢弃、Agent Direct/Relay RTT 与 24 小时稳定性基线已经完成；待受控跨地域节点可用后补充公网 RTT、网络中途丢包、拥塞公平性、长期容量和多实例扩展，不重复实现已经接入的指标。
- 解除条件：受控跨地域环境完成容量与故障压测，指标、阈值和证据路径纳入 Release Checklist。

---

## KI-011 隔离子网路由实验不能替代真实 NAS 验收

- 严重度：预期限制
- 状态：开放
- 首次发现：2026-07-31
- 影响：M3.2 已证明签名建议、审批、纯路由/NAT、转发 ACL、源地址保护、网关离线撤销和系统回滚，但不能证明真实 NAS 的发行版、权限、物理 LAN、防火墙和长期运行兼容性。
- 复现：`scripts/test-agent-subnet-route.sh` 只使用开发服务器上的三个临时 namespace 和 `192.168.232.0/24`，不访问真实 `192.168.0.0/24` 或 NAS。
- 临时缓解：M3.2 只勾选可由自动化证据支持的路由能力；`M8.1 NAS 候选接入` 与 `BLK-002` 保持人工门禁。
- 已完成缓解：`/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z` 验证所有临时路由、TUN、namespace、nftables 和 forwarding 状态均被清理，宿主与 1Panel 基线不变。
- 计划：完成 Linux 安装/升级阶段后，由用户在 NAS 本地安装并先验证普通节点，再单独审批真实子网。
- 解除条件：真实 NAS 按 `EXECUTION_PLAN.md` M8.1 顺序完成普通节点、Direct/Relay、升级卸载、子网审批、ACL 和离线撤销证据。

---

## KI-012 Agent 路径遥测和 Relay 指标尚未接入控制台（已解除）

- 严重度：预期限制
- 状态：已解除
- 首次发现：2026-07-31
- 影响：首页、节点、Relay 和拓扑仍不能显示当前 Direct/Relay 选择、Agent 端到端流量/延迟和 Relay 健康数值。
- 临时缓解（历史）：Controller 使用结构化 `unavailable`、空值和原因；控制台显示“未采集”，绝不把零、离线、Direct 或健康作为默认值。
- 根因：Agent 控制协议和 Relay 服务最初只提供本地诊断，没有经过身份认证、重放保护和有界保留的 Controller 上报契约。
- 已完成：Agent 用节点身份密钥签名 boot/sequence、当前 Direct/Relay 路径、业务字节、握手和认证 RTT 累计值；Controller 验证当前 Peer 集合、Relay 目录、时间、签名和同 boot 单调性，保存最新值与最多 25 小时/1800 样本。Relay 用目录身份密钥签名无节点身份、端点、Lease 或 payload 的全局累计指标，Controller 同样拒绝重放、计数回滚和畸形分类总和，并以 25 小时/9000 样本覆盖最快 10 秒上报周期。Console 展示新鲜/陈旧状态、当前路径/Relay、24 小时流量、握手成功率、延迟和 Relay 流量/丢弃/错误。
- 解除日期：2026-08-02
- 解除证据：Core、Agent、Relay、Controller 严格 Clippy 与单测；真实 PostgreSQL 覆盖有效签名、错误签名、重放、过期、回滚和畸形报告；隔离双 Relay/双 Agent 全链路验证签名推送、密文转发、故障切换和 Direct 恢复；Console Playwright 覆盖可用指标状态。
- 解除条件（已满足）：真实 Agent/Relay 指标经过权限、边界、错误和运行链路测试，控制台只对新鲜认证报告显示数值，缺失或陈旧报告不推断健康。

---

## KI-013 Linux 正式离线发布签名尚未执行

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：M5.1 已验证 Ed25519 清单签名、首次公钥固定、篡改拒绝和原子生命周期，但测试密钥不能作为正式 Release Candidate 信任根。
- 临时缓解：所有 M5.1 产物和证据明确标记为测试签名；仓库、包和证据中不保存签名私钥，不把测试签名描述为生产签名。
- 已完成缓解：`/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z` 覆盖错误签名、错误公钥、外层/内层篡改、防降级和回滚验证。更新控制面现只接受 32 字节发布公钥与已签名不可变清单；Agent 和无网络 root helper 独立复验签名/归档并复用原子回滚，Console 没有私钥上传字段。以上降低在线 Controller 被攻陷后的任意代码执行风险，但不替代正式签名仪式。
- 计划：在 M9.1 前建立离线密钥生成、双人授权、备份、撤回、版本与公钥分发流程，并从干净固定提交生成 RC 签名。
- 解除条件：正式离线签名仪式和恢复演练完成，生产公钥通过独立认证渠道发布，RC 产物签名、哈希和来源证明可复核。

---

## KI-014 arm64 仅完成真实交叉构建，尚未完成目标设备运行验收

- 严重度：预期限制
- 状态：开放
- 首次发现：2026-07-31
- 影响：`xs-agent` 与 `xs` 已构建为 AArch64 ELF，但尚不能证明具体 NAS 内核、glibc、systemd、TUN、nftables 和权限模型兼容。
- 临时缓解：安装器严格绑定 `aarch64-unknown-linux-gnu` 与主机架构，错误目标拒绝；文档不把交叉构建描述为 NAS 实机通过。
- 已完成缓解：M5.1 真实交叉编译、ELF Machine 检查和签名打包通过，证据为 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。
- 计划：按 `BLK-002` 由用户在 NAS 本地先作为普通节点安装，完成 systemd、注册、Direct/Relay、升级和卸载后再审批子网。
- 解除条件：真实 arm64/NAS 目标完成 M8.1 规定的可重复测试并保存证据。

---

## KI-015 数据库备份尚未静态加密和异机复制（已解除）

- 严重度：中
- 状态：已解除
- 首次发现：2026-07-31
- 影响：M5.2 已验证私有目录、自定义归档、大小/SHA-256 清单、篡改拒绝和恢复回滚，但宿主 root 或备份介质泄露仍可能暴露数据库内容，单机磁盘故障也可能同时丢失数据库与备份。
- 历史复现：旧版 `deploy/docker/db-tools.sh` 生成模式 `0600` 的明文 PostgreSQL custom archive 和 manifest，没有加密层或远端复制。
- 已完成：`pg_dump` 现在流式进入 age X25519 加密，数据库归档不落明文文件；认证加密 manifest 绑定 schema、备份名、密文大小/哈希、接收者 Key ID 和时间。公开 index 与复制回执允许无私钥校验传输完整性，离线 identity 深度校验同时验证 manifest、完整密文认证和 `pg_restore` 目录。
- 异地边界：每次备份自动复制到必须位于不同文件系统、带部署/目标私有 marker 的挂载点；支持幂等复制、仅从完整副本取回、独立本地/副本保留期、最小保留数和不可复用销毁墓碑。RC 预检强制本地至少 7 天、副本至少 30 天、至少 3 份。
- 解除日期：2026-08-02
- 解除证据：隔离 `make test-docker-deployment` 覆盖真实 PostgreSQL schema、密文篡改拒绝、错误 identity 拒绝、深度校验、异地取回、恢复前安全备份、恢复、保留清理、销毁墓碑和同名复用拒绝；ShellCheck、秘密扫描和宿主/1Panel 基线复核通过。
- 剩余外部门禁：正式离线 age identity 生成/托管、真实异地主机或对象存储挂载和生产恢复演练属于 `BLK-007`，不重新打开产品实现缺陷。

---

## KI-016 Windows 10 与 NetAdapterCx Ethernet 官方支持矩阵冲突

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：任务书要求 Windows 10/11 Agent 和基于 NetAdapterCx 的自研虚拟网卡，但微软官方版本表将 Windows 10 2004 的 NetAdapterCx 2.0 标为仅支持 MBBCx；因此当前不能诚实声明 Windows 10 Ethernet 虚拟 NIC 受支持。
- 证据：微软 `NetAdapterCx version overview` 将 Windows 11 24H2 列为 UMDF 2.33 / NetAdapterCx 2.5 Ethernet 支持平台，将 Windows 10 2004 的 2.0 列为 MBBCx only；`QA_MATRIX.md` 将 Windows 11 LTSC 2024 作为驱动强制平台，Windows 10 为条件允许时兼容性。
- 临时缓解：M6.1 首版只面向 Windows 11 24H2 UMDF NetAdapterCx，不向 Windows 10 分发、不伪造构建或兼容结果；共享 ABI 与用户态 Agent 保持可复用。
- 计划：完成 Windows 11 VM 门禁后，在独立 Windows 10 VM 评估最小 clean-room NDIS miniport 路径或由项目所有者正式调整 Windows 10 支持范围；任何新路径必须复用同一安全 ABI 并重新执行驱动审计。
- 解除条件：Windows 10 获得受支持的实现路径并完成 WDK 构建、测试签名、安装卸载、收发、异常生命周期和 Driver Verifier，或正式任务书明确移除 Windows 10 强制范围。

---

## KI-017 Windows 当前安装编排仅限 WDK 测试 VM

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：`installers/windows` 可在快照 VM 验证测试签名驱动生命周期，但依赖微软禁止重分发和生产使用的本机 WDK DevGen，不能作为最终用户安装器或 RC 分发物。
- 临时缓解：脚本名称、README、状态和参数均标记 test-only；必须显式提供测试 signer thumbprint 和 Microsoft-signed DevGen 路径，不下载、不打包、不修改 BCD，不允许既有 xsnet 状态。
- 已完成缓解：安装失败和卸载只操作精确记录的 `Root\XSNET` instance 与 `oem#.inf`，20 秒有界等待，设备或 driver-store 残留返回失败并保留状态。测试包构建固定本机 Microsoft-signed WDK 工具、测试 signer、精确 allowlist、INF `DriverVer`、ABI `1..1` 和哈希；安装前后核对期望/INF/driver-store 版本，受限状态记录 exact ABI v1。VM 编排已真实执行完整阶段、双重重启、Verifier oneboot、系统基线和最终证据哈希，clean uninstall 后设备、包、状态和 DriverStore 残留均为 0；见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。
- 计划：为正式签名包实现受支持的软件设备创建、升级、回滚和企业部署路径，并单独执行安装器威胁建模。
- 解除条件：不依赖不可分发测试工具的正式安装器完成签名、干净安装、重复安装、升级、失败回滚、卸载、重启和零残留验收。

---

## KI-018 Windows Agent xsnet Win32 transport 尚未完成整包和实机验证

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：该条目只跟踪自研 `xsnet` transport 路径。隔离 Win32 transport、本地命名管道、私有存储和 Service crate 已实现并通过各自最小 Windows target 编译，但完整 Agent 尚未使用 `xsnet` 链接运行。专用 SYSTEM VM harness 已真实完成设备枚举、独占 handle、七个 `DeviceIoControl`、Tx/Rx、reopen、LUID 和 PnP restart；它没有执行 Rust Named Pipe、存储 ACL/原子替换或 SCM。首版 Wintun Agent 是独立发布路径，不能反向证明本条 `xsnet` transport 已完成。
- 临时缓解：`XsnetTransport` 只暴露 Success/Rejected/Indeterminate 三类结果；平台 crate 不把任何 Win32 失败映射为 Rejected，所有未知结果、异常字节数、direct 输入变异和畸形成功响应强制重连。`XsnetDeviceSession` 每个方法只执行一个请求，不轮询、不后台重试、不在 Drop 中 I/O，且不接入 runtime。Agent 保持全局禁止 unsafe。
- 已完成缓解：18 个 Agent xsnet 测试和 3 个 transport crate 测试覆盖 ABI/IOCTL、状态、单步会话、配置先于设备打开校验、协商 buffer 上限、拒绝/毒化、失败启动释放、无效 RX/Drop 零 I/O、显式 shutdown 重试、LinkDown/Detach、buffer mapping、长度、接口列表、identity schema/LUID 与非 Windows 拒绝；源码门禁在 VM 验证前禁止 runtime 接入、后台线程、sleep 和 session Drop I/O，六个 transport unsafe 块保持隔离。Windows 本地 IPC 固定名称、first-instance、远程拒绝、LocalSystem/Administrators DACL、不可继承 handle 和 16+1 容量边界；私有存储固定 exact protected DACL、reparse/父目录检查和 write-through 替换；Service 固定名称、四阶段状态、STOP/SHUTDOWN 一次性通知、状态竞争锁和四个隔离 unsafe 块，且不包含安装 API。Linux 回归、三个最小 Windows crate check 和交叉 Clippy已通过；identity 扩展后的最新完整证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T173643Z`。
- 计划：`xsnet` 保持测试签名实验路径；只有在未来获得正式签名与同类受控 VM 时，才编译链接完整 Agent 并执行 SCM、命名管道/存储 DACL、原子替换/崩溃恢复、空 TX/满 RX 精确状态和 Agent crash/sleep 生命周期。当前首版发布使用 ADR-077 的 Wintun adapter/session，不把 Wintun 证据冒充 `xsnet` 证据。
- 解除条件：完整 Windows Agent 与驱动通过编译、unsafe 复核、设备枚举、六 IOCTL、取消/移除、Agent crash 和睡眠恢复测试。

---

## KI-019 源码 SBOM 尚未覆盖容器操作系统包和许可证全文（已解除）

- 严重度：中
- 状态：已解除
- 首次发现：2026-07-31
- 影响（历史）：源码 SBOM 不能单独证明运行镜像中的 Debian、Alpine、NGINX 和系统包。
- 临时缓解（历史）：manifest 明确区分源码依赖与运行镜像，避免过度声明。
- 已完成缓解：`/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final` 验证 321 个 Cargo、110 个 npm、许可证策略、禁用依赖、确定性双格式输出和宿主基线不变。
- 已完成：从 exact image rootfs 生成 dpkg/apk OS 包 CycloneDX、113/113 包逐包许可证全文闭包、Dockerfile hash 和 in-toto/SLSA provenance；四镜像干净构建、双生成一致性、revision mismatch 拒绝和宿主保护验证均通过。
- 解除证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z` 绑定精确 revision、四个 image ID、113 个 OS 包、逐包许可证材料和 provenance；Grype 扫描与 disposition 另有同目录证据。
- 解除日期：2026-08-01。
- 解除条件（已满足）：所有最终镜像 digest、OS 包、许可证文本、漏洞处置和来源证明可从固定提交重建并由 Release Checklist 验证。

---

## KI-020 Windows 路由管理尚未完成实机编译与运行验收

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：IP Helper FFI、地址/DAD、精确路由、事务补偿、manifest 和恢复源码已实现并通过 MSVC target check/Clippy，且首版 Wintun Agent runtime 已调用该准备层；但仍没有真实 Windows 在线地址/路由、DAD、崩溃恢复、PnP 或睡眠证据，因此不得宣称 Windows 网络生命周期已通过。
- 临时缓解：默认路由、保留网段、外部重叠、所有权漂移和无界路由表在任何系统写入前失败关闭，原生路由表由 RAII 无条件释放，所有写入只接受精确 route/address key；Windows 子网路由继续失败关闭。
- 计划：在已验证的 Windows 11 VM 运行当前 Wintun Agent，验证表释放、精确错误映射、地址/DAD、路由补偿、manifest 崩溃恢复、Agent crash 和睡眠；`xsnet` 路径继续单独跟踪。
- 解除条件：完整 Agent 在 Windows VM 中证明地址/DAD、路由添加更新删除、冲突拒绝、崩溃恢复、睡眠/PnP 和卸载零残留。
## KI-021 运行时基础镜像仍有无当前修复版本的漏洞发现

- 状态：OPEN
- 证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z/vulnerabilities/summary.json`。
- 当前结果：Critical 2、High 4；`total_fixable_findings` 为空。Controller/Relay 各 Critical 1、High 2，Console 与 db-tools 无 Critical/High。
- 已完成缓解：Controller/Relay 删除 curl 并切换固定 digest distroless；Console/db-tools 升级 Alpine 包；Console 删除 curl 和未使用的 `nginx-module-image-filter`/TIFF 包链。db-tools 的 age 由固定上游 `v1.3.1` 提交以固定 `x/crypto v0.52.0` 重建，替代仍内嵌有漏洞 Go 依赖的发行包。相对最初扫描 Critical 从 44 降至 2、High 从 115 降至 4，当前有明确修复版本的发现为 0。
- 有界 disposition：剩余项仅为 glibc `CVE-2026-5435`、`CVE-2026-5450`、`CVE-2026-5928`。源码和精确镜像二进制均不引用对应 `ns_printrrf`/`ns_sprintrr`/`fp_nquery`、`scanf` family 或 `ungetwc` API；`make verify-image-vulnerability-disposition` 会拒绝新增 Critical/High、可修复版本、包/API 漂移。该结论只降低当前调用面的可达性，不等于修复或永久豁免。
- 复核期限：2026-08-31，或基础镜像 digest、glibc 版本、扫描数据库/结果、二进制导入集合任一变化时立即重扫和重做 disposition；RC 前必须再次复核。
- 解除条件：基础镜像提供修复并升级重扫为 0，或由项目所有者在期限内正式接受剩余风险；扫描报告不得隐藏或自动忽略。

---

## KI-022 Windows 一键安装的最终在线闭环尚未完成

- 严重度：高
- 状态：开放
- 首次发现：2026-08-04
- 影响：Windows 11 x64 已完成 Wintun 适配器的真实创建/清理、交叉测试、离线发布包构建和安装器完整性校验；生产 Controller 回环及 `vpn.qinwen.co` 公网下载也已通过。仍需在受控 Windows VM 使用一次性 Enrollment Token 验证注册、Windows 服务启动、CLI readiness、基本连通性和卸载/重新安装。当前不能把离线 package QA 或公开下载写成最终在线接入结果。
- 已完成缓解：引导器固定 `https://vpn.qinwen.co`，固定 manifest 摘要，校验 HTTPS、归档大小/哈希、精确 payload tree、每个 payload hash、Wintun DLL hash 与有效的 `CN=WireGuard LLC` Authenticode 签名；失败时删除仅由本次安装创建的服务和目录。Controller 只暴露精确的 `install.ps1`、manifest 和 ZIP 文件名，目录或未知文件拒绝。
- 计划：公开发布目录、PowerShell User-Agent、`/install/windows`、Linux `/install`、下载哈希和 404 边界已完成；下一步仅在受控 Windows 11 VM 生成短时 Enrollment Token，实际运行管理员 PowerShell 安装、服务和网络验证，并保留失败关闭、普通网络、卸载和重装证据。
- 解除条件：上述生产下载/注册/服务/基本数据面/卸载重装实测通过，且没有把 Token、密码、私钥、产物或环境文件写入 Git、日志或文档。Windows 10、完整 `xsnet` 实机 Agent、睡眠/路由恢复和独立安全审计仍由既有条目单独跟踪。

### 2026-08-08 复核补充

- 生产 Controller 的 `vpn.qinwen.co` 健康、Windows/Linux 引导、Windows manifest/ZIP 和 Linux 双架构签名文件已逐字节复核；该下载门禁已完成。
- `KI-022` 仍保持开放，因为没有当前可控 Windows VM 会话执行一次性 Token Enrollment、SCM、CLI readiness、真实地址/路由、数据面、卸载和重装。不得用离线 Wintun smoke 或公开下载结果替代在线闭环。

---

## 2026-08-09 生产审计补充

- `KI-019` 在 Gate 01 revision 的源 SBOM 为 Cargo 325、npm 110、总计 435；Gate 23 升级 SQLx `0.9.0` 并删除 `rsa` 拓扑后，精确 revision `3bf8619` 的预期计数为 Cargo 296、npm 110、总计 406。许可证、哈希和计数漂移继续 fail-closed，完整 CI 与 clean-checkout baseline 通过。
- `KI-021` 仍开放：修复分支五镜像逐字节可复现且 `3d93656` 已部署，但生产运行的 glibc Critical/High 仍只有限时、精确 API 可达性 disposition，未被上游修复，也未获得正式风险接受。
- 新增生产级残余问题统一由 `audit/production-readiness/OPEN_FINDINGS.md` 和 `BLK-008` 跟踪；修复分支 CI PASS 不表示 Release Candidate。

---

## KI-023 生产 bootstrap 与全量凭据轮换尚未闭合

- 严重度：严重
- 状态：开放
- 首次发现：2026-08-08 正式生产审计；2026-08-09 Gate 16 后重新定界。
- 影响：生产 Controller 已不再使用 PostgreSQL bootstrap 超级用户，运行/迁移/所有者角色分离和负向权限通过；但 bootstrap 本身及 SSH、Console、Controller session/enrollment/node、历史 Redis/MySQL、TLS、备份/更新/恢复和 CI 类凭据尚无完整“新值激活 + 旧值拒绝”证据。Gate 02 继续 `FAIL`。
- 已完成缓解：runtime 与 migrator 使用新建独立角色和仓库外 Secret；bootstrap 不在长驻 Controller secret 目录；临时明文 staging 已删除；证据中不记录新值。
- 计划：按 `audit/production-readiness-remediation-v2/CREDENTIAL_ROTATION.md` 逐项轮换，使用不回显渠道激活新值，从独立会话验证旧值拒绝，并扫描当前树、Git 历史、CI、镜像层、日志、截图和 QA 证据。
- 解除条件：所有非外部条目 `rotated=yes` 且 `old rejected=yes`（新建、无旧值的身份允许有原始创建证据）；外部所有者条目完成后 Gate 02 才可 `PASS`。

---

## KI-024 生产主机外部磁盘告警未闭合

- 严重度：高
- 状态：外部阻塞（内部项已完成）
- 首次发现：2026-08-09 Gate 14 生产基线。
- 已完成：仅密钥 SSH、root/password/旧钥拒绝、最小 INPUT 默认拒绝、1Panel TCP `188` 公网关闭与受限 tunnel、路由/IP rule/nftables/`1panel-network` 不变量和回滚安全均已通过。全部 157 个升级和 9 个依赖已安装，新内核 `6.8.0-137-generic` 通过 one-shot/fallback/watchdog 和内外部回归；根分区从 `83%` 降至 `77%`，约 `14.15 GB` 可用。证据为 Gate 14 防火墙根和 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate14-maintenance-20260809T081438Z`。
- 影响：主机仍没有独立于被监控服务器的 warning/critical 磁盘通知目标、真实送达回执或 on-call acknowledgement。主机本地日志或静态状态不能证明通知路径可用，因此 Gate 14/20 不能因补丁和空间恢复而判定 `PASS`。
- 计划：由所有者/SRE 提供并批准外部通知 provider、destination、凭据和 on-call；配置 warning/critical 阈值，分别触发真实告警，确认外部接收/确认，再清除条件并证明恢复通知。不得把模拟、本机 journal 或未送达事件计作通过。
- 解除条件：warning 与 critical 事件均由独立目标真实收到并被记录的 on-call 确认，故障解除后 recovery/closure 也真实送达；证据无秘密且加入审计索引。完成前 Gate 14 保持 `PARTIAL`。

---

## KI-025 计划域名严格 TLS 与 Console 公网路径未闭合

- 严重度：高
- 状态：外部阻塞（仓库侧准备已完成）
- 首次发现：2026-08-08 CDN `525`；2026-08-09 Gate 13 独立定界。
- 影响：`vpn.xiashikeji.cn` 的边缘证书可验证，但根、health、install 与 WebSocket 路径均返回 `525`；直连源站使用计划域名 SNI 时 TLS 握手失败。计划域名无法提供严格 TLS Console/API/WebSocket 路径，Gate 13/19 不能通过，项目不能正式生产上线。
- 根因：源站无计划域名证书和 1Panel/OpenResty vhost；现有站点证书只覆盖 `qinwen.co`，且现有站点根路径代理 Controller 而非 Console。
- 已完成缓解：提交 `94ccae3` 增加失败关闭的严格 TLS/SNI/HTTP/WebSocket 审计器及负向测试，并提供只含占位符、TLS 1.2/1.3、HTTP 308、Console loopback 和 WebSocket 透传的 OpenResty 模板。证据已秘密扫描和 SHA-256 封存；未修改生产配置。
- 计划：所有者按 `audit/production-readiness-remediation-v2/CDN_TLS.md` 批准并执行证书、计划域名 vhost、CDN Origin Host/SNI 和 strict 验证变更，以完整基线和自动/手工回滚保护；随后从独立外部客户端和真实浏览器复验。
- 解除条件：源站和 CDN 严格证书/主机名验证、根/health/install、登录、认证 API、WebSocket、Console E2E 和未知路由全部通过；无关 1Panel 站点、容器、路由、防火墙与 `1panel-network` 保持不变。

---

## KI-026 transitive `paste` 未维护公告需要限时复核

- 严重度：低（信息类/P2；不是已知漏洞）
- 状态：已解除（2026-08-10，有界 disposition 有效至 `2026-08-31`）
- 首次发现：2026-08-09 Gate 23 plain `cargo audit`。
- 影响：`paste 1.0.15` 通过 `rtnetlink` 依赖拓扑进入锁文件；`RUSTSEC-2024-0436` 表示上游已停止维护，没有 CVSS、漏洞利用结论或 patched version。当前 `cargo audit` 因零 vulnerability 返回成功，但仍输出该 informational warning。
- 已完成处置：plain `cargo audit` 原始 JSON 证明 vulnerability 为 0 且 `settings.ignore=[]`；`cargo-deny` 仅保留带原因和 `2026-08-31` 截止日期的显式 exception。精确锁定路径为 `xs-agent -> rtnetlink 0.21.0 -> netlink-packet-core 0.8.2 -> paste 1.0.15`。
- 上游复核：`rtnetlink` `main` 精确提交 `e7799b6ee24267586e6aadc0e3fb415b4d921dd4` 仍为 `0.21.0`；`netlink-packet-core` `main` 精确提交 `571d8bb5fa1dbaa875e8aede3f214c87f70b955b` 仍声明 `paste = "1"`。当前没有可直接升级且移除该依赖的上游版本；仅为消除信息告警而 fork/vendor 网络栈会引入更大的长期维护面，因此不采用。
- 解除证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z`；专项报告 `audit/production-readiness-remediation-v2/DEPENDENCY_TOPOLOGY.md`。
- 自动重开：到达 `2026-08-31`，或 `Cargo.lock`、`rtnetlink` 路径/feature、上游 manifest、公告分类、Rust/SQLx、正式 RC revision 任一变化时立即重新审计。不得静默 ignore。
- 边界：该 P2 处置不关闭 `KI-021`、Gate 24、正式签名发布、生产部署或第三方审计。

---

## KI-027 Linux Agent 普通主机恢复矩阵尚未闭合

- 严重度：高
- 状态：外部阻塞（可安全自动化子矩阵已完成）
- 首次发现：2026-08-08 正式生产审计；2026-08-10 Gate 06 重新验证。
- 已完成：exact revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02` 在 GitHub Actions run `31352258781` 通过真实 transient systemd `SIGKILL` 单次自动重启、状态保留、私有 namespace TUN 重建、隔离链路 down/up 存活与最终 cleanup。artifact `9049384061` 的归档和内部 SHA-256、无值秘密扫描通过。
- 影响：hosted x86_64 runner 不能证明普通部署主机整机 reboot、disk-full、DHCP/address/default-route churn、竞争 VPN 路由、重复失败/start-limit 或 arm64 硬件行为。生产服务器当前没有宿主 Agent，且不得在该服务器执行破坏性故障注入，因此 Gate 06 仍为 `PARTIAL`。
- 计划：提供可重装、带控制台和独立证据导出的普通 Linux x86_64/arm64 测试主机；按 `audit/production-readiness-remediation-v2/LINUX_AGENT_RECOVERY.md` 执行完整矩阵，保留故障前后普通网络、路由、TUN、状态、服务、日志和清理证据。
- 解除条件：全部剩余真实主机场景通过，失败注入后普通网络与非项目路由不受损，Agent 可恢复且卸载无残留；证据经 SHA-256 和无值秘密扫描加入索引。NAS 仍由独立 Gate 12 验收。
