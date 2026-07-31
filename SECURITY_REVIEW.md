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
- 节点在线状态来自当前进程已认证控制连接，不以最近候选、固定夹具或数据库时间伪造；未接入指标显式不可用；
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

- 按微软支持矩阵和 Windows 11 LTSC 2024 强制门禁改用 UMDF 2.33 + NetAdapterCx 2.5；Windows 10 不受支持组合已记录为 `KI-016`，不作兼容声明；
- 驱动设计不包含密码学、身份、ACL、路由、NAT、Relay、更新或秘密，UMDF 使用系统分配数据缓冲区且拒绝直接硬件访问；
- INF 草案仅授予 LocalSystem、标记 exclusive、禁用 host 共享、拒绝内核客户端和空/未知 file object；每个请求还要求 user-mode、已接受 file object 和读写 access 位；
- IOCTL 禁止 `FILE_ANY_ACCESS`、`METHOD_NEITHER` 和共享可写环；控制面 buffered，包方向 direct I/O，当前未实现包路径和 SetLink 统一返回 `STATUS_NOT_SUPPORTED` 并保持断链；
- ABI v1 与会话模型使用固定字节布局、精确总长度、严格递增非极值 sequence、单 owner、状态顺序、MTU/队列和 1 MiB/64 包/9000 字节硬上限；Release 严格告警与 ASan/UBSan 均通过；
- DriverEntry、DeviceAdd、file create/cleanup/close、串行控制队列、cancel、D0/release reset、adapter start/stop 和 packet queue start/stop/cancel 骨架已写入，并由源码不变量脚本检查；
- WDK/MSBuild、InfVerif、INF ACL 实际应用、NetAdapterCx API 编译、ring 收发、请求取消竞态、PnP/power 实际行为、测试签名、Driver Verifier 和 VM 异常输入仍未验证，因此 M6.1 和 `ACCEPTANCE.md` K 项保持未完成。

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
- M5.1 使用临时测试签名密钥验证机制，未创建或导入正式生产发布私钥；正式离线根签名、密钥托管、撤回、分批发布、SBOM 与构建来源证明仍未完成；
- 全量证据：`/srv/xs-nexus/artifacts/qa/m5.1-20260730T232953Z`。

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
