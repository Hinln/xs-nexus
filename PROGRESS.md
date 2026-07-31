# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-31 00:35 UTC
当前 Git 提交：M6.1 ABI/架构中间检查点准备中（以本文件所在提交为准）
当前总状态：`ACTIVE_AUTONOMOUS_DEVELOPMENT`
当前里程碑：`M6.1 Windows 驱动设计和构建`

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
- 管理快照接入真实网络、节点、Token、组、ACL、子网、Relay、拓扑、用户、审计、告警和系统能力，任何未接入路径/流量/延迟/健康指标均显式“未采集”；
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
- `make test-windows-xsnet-abi` 已在 Clang 21 Release `-Werror` 和 GCC 15 ASan/UBSan 两套配置实际通过 ABI 与会话测试；该结果不代表 WDK 构建或 Windows 实机通过。
- 已加入 Windows 11 24H2 x64 UMDF 2.33 / NetAdapterCx 2.5 WDK 工程、仅 LocalSystem INF、独立 host、安全 IOCTL、file object、PnP/power、adapter 与断链 packet queue 骨架；`make test-windows-xsnet-source` 实际通过，但没有 WDK 编译证据。

## 当前工作点

- M5.2 已完成全部计划内实现与全量验证，部署、迁移、备份、恢复和回滚均有实际证据；
- 宿主既有 PostgreSQL/Redis 公网暴露仍由 `BLK-005` 阻塞，项目没有修改 1Panel 或生产防火墙；
- M6.1 已完成 ABI、会话状态机和首轮 UMDF WDK 源码骨架；SetLink/TX/RX 当前显式失败关闭，下一步实现实际 ring 与 direct-I/O 请求配对；
- 开发服务器已确认仅有 `clang-cl`、CMake 和 Ninja，没有 WDK、MSBuild、Windows SDK 或 VM；`BLK-001` 继续阻塞真实驱动构建、测试签名和实机验收；
- 真实 Windows VM、正式驱动签名和日常 Windows 电脑保持人工门禁，不伪造实机结果。

## 下一步

1. 设计并实现 NetAdapterCx ring 与 direct-I/O 请求的有界配对、backpressure 和一次完成；
2. 增加取消、cleanup、睡眠和 Agent crash 的并发模型测试；
3. 补充 Windows Agent ABI 客户端与安装/卸载脚本；
4. 准备 WDK 测试签名命令和 VM 验收脚本，但不声称 Linux 能编译驱动；
5. 获得 Windows VM 后执行 WDK、InfVerif、安装、Driver Verifier 和异常生命周期验收。

## 下一条准确命令

```bash
sed -n '533,570p' EXECUTION_PLAN.md
rg -n 'Windows|driver|xsnet|IOCTL|WDK|Verifier|签名' IMPLEMENT.md ACCEPTANCE.md QA_MATRIX.md SECURITY_REVIEW.md BLOCKERS.md
find drivers installers/windows -maxdepth 4 -type f -print 2>/dev/null | sort
command -v clang-cl || true; command -v x86_64-w64-mingw32-gcc || true
```

## 最近测试

- 时间：2026-07-30 23:35 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`./scripts/validate-m52.sh`；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`；
- 覆盖：格式化、Clippy、构建、全量单元/集成测试、真实 PostgreSQL、TUN/候选/NAT/Relay/ACL/子网路由 namespace 网络实验、Linux 签名安装生命周期、真实 x86_64/aarch64 release 构建、Docker/1Panel 迁移部署备份恢复回滚、秘密扫描、ShellCheck、npm audit，以及 Docker 容器/网络、`1panel-network` 成员、默认路由和 nftables 前后基线。

## 当前失败

无未解决测试失败。

## M6.1 当前验证

- 命令：`make test-windows-xsnet-abi`；
- 结果：通过；
- 覆盖：Clang Release 严格告警、GCC ASan/UBSan、固定消息头、长度/版本/type/flag/sequence、单 owner、协商顺序、MTU/队列、link、批次和 cleanup 负向测试，以及有界队列的原子入队、背压、小输出、部分出队、环绕和 payload 清零；
- 当前实现：有界 IPv4 队列、TX 空请求/RX framed 请求、同步 direct-I/O、协商深度、SetLink 门禁与 NetAdapterCx TX/RX 系统缓冲区复制源码已完成；Windows 部分尚未经过 WDK 编译或执行；
- 测试安装准备：已加入只面向快照 VM 的 PowerShell 安装/卸载脚本，固定 signer thumbprint、Microsoft-signed WDK DevGen、精确状态回滚和残留拒绝；尚未在 Windows 执行；
- 安装器验证：`make test-windows-xsnet-installer` 通过，本地 Windows PowerShell parser 对三份脚本零语法错误；未运行设备安装、卸载或签名命令；
- 不覆盖：WDK、NetAdapterCx、INF、签名、安装、Windows 收发、PnP/power、Driver Verifier 和蓝屏。

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

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
