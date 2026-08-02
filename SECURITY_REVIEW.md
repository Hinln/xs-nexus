# SECURITY_REVIEW.md — 安全审查清单

内部安全审查不能替代独立第三方协议、密码学和驱动审计。

---

## 1. 威胁主体

- 未认证公网攻击者；
- 恶意已注册节点；
- 被攻陷节点；
- 不可信 Relay；
- 被攻陷 Controller；
- 恶意管理员；
- 泄露 Enrollment Token；
- 软件供应链攻击者；
- 本地低权限用户；
- 恶意驱动输入；
- 中间人；
- 重放者；
- DoS 攻击者。

---

## 2. 身份和凭证

- 节点私钥本地生成；
- 私钥权限正确；
- 私钥不上传；
- Token 只存哈希；
- Token 有过期、次数和范围；
- 节点证书有网络、节点、序列和过期；
- 吊销真实生效；
- 密钥轮换；
- 时钟偏差处理；
- 凭证降级和回滚防护；
- 日志无秘密。

---

## 3. XSP/1

- 明确的握手 transcript；
- 身份与临时密钥绑定；
- Network ID 绑定；
- 协议版本绑定；
- 防降级；
- 独立收发密钥；
- 唯一 nonce；
- 序列号不会复用；
- 重放窗口边界测试；
- Key Epoch 切换；
- 旧密钥短窗口；
- 篡改包丢弃；
- 错误消息不形成 oracle；
- 无反射放大；
- 地址发现请求需要活动凭证和节点身份签名；
- 地址发现响应绑定精确请求且小于请求；
- 候选广告由节点签名、generation 单调并由 Controller 重新签名；
- 新端点仅在认证握手、认证 Peer 流量或 AEAD PathResponse 后晋升；
- 资源限制；
- Fuzz；
- 测试向量；
- 未审计风险明确。

### M1.3 验证状态

- 四消息握手、身份/transcript/版本/套件绑定、双向 Finish 和 RFC 原语向量已自动验证；
- AEAD Tag、AAD、虚拟源地址、1024 位重放窗口、旧/未来 Epoch 和旧 Epoch 退休已覆盖正负测试；
- 两个隔离 namespace 已验证密文抓包、Tag 篡改、重放、伪造源地址、自动轮换和 Controller 中断容错；
- 第三方协议与密码学审计仍未完成，`KI-001` 保持开放。

### M2.1 验证状态

- XSD/1 使用固定 334 字节认证请求和 188 字节 Controller 签名响应，绑定 Network/Node/Request ID、完整请求哈希、时间和观察端点；
- Controller 在响应前验证活动凭证，并对每来源限速；无效或未认证报文静默丢弃，不形成匿名放大；
- 候选广告通过认证 WebSocket 提交，以节点长期身份签名，最多 16 项、短期过期、优先级唯一且 generation 单调；
- 相同 generation/payload/signature 重试幂等，低 generation 或同 generation 不同内容拒绝；
- 隔离 namespace 已验证首选路径失败后的回退，以及建立会话后更高优先级路径的 AEAD Challenge/Response 晋升；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.1-20260729T175243Z`；
- 第三方协议审计仍未完成。

### M2.2 验证状态

- 双方主动 ClientHello 使用现有身份、凭证、transcript 和四消息握手认证，不引入未认证打洞探测包；
- 每进程同时主动握手最多 32 个、每 100 ms Tick 最多启动 8 个、每 Peer 每轮最多尝试 8 个候选，握手/路径探测/退避均有硬上限；
- 映射保活使用 AEAD Keepalive，不接受明文心跳，也不会把业务数据发往控制面；
- 未知 UDP 来源不能仅凭地址晋升：Header 必须绑定当前 Network、目标 Node、已配置 Source Node 和已建立 Session ID，并继续通过 AEAD、Epoch、序列和重放窗口；
- 隔离 nftables 模型已验证普通双端 NAT、受限 NAT、端口受限 NAT、公网 IP 重绑定、对称 NAT 无 Direct 和 UDP 封锁后恢复；
- 对称 NAT 与 UDP 封锁阶段不会降级为明文或伪造 Direct 成功，后续由 M2.3 自研 Relay 提供回退；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.2-20260730T092547Z`；
- nftables 模型不能替代运营商 CGNAT、真实公网 IPv6、多出口和长期网络抖动实测，相关限制继续记录于 `KI-009`。

### M2.3 验证状态

- `XSR/1` 注册使用活动 Controller 凭证和节点 Ed25519 身份签名，Relay Lease 绑定 Network、Node、Relay、UDP 来源端点和短期过期时间；
- Relay Data envelope 只携带有限路由元数据和端到端 XSP/1 密文，Relay 不持有 traffic key，也不能生成目标节点可接受的业务包；
- 匿名、错误来源端点、过期 Lease、重放、无目标 Lease、空 payload 和超限报文静默丢弃；每来源注册和每 Lease 包/字节/队列资源均有硬上限；
- 隔离 namespace 已验证 Relay 抓包不包含原始虚拟 IP 包或业务明文标记、伪造来源被拒绝、主备 Relay 切换和 Direct 恢复后的 AEAD 回切；
- UDP 单次发送失败仅作为路径不可达，不再终止 Agent；候选仅刷新过期时间不会重置正在进行的握手和回退状态；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m2.3-20260730T113304Z`；
- 第三方协议审计、真实公网容量、延迟/丢包指标和运营商网络实测仍未完成。

---

## 4. 控制器

- TLS 校验；
- WebSocket 认证；
- 配置签名；
- 版本单调；
- 发现端点配置有界且只由 Controller 签名配置发布；
- 候选数据库记录有短期过期、单节点唯一行和审计事件；
- API 权限；
- 管理操作审计；
- 密码哈希；
- Session/Cookie 安全；
- CSRF；
- XSS；
- SQL 注入；
- SSRF；
- 文件上传；
- 速率限制；
- 登录保护；
- 备份加密；
- 审计不可被普通管理员删除。

### M4.1/M4.2 验证状态

- 初始管理员只在用户表为空时于 PostgreSQL advisory lock 内创建，密码只以 Argon2id PHC 哈希保存，不记录输入值；
- 登录对存在和不存在用户执行密码工作，统一返回无效凭据；每用户/来源 15 分钟最多 5 次失败，成功后清理失败记录；
- 会话和 CSRF 使用独立 32 字节 CSPRNG 值，数据库仅保存域分离 SHA-256 摘要；Cookie 为 HttpOnly、SameSite=Strict，生产默认 Secure；
- 会话有绝对到期、每用户最多 10 个、注销撤销和 CSRF 轮换；管理写操作同时要求有效会话角色和 CSRF；
- 管理员、操作员和审计员权限全部在 Controller 强制执行，浏览器隐藏按钮不是授权边界；Bootstrap bearer Token 仍只供服务端自动化使用；
- 快照和用户 API 不返回密码、会话、CSRF、Token、私钥或 hash；Enrollment Token 明文仍只在创建响应中出现一次；
- 节点在线状态来自当前进程已认证控制连接，不以最近候选、固定夹具或数据库时间伪造；路径/流量/RTT 只来自节点身份签名的新鲜报告，缺失或陈旧指标显式不可用；
- 浏览器主流程覆盖安全 Cookie、CSRF、越权、注销、Console/Page Error 和未解释 4xx/5xx；秘密扫描和 npm audit 通过；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m4.2-20260730T223401Z`。

---

## 5. Relay

- 节点认证；
- 会话绑定；
- 目的节点授权；
- 限速；
- 队列限制；
- 放大系数；
- 空闲清理；
- 畸形包；
- 匿名流量；
- 多租户隔离；
- 无明文；
- 无密钥；
- 元数据最小化；
- 日志脱敏。

---

## 6. 路由和 ACL

- 默认拒绝；
- 双端执行；
- 身份与源地址绑定；
- 子网审批；
- 网关权限；
- 重叠路由；
- 更具体路由攻击；
- 默认路由保护；
- DNS 和本地网段保护；
- 配置失败保留旧策略；
- 网关离线；
- 被吊销节点；
- 防止策略解释与真实执行不一致。

### M3.1 验证状态

- ACL 使用严格默认拒绝和规范化规则顺序，未知源、未知目标、无匹配规则和无效策略均失败关闭；
- 发送端在 XSP/1 加密前检查内层源/目标、协议和目标端口，接收端在认证解密后以会话 Peer 身份重新绑定虚拟源地址并执行同一策略；
- Controller 将节点、组、标签和 ACL 写入签名配置，策略版本与配置版本分别单调递增，原子替换使用期望版本防止并发覆盖；
- Agent 只有在签名、Key ID、Network、Node、配置版本、策略版本和 ACL 编译全部成功后替换状态；低版本、同版本异内容、无签名或无效 ACL 不影响最近有效策略；
- Controller 使用数据库约束和事务保持活动地址唯一，吊销后地址进入冷却；网络地址池重叠被拒绝，Agent 发现本机系统路由与虚拟池重叠时不创建 TUN；
- namespace 测试验证发送端拒绝、接收端拒绝、允许的 ICMP/TCP/UDP 和拒绝负载不泄露；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m3.1-20260730T135838Z`；
- 子网审批、纯路由/NAT、网关离线撤销和子网重叠由 M3.2 的独立证据验证。

### M3.2 验证状态

- 本地网段发现只产生节点签名建议；Controller 校验节点身份、generation、有效期和管理员审批后才写入签名配置；
- 子网路由拒绝默认、保留、虚拟地址池、部分重叠、无建议、重复 scope 和过期网关，精确同前缀只允许不同优先级；
- XSP/1 普通会话仍严格绑定双方虚拟地址；routed API 仅由已编译的签名子网策略启用，解密后再次绑定认证 Peer、内层源/目标和 ACL；
- 网关只为项目 TUN 与审批接口显式开启 IPv4 forwarding；NAT 规则限定入接口、出接口和目标前缀，并使用项目独占带 owner marker 的 nftables 表；
- 配置变更、暂停、shutdown、Drop 和 stale manifest 恢复会先失败关闭转发，再删除项目 NAT 表、恢复原 sysctl 和撤销项目路由；
- 隔离 namespace 已验证审批前不可达、未授权和伪造源拒绝、纯路由/NAT 可达、网关候选过期撤销和无宿主资源残留；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m3.2-20260730T211914Z`；真实 NAS 与生产防火墙仍为人工门禁。

---

## 7. Windows 驱动

- 最小内核逻辑；
- IOCTL 权限；
- 缓冲区长度；
- 整数溢出；
- 生命周期；
- 并发；
- 引用计数；
- 取消 I/O；
- Agent 退出；
- 设备移除；
- 睡眠；
- 升级；
- Driver Verifier；
- 测试签名和正式签名边界；
- 崩溃转储无密钥；
- 不允许任意本地进程注入数据包。

### M6.1 当前验证状态

- Windows 路由管理新增隔离安全 Rust 事务核心与 IP Helper 平台层：系统表限制 4096 条并由 RAII 无条件 `FreeMibTable`，只接受明确 LUID、规范前缀、on-link 下一跳和固定 metric；默认/保留/外部重叠与 manifest 所有权漂移在写入前拒绝。地址创建后必须观察 DAD Preferred 才创建路由，其他状态、查询失败或超时会精确删除地址；路由失败同时保留原始错误、路由补偿失败和地址清理失败。11 个模型测试与 MSVC target check/Clippy 通过，但尚未在 Windows 执行，不改变 M6.1/K 项状态。
- manifest codec 固定 schema 1、严格拒绝未知字段与尾随数据、限制 64 KiB、要求安全 host 地址、精确 LUID 和规范排序 project route key。恢复只删除 manifest 中记录且系统快照仍 exact/project-owned 的路由；相同或重叠前缀出现外部 route 时停止，不做前缀级或接口级清理。模型测试累计 15 项；磁盘 ACL、原子写入和恢复执行仍未完成。
- Windows manifest 现复用私有存储的整条 reparse 检查、父目录/文件 exact protected DACL、同目录 `create_new` 临时文件、`sync_all` 和 write-through Replace/Move；删除 manifest 前执行同样的路径和 ACL 验证。恢复执行逆序尝试所有计划内 exact route，再删除 exact address，并聚合全部失败而不提前返回。累计 16 项模型测试与两个 crate 的 MSVC target check/Clippy 通过；NTFS 和 IP Helper 仍未实机执行。
- Agent Windows 网络准备层固定 stale recovery→Preparing→DAD→routes→Active 顺序，shutdown 只在完整清理后删除 manifest；runtime 源码门禁禁止引用，避免无 VM 证据时启用主机路由。LUID 由同一独占 xsnet device handle 的版本化 identity query 从驱动自身 `NETADAPTER` 获取，不按可注入名称或全局枚举查找；错误 schema、reserved、长度和零值失败关闭。源码门禁与模型回归通过，但 identity IOCTL、Agent 模块和 IP Helper 尚未越过 WDK/Windows VM 门禁。
- 网络准备公开边界不再接受调用方提供的裸 LUID，只接受已打开的 `XsnetDeviceSession<Win32DeviceTransport>`；内部 LUID 参数函数保持私有，源码门禁同时拒绝重新公开，缩小未来编排误接任意接口的风险。
- 镜像供应链生成器不执行 rootfs 内二进制，只导出临时容器并以有界 tar 读取 dpkg/apk 数据库和许可证文件；拒绝 symlink/非普通目标、路径穿越、超限 rootfs/材料/包数、未知数据库、标签漂移和重复包身份。输出固定 image content ID 与 Dockerfile hash，避免把 mutable tag 当证据；生成器不访问网络。漏洞扫描和发行版未携带的许可证全文继续明确开放。
- 漏洞扫描器不安装到宿主系统：固定 Grype v0.116.1 下载 URL 与 SHA-256，解压到私有临时目录，数据库也与宿主缓存隔离。脚本只允许读取仓库 QA 证据，扫描前逐项确认当前 Docker image ID 等于 manifest，报告在私有 staging 全部完成后发布；工具、数据库状态、源 manifest 和每份报告均哈希绑定。网络数据库本身仍属于时点证据，High/Critical 必须后续处置。

- 按微软支持矩阵和 Windows 11 LTSC 2024 强制门禁改用 UMDF 2.33 + NetAdapterCx 2.5；Windows 10 不受支持组合已记录为 `KI-016`，不作兼容声明；
- 驱动设计不包含密码学、身份、ACL、路由、NAT、Relay、更新或秘密，UMDF 使用系统分配数据缓冲区且拒绝直接硬件访问；
- INF 草案仅授予 LocalSystem、标记 exclusive、禁用 host 共享、拒绝内核客户端和空/未知 file object；每个请求还要求 user-mode、已接受 file object 和读写 access 位；
- IOCTL 禁止 `FILE_ANY_ACCESS`、`METHOD_NEITHER` 和共享可写环；控制面 buffered，包方向 direct I/O；SetLink 仅在双队列 started 后允许，TX/RX 使用同步有界请求并在失败时保持状态；
- ABI v1 与会话模型使用固定字节布局、精确总长度、严格递增非极值 sequence、单 owner、状态顺序、MTU/队列和 1 MiB/64 包/9000 字节硬上限；Release 严格告警与 ASan/UBSan 均通过；
- 平台无关数据平面模型使用每方向 64 包固定上限、规范原始 IPv4 version/IHL/总长度、协商 MTU 校验和原子批次入队；容量不足或输出过小时不部分修改队列，出队和 reset 对完整固定槽清零，避免失败请求泄漏陈旧 payload；
- ring 回调只在 Passive packet queue 生命周期中访问 NetAdapterCx 系统分配缓冲区，要求虚拟地址扩展、连续的一包一 fragment 和合法 capacity/offset/length；TX 只读 packet/fragment，RX 明确填充原始 IPv4 `Layer2TypeNull` layout，异常描述符停止消费并断链；
- direct-I/O 使用同步非挂起请求，不把 NetAdapterCx ring 或私有槽映射给 Agent；TX 空请求与 RX framed 请求方向固定，空/满/小缓冲/队列未启动在状态机前失败，因此包和 sequence 均不前移；固定硬上限与协商队列深度同时执行；
- 固定种子压力测试在 Clang Release 和 GCC ASan/UBSan 下分别覆盖每域 30,000 次任意字节解析、消息写入、会话调用与队列操作；被拒绝的会话请求必须逐字段保持状态，失败队列操作必须保持元数据和完整 64 槽字节，六类有效消息逐字节变异未发现越界、未定义行为或失败状态提交；
- 生命周期 harness 按单一 wait lock 的串行临界区穷举六类 teardown 的全部 720 种顺序；活动请求在取消胜出或完成胜出路径中只结算一次，cleanup、睡眠、移除和重复取消保持断链，旧 owner 不在恢复后自动复活。模型不持有跨回调 request/ring 引用，也不替代 WDF 引用计数或实际调度验证；
- Rust Agent ABI 客户端只允许单飞请求；驱动明确拒绝时保持状态和 sequence，任何无法确定驱动是否已提交的传输结果都会毒化 handle 并要求重新打开。Attach 参数和 sequence 只在成功响应验证后提交，TX 响应再次校验固定头、精确长度、规范批次、协商 MTU/深度和原始 IPv4；
- `XsnetDeviceSession` 在任何 I/O 前校验 MTU/深度，以协商上限推导 TX buffer，启动只执行 Hello/Attach/SetLink，每次收发只执行一个请求且不自动重试，shutdown 按 LinkDown/Detach 排序并允许重复 Detach；权威拒绝保留状态，不确定或畸形完成强制替换 handle，Drop 不执行设备 I/O；
- `XsnetTransport` 隐藏请求字段并只提供只读 buffer 访问，将 OS 结果固定为 Success/Rejected/Indeterminate；只有具有权威未提交证明的状态才可 Rejected，首版 Win32 失败默认 Indeterminate。隔离 target-specific crate 承担全部六个 `unsafe` 块，不降低 Agent 或 workspace 的全局禁用规则；完整规范见 `docs/WINDOWS_XSNET_TRANSPORT.md`；
- Windows 本地管理端点固定为 `\\.\pipe\xs-nexus-agent`，不接受配置选择任意名称；首实例门禁防止启动前抢占，`PIPE_REJECT_REMOTE_CLIENTS` 禁止远程客户端，受保护 DACL 仅授予 LocalSystem 和 built-in Administrators，handle 不继承。请求处理继续复用 4 KiB/512 KiB/512 peers/2 秒的共享只读协议边界；16 个活动处理器外保留一个 listener，许可耗尽时不再接收。SDDL、原始安全属性和释放只存在于独立 crate 的三个 unsafe 块，详见 `docs/WINDOWS_AGENT_LOCAL_IPC.md`；最小 Windows check、交叉 Clippy 和全量 Linux 门禁证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T040415Z`；
- Windows 私有存储文件和目录使用不同固定 SDDL，均为 protected DACL 且仅 LocalSystem/Administrators；目录 ACE 含 OI/CI 以保护新临时文件。读路径要求整条现有路径无 reparse、直接父目录与文件 ACL 精确匹配、打开前后长度一致和完整读取；写路径只在已硬化父目录创建同目录新文件，先回读 ACL 再写入/flush，随后 write-through 原子替换并再次验证。共享 Agent 代码无 unsafe，全部安全描述符与替换 FFI 隔离在最小 crate 的十个可计数 unsafe 块；详见 `docs/WINDOWS_AGENT_STORAGE.md`；最小 Windows check、交叉 Clippy 与全量 Linux 门禁证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T041702Z`；
- Windows Service runtime 固定 `XsNexusAgent`，不从配置接受服务名；SCM 只接受 STOP/SHUTDOWN，首个停止原子切换 STOP_PENDING 并一次性通知 Agent，状态互斥防止启动/停止并发重新上报 RUNNING。回调 panic、配置或 runtime 失败均以 service-specific failure 停止；Agent 无 unsafe，四个 FFI 块隔离在 `crates/windows-service`。runtime 明确不含创建、删除或重配服务 API，安装身份和 LocalSystem token 仍由签名安装器与 VM 验证，详见 `docs/WINDOWS_AGENT_SERVICE.md`；全量门禁证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T043139Z`；
- 测试安装器只在管理员 Windows 11 26100+ 接受精确 INF/CAT/DLL、有效且匹配显式 thumbprint 的 signer 和本机 Microsoft-signed WDK DevGen；不下载或分发 DevGen，不修改 BCD/测试签名模式，不通过 ExecutionPolicy Bypass；状态 ACL 仅 LocalSystem/Administrators，卸载只操作记录的 ROOT instance 与 `oem#.inf`，残留失败关闭；
- 测试包构建脚本要求全路径 Microsoft-signed MSBuild/InfVerif/Inf2Cat/SignTool、有效私钥及 code-signing EKU、全新非重解析输出目录、Release x64、禁用工程自动签名、先签 DLL 再生成并签名 `10_GE_X64` catalog，以及显式 SHA-256 test signing；脚本不创建证书、不修改信任或 BCD，只输出精确三文件包、工具日志和哈希清单；
- VM 编排要求管理员、Windows 11 26100+、可识别虚拟机、一次性运行目录和显式快照声明；各阶段不可覆盖，Verifier 使用 standard + oneboot，启用和禁用后都必须观察到人工重启，卸载前后要求精确设备/driver-store 状态。快照 ID 仅记录操作员断言，CollectVerifier 明确不包含场景结果或验收声明；
- 测试包与 VM 工作流静态回归证据为 `/srv/xs-nexus/artifacts/qa/m6.1-vm-workflow-20260731T021432Z`；秘密扫描和 Linux 便携测试通过，宿主默认路由、去计数器 nftables 结构、完整 `1panel-network`、namespace 和 TUN 前后不变；该证据不包含 Windows 执行结果；
- 运行时兼容固定 exact ABI v1：消息 header 与 Hello min/max 都为 v1，避免把无法跨 header 解析的 Hello 错当版本协商；`DriverVer` 只表示包身份，构建清单、预期输入、INF、staged driver-store 和受限状态必须一致，不能替代 ABI 检查；
- 测试安装器只允许零既有设备/包的 clean install，不实现热替换、热降级或生产回滚；受限状态 schema 2 强制 ABI/driver version，未知或旧结构拒绝。失败恢复仍是精确卸载和快照。静态/便携证据为 `/srv/xs-nexus/artifacts/qa/m6.1-compatibility-20260731T022619Z`，真实升级事务保持未验证；
- DriverEntry、DeviceAdd、file create/cleanup/close、串行控制队列、cancel、D0/release reset、adapter start/stop 和 packet queue start/stop/cancel 骨架已写入，并由源码不变量脚本检查；
- Win32 transport 现隔离为 `no_std + alloc` crate：默认 deny unsafe，仅 `platform.rs` 允许六个精确 unsafe 块；只调用 Configuration Manager、`CreateFileW`、`DeviceIoControl` 和 `CloseHandle`，使用唯一 GUID 路径、读写权限、零共享、同步 I/O、已初始化自有缓冲区和 u32 长度门禁。身份查询在同一 handle 上严格验证 schema 和 nonzero LUID。所有数据请求的 Win32 失败、异常字节数或 direct 输入变异均归类 Indeterminate，不读取通用错误码推断“未提交”；
- transport crate 已实际通过 `x86_64-pc-windows-msvc` core/alloc target check 和交叉 Clippy，证据 `/srv/xs-nexus/artifacts/qa/m6.1-win32-transport-20260731T030644Z`；完整 Agent 的 Windows 编译在 `ring` 需要 SDK C 头处失败并保留证据，不能从最小 crate 推断链接、运行或设备安全；
- 空 TX 与满 RX 在驱动端当前以失败状态立即完成，但 Win32 transport 在没有 VM 证据前将所有失败调用保守归类 Indeterminate；因此未把通用错误码推断为权威 Rejected，也未启用轮询线程、后台重试或 Agent runtime。18 个 Agent xsnet 测试覆盖失败启动释放、无效 RX/Drop 零 I/O 和显式 shutdown 重试；源码门禁强制配置先于设备打开/IOCTL，并在 VM 前禁止 runtime 引用、线程、sleep 和 session Drop I/O。全量 Linux 门禁证据为 `/srv/xs-nexus/artifacts/qa/m6.1-agent-session-20260731T033841Z`；
- 2026-07-31 生命周期模型加入后连续三轮源码、五组 Release/ASan/UBSan 与安装器静态门禁通过，证据为 `/srv/xs-nexus/artifacts/qa/m6.1-lifecycle-20260731T014028Z`；后续 transport 回归证据为 `/srv/xs-nexus/artifacts/qa/m6.1-transport-contract-20260731T015900Z`。命名管道、direct-I/O、ring 和全部 PowerShell 工作流仍未在 Windows 执行，命名管道有效 DACL、拒绝矩阵以及 WDF 对 METHOD_IN_DIRECT 缓冲区、对象引用、queue stop/cancel 和通知竞态的实际行为仍未知；Windows SDK/WDK/MSBuild、InfVerif、INF ACL 实际应用、ring 收发、PnP/power、测试签名、Driver Verifier 和 VM 异常输入保持未验证，因此 M6.1 和 `ACCEPTANCE.md` K 项保持未完成。

---

## 8. 更新供应链

- 离线根签名；
- 在线 Controller 不持有根私钥；
- 清单签名；
- 文件哈希；
- 平台/架构绑定；
- 防版本回滚；
- 分批；
- 撤回；
- 回滚；
- 镜像固定摘要；
- SBOM；
- 依赖漏洞扫描；
- 构建可追溯；
- 安装器不执行未签名脚本。

### M5.1 验证状态

- 发布清单使用外部 Ed25519 分离签名，安装器在解析字段前先验证签名，再严格绑定产品、版本、平台、架构、目标、归档文件名、长度和 SHA-256；
- 归档只接受固定目录与文件 allowlist、目录/普通文件类型和精确 `PAYLOAD.SHA256`，符号链接、设备、额外成员、缺失成员和任意内容篡改均失败关闭；
- 首次安装固定可信发布公钥，后续升级拒绝不同公钥；当前版本完整性在升级、回滚和卸载前重新验证；
- 外部降级被拒绝，显式回滚只允许已安装且仍通过原签名、外部清单和逐文件哈希验证的版本；
- 激活采用版本目录和原子符号链接；服务失败会恢复原版本、systemd 单元和活动状态并删除失败版本；
- 安装包不包含也不执行安装脚本；安装器只安装经过 allowlist 和签名链验证的二进制、systemd 单元、示例配置与文档；
- Enrollment Token 只从受限文件读取，不进入进程命令行、状态、包和日志；默认卸载保留身份，网络 cleanup 必须先验证本地身份、签名状态和恢复清单；
- M5.1 使用临时测试签名密钥验证机制，未创建或导入正式生产发布私钥；正式离线根签名、密钥托管、撤回、分批发布、容器操作系统 SBOM 与构建来源证明仍未完成；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。

### 源码依赖供应链验证状态

- 源码依赖 SBOM 只从已提交 lock、Cargo metadata 和固定 npm 许可证快照生成，不在生成或 CI 中访问网络；npm 快照建立时逐包核对官方注册表 exact version、license 和 `dist.integrity` 与 lock 一致；
- 每个 Cargo 组件必须具备 `Cargo.lock` SHA-256 checksum 和可解析许可证表达式；每个 npm 组件必须具备唯一 exact version、SHA-512 integrity 和快照许可证，集合差异失败关闭；
- 许可证策略解析 `AND`、`OR`、括号与 `WITH`，只接受至少一条获准选择路径并保留完整 SPDX 表达式；历史斜杠写法规范化为 `OR`，未知标识不会被静默接受；
- 禁用产品既在全部源码依赖名称中拒绝，也由独立源码扫描器拒绝新增运行路径引用；三个现有负向引用被精确固定，数量或位置变化即失败；
- 输出目录必须是不存在的绝对真实路径，避免覆盖或经符号链接改写证据；CycloneDX、SPDX 和 manifest 使用固定排序、输入摘要派生 UUID 与 `SOURCE_DATE_EPOCH`，双次生成逐字节一致；
- 证据：`/srv/xs-nexus/artifacts/qa/supply-chain-20260731-final`。尚未覆盖容器基础镜像中的 Debian/Alpine/NGINX/PostgreSQL 包、许可证全文、漏洞豁免流程或最终构建来源，因此 Release Checklist 的完整 SBOM 保持未完成。

---

## 9. 主机和 1Panel

- 不暴露数据库；
- 不破坏 SSH；
- 不重建外部网络；
- 容器非 root；
- 只授予必要 capability；
- Secret 仓库外保存；
- 日志轮转；
- 备份恢复；
- 防火墙最小开放；
- UDP Relay 限速；
- 管理控制台 TLS；
- 开发环境与 RC 隔离。

### M5.2 验证状态

- 所有服务显式使用非 root UID，根文件系统只读，丢弃全部 capability，启用 `no-new-privileges`、PID 上限、tmpfs 和有界 json-file 日志；
- Secret 只从仓库外、非符号链接、私有权限文件只读挂载；Controller 直接环境值与 `_FILE` 同时出现时失败关闭；
- Compose 只引用名称、driver 和子网均匹配基线的外部 `1panel-network`，不创建数据库容器、不发布数据库端口、不执行 prune 或网络变更；
- dev/RC 分离项目、schema、端口、Secret、备份和状态路径；RC 还要求干净 Git、固定 HEAD revision 和一致镜像标签；
- 迁移在服务激活前运行；激活失败恢复上一镜像集合。备份使用 PostgreSQL 18 客户端、私有目录、自定义归档和大小/SHA-256/时间清单，恢复要求精确 schema 确认并先做安全备份；
- 备份静态加密、异机复制和恢复保留策略尚未完成，记录为 `KI-015`；既有 1Panel 数据库公网端口仍为 `KI-006`/`BLK-005`；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m5.2-20260731T001922Z`。

---

## 10. 安全退出条件

以下任一存在时不得描述为生产可用：

- 协议存在未解释身份或 nonce 风险；
- ACL 可绕过；
- Relay 可匿名使用；
- 更新签名可绕过；
- 驱动存在蓝屏或越权；
- 默认路由可能破坏；
- 真实秘密进入 Git；
- P0/P1 安全问题未关闭；
- 没有恢复和吊销验证；
- 自研密码协议未经独立审计但文档声称已安全。
## 11. 运行时健康检查与漏洞收敛复核（2026-07-31）

- Controller/Relay 镜像不再携带 `curl`；健康检查由同一受限非 root 二进制访问固定 loopback 端口，不读取环境目标、不接受 CLI 地址、不访问容器外网络。
- 探测器有连接/读写超时、8 KiB 响应上限、完整 HTTP 头和 HTTP 200 门禁；异常响应失败关闭，测试覆盖拒绝、超时、非 200、畸形和超限输入。
- Console 与 db-tools 构建使用 `apk upgrade --no-cache`，旧扫描中带明确修复版本的 Alpine OpenSSL、expat、libxml2、curl/libcurl 等发现已从 fixable 集合移除。
- 精确提交 `2eefae9` 的最终扫描只剩 Critical 2 和 High 6，Grype 当前未给出供应商修复版本。Controller/Relay 各为 glibc 1 Critical/2 High，Console 为 TIFF 2 High，db-tools 无 Critical/High。它们不是自动接受项，Release Candidate 前必须逐项确认可达性、供应商状态、基础镜像升级/替换方案和期限。
- Controller/Relay 使用固定 digest distroless、无 shell/包管理器；Console 已移除 curl 及其反向依赖链。该最小化通过完整 Docker 生命周期，不依赖容器内调试 shell。
- Console 未使用 NGINX image-filter 功能，最终镜像已删除该模块及 TIFF 包链；新扫描中 Console 无 Critical/High。
- 剩余 glibc 三项采用到期日为 2026-08-31 的有界 disposition：当前源码与精确 Controller/Relay 二进制不引用受影响 API，自动门禁拒绝报告集合、修复状态或导入面的变化。此结论不是漏洞修复；基础镜像或扫描结果变化时必须立即重做，RC 前仍需正式复核。
## 12. Relay 指标数据最小化（2026-07-31）

- 指标只包含全局累计计数、字节数和从进入内存队列到 UDP send 成功的时长；不包含 XSP/1 密文、业务明文、节点 ID、网络 ID、端点或 lease ID。
- 丢弃按 invalid/authentication/replay/rate-limit/queue/destination/send 分类，并提供可审计总数；不把不可观测的公网 UDP 丢失推断为零或精确比例。
- 延迟使用单调 `Instant`，只在完整 datagram 成功发送后提交样本；失败发送计入 drop 与 I/O error，不污染成功延迟。
- 指标端点与 health listener 共用，Compose 不向宿主或公网发布该 TCP 端口；Relay 另用目录身份密钥在独立域下签名推送相同累计值，Controller 按 boot/sequence、时间、分类总和和同 boot 单调性验证，最多保留 25 小时/9000 样本。

## 13. M7.1 连续回归复核（2026-07-31）

- 提交 `52867cd` 的三轮聚合证据 `/srv/xs-nexus/artifacts/qa/m7.1-three-round-20260731T195752Z` 连续通过 Linux 数据面/NAT/Relay/ACL/路由、安装部署恢复、Windows 源码门禁、UI、秘密扫描、SBOM 和漏洞 disposition。
- 三轮没有新增安全告警、鉴权绕过、秘密命中、路由/namespace/TUN/Compose 残留或 `1panel-network`/默认路由/nftables 漂移；这仍不是第三方协议/驱动安全审计，也不解除 Windows VM、生产防火墙或真实 NAS 门禁。
---

## Windows route ownership re-review (2026-07-31)

The IP Helper snapshot boundary now requires `MIB_IPFORWARD_ROW2.SitePrefixLength` to equal the canonical destination prefix length before a row can be marked project-owned. Protocol, origin, metric, unspecified next hop, exact LUID, and canonical prefix checks remain required. A row with a matching route key but mismatched site-prefix semantics is treated as foreign; recovery therefore fails closed instead of deleting it. No runtime integration was added before Windows SDK/WDK and VM evidence.

## Runtime image license closure review (2026-07-31)

Runtime image license evidence now fails closed per exact installed package rather than treating package-manager declarations as proof of full text. Alpine SPDX identifiers bind to official distribution text files retained from a build-only package; Debian packages bind to exact copyright files through bounded safe documentation links. Missing or mismatched text, unsafe links, duplicate package identities, material hash changes and package/closure drift are rejected. Public-domain and virtual metapackage cases remain explicit and do not invent a license grant.

Clean-commit evidence `/srv/xs-nexus/artifacts/qa/image-supply-chain-20260731T230203Z` binds revision `187b735f8347d9d34c0598e96183547c44dd60a1` to all four image IDs and 113/113 package closure entries. The attached Grype 0.116.1 scan reports Critical 2, High 4, Medium 16 and Negligible 24 with no currently advertised fix; the exact glibc Critical/High set passed the existing binary-import disposition gate. This remains a time-bounded risk acceptance, not remediation.

## 14. Signed update privilege-boundary review (2026-08-02)

- Controller configuration loads only an optional raw Ed25519 public key; release import verifies an exact immutable manifest, detached signature, target-bound HTTPS URL and archive metadata. No API, model, migration, Console field, log, or audit event accepts a release private key.
- Rollout pause overrides all delivery; minimum version cannot override pause; versions never downgrade; percentage uses a deterministic identity bucket. Policy generation and node configuration version prevent lost updates.
- Runtime reports are domain-separated and signed by the node identity. Controller compares network, node, current database-assigned channel, time, platform/architecture, update state, and error-code grammar before persisting a strictly newer report.
- Agent treats directives as untrusted scheduling input, checks the Controller-signed assigned channel, pins the offline public key, permits HTTPS only, applies connection/overall timeout and 512 MiB bounds, and publishes ready state atomically only after exact size/SHA-256/signature checks.
- The systemd root helper has no network access, rejects unsafe paths/links/permissions, copies the archive into a private root-owned directory, re-verifies all public material after the privilege transition, and calls the existing signature-verifying atomic installer. Tampered archive tests prove the installer is not invoked.
- Residual gates: the production offline signing ceremony, authenticated public-key distribution/rotation/revocation, real signed storage and RC rollback exercise remain `KI-013`; existing older Agents that reject unknown node fields require a two-phase fleet rollout.

## 15. Authenticated telemetry review (2026-08-02)

- Agent reports bind Network/Node, a random nonzero process boot ID, monotonic sequence and the exact active signed-configuration peer set. Ed25519 signing uses a domain distinct from control authentication, candidates, runtime updates and Relay reports.
- TX counters advance only after a business packet enters the encrypted data plane; RX counters advance only after AEAD, source binding and ACL acceptance. Per-peer and aggregate cumulative counters cannot decrease within one boot; removed peers are folded into aggregate retired counters.
- Active-path latency uses encrypted PathChallenge/PathResponse, not plaintext echo. Controller rejects unknown peers/Relays, impossible path state, malformed latency, aggregate inconsistency, stale/future reports, replay and per-peer/aggregate rollback.
- Relay reports contain no Network/Node, endpoint, Lease ID or payload. The Controller trusts only the active catalog key, and the public HTTP route still requires a valid signature before storage.
- PostgreSQL retains latest rows plus bounded 25-hour samples; Console freshness gates prevent stale reports from becoming authorization or health truth. Telemetry is operational evidence only and never changes ACL, path authorization or update eligibility.
