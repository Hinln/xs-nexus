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

### M4.1/M4.2 自动化覆盖

- `apps/controller/tests/controller_db.rs` 使用真实 PostgreSQL 覆盖 Argon2id 用户、统一无效登录、登录限速、安全 Cookie、CSRF 轮换、会话撤销、用户创建、审计员越权拒绝、快照脱敏和实时控制连接上下线；
- 管理快照只返回数据库与当前进程实际状态；更新发布、节点分配通道、Agent 签名更新上报、节点身份签名路径/流量/RTT 和 Relay 目录身份签名指标均为真实数据。缺失或陈旧遥测与尚未接入的备份执行状态仍返回带原因的 `unavailable`/`stale`，测试明确禁止秘密/hash 字段和伪造指标；
- `apps/console/tests/console.spec.ts` 覆盖登录、16 个页面、Relay 认证指标、更新发布/灰度/节点通道请求体、空/403/503、审计员只读、页面无横向溢出、跳转链接、节点详情焦点恢复与 404；
- `apps/console/tests/visual.spec.ts` 使用固定数据和 Chromium，在 1440×900、1920×1080、1280×720、1024×768、768×1024、390×844 下生成登录、全部页面、节点详情和 404 截图；
- 桌面状态矩阵额外覆盖所有页面空态、大量节点、长 IPv6、离线、部分服务不可用、加载、无权限和服务错误，共生成 135 张截图；
- 浏览器测试监听 Console、Page Error 和 4xx/5xx；登录 401、权限 403、预期 503 被精确断言，其余错误必须为零；
- `scripts/validate-m42.sh` 汇总全部历史网络回归、真实 PostgreSQL、前端单元/构建/E2E/视觉、秘密扫描、ShellCheck、npm audit，以及 Docker、`1panel-network`、默认路由和 nftables 前后基线；
- 固定浏览器：Playwright `1.62.1`，Chromium for Testing `151.0.7922.34`；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`；截图：`/srv/xs-nexus/artifacts/visual/m4.1`。

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

### M5.1 自动化结果

- 已通过：干净安装、重复安装、哈希错误、签名错误、错误公钥、包内篡改、服务启动失败、升级、升级失败自动回滚、显式回滚、卸载、卸载后重装、身份保留和无项目服务/网络残留；
- 已通过：真实 x86_64 与 aarch64 release 构建，分别验证 ELF `X86-64` 与 `AArch64`；
- Enrollment Token 的一次性、过期和错误语义由 M1.1 Controller/PostgreSQL 集成测试覆盖；安装器只接受 token 文件路径并在 staging 后删除，不在命令行或状态中输出 token；
- 截断或下载中断产物由外部清单长度、文件名和 SHA-256 校验失败关闭；安装器不内置下载器；
- systemd 自动重启与 SIGKILL 后可信 cleanup 已在真实 transient unit 验证；精确 Gate 06 补充证据为 revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02`、run `31352258781`、job `93345151258` 和 artifact `9049384061`；整机重启、真实 arm64/NAS 运行仍属于实机门禁；
- 证据：`/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。

### M5.2 自动化结果

- `scripts/test-docker-deployment.sh` 使用真实外部 PostgreSQL 和既有 `1panel-network`，构建并启动 Controller、Relay、Console 和一次性迁移/运维镜像；
- 已验证三服务健康、非 root、只读根文件系统、无 capability、`no-new-privileges`、日志轮转、仅外部网络和无数据库端口；
- 已验证 API 数据跨容器激活持久化、自定义格式备份、五字段清单、大小/SHA-256/归档校验、篡改拒绝和恢复后数据精确回到快照；
- 已验证错误数据库端点导致迁移失败时当前服务镜像不变，错误 Controller 镜像激活失败后自动恢复上一组健康镜像；
- 测试前后比较 Docker 网络、`1panel-network` 成员、默认路由和 nftables，并删除项目容器与测试 schema；
- `scripts/validate-m52.sh` 汇总全部历史单元/集成/namespace/安装回归、真实 x86_64/aarch64 构建、M5.2 部署生命周期、秘密扫描、ShellCheck、npm audit 和宿主基线；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`。

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

### M6.1 当前自动化覆盖

- Windows 路由事务模型覆盖默认/保留/零 LUID、外部精确与部分重叠、manifest 所有权漂移、4096 条表上限、additions-first、逆序补偿、删除恢复、补偿失败显式上报、Tentative→Preferred、Duplicate 拒绝和路由失败后地址清理失败；Linux 模型 11 项、MSVC target check 与双平台 Clippy 通过。未覆盖 Windows IP Helper 运行、真实 DAD、PnP/睡眠、manifest 崩溃恢复或实际路由残留。
- xsnet identity query 保持 ABI v1 六类消息不变，独立 schema v1 覆盖精确 8/16 字节、错误版本、非零 reserved、零 LUID 和短响应拒绝；源码门禁要求同一独占 handle、`NetAdapterGetNetLuid`、唯一 present interface 和六个集中 unsafe 块。Portable C 与 Rust transport 模型通过；未覆盖 WDK 编译、真实 device handle、NetAdapterCx 返回值或 PnP 后 LUID 行为。
- 2026-07-31 完整 M6.1 聚合验证通过：`/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T173643Z`。包含 workspace Clippy/单测、真实 Agent 控制面回归、C Release 与 ASan/UBSan、MSVC target check/Clippy、Windows 源码门禁、SBOM/秘密扫描、npm audit 和宿主网络状态前后比较。
- 镜像 SBOM 模型测试覆盖 image/Dockerfile 映射、Debian/apk/distroless 包解析、Alpine virtual metapackage、缺失包字段、路径穿越、逐包许可证闭包、CycloneDX 与 provenance subject。正式验证入口构建四个当前提交镜像、双生成比较、拒绝 revision mismatch，并比较宿主网络状态；当前 accepted 证据和漏洞结果见本节后续 `Runtime image license closure`，本条不再表示待完成。
- 运行证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T180211Z` 已实际构建四镜像并识别 344 个包，生成两套逐字节一致的 manifest/CycloneDX/provenance/license materials；revision mismatch 负向测试通过，宿主 Docker 容器和网络、`1panel-network`、默认路由、nftables 与失败服务均保持不变。
- 漏洞扫描入口要求固定 scanner 版本/哈希、manifest 路径边界、tag→image ID 再验证、隔离数据库、每镜像 JSON 与 summary hash，并比较宿主网络状态；正式扫描、负向门禁和 disposition 已在 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z` 通过，可修复项为 0，剩余 glibc 风险由 `KI-021` 跟踪。
- 严格 manifest 增加 JSON round-trip、未知字段/尾随数据/超限拒绝、不安全地址、非规范 route order、缺失资源幂等恢复、exact route/address 清理选择与外部重叠拒绝；累计 15 项模型测试。仍未覆盖 NTFS 私有持久化、进程中止窗口和 Windows 实际恢复。
- 受保护 manifest 平台边界增加私有读取、原子写入和验证删除源码路径；恢复执行测试证明逆序尝试全部 exact route、随后地址删除，并同时保留所有失败。累计 16 项模型测试，`windows-private-storage` 与 `windows-route-manager` 均通过 MSVC target check/Clippy；仍未覆盖真实 NTFS、杀进程窗口和 IP Helper 残留。
- `make test-windows-agent-routing` 增加 Agent Cargo/module/config/准备顺序、Preparing/Active、恢复/清理、runtime 未接入和禁止 unsafe/后台线程/子进程源码门禁，并复用 16 项模型与两个 Windows crate 交叉检查；该门禁不替代完整 Agent Windows 链接或 VM 执行。

- `drivers/windows-xsnet/src/abi.c` 不依赖 Windows 结构体布局，逐字段读取固定小端头和批次描述符；
- `drivers/windows-xsnet/tests/abi_test.c` 覆盖正确消息、截断、Magic、版本、header、flag、payload 上限、精确总长度、零 sequence、空/超限批次、描述符长度、间隙、短包和隐藏尾部；
- `drivers/windows-xsnet/tests/session_test.c` 覆盖单 owner、版本和能力协商、调用顺序、严格递增 sequence、上限重开、MTU、队列深度、link、包 MTU、幂等 Detach 和 cleanup；
- `drivers/windows-xsnet/tests/dataplane_test.c` 覆盖双包往返、批次原子入队、IPv4 version/IHL/总长度、MTU、容量背压、小输出不消费、单包/部分出队、环绕和 reset 后 payload 清零；
- `drivers/windows-xsnet/tests/stress_test.c` 以固定种子分别执行 30,000 轮任意消息解析、消息写入往返、任意批次解析、会话状态操作和有界队列操作；所有失败会话操作必须逐字段保持原状态，所有失败队列操作必须保持元数据和完整 64 槽字节不变，并对 Hello、Attach、SetLink、TxBatch、RxBatch、Detach 六类有效消息执行逐字节变异；
- `drivers/windows-xsnet/tests/lifecycle_test.c` 从活动 owner/link/双队列/请求状态穷举 cleanup、TX cancel、RX cancel、D0 exit、hardware release 和 I/O stop 的全部 720 种顺序，验证单次完成/取消、断链、session/队列清理、睡眠后重新认证、队列重启和重复 teardown 幂等；
- `scripts/test-windows-xsnet-abi.sh` 在 Clang 21 Release `-Wall -Wextra -Wpedantic -Werror` 与 GCC 15 ASan/UBSan 配置编译运行五组测试；
- `scripts/validate-windows-xsnet-source.py` 固定 Windows 11 24H2、UMDF 2.33、NetAdapterCx 2.5、x64、测试签名元数据、仅 LocalSystem SDDL、独立 UMDF host、拒绝内核客户端/未知 file object/直接硬件访问、direct/buffered IOCTL 和关键生命周期回调；
- TX/RX queue callback 已缓存 ring collection 和必需的虚拟地址扩展，按一包一 fragment 在系统缓冲区与私有有界队列间复制；Windows 侧 Ethernet frame 与私有 raw IPv4 ABI 间显式剥离/合成固定 14 字节头，RX 填充 Ethernet/IPv4 layout。畸形 ring 数据停止当前推进；源码门禁禁止在 callback 内同步改变 link state，避免重入 stop/cancel；
- TX direct-I/O 使用空 payload 请求和 framed 输出；RX direct-I/O 使用 framed direct 输入。请求同步完成且不挂起，空/满/小缓冲/未启动先失败，成功才推进 sequence；SetLink 要求双队列 started，stop/cancel 退回 Attached 并断链；固定 64 包和协商深度同时生效；
- `scripts/validate-windows-xsnet-installer.py` 静态检查测试安装器的管理员门禁、Windows build 下限、精确三文件包、重解析点拒绝、signer thumbprint、Microsoft-signed WDK DevGen、PnPUtil、精确状态、20 秒有界等待、失败回滚和残留拒绝；
- 本地 Windows PowerShell parser 对模块、安装和卸载脚本执行零语法错误解析。该源代码检查本身不替代实机；2026-08-04 的独立 VM 证据已另外真实调用 PowerShell 7.6、PnPUtil、DevGen 和设备 API；
- 2026-07-31 连续三轮源码门禁、四组 Release/ASan/UBSan 测试和安装器门禁均通过；证据为 `/srv/xs-nexus/artifacts/qa/m6.1-portable-stress-20260731T012940Z`；
- 生命周期模型加入后再次连续三轮通过五组 Release/ASan/UBSan、源码与安装器门禁；证据为 `/srv/xs-nexus/artifacts/qa/m6.1-lifecycle-20260731T014028Z`；
- `apps/agent/src/windows_xsnet.rs` 的前 8 个测试覆盖 C ABI/IOCTL 固定向量、完整 Hello/Attach/SetLink/TX 流程、规范 IPv4 批次、单飞请求、已知拒绝复用 sequence、未知结果强制重连、畸形响应失败关闭、transport 分类和非 Windows 拒绝；原契约证据为 `/srv/xs-nexus/artifacts/qa/m6.1-transport-contract-20260731T015900Z`；
- 新增 10 个 `XsnetDeviceSession` 测试固定配置先于设备打开校验、无重试的 Hello/Attach/SetLink 启动、由 MTU/深度推导的 TX 容量、单步 TX/RX、权威拒绝保持 sequence、不确定结果毒化、失败启动立即停止并释放 transport、无效 RX 零 I/O、Drop 零 I/O、LinkDown/Detach 顺序、重复 Detach 和 shutdown 显式重试；适配层没有后台线程、轮询或 Drop I/O，且未接入 Agent runtime；
- `crates/windows-transport` 以 `no_std + alloc` 实现精确 GUID 单接口解析、独占同步 handle、六个 ABI IOCTL、独立 identity IOCTL、buffered/IN_DIRECT/OUT_DIRECT 映射、初始化输出、输入复制和全失败 Indeterminate；Agent 保持 `#![forbid(unsafe_code)]`，六个 unsafe 块只存在于平台文件；
- `scripts/test-windows-xsnet-transport.sh` 先运行全部 18 个 Agent xsnet 测试，再实际为 `x86_64-pc-windows-msvc` 构建 core/alloc 与 transport，并对同一 target 运行 Clippy `-D warnings`；transport crate 的 2 个 Linux 测试覆盖超限、未知 IOCTL 和空/多接口，Agent 覆盖配置先于设备打开校验、失败释放、无效数据零 I/O、析构边界及非 Windows 打开拒绝；最新证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T033841Z`；
- `scripts/validate-windows-xsnet-source.py` 额外固定 session 必需实现与负向测试、配置校验先于首个 IOCTL 和设备打开，并在 WDK/VM 门禁前拒绝 Agent runtime 引用、后台线程、sleep 或 session Drop I/O；
- `crates/windows-local-ipc` 将 SDDL 转换、`SECURITY_ATTRIBUTES` 和 Tokio 原始创建调用限制在三个可计数 unsafe 块；固定管道名、first-instance、拒绝远程客户端、仅 LocalSystem/Administrators DACL、不可继承 handle、4 KiB 请求、512 KiB 响应和 17 个 OS 实例上限；
- `scripts/test-windows-agent-ipc.sh` 固定上述源码不变量，运行既有 Linux 私有 socket 集成回归和 Windows crate 单测，并实际为 `x86_64-pc-windows-msvc` 执行 check 与 Clippy warnings-as-errors；16 个活动处理许可耗尽时 Windows listener 停止接收而不创建任务或退出；
- `crates/windows-private-storage` 固定仅 LocalSystem/Administrators 的 protected 文件与目录 DACL，逐字节比较实际和期望 ACL，并拒绝相对路径、任意 reparse 路径链、宽松父目录、非普通文件/目录、长度变化和非同目录临时文件；写入使用 `create_new`、`sync_all`、`ReplaceFileW`/`MoveFileExW` write-through，绝不带覆盖式 Move flag；
- `scripts/test-windows-agent-storage.sh` 运行源码不变量、现有 Linux identity/storage 测试，并为 `x86_64-pc-windows-msvc` 实际 check 与交叉 Clippy；该结果未执行 Windows ACL、junction、替换或崩溃恢复行为；
- `crates/windows-service` 固定 256 UTF-16 unit 以内的路径无关服务名，并把 dispatcher、control handler 和 status FFI 限制在四个可计数 unsafe 块；状态锁和原子 STOP 保证并发 STOP/SHUTDOWN 不被后续 RUNNING 覆盖，Agent 通过 Notify/watch 复用现有关闭契约；
- `scripts/test-windows-agent-service.sh` 固定 `XsNexusAgent`、Windows-only `service --config`、四阶段 SCM 状态、STOP/SHUTDOWN/INTERROGATE、一次性通知、panic/失败关闭、无安装 API 和无轮询，并实际运行 portable 单测、Agent parser 回归、MSVC target check 与交叉 Clippy；
- `scripts/validate-m61-agent-session.sh` 汇总格式化、workspace Clippy、Rust/npm 单测、真实 PostgreSQL Agent 控制面、五组 C Release/ASan/UBSan、源码/安装器/VM/兼容/transport/本地 IPC/私有存储/Service 门禁、独立实现、SBOM、秘密扫描、ShellCheck、npm audit，以及 Docker、`1panel-network`、默认路由、nftables、namespace/TUN 和失败服务前后基线；最新证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`；
- 以上仍未链接完整 Windows Agent，也未调用 Rust Named Pipe/存储/SCM 或服务安装/停止；但专用 SYSTEM VM harness 已真实调用 Configuration Manager、独占设备打开/关闭、七个 DeviceIoControl、Tx/Rx、LUID 和 PnP restart，WDK/VM 驱动门禁不再由 `BLK-001` 阻塞；
- `scripts/windows/build-xsnet-test-package.ps1` 只在显式确认的 Windows 11 26100+ 管理员测试 VM 中运行，固定 Microsoft-signed MSBuild/InfVerif/Inf2Cat/SignTool、Release x64、`SignMode=Off`、先嵌入签名 DLL 再生成并签名 `10_GE_X64` catalog、SHA-256 test signer 和精确 INF/CAT/DLL allowlist；不修改 BCD、信任根或测试签名策略；
- `scripts/windows/invoke-xsnet-test-vm-stage.ps1` 以 Initialize/Install/EnableVerifier/CollectVerifier/DisableVerifier/Uninstall 六个不可复用阶段保存系统、网卡、路由、设备、驱动、Verifier 和错误事件证据；要求相同 VM/快照声明、Verifier 前后两次人工重启、精确健康状态和卸载零残留，最终生成 SHA-256 证据清单；
- `scripts/validate-windows-xsnet-vm.py` 禁止下载、BCD、执行策略绕过、自动重启、无限等待和验收伪声明；该 validator 仍只是源码门禁。2026-08-04 的独立 VM run 已另外执行 WDK、签名、standard/UMDF/Application Verifier 和设备操作；
- 测试包与 VM 工作流最终自动化证据为 `/srv/xs-nexus/artifacts/qa/m6.1-vm-workflow-20260731T021432Z`，包含 ABI Release/ASan/UBSan、源码、安装器、VM 门禁、秘密扫描以及默认路由、规范化 nftables、完整 `1panel-network`、namespace/TUN 前后比较；
- `scripts/validate-windows-xsnet-compatibility.py` 强制 C header、Rust Agent 和安装状态共同固定 exact ABI v1，Hello header/min/max 都为 v1，INF 只有一个四段 `DriverVer`，构建清单记录 driver version/ABI `1..1`/IPv4，安装器在 staging 前后核对版本并拒绝任何既有 xsnet；
- 测试安装器不支持 in-place upgrade；替换包只能在快照 VM 停止 Agent、关闭 handle、精确卸载后 clean install。兼容/回滚边界证据为 `/srv/xs-nexus/artifacts/qa/m6.1-compatibility-20260731T022619Z`，真实版本升级、回滚和跨 ABI 拒绝仍未执行；
- 当前结果除平台无关模型与交叉编译外，已真实验证 MSBuild 属性、InfVerif、Inf2Cat、测试签名、VM clean install/uninstall、direct-I/O、NetAdapterCx ring Tx/Rx、LUID、PnP restart、standard Driver Verifier 及 UMDF/Application Verifier。仍未完成命名管道有效 DACL/拒绝矩阵与九命令运行、空 TX/满 RX 的 Win32 权威拒绝映射、完整 Agent Windows 链接/runtime、SCM、IP Helper/DAD/route、sleep/power、生产更新/回滚和正式签名。
- 2026-08-04 accepted run `vm-validation-final-004`：测试包 `15.39.27.376`；普通 PnP restart 285 ms，verifier-enabled 三轮为 1,443/2,113/877 ms；每轮 SYSTEM Tx/Rx 通过；新增 WDF/NDIS/相关 WER/错误事件均为 0；clean uninstall 后 device/package/state/DriverStore 均为 0。证据说明与 SHA-256 见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`。

---

### 独立实现与源码供应链自动化覆盖

- `scripts/validate-independent-implementation.py` 扫描 Agent、Controller、Relay、协议、驱动、安装器、部署和脚本源码；只允许 Agent 对已知第三方接口前缀的排除保护，以及 Windows 源码门禁中的禁止断言，任何新增禁用产品引用失败关闭；
- `scripts/generate-source-sbom.py` 离线绑定 `Cargo.lock`、`cargo metadata --locked`、`package-lock.json` 和精确 npm 许可证快照，生成排序稳定的 CycloneDX 1.6、SPDX 2.3 与 SHA-256 manifest；
- `scripts/test-source-sbom.py` 双次生成并逐字节比较，固定 321 个 Cargo、110 个 npm、总计 431 个组件，验证全部许可证、PURL 和锁定摘要，并覆盖缺失快照项、拒绝许可证、禁用依赖和覆盖既有目录；
- `make test-independent-implementation` 与 `make test-source-sbom` 已接入 CI；M0.2 文档门禁继续验证 clean-room、原创协议和明确不兼容说明；
- 完整证据：`/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。该范围不包含容器操作系统包或许可证全文，不能替代 RC 容器 SBOM。

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
## 9. 运行时镜像漏洞回归（2026-07-31）

- 固定 loopback 健康探测单测覆盖：成功 200、非 200、畸形头、超大头、连接拒绝、读超时、非 loopback 和非法路径；通过。
- 严格命令解析覆盖 Controller/Relay 的未知命令及带参数 `healthcheck`；通过。
- `make test-docker-deployment` 真实重建四镜像，并验证迁移、三服务健康、安全属性、备份恢复和失败回滚；通过。
- `make validate-image-supply-chain`：通过；证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T182323Z`。
- `make scan-image-vulnerabilities EVIDENCE_DIR=/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T182323Z`：通过；所有当前有供应商修复版本的发现为 0，残余 Critical/High 保留报告等待 disposition。
- distroless/Console 进一步最小化后再次运行完整 Docker 生命周期：通过；无 shell 的 Controller/Relay 仍能迁移、启动、健康检查和回滚。
- distroless `status.d` 正向解析、普通 Debian status、md5sums 排除和不完整身份负向测试：通过。
- 最终供应链证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T183424Z`：通过；最终 Critical 2、High 6，当前可修复项为 0。
- Console 删除未使用 image-filter/TIFF 包链后，`make test-docker-deployment` 与 `make validate-image-supply-chain` 通过；新证据 `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T190639Z` 为 Critical 2、High 4，Console/db-tools 无 Critical/High。

### Runtime image license closure (2026-07-31)

- `python3 scripts/test-image-sbom.py`: passed with negative coverage for missing SPDX text and unsafe material paths.
- Real rootfs dry run: Controller 10/10, Relay 10/10, Console 32/32 and db-tools 61/61 package closure records.
- Clean-commit `make validate-image-supply-chain`, digest-bound Grype scan and vulnerability disposition passed at `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T091605Z`: 113/113 installed OS packages have closure records; Critical 2, High 4, Medium 16, Negligible 24; no currently advertised fix and no fixable finding.
- Backup dependency negative-to-positive regression: `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260802T090303Z` was correctly rejected for `GHSA-w879-237q-wc7r` in the packaged age binary; rebuilding fixed age `v1.3.1` source with `x/crypto v0.52.0`, rerunning the full Docker lifecycle, and rescanning at the current evidence path passed without a waiver.
- `make verify-image-vulnerability-disposition EVIDENCE_DIR=/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T190639Z`：通过；严格匹配剩余三个 glibc CVE，拒绝 fixable/集合漂移，并验证 Controller/Relay 精确镜像二进制不导入受影响 API。
## 10. Relay 可观测性与资源韧性回归（2026-07-31；2026-08-10 复核）

- 单元测试验证接收/转发字节、分类与总丢弃、发送 I/O 错误、延迟样本/平均/最大值的稳定快照。
- 真实 UDP Relay 测试验证认证注册、逐字节密文转发、重放和端点伪造拒绝后，指标与实际事件一致。
- `test-agent-relay.sh` 在双 Relay fallback、主 Relay 故障切换和 Direct 恢复场景中从真实 `/metrics` 断言 `packets_received`、`bytes_received`、`packets_forwarded`、`bytes_forwarded`、`forwarding_latency_samples` 和 `packets_dropped`。
- 完整命令：`make validate-m23`；结果通过；证据 `/srv/xs-nexus/artifacts/qa/m2.3-20260731T185454Z`。
- Gate 08 资源单测在分配来源状态和执行公钥验签前施加全局注册预算，并覆盖来源地址喷洒、共享全局包队列、共享全局字节队列、拒绝不消耗重放/速率状态、cleanup 释放计数后可重试；配置同时拒绝小于单节点完整队列的全局上限。
- 专项 `relay-resilience` job 使用真实 PostgreSQL、两个 Agent、两个 Relay、TUN 和 namespace：主 Relay 停止后切换备用 Relay，主 Relay 重启后双方重新注册，再停止备用 Relay并通过重启后的主 Relay 恢复，最后恢复认证 Direct；同时验证密文不可见、重放/伪造拒绝、进程存活和清理。
- 精确 revision `bad114e9bea46531fcfb23ad871dc5fab7ed8c1e` 的 GitHub Actions run `31358498444` 全部六个 job 通过；Relay 专项 job `93362562136` 和 artifact `9051561308` 通过，归档 digest 为 `d2b4a858d8db2e18b780d7b0cb279b985ff392e04a8b0a7021228e783b8f6b67`。

## 11. M7.1 三轮聚合回归（2026-07-31）

## 12. M7.3 性能与稳定性基线（2026-07-31）

- `make test-controller-scale` 使用真实 PostgreSQL、Controller Router 和 `/v1/enroll` API，32 路并发完成 100、500、1000 节点注册；同时读取真实 Console 快照、节点计数和虚拟地址唯一性。证据：`/srv/xs-nexus/artifacts/qa/controller-scale-20260731T210210Z`。
- 注册吞吐基线：100 节点批次 39.18/s，500 节点增量 18.20/s，1000 节点增量 8.66/s；Console 快照分别约 50.2、130.9、192.6 ms；数据库计数查询分别约 2.72、1.70、2.30 ms。该结果是当前测试主机和独立 schema 的基线，不构成公网容量承诺。
- `XS_STABILITY_DURATION_SECONDS=30 XS_STABILITY_SAMPLE_INTERVAL_SECONDS=5 make test-docker-deployment` 通过，证据：`/srv/xs-nexus/artifacts/qa/runtime-stability-20260731T205004Z`。采样覆盖完整容器进程树，Controller/Relay/Console 分别验证重启恢复、RSS、FD、线程、日志大小和进程 PID 变化。
- 加密吞吐、真实 Relay 吞吐、Direct/Relay RTT、Agent 空闲资源、WebSocket 广播和 24 小时稳定性均已完成。长测三服务各 1420 样本，修正后的真实重启门禁回归为 `/srv/xs-nexus/artifacts/qa/runtime-stability-20260802T061521Z`；旧脚本的错误退出和修复见 `XS-2026-0004`，未被隐藏。

### 签名更新与灰度发布自动化结果（2026-08-02）

- Core 26 个单测覆盖严格版本/清单、通道、确定性分桶、暂停、最低版本和运行时报告签名输入；
- Agent 48+3 个单元/命令测试覆盖 pinned key 权限、指令漂移、HTTPS/大小/哈希、私有 staging、root helper 二次验证、篡改拒绝和 installer 调用边界；
- Controller 真实 PostgreSQL 集成覆盖无效签名、URL 漂移、发布不可变、策略代次冲突、暂停/最低版本、节点通道配置版本冲突、签名配置字段、运行时报告和审计脱敏；
- `make test-agent-control` 验证认证、配置同步和新控制消息兼容；`make test-linux-installer`、`make test-agent-systemd` 验证新 path/service 的安装、回滚、卸载与 systemd 沙箱；
- Console 生产构建、4 个单测、9 条非视觉 E2E 和 2 条视觉矩阵通过，固定视口为 1440×900、1920×1080、1280×720、1024×768、768×1024、390×844；
- `cargo clippy -p xs-core -p xs-controller -p xs-agent --all-targets -- -D warnings` 通过。
- `make test-protocol-throughput` 在 release profile 对 50,000 个 1200 字节 IPv4 包执行完整 XSP/1 seal/open，结果 161,314 往返/s、184.61 MiB/s；证据 `/srv/xs-nexus/artifacts/qa/protocol-throughput-20260731T211232Z`。该结果覆盖 AEAD 与协议校验，但不包含 TUN、UDP socket、Relay 或公网路径开销。
- `make test-relay-throughput` 使用真实 UDP Relay、认证注册和两节点 Lease 转发 10,000 个帧，64 帧窗口下为 87,822 包/s、18.09 MiB/s，内部转发延迟平均 4 µs、最大 161 µs，指标断言零协议丢弃；证据 `/srv/xs-nexus/artifacts/qa/relay-throughput-20260731T211903Z`。首次无限突发因测试接收 socket 缓冲区丢包失败，未作为产品结论；有界窗口保留真实 Relay 全路径。
- Gate 08 专项把同一路径扩展到 5,000,000 个 216 字节帧，并按真实协议以同端点、同身份和唯一签名请求周期续租，而不是延长服务端 Lease；70.213 秒内达到 71,212.14 包/s、14.67 MiB/s，内部延迟平均 5 µs、最大 150 µs，断言 5,000,000 个转发、零 Relay 丢弃和零残留包/字节队列。证据为 run `31358498444` artifact `9051561308`；它仍是 hosted loopback 基线，不是公网或多实例容量承诺。
- Controller 配置事务提交后通过有界 Tokio broadcast 通道按 network ID 通知控制连接；集成测试使用同一凭据建立两条真实 WebSocket，在候选广告生成版本 5 后断言第二条连接自动收到相同签名配置，同时保持唯一节点 presence 语义和连接引用计数。`make test-controller-db` 通过。
- `make test-agent-rtt` 在同一真实双 Agent/namespace/TUN 环境中先强制 Relay fallback，再恢复认证 Direct 路径；每条路径采样 30 次业务 ICMP。脚本强制 release profile，并断言实际 Agent 进程指向配置的 release 二进制。Direct 平均/p95 为 0.91/1.13 ms，Relay 为 1.16/1.45 ms，平均增量 0.25 ms；两个 Agent 的 10 秒空闲基线平均为 0.55% 单核 CPU、7.95 MiB RSS、9 线程、15 FD；证据 `/srv/xs-nexus/artifacts/qa/agent-rtt-20260731T221036Z`。debug profile 的约 8.5% CPU 不作为性能结论。
- `make test-windows-agent-routing` 通过：16 个 `xs-windows-route-manager` 事务/DAD/manifest/恢复单元测试、Windows 路由源码门禁、x86_64-pc-windows-msvc target check 和交叉 Clippy；新增 IP Helper `SitePrefixLength` 精确所有权约束。证据日志为服务器临时 `/tmp/xs-windows-routing-2.log`；该证据不代表 WDK 编译或 Windows 实机验收，runtime 仍保持隔离。

## 13. 数据库加密备份与异地边界（2026-08-02）

- `make test-docker-deployment` 使用真实 PostgreSQL 测试 schema 和独立 `/dev/shm` 文件系统副本挂载，证明 `pg_dump` custom stream 直接进入 age X25519 加密，本地与副本均不存在 `.dump` 明文文件；
- 正向覆盖自动复制、两端密文/index/回执一致、离线 identity 完整解密认证、`pg_restore --list`、从副本取回、恢复前安全备份、目标恢复和 Controller 健康恢复；
- 负向覆盖密文尾部篡改、无关 identity、冲突/缺失副本状态和已销毁备份名复用；公开校验不被描述为身份认证，深度校验必须显式只读挂载离线 identity；
- 保留测试把本地保留期降为 0、保留副本，再从副本取回；随后把副本保留期降为 0，验证最小保留、认证时间边界、销毁墓碑、原 hashes/target ID 记录和两端明文缺失；
- `bash -n`、ShellCheck、`make security-check` 和宿主 Docker/`1panel-network`/默认路由/nftables 前后基线必须同时通过。隔离测试证明产品边界，不替代 `BLK-007` 的正式 identity、真实异地主机和生产数据恢复。

- 首次聚合第 1 轮通过 Linux 全链路、M6.1 源码门禁、UI 与供应链处置；第 2 轮在候选路径测试捕获单向探测被误当作双向就绪的竞态，失败证据 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T192431Z`。
- 定向测试现必须先观察两个 Agent 都把对端新地址标记为 `authenticated_path_probe`，再发送双向 ICMP；等待上限和业务断言均未放宽。
- 双向等待版在下一次聚合首轮仍失败，但旧 ERR trap 被 cleanup 覆盖；诊断已改为只保留首错误并保留失败临时目录。随后 fallback/path 组合 12 轮和完整 M5.2 通过，证据 `/srv/xs-nexus/artifacts/qa/m5.2-20260731T194855Z`；连续三轮尚未重新建立。
- 提交 `52867cd` 后从零重跑三轮聚合全部通过，证据 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z`。每轮包含 M5.2 Linux/安装/部署全量、M6.1 Windows 源码/交叉门禁、6 个 E2E、2 个视觉矩阵、image SBOM 和 glibc disposition verifier；没有新增失败、路由/namespace/TUN/Compose 残留或宿主基线变化。

## 14. Windows signed Wintun enrollment package (2026-08-04)

- [x] `cargo test -p xs-agent -p xs-windows-wintun --target x86_64-pc-windows-msvc --offline`：45 项通过（Agent 43、Wintun 2）。
- [x] `cargo test -p xs-controller --lib --bins --target x86_64-pc-windows-msvc --offline`：13 项通过；完整 Controller 数据库集成未运行，因为 VM 未配置 `XS_TEST_DATABASE_URL`。
- [x] `cargo fmt --all -- --check` 和 `cargo clippy -p xs-agent -p xs-windows-wintun --all-targets --target x86_64-pc-windows-msvc --offline -- -D warnings`：通过。
- [x] Windows 11 x64 VM Wintun smoke：临时 adapter/session 成功创建，LUID 与 interface index 非零，退出后 adapter 不存在；不写入地址、路由或默认路由。
- [x] `windows-release-20260804-r3`：ZIP、manifest、引导器摘要、精确成员集合、逐文件 SHA-256、Wintun DLL SHA-256 与 Authenticode 发行方校验全部通过。
- [ ] 生产 Controller 只读发布目录、PowerShell `/install`、真实 Enrollment Token、Windows 服务/CLI readiness、端到端数据面和卸载/重新安装：待 `KI-022` 闭环。

## 15. 生产候选继续开发复核（2026-08-08）

- [x] 生产服务器 OS、资源、Docker、1Panel、TUN、namespace、nftables、服务、容器和外部网络只读基线：`/srv/xs-nexus-qa/baseline/20260807T061137Z`。
- [x] Chrony/NTP 恢复：确认 86400.300154 秒偏差、RTC/外部 Date/NTP 一致，恢复后容器身份、网络、默认路由、nftables 和下载接口不变：`/srv/xs-nexus-qa/artifacts/time-sync-precorrect-20260807T063050Z`。
- [x] RC 镜像 tag/revision 门禁：正确五镜像精确 tag 通过，stale tag 被拒绝，宿主不变量保持：`/srv/xs-nexus-qa/artifacts/deploy-tag-guard-20260807T062812Z`。
- [x] Windows 生产发布：回环和 `vpn.qinwen.co` 公网的三件套精确文件、bootstrap/manifest/ZIP SHA-256、PowerShell UA 和未知文件 404 通过；计划域名 `vpn.xiashikeji.cn` 的 CDN 525 转入人工 TLS 门禁。
- [x] 数据库暴露：项目 PostgreSQL 无 host binding，外部 TCP `3306`/`5432`/`6379`/`28080`/`28081` 不可达，项目容器健康：`/srv/xs-nexus-qa/artifacts/database-exposure-20260808T063400Z`。
- [x] Console 供应链回归：`nanoid` `3.3.18`，npm high/critical 为 0，Vite build 和 4 项 Vitest 通过；最终聚合证据写入 `/srv/xs-nexus-qa/artifacts/production-continuation-20260808`。

## 16. 最终 M5.2 与生产部署矩阵（2026-08-08）

- [x] 精确提交 `ff9551d322067c934d2ac7d55a62af8896660bb3`：`./scripts/validate-m52.sh` 返回 `validation_status=0`，证据 `/srv/xs-nexus-qa/worktrees/653452d-docker-lifecycle/repo/artifacts/qa/m5.2-20260808T100655Z`。
- [x] 候选路径：真实双 namespace 连续三轮通过；允许两个安全等价的路径证明原因 `authenticated_path_probe` 与 `authenticated_peer_traffic`，但仍强制抓取加密 PathChallenge、至少一侧完成匹配 PathResponse、双端新地址激活和双向 ICMP。
- [x] NAT：full-cone、restricted、port-restricted、public rebind、symmetric-no-direct、blocked-recovery 全部通过；nft 计数读取完整消费输出，消除 `pipefail` 下生产者 SIGPIPE 假失败。
- [x] 双架构发布：x86_64 与 AArch64 release 包、ELF 架构、清单、64 字节 Ed25519 签名通过；QA 镜像补齐目标 libc 头文件后 `ring` AArch64 C 代码实际交叉编译。
- [x] Docker/1Panel：部署测试、真实迁移、迁移前 age 备份、三服务健康、精确 OCI revision、活动部署记录和备份公开校验通过；部署证据 `/srv/xs-nexus-qa/artifacts/deployment-ff9551d322067c934d2ac7d55a62af8896660bb3-20260808T103213Z`。
- [x] 宿主恢复：`1panel-network` 成员名、Docker 网络集合、默认路由、按容器服务名规范化的 nftables 语义和失败服务前后相同，无 namespace/TUN；PostgreSQL 无 host binding。
- [x] 公网：`vpn.qinwen.co` 健康、Linux/Windows 引导、10 个发布文件逐字节一致、未知文件 404；外部 TCP `3306`/`5432`/`6379`/`28080`/`28081` 关闭，控制探针 `122` 打开。
- [ ] 外部门禁：`vpn.xiashikeji.cn` 功能路径 525、Windows 在线客户端、真实 NAS、正式密钥/异地恢复、凭据轮换和第三方审计未完成；生产防火墙、主机补丁/reboot 和有界磁盘治理后续已完成，外部告警仍为 `BLOCKED_EXTERNAL`。

## 17. Gate 01 发布 provenance 回归（2026-08-09）

- [x] `scripts/test-release-provenance.sh`：clean source、annotated signed tag、规范 source/epoch、完整 release 文件集、SHA-256、SBOM、in-toto/SLSA subjects、Ed25519 manifest/SHA256SUMS 签名正向通过；脏 source、错误 tag/commit/source/epoch、额外/缺失文件、symlink/path、摘要、subject、签名与篡改负向拒绝。
- [x] Linux installer/one-click：schema 2 绑定 platform/arch/archive/source commit/build epoch/XSP/1；签名正确但 commit 伪造的清单在激活前被运行时身份核对拒绝。
- [x] OCI/Console：Edge、Console、Controller、Relay、db-tools 双无缓存构建稳定；revision/version/source labels 与 Console `version.json` 精确匹配构建输入。
- [x] GitHub Actions run `31289641228` 对 exact head `fea456b3d6feff36856b1f2066ace8a22b650bce` 全部通过：baseline、protocol fuzz、image reproducibility、real Console E2E。
- [x] 失败证据未隐藏：run `31289302264` 因 `Cargo.lock --locked` 拒绝失败，artifact `9030926588` 保留；修正 lock 后复跑相同门禁通过。
- [x] 分支生产证据：精确 `3d93656` clean release checkout/image、runtime reverse verification、四次 `ff9551d3` 自动回滚和最终升级通过。
- [ ] 正式发布证据：所有者离线密钥仪式、signed RC tag/bundle、main 合并和该正式 RC 的部署仍未执行，Gate 01 保持 `FAIL`。

## 18. Gate 16 PostgreSQL 最小权限与生产部署（2026-08-09）

- [x] 精确 revision `3d93656cc9ec3ea35d58e453118154b25bcc4e14` 的 GitHub Actions run `31294988591`：baseline、真实 PostgreSQL/Controller Console E2E、协议 fuzz、五镜像双无缓存复现全通过。
- [x] 隔离全量验证：格式化、Clippy、Controller 测试、ShellCheck、真实 PostgreSQL 最小权限、完整 Docker 备份/恢复/迁移/失败激活/清理生命周期通过；证据 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-postgres-least-privilege-20260809T045131Z`。
- [x] 生产角色：`xs_nexus_owner` 不可登录；`xs_nexus_app` 和 `xs_nexus_migrator` 均非 superuser、createdb、createrole、replication、bypassrls；运行时存在 app 活动连接。
- [x] 生产负向权限：应用角色建库、建角色、建 schema、改表和写 `_sqlx_migrations` 全部真实拒绝；Controller 启动不迁移并核对精确迁移状态。
- [x] 生产安全部署：变更前基线、正常加密备份、临时加密 rollback 备份和隔离 restore 通过；每次变更前均存在 20 分钟 systemd 自动回滚。
- [x] 失败证据保留：tmpfs `docker cp`、缺少 `CAP_CHOWN`、二进制版本文本/JSON 假设、Docker 规则字节级比较四次失败；四次自动回滚均恢复 `ff9551d3` 健康服务，未删除或放宽验证。
- [x] 最终部署：第五次即时验证通过；全新 SSH 会话复核健康、版本、DB 活动角色、`1panel-network`、OpenResty、route/rule/nft；随后才取消回滚并删除临时明文 staging。
- [x] 最终不变量：`1panel-network` ID/subnet、无关 OpenResty、默认路由、IP rule 和非项目 nftables 规则不变；PostgreSQL 无 host binding；失败 systemd unit 为 0。
- [x] 证据完整性：生产证据目录 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate16-production-deployment-20260809T045813Z` 共 81 个文件，`SHA256SUMS` 复核通过；通用秘密扫描的三项路径型误报逐行固定复核，任何额外发现仍失败关闭。
- [ ] Gate 02 独立残余：bootstrap 及其他已披露凭据轮换、旧值拒绝尚未完成，不因 Gate 16 通过而勾选。

## 19. Gate 14 SSH 与最小 INPUT 防火墙（2026-08-09）

- [x] exact source `ae74783cdf9f75fd90e496fe837e50b744990310`、GitHub Actions run `31300939362`、本地源码测试和隔离 namespace nft apply/reapply/remove 通过。
- [x] 生产只读基线覆盖 listeners、SSH/accounts、route/rule/interfaces、nftables、Docker/1Panel、更新、磁盘、服务和 `1panel-network`；第二 SSH 会话与 sudo/服务/网络不变量通过。
- [x] 20 分钟 systemd 自动回滚在变更前启用；旧 SSH drop-in、authorized_keys、项目防火墙文件精确封存。全新会话与外部验证通过后才取消并删除回滚材料。
- [x] SSH 仅 `ubuntu` publickey；root/password/keyboard-interactive 和不必要 forwarding/tunnel 关闭；当前钥匙接受，password-only、root key 与旧钥匙拒绝；只允许 loopback TCP `188` local tunnel。
- [x] `inet xs_nexus_host_guard` 默认 drop；不 flush/修改 Docker、1Panel 或其他 nftables 表。外部 TCP `80`/`122`/`443` 开放，TCP `188`、数据库、loopback 应用及 TCP discovery/relay 关闭或过滤。
- [x] firewall service restart、默认 route、IP rule、非项目 nftables、protected container ID、OpenResty、四个项目容器、`1panel-network` 和 failed units 前后通过；回滚取消后再次验证。
- [x] 最终证据 117 文件和应用证据 56 文件 SHA-256 通过；无值秘密扫描 0 findings；验证器工具缺陷单独 disposition。
- [x] 受保护升级：新数据库备份、157 个旧包重打包/验签、关键配置备份、三 SSH 会话和 60 分钟自动降级先就绪；157 个升级和 9 个依赖全部安装，独立验证 0 pending upgrade、空 `dpkg --audit`。
- [x] 受保护 reboot：新 `6.8.0-137-generic` one-shot、旧 `6.8.0-124-generic` saved fallback、内部健康和 Boot-ID 外部批准 watchdog 通过；最终移除临时 GRUB/unit，首个内核为新版本且无 reboot-required。
- [x] 重启回归：SSH、host firewall、默认 route、IP rule、Docker 规则严格语义、published ports、受保护 container ID、OpenResty、四项目容器、`1panel-network`、外部端口、当前/旧/root/password 认证和受限 tunnel 全部通过。
- [x] 有界清理：仅删除精确项目 build cache、157 个验签 rollback 包、临时配置归档、APT 下载和 `dpkg-repack`；不运行 global prune/autoremove；根分区由 `83%` 降至 `77%`，约 `14.15 GB` 可用。
- [x] 维护证据 218 文件 SHA-256 复核和无值秘密扫描 0 findings；所有失败尝试与纠正 disposition 保留。
- [ ] Gate 14 唯一残余：外部 warning/critical 磁盘告警未真实送达和 on-call 确认，状态 `BLOCKED_EXTERNAL`；Gate 14 仍为 `PARTIAL`。

## 20. Gate 13 计划域名 strict TLS（2026-08-09）

- [x] 只读 DNS/边缘：计划域名解析到 EdgeOne；边缘 TLS 1.3、证书链和主机名验证通过。
- [x] 失败复现：公网 `/`、`/health/ready`、`/install` 与 `/v1/control` 均为 HTTP `525`；直连源站并发送计划域名 SNI 时 TLS `unrecognized_name`，不产生 HTTP 响应。
- [x] 源站定界：无计划域名 vhost/证书；现有证书仅覆盖 `qinwen.co`；现有 public root 指向 Controller `28080`，而 Console `28081` 的 root/health/WebSocket 控制验证通过。
- [x] 工具回归：严格 TLS 最低版本、证书/主机名验证、SNI、精确 HTTP status、完整 WebSocket accept、畸形 header、非法输入和证书失败关闭均通过。
- [x] 模板源码门禁：HTTP 308、TLS 1.2/1.3、证书占位符、HSTS、Console-only loopback、WebSocket header 存在；TLS 1.0/1.1、Controller root upstream 和禁用验证标记被拒绝。
- [x] 证据完整性：只读根和精确提交工具根均无生产变更；后者 8 文件 SHA-256 与秘密扫描通过，路径记录在 V2 `EVIDENCE_INDEX.md`。
- [ ] 生产修复：所有者批准证书、计划域名 vhost、CDN Origin Host/SNI/strict mode，并完成 direct-origin、CDN、浏览器、登录、认证 API、WebSocket、Console 与未知路由 E2E。完成前 Gate 13 保持 `FAIL`。

## 21. Gate 23 缺陷与供应链回归（2026-08-09）

- [x] JSON 拒绝回归：畸形 JSON、未知字段、错误类型和超限 body 只返回精确通用 400 envelope；响应不含 parser category、字段名、line/column 或输入片段。
- [x] PostgreSQL 日志回归：生产容器精确 `json-file`、`max-size=10m`、`max-file=5`；加密备份、回滚演练、独立复核、容器身份、路由、nftables 和 `1panel-network` 不变量通过。
- [x] Rust/SQLx 回归：Rust `1.94`，SQLx `0.9.0`，动态 migration/schema SQL 使用显式 `AssertSqlSafe`；格式化、严格全 target/feature Clippy、workspace all-feature tests 和真实 PostgreSQL 最小权限测试通过。
- [x] 依赖门禁：plain `cargo audit` 无 vulnerability 且 ignore 为空；`rsa` 不在 lock；`cargo deny --all-features check` 的 advisories/bans/licenses/sources 全通过。`paste` 未维护提示保持可见并由 `KI-026` 限时跟踪。
- [x] 前端与 CI：npm clean install、lint、unit、build、high audit 通过；GitHub Actions run `31313868529` 的 baseline、Console real E2E、protocol fuzz 和 image reproducibility 四个 job 全通过。
- [x] 全量证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-final-20260809T134042Z` 的全部 status 为 0，manifest 复核通过，源风险与仓库/历史秘密扫描为 PASS。
- [x] 失败证据：九个失败 root（API 三个、全量/空间六个）保留原文件，新增 `FAILED_NON_AUTHORITATIVE` disposition 和非覆盖 manifest；独立校验根 `/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate23-evidence-seal-verification-20260809T141601Z` 通过。
- [x] 宿主恢复：生产健康、失败服务 0、临时 QA 容器/网络/namespace 0，默认网关 `10.3.0.1`/`eth0` 和 `1panel-network` ID/子网不变；运行 revision 仍为 `3d93656`。
- [ ] 结论边界：这些测试只关闭四个可自行修复 finding，不关闭外部 Critical/High、正式 release、真实 Windows/NAS/WAN、异地恢复或第三方安全审计，Gate 23 保持 `FAIL`。

## 22. Gate 24 依赖拓扑复核（2026-08-10）

- [x] 精确源码：从完整 Git bundle 分离 checkout `45dbc19690f6f738a43cabf2c344118d0b8bc055`，工作树干净且 source bundle SHA-256 已记录。
- [x] 原始工具：`cargo audit --json` 为 0 vulnerability、空 ignore；`cargo deny --all-features check` 的 advisories/bans/licenses/sources 全通过；期限策略脚本通过。
- [x] 拓扑：精确证明 `paste 1.0.15 <- netlink-packet-core 0.8.2 <- rtnetlink 0.21.0 <- xs-agent`，没有用 feature 裁剪隐藏路径。
- [x] 上游：保存 `rtnetlink` 与 `netlink-packet-core` `main` 的精确 commit、原始 manifest 和摘要；当前上游仍未移除 `paste`。
- [x] 处置：`KI-026`/ADR-089 保持告警可见、禁止 cargo-audit ignore、固定 `2026-08-31` 自动失效和变化触发重审，不引入私有网络栈 fork。
- [x] 宿主保护：容器、Docker 网络、`1panel-network`、默认路由、IP rule、规范化 nftables、failed units 前后不变；临时 QA 容器与工作树清零。
- [x] 证据：`/srv/xs-nexus-qa/artifacts/production-readiness-remediation-v2/gate24-dependency-topology-20260809T170012Z` root-only 封存，`SHA256SUMS` 独立复核通过。
- [x] CI Action 回归：全部远程 action 使用 40 位 commit SHA；checkout/setup-node/upload-artifact/Buildx 固定为已复核 Node 24 commit 与版本标签；checkout 全部关闭 credential persistence。负向 fixture 覆盖 mutable/旧 SHA/错误标签/畸形引用/缺失 credential hardening。
- [x] 最新实现提交 `98ca145c197b1d2af20d7cf5df28008820a25e50` 的 GitHub Actions run `31325753985` 四项 job 全通过，四个 check-run annotation 数均为 0。
- [ ] 结论边界：`KI-021`、正式签名发布、精确 revision 部署和第三方审计仍开放，因此 Gate 24 为 `PARTIAL`、总体为 `NO_GO`。

## 23. Gate 04 XSP/1 状态机与 Fuzz（2026-08-10）

- [x] 丢失 KeyUpdateAck：首次 KeyUpdate 被处理、Ack 丢失后，重试保持 payload/Epoch 但使用新序列；接收方通过旧 Epoch 接受并再次确认，发起方只在匹配 Ack 后切换发送 Epoch。
- [x] 丢失 PathResponse：首次 Challenge 被处理、Response 丢失后，重试保持 Path ID/token 但使用新序列；匹配新 Response 后完成探测，旧 Challenge/Response 密文重放继续失败。
- [x] 丢失 ServerFinish：重复 ClientFinish 命中摘要绑定的精确 ServerFinish 缓存；不会重新加密固定 nonce Finish，不改变 Established 状态，也不授权路径迁移。
- [x] 重试耗尽：KeyUpdate 有界尝试耗尽返回完整重握手动作，清除 pending update，不静默开始另一轮相同 Epoch 更新。
- [x] 协议变更纪律：规范、密码学设计、威胁模型、兼容性 ADR、测试向量元数据、Fuzz target 和 corpus 同步；线格式、版本和既有字节向量不变。
- [x] 精确 revision `875395352afc8a17dafc17b2496d62ce701ee4ae` 的 run `31350065978`：格式、严格 Clippy、构建、全量测试、真实数据库/namespace、Console E2E、五镜像双无缓存复现及六目标 AddressSanitizer Fuzz 全通过。
- [x] Fuzz 证据：六目标各 180 秒，总计 134,262,706 次；525 项 SHA-256 复核通过，artifact 秘密扫描 0 findings/0 incomplete。
- [ ] 外部边界：Gate 04 内部门禁为 `PASS`，但 Gate 05 第三方独立审计仍为 `BLOCKED_EXTERNAL`，不得从内部 Fuzz 推导生产密码学安全认证。

## 24. Gate 06 Linux Agent 恢复矩阵（2026-08-10）

- [x] 真实 systemd 策略：transient unit 使用 `Restart=on-failure`；`SIGKILL` 后必须出现且仅出现一个不同主 PID，重启计数精确为 1。
- [x] 状态与 TUN：签名状态跨崩溃保留；非持久 TUN 在服务私有 network namespace 重建，宿主 namespace 始终无同名接口。
- [x] 网络变化：私有 namespace 内链路 down/up 后 Agent 继续运行；随后正常停止并完成可信 cleanup。
- [x] 沙箱真实性：Agent 从独立测试安装根执行，`ProtectHome=yes` 保持启用；生产安装路径不被创建或修改；正式 unit 在隔离 root 中验证。
- [x] 精确证据：revision `fb45fd43256d65cb4c72824d6cee0bec0884ad02` 的 run `31352258781` 五 job 全通过；artifact `9049384061` 归档/内部 SHA-256 与无值秘密扫描通过。
- [x] 失败链：runs `31351583134`、`31351658402`、`31352027606` 分别保留 ShellCheck、受保护 home 执行和 host-path unit verify 缺陷，没有跳过或弱化测试。
- [ ] 实机边界：普通部署主机整机重启、disk-full、DHCP/address churn、竞争 VPN 路由、重复失败/start-limit 与 arm64 硬件仍未验证；Gate 06 为 `PARTIAL`，不得判定生产通过。

## 25. Gate 09 ACL 强制执行矩阵（2026-08-10）

- [x] 三节点拓扑：三个真实 Linux Agent、TUN 和 namespace 在 Controller 不可用时保持 `network_active` 并使用相同最近签名版本；测试前后均确认离线状态和版本未漂移。
- [x] 允许矩阵：A→B 的 ICMP、指定 TCP 和指定 UDP 双向业务成功。
- [x] 拒绝矩阵：A→C 与 C→B 的 ICMP/TCP/UDP、A→B 异常 TCP/UDP 端口、C 伪造 A 虚拟源、C 经允许 A 身份路由均在发送端失败，抓包无匹配 XSP/1 数据帧。
- [x] 双端执行：两个 B-only deny 端口由发送方产生认证 XSP/1 密文，但 B 的 TUN 不产生对应 TCP/UDP 明文。
- [x] 身份边界：协议测试分别篡改认证帧的源 Node ID、目标 Node ID、路径头和虚拟源地址，全部拒绝；随后原有效帧仍可接受。
- [x] 版本边界：低外层版本、同版本异内容、无效签名、无效 ACL、较高外层版本包裹较低 `policy_version` 全部拒绝且当前状态不变。
- [x] Relay 绕过：认证 Relay 活动时，被拒绝 UDP 不产生匹配 XSR/1 Data 且不抵达 B；既有允许流量、故障切换、重启、重新注册和 Direct 回切继续通过。
- [x] 子网绕过：审批路由存在时，被拒绝 TCP/UDP 不产生匹配 XSP/1 帧且不抵达 LAN 服务；允许 ICMP/TCP、NAT、伪造拒绝、网关离线和 route/nftables cleanup 继续通过。
- [x] 精确证据：revision `e908e67d6d745f91ef44b1f5c1613d1b5e3cad3b`，run `31360862865` 全七 job，ACL job `93369332314`，artifact `9052383034`；归档 digest、七个下载文件 SHA-256 和无值秘密扫描通过。
- [x] 结论边界：Gate 09 为 `PASS`；真实 WAN、真实 NAS/subnet router 与独立安全审计由其他 Gate 保持原状态，总体仍为 `NO_GO`。

## 26. Gate 15 1Panel 共存矩阵（2026-08-11）

- [x] 当前源码：application/edge Compose 仅声明 exact external `1panel-network`，全部七个服务连接；driver/IPAM/subnet ownership、privileged、host network、Docker socket 和额外 managed network 均被 validator 拒绝。
- [x] 生命周期源码：global prune、network create/remove/prune/connect/disconnect、unscoped Docker remove、volume delete 和 unscoped Compose down 均失败关闭；正式 stack 只允许两个 project-scoped `down --remove-orphans` 路径。
- [x] 渲染：私有无秘密 fixture 完整插值三个 profile 和 edge，`docker compose config --format json` 的 service/network set 与受审集合精确相等。
- [x] 项目 restart/down：非 Compose sentinel 存活，external network exact ID 不变，证明 project-scoped lifecycle 不删除网络或未知容器。
- [x] Docker restart：真实 daemon restart 后 external network exact ID、sentinel exact ID/running 和 attachment、默认路由保持。
- [x] Cleanup：只删除当前 run label 的 sentinel/network；最终 stable network inventory 和默认路由与基线相同，最终 PASS 仅在 cleanup 后输出。
- [x] 真实生产：Gate 14 Boot-ID host reboot 与 Gate 16 project upgrade/四次 automatic rollback 证明 1Panel/OpenResty/网站、数据库边界、SSH、route/rule、non-project nftables、protected containers 和生产网络 ID/subnet 保持。
- [x] 失败链：runs `31364684902`、`31366046340`、`31367632436`、`31504209932` 及四个 artifacts 保留；没有 skip、allowed failure、suppress 或弱化目标 exact-ID 断言。
- [x] 精确证据：revision `8a9174866ebdf4ff76e7d987e006acb64312e3b6`，run `31504402285` 全八 job，job `93822197946`，artifact `9106406005`；archive/inner SHA-256 和无值秘密扫描通过。
- [x] 结论边界：Gate 15 为 `PASS`；当前分支未部署，正式签名、计划域名、凭据轮换、外部设备与总体 Production Ready 状态不变。

## 27. Gate 18 更新与发布供应链矩阵（2026-08-12）

- [x] 统一契约：Core、Agent、Controller、构建器、安装器和一键引导消费 schema 2 十二字段清单；来源、epoch、协议、平台、架构、target、归档 identity/size/hash 全绑定。
- [x] 边界：版本组件为规范 `u32`，epoch 为规范正 `u64`，归档不超过 512 MiB；错误边界由构建器和消费者共同拒绝。
- [x] 密钥与撤销：一至四把唯一 PEM 公钥支持 old+new 重叠；未知/重复/退役/无签名/篡改输入失败关闭；Controller 单向撤销与 root-owned ledger 双重执行。
- [x] 故障矩阵：中断、截断、超限、写失败、真实 tmpfs ENOSPC、错误平台/架构、来源/归档漂移、篡改、旧版、撤销目标和激活失败均不发布候选。
- [x] 回滚与状态：显式回滚重新验证 metadata/撤销，撤销目标在执行任何已安装候选前拒绝；失败保留当前版本、节点身份、签名状态、服务状态和路由。
- [x] 网络与控制台：真实独立 namespace 证明路由只在活动期存在且无残留；Console 撤销使用固定原因、不可逆确认、状态/过滤，不存在私钥上传面。
- [x] 失败链：八个失败 runs/artifacts 全保留，ShellCheck、fixture、installer、rollback ordering、Relay convergence、Clippy 与 SQLx 根因均修复，没有 skip、suppress、allowed failure 或阈值下降。
- [x] 精确证据：candidate `b8cd49cf2be401cfe3b2d289a8cd1a50c3cc5bb1`，run `31515281011` 全九 job，job `93858773221`，artifact `9110864186`；archive/inner SHA-256 和无值秘密扫描通过。
- [ ] 外部门禁：正式离线密钥仪式、认证生产分发、签名 RC、真实 aarch64/NAS/Windows 与生产 release/rollback 链尚未执行。
- [x] 结论边界：仓库侧子矩阵通过，但 Gate 18 保持 `PARTIAL`，生产未变且总体仍为 `NO_GO`。

## 28. Gate 19 真实 Console 与视觉矩阵（2026-08-12）

- [x] 真实运行时：独立 PostgreSQL schema、真实 Controller binary、生产 Console build/preview 和 Chromium；Redis 不在当前部署数据路径中，未用 Redis/API Mock 替代。
- [x] 无拦截：`tests-real` 源码守卫拒绝 `page.route`、`context.route` 和 HAR；所有 `/v1` 交互到达真实 Controller。
- [x] 状态与初始化：真实 stop/resume loading、错误登录 401、正确登录、空状态、全部初始页面、网络/Token/双节点/审计员和审计事件通过；快速双提交只产生一个网络。
- [x] 安全边界：ACL 默认拒绝、malformed JSON 400、缺 CSRF 403、未知端点 404、双标签页旧配置 409、审计员无写 UI 且直接绕过 403、注销后 session 401。
- [x] 破坏性操作：取消节点吊销发送 0 请求，确认发送恰好 1 请求；校验 canonical 200 body、冷却、配置版本与双区域 revoked 状态。
- [x] 故障恢复：真实浏览器 offline 后旧数据替换为明确错误，只有 Console/users 两项请求报告断网；恢复网络和 reload 后真实 dashboard 返回。
- [x] 视觉覆盖：`1920x1080`、`1440x900`、`1280x720`、`1024x768`、`768x1024`、`390x844`；每视口覆盖登录、16 管理页、节点 dialog 和 404，共 132 张截图。
- [x] 可访问性与布局：document 无整体横向溢出，宽表在 table-shell 内滚动，交互控件有名称，skip-link 可由键盘聚焦并把焦点移到主内容。
- [x] 浏览器观测：8 expected、0 unexpected/skipped/flaky；14 个观测集、0 page error、0 HTTP 5xx，只有显式 offline 的两个预期 request failure。
- [x] 证据安全：real traces 关闭，生成凭据位于 evidence 外；下载 artifact 不含 trace/video/凭据表面；无值扫描 0 findings。
- [x] 精确证据：revision `5505893710ab1d15e06495603dff08bf5c1e035f`，run `31529393933` 全九 job，job `93905489516`，artifact `9116327161`；archive digest 和 139/139 内部文件 SHA-256 通过。
- [ ] 公网硬门禁：计划域名严格 origin TLS/SNI、CDN、公开 Console/API/WebSocket、外部浏览器和当前分支生产部署未执行。
- [x] 结论边界：内部生产形态子矩阵通过，但 Gate 19 保持 `PARTIAL`、Gate 13 保持 `FAIL`、总体保持 `NO_GO`。

## 29. Gate 20 生产可观测性与告警矩阵（2026-08-12）

- [x] 遥测契约：Agent schema 1 原签名兼容，schema 2 增加 ACL/重放累计值；Relay 分类累计值验签、单调和持久化；Controller 聚合不返回主体标识、地址、端点、载荷或秘密。
- [x] 真实数据层：专项 CI 使用真实 PostgreSQL 完成节点注册、Agent/Relay 遥测、管理认证、聚合、新鲜度、完整性、Redis `not_applicable` 和负向边界。
- [x] 主机采集：CPU、内存、网络、FD、任务、磁盘、inode、受限日志、Docker 状态/stats、HTTP health、Controller、TLS、备份年龄及两类回执均有失败关闭路径。
- [x] TLS/文件安全：真实本地证书链与 hostname/SNI 通过；URL 凭据/query/fragment、非 loopback HTTP、链接/宽权限/异主私有文件、未来/陈旧/错误回执均拒绝。
- [x] 告警生命周期：warning firing、同严重度去重、warning→critical、critical 非零、resolved、通知失败重试和 256 项队列上限通过。
- [x] 外呼边界：Controller/Webhook 禁用环境代理和重定向；正式 Webhook 只允许 HTTPS；两个令牌分离且不进入 URL、环境示例、输出、日志或证据。
- [x] systemd：hardened oneshot/timer 通过离线 `systemd-analyze verify`，四分钟超时、空 capability、私有目录和内核/命名空间保护保持。
- [x] 失败链：`31535053669`、`31537181594`/`9119262890`、`31537531579`/`9119401167` 完整保留；没有删除失败、allowed failure、skip、suppress 或断言弱化。
- [x] 精确证据：revision `9291400ac030045e8ea2955ea137e7dc8be37a85`，run `31537716553` 全十 job，job `93932721320`，artifact `9119468792`；archive/两层 SHA-256、场景矩阵和两次独立无值扫描通过。
- [ ] 生产送达：独立目的地 warning/critical、on-call acknowledgement、故障解除后的 resolved closure、正式 TLS/备份持续采样与留存仍为 `BLOCKED_EXTERNAL`。
- [x] 结论边界：仓库侧子矩阵通过，但 Gate 20 保持 `PARTIAL`，生产未变且总体保持 `NO_GO`。

## 30. Gate 21 性能与容量矩阵（2026-08-12）

- [x] 1,000 节点通过真实 Controller Router、Enrollment、认证、签名配置、IPAM 和 PostgreSQL 注册；不使用批量数据库插入代替产品路径。
- [x] 1,000 个真实 WebSocket 完成认证和大配置同步；1,001st Enrollment 与控制会话分别在凭据分配和 upgrade 前失败关闭。
- [x] `chunked-v1` 覆盖 canonical base64、transfer ID、顺序、数量、长度、精确 chunk size、SHA-256、嵌套拒绝、错误清理与旧客户端大消息拒绝。
- [x] Controller 限制为 1,000 节点、1,000 会话和 64 并发配置发送；资源、速率、p95、DB 查询/连接、Console 和 presence 均有明确阈值并通过。
- [x] XSP/1 release 吞吐与两个 release Agent 的真实 namespace Direct/Relay RTT、CPU、RSS、线程和 FD 完成。
- [x] 失败链 `31550700862`、`31551651020`、`31552305120`、`31558236452`、`31602351182`、`31603038239` 保留；cleanup false-green、retained frames 和 strict Clippy 根因均修复。
- [x] 精确 revision `f4a39c2c74b6f75e6f5284cff1b8599de9aeb363`，run `31603852656` 全 11 job，job `94137661764`，artifact `9144433450`；外层 41/41 与三份内部 manifest、revision 绑定及零发现秘密扫描通过。
- [ ] 公网、多地域、多实例、volumetric/distributed abuse、小时/天级容量仍为 `BLOCKED_EXTERNAL`。
- [ ] Gate 22 当前 revision 至少 24 小时采样及完整故障注入尚未执行，状态保持 `UNKNOWN`。
- [x] 结论边界：Gate 21 仅内部单实例子矩阵完成，仍为 `PARTIAL`；生产未变，总体保持 `NO_GO`。

## 31. Gate 22 当前 revision 长测校准（2026-08-13）

- [x] harness 将 600 秒校准与至少 86400 秒正式模式显式分离；校准成功不能生成 Gate PASS。
- [x] 六服务采样覆盖 CPU、RSS、线程、FD、连接、日志、路由/规则/接口、Direct/Relay 状态与错误计数；计划重启按进程代际分析，PostgreSQL 同时保留 FD 2048 和连接 32 的全局硬上限。
- [x] Controller、Relay、PostgreSQL、Agent 重启，配置更新、Enrollment/Revocation、受控 loss/latency 与 subnet-route 更新八项事件均实际执行并 PASS。
- [x] 并发路由更新只对真实 `409` 重新读取版本后有界重试；每次尝试留证，其他 HTTP 状态不重试。
- [x] 该阶段 exact revision `6e63424298e491c0035e1d138de5d03b0ab83c27` 的 run `31649030446` 全 13 job，Gate 22 job `94289075250`，artifact `9162201059` 和 GitHub digest `sha256:73918119786fe5d25dcceb7f8cdb9509ff3a1ac5439780e1972c4201e8d08e59` 通过。
- [x] 独立下载校验 84/84 文件、零秘密发现、264 个服务样本、九类宿主不变量逐字节一致；无 namespace、TUN、容器、网络或卷残留。
- [ ] 至少 24 小时的精确 revision 正式运行尚未执行；当前只有 600 秒校准，Gate 22 保持 `UNKNOWN`。
- [ ] 正式运行环境为 `BLOCKED_EXTERNAL`：生产主机变化后的 SSH 指纹尚未带外确认，且没有另一台已批准特权 Linux QA 主机；不得绕过主机身份验证。
- [x] 结论边界：生产未连接或修改，总体保持 `NO_GO`。

## 32. Gate 25 干净发布与部署演练矩阵（2026-08-13）

- [x] 完整 Git 历史 bundle、detached clean checkout、精确 revision/tree、空工作树和 `git fsck` 通过；浅克隆输入失败关闭。
- [x] 临时 SSH 签名 tag 验证通过且证据明确标记 test-only；不冒充 owner-controlled 正式 RC。
- [x] 五个 runtime 镜像执行两轮 no-cache OCI 构建并逐镜像匹配；workspace 从 clean checkout 重新构建。
- [x] PostgreSQL 密码在仓库外生成，bootstrap/application 分离；等待最终 init marker、`pg_isready` 与真实 `SELECT 1` 后才继续，不接受临时初始化服务。
- [x] 真实 namespace Direct ACL、Relay fallback/recovery、子网路由与签名更新失败矩阵通过；首次安装和重装两次 Docker 生命周期通过。
- [x] 10/10 阶段、10/10 夹具不变量和 10/10 最终不变量 PASS；容器、网络、卷、默认路由、规则、链路、namespace、nftables、failed services 和 `1panel-network` 无残留。
- [x] exact revision `6e63424298e491c0035e1d138de5d03b0ab83c27`、run `31649030446` 全 13 job、Gate 25 job `94289075249`、artifact `9162246723`、GitHub digest `sha256:d69d7ff0d24195f629a5032152b1c1851f0cab8d79048ae90ca00af5fa9659fe`；111/111 清单和零秘密扫描通过。
- [x] 六轮失败链全部保留并按根因修复；没有 skip、allowed failure、阈值降低、全局 Docker prune 或生产变更。
- [ ] owner-controlled signed RC/main、独立操作者、新鲜非托管主机、正式 secrets、当前生产升级/回滚/恢复发布尚未执行，均为正式硬门禁。
- [x] 结论边界：仓库子矩阵完成，但 artifact 明确 `formal_signed_rc=false`、`independent_operator=false`、`production_mutation=false`；Gate 25 保持 `FAIL`，总体保持 `NO_GO`。

## 33. Gate 02 凭据轮换外部边界复核（2026-08-13）

- [x] `CREDENTIAL_ROTATION.md` 不记录任何值，逐类列出 rotated、old rejected、证据和状态；新建 app/migrator 身份与旧身份不存在的边界明确。
- [x] bootstrap 已从长驻 Controller runtime 移除，SSH password path 已禁用，仓库/证据 secret scanner 与仓库外 secret 规则通过；这些缓解不等于轮换完成。
- [x] 每个未完成条目均准确标记 `BLOCKED_EXTERNAL`，并由 `BLK-011` 映射到 owner inventory、secret channel、云/NAS/CI/设备权限、维护窗口和独立验证者。
- [x] 未尝试历史聊天凭据、未绕过变化后的 SSH host key、未使用生成测试值冒充正式新值、未把“服务未使用”当作旧值拒绝。
- [ ] 所有者激活每个真实新值并从独立主体证明旧值拒绝尚未完成；Gate 02 保持 `FAIL`，`PRV2-001` 保持 `BLOCKED_EXTERNAL`。

## 34. Exact-head Relay 双端收敛回归（2026-08-13）

- [x] 失败保留：revision `f4c87074a65cf8e8b8df41664700a3f81131a40c` 的 run `31652099523` 为 12/13；Relay job `94298488921` 与 artifact `9163073115` 保留首个 failover ICMP 超时和完整进程日志。
- [x] 根因验证：旧 harness 只等待发送端备用路径；修复在 primary、backup、restart 和 Direct 四个边界同时验证两个 Agent 的 exact endpoint、允许原因和 established session。
- [x] 断言保持：产品故障/保活定时器、每端路径等待上限、ICMP 成功、ACL 绕过拒绝、Relay 密文、指标和 namespace/进程清理均未放宽。
- [x] 精确回归：revision `c0c059e84806f70586f37ca3f3bb33cdd602c4a4` 的 run `31653564044` 全 13 job 通过；Relay job `94302909276`、artifact `9163588782` 外层 8/8、容量层 4/4 与零秘密扫描通过。
- [x] 受影响 Gate 25：artifact `9163844194` 通过 110/110 外层、8/8 嵌套清单、10/10 阶段与两组 10/10 不变量。
- [x] 结论边界：这是 hosted 仓库回归，不是正式独立发布或生产演练；Gate 25 保持 `FAIL`，总体保持 `NO_GO`。

## 35. Exact-head 路径恢复与性能稳态回归（2026-08-13）

- [x] Gate 25 接收端分类失败保留：run `31654905139`、job `94307126471`、artifact `9164303385`；只接受精确主 Relay established session 的合法 `relay_failover`，不接受任意端点或未认证会话。
- [x] Gate 22 配置更新恢复失败保留：run `31655990346`、job `94310433905`、artifact `9164596254`；两端版本应用后必须再恢复精确 established Direct 路径，随后原 ping 仍必须成功。
- [x] 性能稳态失败保留：run `31657413129`、job `94314821959`、artifact `9164999014`；Direct 正式平均/p95 `7.242`/`39.441` ms 严格失败，没有把异常 rerun away。
- [x] 稳态门控：最多 12 轮、每轮 10 包并逐文件留证；每轮前后验证双端精确 established Direct，warm-up p95 必须 ≤10 ms，正式 100 包平均/p95 仍必须 ≤5/10 ms。
- [x] 最终精确回归：revision `795b1ea461a179958aed27e6935faba8f36e43ce`、run `31658778589` 全 13 job 通过；性能/Gate 22/Relay/Gate 25 jobs 为 `94318968086`/`94318968217`/`94318968289`/`94318968293`。
- [x] 独立证据：四个 artifact `9165514010`/`9165687987`/`9165498146`/`9165661918` 分别通过 72/84/12/118 个 SHA-256 清单项、精确 revision/status 和零秘密扫描；九类 soak 宿主不变量及 Gate 25 两组各 10 个不变量逐字节一致。
- [x] 结论边界：600 秒校准不满足 ≥86400 秒；test-only hosted rehearsal 不满足正式签名、独立操作者和生产变更。Gate 22=`UNKNOWN`、Gate 25=`FAIL`、总体=`NO_GO`。

## 36. Docker 端口负向夹具回归（2026-08-13）

- [x] 失败保留：revision `999b420cbf274e47bf28d3a4f86600e4f81f72c4`、run `31660307047` 为 12/13；Gate 25 job `94323554733`、artifact `9166162670` 保留端口夹具未确认 listener readiness 的失败。
- [x] 夹具真实性：启动前固定端口必须空闲；启动后精确 PID 必须存活且 `ss` 必须观察到监听；超出 20×100ms 有界等待即失败，不执行产品预检。
- [x] 产品断言不变：已确认占用端口时 `preflight` 仍必须拒绝；没有跳过负向测试、换随机绿色端口、增加 allow-failure 或放宽部署条件。
- [x] 最终精确回归：revision `67152427acc197deca49443a8527c74b18d71098`、run `31661323846` 全 13 job 通过；Gate 25 job `94326567391`、artifact `9166570284`、digest `sha256:b19a27070227992de3c5440fae4377d67e58351f4b012fdd9b3129fdee954825`。
- [x] 最终独立证据：性能/Gate 22/Relay/Gate 25 artifacts `9166384854`/`9166550402`/`9166363481`/`9166570284` 分别通过 72/84/12/118 个清单项、精确 revision/status 和零秘密扫描；所有相应不变量通过。
- [x] 结论边界：本项只关闭测试同步缺陷；Gate 22=`UNKNOWN`、Gate 25=`FAIL`、总体=`NO_GO`。

## 37. 原生 Windows Agent/CLI 构建门禁（2026-08-13）

- [x] 固定 `windows-2025` 原生 runner 和 `x86_64-pc-windows-msvc` host；非原生 host 或脏 tracked checkout 失败关闭。
- [x] release 构建 `xs-agent.exe`、`xs.exe` 和六个 Windows 边界 crate；执行 51 个 Agent 测试、23 个边界测试、CLI 测试与严格 Clippy。
- [x] CI 验证器要求 Windows job、固定 runner、Rust 1.94.0、专项脚本和 artifact 上传，负向夹具拒绝删除或漂移。
- [x] 独立实现扫描器只对精确 Windows CI 编排脚本允许 Wintun 测试引用；其他脚本和非适配器 runtime 路径仍拒绝，WireGuard 边界未扩大。
- [x] 失败链保留：run `31666007136` 的 Windows job 通过而 baseline job `94340603963` 因误分类失败；未跳过扫描器或允许 baseline failure。
- [x] 最终精确代码 revision `16ee4bf7be4687f2ac307c7a1235f7d92275e855` 的 run `31667104728` 全 14 job；Windows job/artifact `94343940285`/`9168434013`。
- [x] 独立证据通过 12/12 文件哈希、revision/PASS/native-host、二进制哈希、测试摘要和仓库扫描器；GitHub digest 为 `sha256:473fedb5ec6c01c6a51e6149de522665bb94c0b0cf87c4f22472b35f8bcaa2ca`。
- [ ] 未安装设备、未运行 Driver Verifier、未执行在线 Agent/SCM/路由/睡眠/崩溃/升级/卸载矩阵；Gate 11 保持 `BLOCKED_EXTERNAL`。

## 38. 正式门禁精确 HEAD 夹具回归（2026-08-13）

- [x] 失败链保留：revision `72d7e5328a1f1ef5f4387521f5cbabcf4a2e23d2`、run `31670477509` 为 12/14；update job/artifact `94353856787`/`9169529635` 与 ACL job/artifact `94353856911`/`9169570590` 保留原始失败。
- [x] 错误签名确定无效：负向夹具使用另一把真实 Ed25519 私钥对相同 manifest 签名，不再依赖可能 no-op 的单字节覆盖；目标公钥验签仍必须拒绝。
- [x] nftables 结构化语义：从 `nft -j` 验证精确 table/chain/type/hook/priority/iif/oif/IPv4 destination/masquerade；错误 table、chain、type、接口、目标和缺少 masquerade 均失败。
- [x] 产品边界不变：签名校验、ACL、NAT、子网路由、流量和清理断言未跳过、降级、增加 allowed failure 或改为模拟。
- [x] 最终精确代码回归：revision `a44d868c26e21285565fa794d482b8092c0bbbf4`、run `31671264821` 全 14 job 通过；update/ACL jobs 为 `94356192731`/`94356192857`。
- [x] 独立证据：13 artifacts、18 manifests、1,027 SHA-256 条目、精确 revision/status 和整包秘密扫描通过。
- [x] 结论边界：本项只关闭夹具缺陷；正式计数不变，总体严格保持 `NO_GO`。
