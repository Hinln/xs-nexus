# PROGRESS.md — 当前项目状态

最后更新时间：2026-08-02 10:15 UTC
当前 Git 提交：CLI 与文档总审计工作树（以本文件后续提交为准）
当前总状态：`BLOCKED_EXTERNAL`
当前里程碑：`所有当前环境可完成工作已闭环；等待外部门禁`

---

## 当前结论

- Linux、Controller、Relay、Console、XSP/1、NAT/Relay、ACL/子网、安装更新、1Panel 隔离部署、认证遥测、加密备份/复制、镜像供应链、三轮回归、性能与 24 小时稳定性均有正式证据；
- 任务书要求的 `xs status`、`xs peers`、`xs ping <virtual-ip>`、`xs path <virtual-ip>`、`xs routes`、`xs netcheck`、`xs diagnostics`、`xs reconnect`、`xs version` 已全部实现并在真实双 Agent namespace 中验证，证据 `/srv/xs-nexus/artifacts/qa/m1.2-cli-completion-20260802T100106Z`；
- Windows 当前环境可完成的 ABI、驱动/Agent/IPC/存储/Service、IP Helper/DAD/路由源码与交叉门禁已完成；完整 Windows runtime 仍禁用，真实链接、WDK、VM、签名、PnP/power 和 Verifier 由 `BLK-001`/`BLK-004` 阻塞；
- 仅剩 `BLOCKERS.md` 和开放 `KNOWN_ISSUES.md` 所列外部门禁；项目仍是部分完成，不是 Release Candidate，也不适合公网生产。

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
- 有界 Unix IPC、九个任务书 CLI 命令和 JSON 输出；读取命令不返回秘密，`xs reconnect` 只发送有冷却、可确认的控制面重连请求；
- 最小权限 systemd 单元和 `PrivateNetwork=yes` transient service 生命周期验证；
- M1.2 初始证据 `/srv/xs-nexus/artifacts/qa/m1.2-20260729T130606Z`，CLI 完整补充证据 `/srv/xs-nexus/artifacts/qa/m1.2-cli-completion-20260802T100106Z`；
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
- 严格默认拒绝 ACL，支持节点、组、标签、Allow/Deny、稳定优先级、TCP、UDP、ICMP、目标端口范围和 Explain；
- Agent 在 XSP/1 加密前和认证解密后双端执行 ACL，将会话 Peer 身份绑定到内层虚拟源地址；
- 签名策略采用严格单调更新，低版本、同版本异内容、无签名和无效 ACL 均保留最近有效状态；
- Controller 使用 PostgreSQL 持久化组、成员和 ACL，策略原子替换并以期望版本防止并发覆盖；
- 活动虚拟地址唯一、节点吊销、地址冷却复用、地址池重叠拒绝和 Agent 系统路由冲突保护完成；
- 两个隔离 namespace 的 ICMP/TCP/UDP 允许、发送端拒绝、接收端拒绝和拒绝负载不可见验证完成；
- M3.1 全量证据 `/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`。
- Agent 通过 Netlink 发现本地直连私网并只发布节点签名建议，Controller 持久化建议和管理员审批状态；
- 子网审批支持启用、暂停、撤销、乐观配置版本、纯路由/NAT、优先级、冲突拒绝和审计，只有 enabled 路由进入签名配置；
- Agent 客户端路由、网关 forwarding、项目独占 nftables NAT 表和 manifest 回滚均由原生 Netlink/netfilter 接口实现；
- XSP/1 新增显式策略守卫的 routed API，普通会话继续严格绑定虚拟地址，路由流量在加密前和解密后双端执行 Peer/子网/ACL 绑定；
- 三个隔离 namespace 已验证审批前不可达、纯路由/NAT ICMP/TCP、伪造源拒绝、网关离线撤销、暂停、shutdown 和崩溃恢复；
- M3.2 全量证据 `/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`。
- Controller 控制台用户、Argon2id 密码、登录限速、HttpOnly/SameSite 会话、CSRF 轮换、注销撤销和 administrator/operator/auditor 服务端授权；
- 管理快照接入真实网络、节点、Token、组、ACL、子网、Relay、拓扑、用户、审计、告警和系统能力；M4 当时尚未接入的路径/流量/延迟/健康指标显式“未采集”，后续状态见 2026-08-02 认证遥测闭环；
- 节点在线状态只由当前进程已认证活跃控制连接决定，连接断开会实时移除并记录数据库时间；
- React 控制台完成登录、首页、节点详情、网络、地址池、Token、组、路由审批、Relay、ACL、用户、审计、告警、更新、设置、备份和 404；
- 6 项 Playwright 主流程、6 个固定视口、135 张截图、空/错误/无权限/大量数据/长 IPv6/离线/部分服务异常和键盘焦点恢复均通过；
- M4.1/M4.2 全量证据 `/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`，截图 `/srv/xs-nexus/artifacts/visual/m4.1`。
- Linux Agent 新增严格 `cleanup --config` 生命周期命令，只使用已存在本地身份、签名状态和可信恢复清单清理项目网络资源，缺失或不匹配状态失败关闭；
- Linux 发布构建器完成 x86_64 与 aarch64 release 构建、ELF 架构检查、确定性归档、逐文件 SHA-256、外部严格清单和 Ed25519 分离签名；
- Linux 安装器完成首次公钥固定、平台/架构绑定、原子版本切换、防外部降级、失败自动回滚、显式历史版本回滚、身份保留、默认卸载保留和显式 purge；
- 生命周期测试覆盖干净/重复安装、外层和内层篡改、错误签名/公钥、升级、失败激活、回滚、清理失败、卸载重装和无服务残留；
- systemd 崩溃测试改为 SIGKILL 后调用发布路径 cleanup，验证恢复清单和主机接口无残留；
- M5.1 全量证据 `/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`，包含真实 x86_64/aarch64 构建和宿主/1Panel 前后基线。
- Controller、Relay、Console 和 PostgreSQL 运维镜像完成多阶段构建；运行时非 root、只读根文件系统、无 capability、启用 `no-new-privileges`、健康检查和日志轮转；
- Compose 只引用外部 `1panel-network`，不创建数据库服务或项目网络，不发布数据库端口；dev/RC 使用独立项目、schema、端口、Secret、备份和状态目录；
- Controller 支持严格 `_FILE` Secret 与独立迁移命令；部署在激活前备份和迁移，失败镜像自动回滚，RC 强制干净 Git 与固定 revision；
- PostgreSQL 18 运维工具完成备份、清单校验、篡改拒绝、精确 schema 恢复、恢复前安全备份和失败回滚；
- M5.2 生命周期测试覆盖实际构建、迁移、健康、持久化、备份恢复、迁移失败保护和错误镜像回滚；
- M5.2 全量证据 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`，最终无项目容器、网络、namespace、TUN、nftables、默认路由或 `1panel-network` 变化。
- 复核微软官方版本表后撤回“KMDF NetAdapterCx 覆盖 Windows 10/11”的错误判断；M6.1 首版改为面向正式门禁 Windows 11 LTSC 2024 的 UMDF 2.33 + NetAdapterCx 2.5，Windows 10 差距记录为 `KI-016`；
- 固定 `xsnet` 内核职责为虚拟 NIC、队列、受控 LocalSystem IPC 和生命周期，密码学、ACL、路由、NAT、Relay、身份和秘密全部留在用户态；
- 实现 ABI v1 固定小端消息头、规范包批次解析器和纯 C 单 owner 会话状态机，拒绝未知版本/类型/flag、非精确长度、重放/回滚/sequence 上限、错误调用顺序、MTU/队列超限、间隙、重叠、隐藏尾部和超限包；
- `make test-windows-xsnet-abi` 已在 Clang 21 Release `-Werror` 和 GCC 15 ASan/UBSan 两套配置实际通过 ABI、会话、数据面与固定种子压力测试；该结果不代表 WDK 构建或 Windows 实机通过。
- 已加入 Windows 11 24H2 x64 UMDF 2.33 / NetAdapterCx 2.5 WDK 工程、仅 LocalSystem INF、独立 host、安全 IOCTL、file object、PnP/power、adapter 与断链 packet queue 骨架；`make test-windows-xsnet-source` 实际通过，但没有 WDK 编译证据。
- 已实现 NetAdapterCx 有界 ring 复制、同步 direct-I/O 请求、双队列 SetLink 门禁、协商深度限制和停止/取消断链，并加入快照测试 VM 专用的严格安装/卸载脚本；Windows 源码和脚本均未执行真实 WDK/设备操作。
- 固定种子压力测试分别执行 30,000 轮任意消息、写入、批次、会话和队列操作，并对六类有效消息逐字节变异；2026-07-31 三轮严格回归证据为 `/srv/xs-nexus/artifacts/qa/m6.1-portable-stress-20260731T012940Z`。
- 生命周期 harness 穷举 cleanup、双队列 cancel、D0 exit、hardware release 和 I/O stop 的全部 720 种顺序，验证取消/完成单次归属、睡眠后重新认证、队列重启和重复 teardown 幂等；三轮证据为 `/srv/xs-nexus/artifacts/qa/m6.1-lifecycle-20260731T014028Z`。
- Rust Agent 侧 ABI 客户端完成固定 IOCTL/字节布局、单飞请求、成功后提交、已知拒绝重试、不确定结果强制重连及 TX/RX 规范 IPv4 批次校验；原证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-client-20260731T015229Z`。
- 安全 `XsnetTransport` 契约已接入隔离 `no_std + alloc` Win32 crate；精确 GUID 单接口、独占同步 handle、六个 ABI IOCTL、独立 identity IOCTL、三类数据 buffer 映射、初始化所有权和六个 unsafe 块通过实际 MSVC target check、交叉 Clippy 和源码门禁；初始 transport 证据为 `/srv/xs-nexus/artifacts/qa/m6.1-win32-transport-20260731T030644Z`，identity 扩展需以本轮 M6.1 证据为准。
- 新增无后台轮询/重试的安全 Rust `XsnetDeviceSession`：任何设备打开/I/O 前验证 MTU/深度并推导 TX 容量，严格执行 Hello/Attach/SetLink，每次仅执行一个 TX/RX 请求，权威拒绝不自动重试，不确定或畸形完成毒化 handle，shutdown 按 LinkDown/Detach 排序且 Drop 不执行 I/O；18 个 Agent xsnet 测试和 2 个 transport crate 测试通过，新增覆盖失败启动释放、无效 RX 零 I/O、Drop 零 I/O 和 shutdown 显式重试。空 TX/满 RX 的 Win32 权威状态尚未在 VM 证明，因此未接入 runtime。
- Windows 源码门禁现强制配置校验先于首个 IOCTL/设备打开，并在 WDK/VM 证据前拒绝 runtime 引用、后台线程、sleep 和 session Drop I/O；`scripts/validate-m61-agent-session.sh` 汇总 workspace Clippy/单测、真实 PostgreSQL Agent 控制面、C Release/ASan/UBSan、Windows 源码/安装器/VM/兼容/transport/本地 IPC/私有存储/Service、独立实现、SBOM、秘密、ShellCheck、npm audit 和宿主基线。
- Windows 本地管理 IPC 已从 Unix-only 实现拆为共享长度帧协议、Unix transport 和 Windows transport；Windows 端固定 `\\.\pipe\xs-nexus-agent`，使用 first-instance、防远程客户端、不可继承 handle、仅 LocalSystem/Administrators DACL、16 个活动处理器和额外 1 个监听实例。`xs-cli` 已增加安全 Named Pipe client，九个命令与服务器均通过 MSVC target check/交叉 Clippy；读取命令无秘密，`ping` 和 `reconnect` 只有受限运行时效果。三个 unsafe 块仅存在于 `crates/windows-local-ipc`，Agent/CLI 继续无 unsafe；完整边界见 `docs/WINDOWS_AGENT_LOCAL_IPC.md`。
- Agent 私有存储已从 Unix-only mode 实现拆为共享 identity/JSON/token 逻辑、Unix transport 和 Windows transport；Windows 端要求绝对路径、整条现有路径无 reparse、直接父目录和文件 exact protected DACL，读操作绑定长度与完整字节，写操作使用同目录 `create_new` 临时文件、精确 ACL、`sync_all` 和 write-through Replace/Move。全部 Windows FFI 隔离在 `crates/windows-private-storage`，最小 MSVC check、交叉 Clippy、workspace Clippy 和 42 个 Agent 单测通过，完整边界见 `docs/WINDOWS_AGENT_STORAGE.md`。
- Windows Service/SCM 边界已隔离到 `crates/windows-service`：Agent 只接受 Windows 平台模式下固定 `XsNexusAgent` 的 `service --config` 入口；SCM 状态严格按 START_PENDING/RUNNING/STOP_PENDING/STOPPED 推进，STOP/SHUTDOWN 通过一次性 Notify 和 watch channel 接入现有 shutdown 契约，状态锁阻止并发 STOP 被后续 RUNNING 覆盖。四个 unsafe 块仅承载 dispatcher、handler 注册和状态上报；runtime 不含安装/删除/重配 API，完整边界见 `docs/WINDOWS_AGENT_SERVICE.md`，全量证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`。
- 安装官方 rustup 目标后，首次全量验证让旧 transport check 与 rustup Clippy 混用不同 sysroot，失败证据保留在 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T035905Z`；transport 脚本现从同一工具目录固定 `cargo`、`rustc` 和 `cargo-clippy`，未切换或放宽测试，随后全量验证通过。
- transport 契约完整回归证据为 `/srv/xs-nexus/artifacts/qa/m6.1-transport-contract-20260731T015900Z`，覆盖 workspace 单测、真实 PostgreSQL Agent 控制面、Clippy、C Release/ASan/UBSan、安装器、秘密扫描和宿主残留复核。
- 新增只面向快照 VM 的测试包构建脚本，固定 Microsoft-signed MSBuild/InfVerif/Inf2Cat/SignTool、Release x64、`SignMode=Off`、先签 DLL 再生成 catalog 并签 catalog、`10_GE_X64`、SHA-256 test signing、精确三文件 allowlist 和哈希清单；不创建证书、不修改 BCD、信任或测试签名策略。
- 新增 Initialize/Install/EnableVerifier/CollectVerifier/DisableVerifier/Uninstall 六阶段 VM 编排，要求可识别 VM、相同快照声明、阶段不可覆盖、Verifier 前后人工重启、精确设备状态、卸载零残留和最终证据哈希；CollectVerifier 明确不包含场景结果或验收声明。
- Windows VM 工作流静态回归证据为 `/srv/xs-nexus/artifacts/qa/m6.1-vm-workflow-20260731T021432Z`；Windows PowerShell 5.1 parser 对两份新增脚本零语法错误，远端 ABI Release/ASan/UBSan、源码、安装器、VM 门禁和秘密扫描通过，宿主路由、规范化 nftables、完整 `1panel-network`、namespace 和 TUN 前后不变。
- Windows Agent、C 驱动头和安装状态现共同固定 exact ABI v1；Hello 的消息头、minimum 和 maximum 均为 v1，明确不把 payload version range 冒充跨 header-version 协商。
- 测试包清单记录 INF 四段 `DriverVer`、ABI `1..1` 与 IPv4 capability；安装器要求显式期望 `DriverVer`，在 staging 前后分别核对 INF 与 driver-store 版本，并把 driver version/ABI 写入受限状态；VM 后续阶段持续复核该状态。
- `docs/WINDOWS_XSNET_COMPATIBILITY.md` 明确测试安装器只支持 clean install/uninstall，替换包必须停止 Agent、关闭 handle、精确卸载并依赖 VM 快照恢复；没有实现或声称热升级、热降级或生产回滚。
- 兼容与回滚边界证据为 `/srv/xs-nexus/artifacts/qa/m6.1-compatibility-20260731T022619Z`，覆盖 workspace 单测、Agent 全目标 Clippy、兼容/安装/VM/源码门禁、C Release/ASan/UBSan、秘密扫描和宿主前后基线。

- Acceptance A 独立实现门禁：扫描全部运行源码并精确固定三个负向引用，新增引用失败关闭；
- 完整源码依赖清单：321 个 Cargo crate、110 个 npm 包，逐组件许可证、PURL 与 lock checksum/integrity；
- 确定性源码 SBOM：离线生成 CycloneDX 1.6、SPDX 2.3 和 SHA-256 manifest，双次输出逐字节一致；
- 供应链 CI 门禁、M0.2 clean-room/原创性验证、秘密扫描和 npm 高危漏洞审计均通过；证据 `/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。

## 2026-07-31 历史工作点

- Windows 路由管理准备已开始：隔离 `xs-windows-route-manager` 已包含事务核心和 IP Helper 平台层，固定 LUID/on-link/metric、4096 条系统表上限、外部重叠拒绝、manifest 精确所有权、additions-first、添加失败逆序补偿和删除失败精确恢复；原始错误与所有 rollback 失败均显式返回。原生层对 `GetIpForwardTable2` 分配使用 RAII 无条件 `FreeMibTable`，支持精确路由 Create/Delete、非持久地址 Create/Delete 和 DAD 状态查询，并要求 `SitePrefixLength` 与规范前缀一致后才承认项目所有权。联合事务仅在 DAD Preferred 后创建路由；Tentative 有界等待，Duplicate/Invalid/Deprecated/Unknown、查询失败和超时均失败关闭并精确清理地址。schema 1 严格 manifest 通过 `xs-windows-private-storage` 完成受保护读取、同目录 write-through 原子替换和 ACL/reparse 验证删除；恢复执行逆序尝试全部精确路由和地址并聚合失败。Agent 网络准备层已形成但仍由 runtime 门禁隔离。可信 LUID 由同一独占 xsnet handle 的 identity schema v1 查询取得，驱动直接调用 `NetAdapterGetNetLuid`；ABI v1 六类消息保持不变，错误长度/版本/reserved/零值全部拒绝。专项验证 `/tmp/xs-windows-routing-2.log` 通过 16 个路由模型单测、源码门禁、MSVC target check 和双平台 Clippy；尚无 WDK 编译、Windows 运行或 runtime 接入。
- 本轮完整 M6.1 聚合验证已通过，证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T173643Z`；验证同时确认 Docker/`1panel-network`、默认路由、nftables 和失败 systemd 服务前后无变化。该证据是 Linux/交叉编译与源码门禁，不是 Windows 实机结果。
- Agent 网络准备的公开 API 已收紧为 `recover_for_session`/`prepare_for_session`，只能从同一 `XsnetDeviceSession<Win32DeviceTransport>` 读取 authoritative LUID；接受裸 `u64` 的内部入口不再公开。routing/transport 专项测试、workspace Clippy、MSVC target check 和源码门禁通过，runtime 仍无引用。
- 运行时镜像供应链工具已实现：直接从 exact Docker rootfs 解析 dpkg/apk 安装数据库，绑定 image ID、revision label、Dockerfile hash、包 PURL、许可证材料与 provenance；包含映射/包字段/路径穿越/revision mismatch 负向门禁和四镜像双生成验证入口。工具单测及四个既有镜像试运行通过；当前提交镜像的正式证据将在干净检查点后生成。
- 当前提交四个镜像的供应链验证已通过，证据为 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T180211Z`：Controller 106 包/120 份材料、Relay 106/120、Console 71/6、db-tools 61/1，共 344 个 OS 包。四个 image ID、revision label、Dockerfile hash、CycloneDX 和 provenance 均绑定；双生成逐字节一致，错误 revision 被拒绝，容器/网络/`1panel-network`/默认路由/nftables 前后不变。漏洞报告和缺失全文仍未完成。
- 新增 digest-bound 漏洞扫描入口：固定 Grype v0.116.1 与官方 archive SHA-256，只接受仓库证据目录，重新核对四个本地 image IDs，使用隔离数据库输出逐镜像 JSON、工具/数据库哈希和 severity/fixable 汇总；扫描发现不会被自动忽略或当成通过豁免。正式扫描将在该脚本形成干净检查点后执行。
- Agent 新增 Windows-only 准备编排：启动前恢复 stale manifest，写 Preparing 后才执行地址/DAD/路由，成功后原子写 Active；shutdown 全部精确清理成功后才删除 manifest。独立源码门禁固定顺序、禁止 unsafe/线程/子进程、禁止 runtime 引用，并加入 `make test-windows-agent-routing` 与 M6.1 全量入口。完整 Windows Agent 仍因 SDK/`ring/lib.exe` 阻塞且模块未接入 runtime，不宣称可用。

- M5.2 已完成全部计划内实现与全量验证，部署、迁移、备份、恢复和回滚均有实际证据；
- 宿主既有 PostgreSQL/Redis 公网暴露仍由 `BLK-005` 阻塞，项目没有修改 1Panel 或生产防火墙；
- M6.1 当时已完成 ABI、会话、便携数据面、NetAdapterCx ring/direct-I/O 源码、测试安装生命周期、确定性压力、teardown 交错模型、Rust Agent ABI 客户端、隔离 Win32 transport、安全命名管道服务器、私有存储和 Service/SCM 边界、测试包构建、VM 分阶段采证和 exact ABI/clean-install 兼容边界；该段所列 Windows 路由管理准备已在后续完成，当前剩余项以本文顶部结论和 `BLOCKERS.md` 为准；
- 开发服务器已确认仅有 `clang-cl`、CMake 和 Ninja，没有 WDK、MSBuild、Windows SDK 或 VM；`BLK-001` 继续阻塞真实驱动构建、测试签名和实机验收；
- 真实 Windows VM、正式驱动签名和日常 Windows 电脑保持人工门禁，不伪造实机结果。
- Acceptance A 已有源码和文档证据；容器操作系统 SBOM、许可证全文和构建来源证明仍留在 Release Checklist，不提前宣称完整 RC 供应链。

## 2026-07-31 历史下一步（已由后续实现取代）

1. 为 Windows 路由事务核心增加隔离 IP Helper FFI：有界复制 `GetIpForwardTable2` 并无条件 `FreeMibTable`，精确 Create/Delete、地址创建与 DAD 状态读取；获得 Windows SDK 环境后编译链接完整 Agent；
2. 获得 Windows VM 后执行设备枚举、六 IOCTL、空 TX/满 RX 精确状态、取消、WDK、InfVerif、安装、Driver Verifier 和异常生命周期验收；
3. 只有权威 no-commit 映射或显式唤醒协议得到证据后，才把有界 Windows 包调度接入 Agent runtime；
4. 在 VM 中验证 exact ABI 拒绝、重复 clean install 和快照回滚，再设计生产升级事务；
5. 保持 M6.2 驱动签名人工门禁，不提前进入依赖核心完成的 M7。

## 2026-07-31 历史恢复命令（不再代表当前下一步）

```bash
git status --short --branch
make validate-m61-agent-session
make test-windows-xsnet-source
sed -n '1,260p' docs/WINDOWS_XSNET_TRANSPORT.md
sed -n '1,260p' docs/WINDOWS_XSNET_COMPATIBILITY.md
```

## 最近测试

- 时间：2026-07-31 03:40 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`make validate-m61-agent-session`；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T033841Z`；
- 覆盖：18 个 Agent xsnet 测试、2 个 transport crate 测试、配置先于设备打开/首个 IOCTL、失败启动释放、无效 RX/Drop 零 I/O、无自动重试的单步启动/TX/RX/失败/Detach、runtime/后台行为源码门禁、`no_std + alloc` Win32 crate、实际 MSVC target check、交叉 Clippy、五组 C Release/ASan/UBSan、真实 PostgreSQL Agent 控制面、源码/安装器/VM/兼容门禁、独立实现、SBOM、秘密、ShellCheck、npm audit，以及 Docker、`1panel-network`、默认路由、规范化 nftables、namespace/TUN 和失败服务前后比较。

## 当前失败

无未解决测试失败。

## M6.1 当前验证

- 命令：`make test-windows-xsnet-abi`；
- 结果：通过；
- 覆盖：Clang Release 严格告警、GCC ASan/UBSan、固定消息头、长度/版本/type/flag/sequence、单 owner、协商顺序、MTU/队列、link、批次和 cleanup 负向测试，以及有界队列的原子入队、背压、小输出、部分出队、环绕和 payload 清零；
- 压力覆盖：固定种子每域 30,000 轮任意输入或状态操作，所有失败会话操作逐字段保持原状态，失败队列操作保持元数据和完整 64 槽字节，六类有效消息逐字节变异；
- 生命周期覆盖：单一 wait lock 语义下穷举六类 teardown 的 720 种顺序，验证 cleanup、queue cancel、I/O stop、睡眠和移除的断链、单次请求结算与幂等恢复；不冒充 WDF/VM 实测；
- Agent 客户端覆盖：18 个 Rust 测试验证 ABI/IOCTL 固定向量、单飞、成功提交、拒绝状态保持、未知结果强制重连、规范批次、畸形响应失败关闭、transport 分类、配置先于设备打开校验、失败启动释放、无效 RX/Drop 零 I/O、shutdown 显式重试、非 Windows 拒绝，以及单步会话启动、协商 TX 容量、TX/RX、无自动重试和有序幂等关闭；
- 当前实现：有界 IPv4 队列、TX 空请求/RX framed 请求、同步 direct-I/O、协商深度、SetLink 门禁、NetAdapterCx TX/RX 系统缓冲区复制源码和 Agent 单步设备会话已完成；空 TX/满 RX 状态映射与 runtime 接入等待 WDK/VM 证据，Windows 部分尚未经过 WDK 编译或执行；
- 测试安装准备：已加入只面向快照 VM 的 PowerShell 构建、安装、卸载和六阶段采证脚本，固定 signer thumbprint、Microsoft-signed WDK 工具、精确状态回滚、双重人工重启和残留拒绝；尚未在 Windows 执行；
- 安装器验证：`make test-windows-xsnet-installer` 与 `make test-windows-xsnet-vm-scripts` 通过；先前模块/安装/卸载脚本和本轮两份新增脚本均由本地 Windows PowerShell parser 零语法错误解析；未运行 MSBuild、InfVerif、Inf2Cat、SignTool、设备安装、卸载或 Verifier 命令；
- 兼容验证：`make test-windows-xsnet-compatibility` 通过，静态强制 Rust/C/安装状态 ABI 都为 v1、INF 只有一个四段 `DriverVer`、构建清单固定 ABI/capability、安装前后核对 driver version 且测试安装器拒绝既有 xsnet；未执行真实版本替换或回滚；
- 不覆盖：WDK、NetAdapterCx、INF、签名、安装、Windows 收发、PnP/power、Driver Verifier 和蓝屏。

## 外部阻塞

- Windows 驱动真实测试需要 Windows 11 测试 VM、快照和 WDK；
- NAS 接入需要用户在 NAS 本地执行安装；
- 正式驱动签名需要外部签名流程；
- DNS 和生产防火墙变更需要人工批准；
- 现有 PostgreSQL、Redis 公网暴露整改需要用户批准修改 1Panel 或云防火墙，见 `BLK-005`。

## 当前风险

- 自研协议握手、AEAD 数据面、地址发现、认证路径迁移、隔离 NAT 矩阵和 Relay 已实现，但真实运营商网络、Relay 公网容量/延迟/丢包、长期 Fuzz 和独立第三方审计尚未完成；
- Windows transport 最小 crate 已通过 MSVC target 编译，单步设备会话已通过 Linux fake transport 测试，但完整 Agent/驱动尚未经过 Windows SDK/WDK 编译链接和 VM 执行，空 TX/满 RX 权威状态和真实设备尚未验证；
- Windows 当前只支持 exact ABI v1 和 clean-install 测试生命周期，跨 ABI、热升级和生产回滚均未实现；
- 源码依赖 SBOM 与四个运行镜像的 OS 包 SBOM、逐包许可证全文闭包及最终构建来源证明均已可复现生成；当前精确证据为 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`。剩余 glibc Critical/High 有界处置不是漏洞修复，仍须按 2026-08-31 到期日或镜像/扫描/API 导入变化提前复核；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器维护窗口重启已完成并由重启后的 24 小时稳定性、Docker 生命周期与宿主网络不变量复核，`KI-007` 已解除；
- 临时凭据后续必须轮换。

## 2026-07-31 M7.3 性能与稳定性基线

- 新增 `scripts/test-runtime-stability.sh`，接入 `make test-docker-deployment` 的可配置 `XS_STABILITY_DURATION_SECONDS`；采集 Controller、Relay、Console 完整进程树的 RSS、线程、FD、CPU ticks 和 Docker 日志大小，并对三个项目容器分别重启后验证健康恢复。短时校准 30 秒通过，证据 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T205004Z`；Docker/1Panel、备份恢复、失败迁移和镜像激活回滚均通过。
- 新增 Controller 真实 API 规模测试 `make test-controller-scale`，使用独立 `xs_nexus_scale` schema、20 个受限 token、32 路并发注册，验证 100/500/1000 活动节点、地址唯一性、Console 快照和数据库查询。证据 `/srv/xs-nexus/artifacts/qa/controller-scale-20260731T210210Z`：100 节点 39.18 注册/s，500 节点增量 18.20/s，1000 节点增量 8.66/s；快照 50/131/193 ms，计数查询约 2 ms，总耗时 83 秒。
- 运行时稳定性仍需完整 24 小时曲线；当前短时资源数据用于校准，不替代 24 小时验收。加密吞吐、Direct/Relay RTT、WebSocket 广播和真实 Relay 吞吐基线已完成。
- 新增 `make test-protocol-throughput`，以 release profile 对 50,000 个 1200 字节 IPv4 包执行完整 XSP/1 ChaCha20-Poly1305 seal/open、身份绑定、重放窗口和 IPv4 校验；有效证据 `/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z` 为 161,314 次加密+解密往返/s、184.61 MiB/s 明文吞吐。该单进程 loopback CPU 基线不等于 Agent/TUN 或公网端到端吞吐。
- 新增 `make test-relay-throughput`，在 release profile 通过真实 UDP Relay、两份认证 Lease、重放/速率/队列门禁和 64 帧有界窗口转发 10,000 个 216 字节 XSR/1 帧；证据 `/srv/xs-nexus/artifacts/qa/relay-throughput-20260731T211903Z` 为 87,822 包/s、18.09 MiB/s，Relay 内部转发延迟平均 4 µs、最大 161 µs，零协议丢弃。该 loopback 基线不等于公网 Relay 容量或 RTT。
- Controller 新增按 network ID 隔离的 WebSocket 配置广播：所有配置写入仅在数据库事务成功提交后发布事件，同网络已认证连接读取最新签名配置；不同网络不会收到事件。真实 PostgreSQL 集成测试以同一节点两条控制连接验证发起连接直接响应、观察连接收到相同版本 5 广播、唯一节点在线计数和多连接关闭生命周期；`make test-controller-db`、Clippy 和秘密扫描通过。
- 新增 `make test-agent-rtt`，复用真实双 Agent namespace、TUN、XSP/1、Relay fallback/failover 和 Direct 恢复测试，各路径采样 30 次业务 ICMP，并采集两个 Agent 的 10 秒空闲 CPU、RSS、线程和 FD。脚本强制 release profile，并核对实际 `/proc/<pid>/exe`。有效证据 `/srv/xs-nexus/artifacts/qa/agent-rtt-20260731T221036Z`：Direct 平均/p95 0.91/1.13 ms，Relay 平均/p95 1.16/1.45 ms，平均增量 0.25 ms；Agent 平均空闲 CPU 0.55% 单核、RSS 7.95 MiB、9 线程、15 FD。早期 debug profile 的资源结果已排除，不作为产品基线。
- 已建立 `docs/PERFORMANCE_REPORT.md`，统一绑定吞吐、规模、查询、RTT、短时资源和 24 小时长测方法；后续 24 小时长测和修正重启回归已完成，当前状态为 `COMPLETE_FOR_CURRENT_LINUX_BASELINE`。

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
./scripts/validate-m31.sh
./scripts/validate-m32.sh
./scripts/validate-m42.sh
./scripts/validate-m51.sh
sed -n '503,520p' EXECUTION_PLAN.md
find deploy -maxdepth 3 -type f -print | sort
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

## 2026-07-31 runtime image license closure

- Implemented a fail-closed one-entry-per-package license closure for all four runtime images.
- Console and db-tools retain the official Alpine SPDX text corpus while removing the build-only `spdx-licenses-text` package before final package inspection.
- Real rootfs dry-run results: Controller 10/10, Relay 10/10, Console 32/32, db-tools 61/61 packages have closure records. Console and db-tools bind 783 and 758 license materials respectively; these dry runs are implementation evidence, not the final clean-commit QA artifact.
- Unit coverage includes missing Alpine text, unsafe rootfs paths, safe Debian documentation links, malformed package metadata, duplicate image mappings and deterministic publication.
- Clean-commit evidence `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z` passed deterministic image generation, package/license closure, provenance, host-state invariants, Grype scanning and the digest/API-bound vulnerability disposition verifier. Result: Critical 2, High 4, Medium 16, Negligible 24, with no currently advertised fix and no fixable finding.

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
## 2026-07-31 运行时镜像漏洞收敛检查点

- 实现提交：`9b727d424e9968b0dcec2c7f8553ea0d79057961`（`fix(supply-chain): remove runtime curl dependency`）。
- Controller/Relay 已增加固定 loopback、无参数、超时和 8 KiB 响应上限的二进制 `healthcheck`；运行时 Debian 镜像删除 `curl`，Console/db-tools 在构建时应用当前 Alpine 安全更新。
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace --lib --bins`、`make test-docker-deployment` 和 `make security-check` 通过。
- 新镜像供应链证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T182323Z`；绑定精确提交与四个 image ID，双生成一致，宿主 Docker/网络/默认路由/nftables 不变。
- 后续最小化提交：`f50b08f`、`d6db189`、`2eefae9`。Controller/Relay 已切换到固定 digest distroless，Console 删除 curl 包链，证据生成器严格支持 distroless `status.d` 包元数据。
- 最终证据：`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T183424Z`。Grype 0.116.1 总计 Critical 2、High 6、Medium 18、Negligible 24，`total_fixable_findings` 为空。最初扫描为 Critical 44、High 115；当前可修复项已清零，但残余发现未获豁免。

## 2026-07-31 最新 M6.1 与 Relay 遥测回归

- 最新 M6.1 聚合验证绑定提交 `9559b398aabdf5f5bce311a1311aa43f648e3bcb`，证据 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T184122Z`；Windows 路由事务、IP Helper 源码、可信 LUID、Agent 准备编排、MSVC target check、C Release/ASan/UBSan、workspace/控制面/供应链门禁全部通过。仍不代表 WDK 或 Windows 实机结果。
- 聚合验证先捕获健康检查测试对单次 TCP read 的错误假设，失败证据 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T183943Z`；测试服务器改为有界读取完整 HTTP 头后通过，没有放宽产品断言。
- Relay `/metrics` 现在公开有界累计接收/转发包和字节、分类丢弃总数、I/O 错误，以及队列转发延迟样本、平均微秒和最大微秒；不记录业务内容、节点身份或 payload。
- 最新 M2.3 聚合验证绑定提交 `b80d780ab2788f9913b4b80997a220c099c40f0f`，证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260731T185454Z`；真实双 Relay fallback/failover/Direct 恢复测试同时断言字节、丢弃和延迟指标。
- 首轮 M2.3 在全部功能测试通过后因 Compose validator 缺少强制 revision 输入失败，证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260731T185001Z`；验证器补齐完整测试专用变量后原样重跑通过。
- Console 已删除未使用的 `nginx-module-image-filter` 与 TIFF 包链，完整 Docker 生命周期通过；提交 `ca7d2c2e026385b6e8f4432de7942b672be71d3a` 的供应链与漏洞证据为 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T190639Z`。结果降为 Critical 2、High 4、Medium 16、Negligible 24，Console/db-tools 无 Critical/High，当前可修复项为 0。
- 剩余 glibc 三个 CVE 已建立可执行 disposition 门禁：精确匹配报告、拒绝可修复项/新增 High 或 Critical，并从镜像提取 Controller/Relay 二进制验证不导入受影响 API；复核期限为 2026-08-31，基础镜像、glibc、扫描结果或二进制导入变化会提前触发复核。Windows runtime 接入继续由 `BLK-001` 阻塞。
- M7.1 首次三轮聚合在第 2 轮捕获候选路径测试的双向就绪竞态，失败证据 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T192431Z`；根因和修复记录于 `XS-2026-0003`。测试现独立等待两个方向完成认证路径探测，未延长超时；修复后的三轮聚合仍须从零重新计数。
- 双向等待后的聚合首轮再次失败，证据 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T194301Z`；旧 ERR 诊断被 cleanup 覆盖，现已修复为保留首错误命令及失败临时目录。定向 10 轮、fallback/path 组合 12 轮和完整 M5.2 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T194855Z` 通过，但 `XS-2026-0003` 仍保持调查中，M7.1 三轮计数为零。
- 提交 `52867cd` 后重新从零执行的三轮完整聚合全部通过，证据 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`；每轮覆盖 Linux 全链路、安装/部署/恢复、M6.1 Windows 源码门禁、E2E/视觉、image SBOM 和漏洞 disposition。当前记录 P0=0、P1=0，三个 P2 均有关闭结论，`XS-2026-0003` 已闭环；M7.1 自动化退出条件完成，但不改变 M6.1/M6.2 外部门禁。

## 2026-08-01 stability gate hardening

- Corrected `scripts/test-runtime-stability.sh` after live evidence disproved the initial `RestartCount` assumption: an operator-requested `docker restart` changes the container PID but does not increment the restart-policy counter. The future-run gate now requires exactly two observed PIDs per service and a counter fixed at its baseline, so any additional automatic restart fails closed.
- Restart injection now uses the supported `docker restart --timeout 20` form and requires its single output line to exactly equal the full target container ID before accepting health recovery.
- Current long test remains the pre-change run started at 2026-07-31T21:22:42Z; the script hardening applies to future runs and does not alter the active process.
- Current HEAD static regression (`make fmt-check`, `make lint`, `make test-image-sbom`, `make security-check`) passed.
- The incorrect intermediate gate is tracked and closed as `XS-2026-0004`; a short post-run integration will exercise the corrected script after the active 24-hour process releases its Compose environment.

## 2026-08-02 24 小时稳定性与签名更新闭环

- 24 小时长测完成三服务各 1420 次采样和一次受控 PID 转换；Controller/Relay/Console 的 RSS、线程、FD 和日志增长均保持有界，`RestartCount` 全程为零。旧脚本只因 `XS-2026-0004` 的错误计数假设在最后退出非零，没有隐藏该失败。
- 修正门禁后的真实 60 秒完整部署回归 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z` 通过：每服务 11 个样本、完整容器 ID 重启证据、恰好两个 PID、零额外自动重启，并完成数据库/容器/网络清理。
- 实现不可变离线签名发布、stable/testing/development 通道、网络/平台/架构策略、暂停、最低版本和确定性基点灰度；Controller 只加载发布公钥，不接受私钥。
- Agent 用身份密钥签名运行时版本/架构/通道/状态，Controller 验证当前分配通道和单调时间后才下发 eligible/required 指令。节点通道变更进入 Controller 签名配置并使用配置版本冲突保护。
- Agent 对 HTTPS 下载执行大小、超时、文件名、SHA-256、离线清单和签名复验；私有 staging 就绪后由无网络 root helper 通过 no-follow 私有复制再次验证，再调用现有原子安装/健康/回滚事务。
- Linux 包、安装、回滚和卸载现包含 `xs-agent-update.path/.service`；root helper 正常路径和归档篡改负向路径、systemd 沙箱及安装器生命周期均通过。
- Console 更新页接入真实发布、策略、节点通道和签名状态 API，审计员只读；加载/空/403/503、显式确认、代次/配置版本冲突、9 条 Playwright 和 6 个固定视口通过。
- 隔离验证通过：Core 26、Agent 48+3、Controller 10+1、真实 PostgreSQL、Agent 控制面、严格 Clippy、Console 构建/单测/E2E/视觉。正式仓库和生产容器在验证期间未被修改。

## 2026-08-02 Agent/Relay 认证遥测闭环

- Agent 数据面新增每 Peer 累计业务收发、握手和认证 RTT；计数只覆盖进入加密数据面的发送包及通过解密、来源绑定和 ACL 的接收包。被替换 Peer 的累计值滚入进程级退役计数，boot 总量不回退。
- Agent 用节点身份密钥签名 Network/Node、随机 boot ID、单调 sequence、精确配置 Peer 集合、Direct/Relay 路径和累计值；Controller 拒绝错误签名、过期/未来报告、重放、同 boot 计数回滚、未知 Peer/Relay 和聚合不一致，保存最新值与 25 小时/1800 样本。Relay 最多保留 9000 样本，以覆盖最快 10 秒周期下的 25 小时窗口。
- Relay 用目录身份密钥签名全局累计租约、字节、转发、分类丢弃、错误和延迟指标；报告明确不含 Network/Node、端点、Lease ID 或 payload。Controller 对同 boot 执行单调验证并以 Relay 目录公钥验签。
- Console 首页、节点、Relay 和拓扑现展示新鲜认证报告派生的当前路径、Relay、24 小时流量、握手成功率、延迟和 Relay 健康；缺失/陈旧值继续显式不可用，不推断默认值。
- 验证通过：Core 29 项、Relay 7+1 项、Agent 既有 48+3 项、严格 Clippy、真实 PostgreSQL签名/重放/回滚负向集成、双 Agent/双 Relay fallback/密文/failover/Direct 恢复与 Reporter 推送、Console 4 项单测和 10 条 Playwright 主流程。`KI-012` 已解除；真实公网 Relay 容量仍由 `KI-010` 跟踪。

## 2026-08-02 数据库认证加密与异地副本闭环

- db-tools 只持有 age X25519 public recipient；`pg_dump` custom stream 在 FIFO 消费完整性与 `PGDMP` 头校验后直接进入 age，不在本地或副本挂载写入数据库明文。
- 加密 manifest 认证 schema、备份名、密文字节数/SHA-256、recipient Key ID 和 UTC 时间；公开 index/复制回执支持无私钥传输校验，离线 identity 深度校验先完整认证 age 密文，再由 `pg_restore --list` 校验归档目录。
- 每次备份自动复制到不同文件系统且带 deployment/target marker 的挂载；支持幂等复制、仅从完整副本取回、本地/副本独立保留期、最小保留数、显式时间确认和不可复用销毁墓碑。RC 预检强制 7/30 天和至少 3 份。
- 完整 Docker 生命周期覆盖篡改、错误 identity、取回、恢复前安全备份、恢复、两阶段保留/销毁和同名复用拒绝，并在退出后确认测试容器/schema 清理和 `1panel-network` 不变。`KI-015` 已解除。
- 正式离线 identity 仪式、真实异地主机/对象存储和生产恢复演练不能由当前环境代办，转为 `BLK-007`，没有被标记为生产已完成。

## 2026-08-02 备份加密依赖漏洞收敛

- 初始备份镜像采用 Alpine 3.22 的 age 1.2.1；`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T085442Z` 暴露其内嵌 Go 依赖中的 18 个 Critical、32 个 High 可修复项。没有添加 ignore、VEX 或风险豁免。
- 更新到 Alpine edge 的 age 1.3.1-r6 后，`/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T090303Z` 仍因内嵌 `golang.org/x/crypto v0.45.0` 的 `GHSA-w879-237q-wc7r` 被 disposition 门禁拒绝。
- 最终 db-tools 使用 digest 固定的 Go 构建阶段，从 age `v1.3.1` 精确提交 `b8564adb6d58329b8a3e267360ca2b0abc4efe1d` 构建 `age`/`age-keygen`，强制并验证 `x/crypto v0.52.0`，运行镜像只复制静态二进制和上游许可证。
- 正式提交 `a120688b4fd25cb22f0d081e77e9923daceafaef` 的完整 Docker 备份生命周期、确定性镜像 SBOM/许可证闭包、Grype 扫描和 disposition 均通过；证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`，结果恢复为 Critical 2、High 4、Medium 16、Negligible 24，可修复项为 0。
