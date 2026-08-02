# KNOWN_ISSUES.md — 已知问题和风险

---

## KI-001 自研协议未完成独立安全审计

- 严重度：高
- 状态：开放
- 说明：内部测试不能替代第三方协议和密码学审计。
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
- 状态：开放
- 说明：Linux 服务器不能证明 Windows 驱动实机稳定。
- 解除：Windows 11 测试 VM、快照、WDK、Driver Verifier。

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

## KI-006 现有 PostgreSQL 和 Redis 端口可从公网到达

- 严重度：高
- 状态：开放
- 首次发现：2026-07-29
- 影响：违反数据库不得暴露公网的最终验收要求，增加凭据猜测和服务漏洞攻击面。
- 复现：从开发服务器外部对 `34.92.139.129` 的 TCP `5432`、`6379` 建立连接成功。
- 证据：`/srv/xs-nexus-qa/baseline/20260729T094000Z/external-port-check.txt`
- 临时缓解：项目不使用公网端口连接数据库，不扩大暴露面，不记录凭据。
- 根因：现有 1Panel 容器在项目开始前已经发布到所有主机接口。
- 计划：由用户批准后在 1Panel 或云防火墙收紧访问，并轮换临时密码。
- 解除条件：外部探测不可达，容器内 `1panel-network` 连接仍通过，且 1Panel 服务正常。

---

## KI-007 开发服务器等待系统重启

- 严重度：中
- 状态：开放
- 首次发现：2026-07-29
- 影响：已安装内核或系统更新尚未通过重启生效。
- 临时缓解：M0.1 不重启，避免中断 SSH 和现有 1Panel 服务。
- 计划：在用户确认维护窗口并完成 1Panel 基线备份后重启，再复跑环境和网络验证。
- 解除条件：重启后内核、Docker、1Panel、SSH、路由和防火墙验证通过。

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
- 计划：在 M8.1/M8.2 增加 Relay 吞吐、RTT 增量、丢包、拥塞和长时间稳定性测试，并接入延迟/丢包指标。
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
- 已完成缓解：安装失败和卸载只操作精确记录的 `Root\XSNET` instance 与 `oem#.inf`，20 秒有界等待，设备或 driver-store 残留返回失败并保留状态。测试包构建固定本机 Microsoft-signed WDK 工具、测试 signer、精确 allowlist、INF `DriverVer`、ABI `1..1` 和哈希；安装前后核对期望/INF/driver-store 版本，受限状态记录 exact ABI v1。VM 编排固定不可复用阶段、双重重启、Verifier oneboot、系统基线和最终证据哈希，但均尚未在 VM 执行。
- 计划：Windows VM 驱动验证通过后，为正式签名包实现受支持的软件设备创建、升级、回滚和企业部署路径，并单独执行安装器威胁建模。
- 解除条件：不依赖不可分发测试工具的正式安装器完成签名、干净安装、重复安装、升级、失败回滚、卸载、重启和零残留验收。

---

## KI-018 Windows Agent Win32 transport 尚未完成整包和实机验证

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：隔离 Win32 transport、本地命名管道、私有存储和 Service crate 已实现并通过各自最小 Windows target 编译，但完整 Agent 因缺少 Windows SDK 工具链尚未链接，且没有在 Windows 调用命名管道、存储 ACL/原子替换、SCM、设备枚举、独占 handle 或 `DeviceIoControl`；驱动的空 TX/满 RX 目前以失败状态完成，而通用 Win32 错误尚不能证明 request 未提交，因此仍不能安全启用持续收发。
- 临时缓解：`XsnetTransport` 只暴露 Success/Rejected/Indeterminate 三类结果；平台 crate 不把任何 Win32 失败映射为 Rejected，所有未知结果、异常字节数、direct 输入变异和畸形成功响应强制重连。`XsnetDeviceSession` 每个方法只执行一个请求，不轮询、不后台重试、不在 Drop 中 I/O，且不接入 runtime。Agent 保持全局禁止 unsafe。
- 已完成缓解：18 个 Agent xsnet 测试和 3 个 transport crate 测试覆盖 ABI/IOCTL、状态、单步会话、配置先于设备打开校验、协商 buffer 上限、拒绝/毒化、失败启动释放、无效 RX/Drop 零 I/O、显式 shutdown 重试、LinkDown/Detach、buffer mapping、长度、接口列表、identity schema/LUID 与非 Windows 拒绝；源码门禁在 VM 验证前禁止 runtime 接入、后台线程、sleep 和 session Drop I/O，六个 transport unsafe 块保持隔离。Windows 本地 IPC 固定名称、first-instance、远程拒绝、LocalSystem/Administrators DACL、不可继承 handle 和 16+1 容量边界；私有存储固定 exact protected DACL、reparse/父目录检查和 write-through 替换；Service 固定名称、四阶段状态、STOP/SHUTDOWN 一次性通知、状态竞争锁和四个隔离 unsafe 块，且不包含安装 API。Linux 回归、三个最小 Windows crate check 和交叉 Clippy已通过；identity 扩展后的最新完整证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T173643Z`。
- 计划：继续实现 Windows 路由管理边界；在具备 Windows Rust 标准库、SDK 和 WDK 的受控环境编译链接完整 Agent/驱动，随后进入 `BLK-001` VM SCM 启停、命名管道与存储 DACL/拒绝矩阵、原子替换/崩溃恢复、设备枚举、六 IOCTL、空 TX/满 RX 精确状态、取消与生命周期验收；只有获得权威 no-commit 证据或新增明确唤醒协议后才接入 runtime。
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
- 解除证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z` 绑定精确 revision、四个 image ID、113 个 OS 包、逐包许可证材料和 provenance；Grype 扫描与 disposition 另有同目录证据。
- 解除日期：2026-08-01。
- 解除条件（已满足）：所有最终镜像 digest、OS 包、许可证文本、漏洞处置和来源证明可从固定提交重建并由 Release Checklist 验证。

---

## KI-020 Windows 路由管理尚未接入 IP Helper 和 DAD

- 严重度：高
- 状态：开放
- 首次发现：2026-07-31
- 影响：当前只有平台无关的精确所有权与 additions-first 事务模型，不能读取、创建或删除 Windows 地址/路由，也不能证明虚拟地址完成 DAD；Agent runtime 不得据此宣称 Windows 网络已可用。
- 临时缓解：新 crate 不接入 runtime；unsafe 只存在于 Windows 平台模块。默认路由、保留网段、外部重叠、所有权漂移和无界路由表在计划阶段失败关闭，原生路由表由 RAII 无条件释放，所有写入只接受精确 route/address key。
- 计划：当前 IP Helper FFI、表释放、精确路由、非持久地址、DAD 有界等待、联合失败补偿、严格 manifest、受保护原子持久化、精确恢复执行及 Agent Windows-only 生命周期准备已完成源码门禁；LUID 现由同一独占 xsnet handle 的版本化 identity query 从 `NetAdapterGetNetLuid` 获取并严格校验，不使用名称或全局枚举。下一步在获得 WDK/VM 后真实编译链接和执行，不提前接入 runtime。
- 解除条件：完整 Agent 在 Windows VM 中证明地址/DAD、路由添加更新删除、冲突拒绝、崩溃恢复、睡眠/PnP 和卸载零残留。
## KI-021 运行时基础镜像仍有无当前修复版本的漏洞发现

- 状态：OPEN
- 证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z/vulnerabilities/summary.json`。
- 当前结果：Critical 2、High 4；`total_fixable_findings` 为空。Controller/Relay 各 Critical 1、High 2，Console 与 db-tools 无 Critical/High。
- 已完成缓解：Controller/Relay 删除 curl 并切换固定 digest distroless；Console/db-tools 升级 Alpine 包；Console 删除 curl 和未使用的 `nginx-module-image-filter`/TIFF 包链。相对最初扫描 Critical 从 44 降至 2、High 从 115 降至 4，当前有明确修复版本的发现为 0。
- 有界 disposition：剩余项仅为 glibc `CVE-2026-5435`、`CVE-2026-5450`、`CVE-2026-5928`。源码和精确镜像二进制均不引用对应 `ns_printrrf`/`ns_sprintrr`/`fp_nquery`、`scanf` family 或 `ungetwc` API；`make verify-image-vulnerability-disposition` 会拒绝新增 Critical/High、可修复版本、包/API 漂移。该结论只降低当前调用面的可达性，不等于修复或永久豁免。
- 复核期限：2026-08-31，或基础镜像 digest、glibc 版本、扫描数据库/结果、二进制导入集合任一变化时立即重扫和重做 disposition；RC 前必须再次复核。
- 解除条件：基础镜像提供修复并升级重扫为 0，或由项目所有者在期限内正式接受剩余风险；扫描报告不得隐藏或自动忽略。
