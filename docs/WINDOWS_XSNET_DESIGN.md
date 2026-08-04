# xsnet Windows 驱动设计

状态：M6.1 源码/交叉门禁与 Windows 11 24H2 WDK/VM 驱动门禁已完成  
实机边界：测试签名驱动的构建、安装、SYSTEM 收发、PnP 热重启、标准/UMDF/Application Verifier 和卸载已验证；完整 Windows Agent、路由/DAD、睡眠、正式签名及 Windows 10 仍未完成

## 1. 平台选择

首个可安装实现选择 UMDF 2.33 + NetAdapterCx 2.5，目标 Windows 11 24H2 x86_64，与 `QA_MATRIX.md` 的 Windows 11 LTSC 2024 强制门禁一致。UMDF NetAdapterCx 从 Windows 11 24H2 开始支持 Ethernet，可使用系统分配数据缓冲区并降低首版内核攻击面。

微软版本表同时把 Windows 10 2004 的 NetAdapterCx 2.0 标为仅支持 MBBCx，这与任务书的 Windows 10 + NetAdapterCx Ethernet 组合冲突。当前不向 Windows 10 分发、不声称兼容；独立实现或范围决策由 `KI-016` 跟踪，不能用 Windows 11 结果代替。

依据仅限微软官方文档：

- [User-mode NetAdapterCx](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/user-mode-netcx)
- [NetAdapterCx version overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/netadaptercx-version-overview)
- [Porting NDIS miniport drivers to NetAdapterCx](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/porting-ndis-miniport-drivers-to-netadaptercx)
- [Windows security model for driver developers](https://learn.microsoft.com/en-us/windows-hardware/drivers/driversecurity/windows-security-model)
- [Failure to Check the Size of Buffers](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/failure-to-check-the-size-of-buffers)
- [Net ring element management](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/net-ring-element-management)
- [NetAdapterSetLinkLayerMtuSize](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/netadapter/nf-netadapter-netadaptersetlinklayermtusize)
- [WdfRequestRetrieveOutputBuffer](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdfrequest/nf-wdfrequest-wdfrequestretrieveoutputbuffer)

不复制 WDK 示例、第三方驱动、Wintun、TAP-Windows 或现成 VPN/组网实现。

## 2. 内核职责

驱动只负责：

- 创建一个三层 IPv4 虚拟 NIC；
- 管理使用系统分配缓冲区的 NetAdapterCx TX/RX 队列；
- 与唯一活动 LocalSystem Agent 交换有界 IPv4 包批次；
- 管理 link up/down、队列取消、PnP、电源和文件句柄生命周期；
- 返回版本、能力和有界诊断计数。

以下能力禁止进入驱动：

- 节点身份、私钥、Enrollment Token 或发布密钥；
- XSP/1 握手、AEAD、重放窗口和密钥轮换；
- ACL、子网审批、路由决策和地址分配；
- NAT 探测、打洞、Relay、Controller 和更新逻辑；
- 业务 payload 日志、包转储或持久化。

## 3. 设备访问

- INF 设备安全描述符只授予 LocalSystem 完全访问，不授予 World、普通用户或交互式管理员进程数据面访问；
- 所有 IOCTL 使用明确读写访问位，不使用 `FILE_ANY_ACCESS`；
- 控制请求使用 buffered I/O，包批次使用 direct I/O，不使用 `METHOD_NEITHER`；
- 每个请求再次检查 requestor mode、关联 file object、句柄会话状态和消息 sequence；
- INF 标记设备栈 exclusive；同一设备只允许一个完成 ABI 协商的 Agent owner，重复打开或第二个 owner 失败关闭；
- 文件 cleanup、进程崩溃、设备移除或睡眠会先 link down、停止新包、取消请求并释放 owner。

## 4. ABI v1

`include/xsnet_abi.h` 定义 32 字节小端固定头：

| 偏移 | 长度 | 字段 |
|---:|---:|---|
| 0 | 4 | Magic `XSN1` |
| 4 | 2 | ABI version |
| 6 | 2 | header size |
| 8 | 4 | message type |
| 12 | 4 | flags，v1 必须为 0 |
| 16 | 4 | payload length |
| 20 | 4 | reserved，必须为 0 |
| 24 | 8 | 非零 sequence |

消息类型固定为 Hello、Attach、SetLink、TxBatch、RxBatch、Detach。未知版本、类型、flag、reserved、零 sequence、非精确总长度和超过 1 MiB payload 均拒绝。

包批次最多 64 包，每包 20–9000 字节。描述符表长度必须等于 `count * 8`；包数据必须紧随描述符、按顺序连续、无间隙、无重叠、无隐藏尾部数据。ABI 不使用编译器结构体布局解析字节流。

direct-I/O 使用明确的双缓冲方向：`DEQUEUE_TX` 的 buffered 输入必须是 payload 长度为 0 的 32 字节 TxBatch 请求，direct 输出为带同一 sequence 的完整 TxBatch；`ENQUEUE_RX` 不接受 buffered 输入，其 direct 缓冲区必须是完整 RxBatch。空 TX、满 RX、输出不足、队列未启动或输入失败均立即返回，不挂起请求、不消费包、不推进 sequence。只有完整成功请求推进 sequence；TX 输出最多协商的 transmit depth，RX 积压不得超过协商的 receive depth。

Attach 在会话锁内提交协商 MTU 和私有队列上限，解锁后才调用 `NetAdapterSetLinkLayerMtuSize`。微软文档说明运行时 MTU 更新会重建 TX/RX queues；任何在锁内调用都会让 queue stop/create 回调重入同一锁。重建完成前 SetLink 因双队列未 started 而失败关闭。

## 5. 状态机

```text
DeviceCreated -> AdapterStopped -> OwnerOpened -> Negotiated -> Attached -> LinkUp
     ^                 |               |             |            |          |
     +-----------------+---------------+-------------+------------+----------+
                       cleanup / cancel / power-down / remove
```

- 只有 `OwnerOpened` 接受 Hello；版本交集为空则关闭会话；
- 只有 `Negotiated` 接受 Attach；MTU、队列和能力必须在驱动硬上限内；
- 只有 `Attached` 可 SetLink；LinkUp 才交换包；
- sequence 必须严格递增，回滚、重复和溢出前会话必须重新建立；
- Detach 幂等关闭虚拟链路，但不影响普通物理网络；
- 取消和 cleanup 可从任意 owner 状态进入，所有完成只发生一次。

`include/xsnet_session.h` 与 `src/session.c` 已实现上述纯状态模型。失败消息不推进 sequence 或状态；`UINT64_MAX` 在处理前拒绝，要求关闭并新建会话。该模型不替代 WDF request 取消和对象生命周期验证。

`tests/lifecycle_test.c` 以驱动当前锁序建立可移植生命周期 harness：file cleanup、packet queue stop/cancel、D0 exit、hardware release 和同步 I/O stop 都先进入同一个 session/私有队列临界区；请求不挂起，不保留跨回调 WDFREQUEST；NetAdapterCx ring 指针只随其 WDF queue context 存活，不进入 session 或 Agent。模型穷举六类 teardown 的全部 720 种顺序，并分别验证取消胜出和完成胜出，因此每个活动请求只归属一次完成路径。睡眠、移除和 Agent cleanup 均断链并清空 owner/包状态；恢复后旧 owner 不会自动复活，必须重新打开并协商。该 harness 只验证锁内状态和动作幂等，不模拟 WDF 对象引用、真实线程调度、PnP/power 回调顺序或 Windows 网络栈。

`apps/agent/src/windows_xsnet.rs` 实现无 `unsafe` 的 Agent 侧 ABI 客户端模型和 `XsnetDeviceSession`：固定 IOCTL、头和批次编码与 C 合约向量互校，单飞请求在成功后才提交 sequence/状态/Attach 参数；明确驱动拒绝保持原状态并允许调用方显式决定是否重试，传输结果不确定则进入 `ReconnectRequired`，禁止猜测 sequence。会话启动严格执行 Hello/Attach/SetLink，TX 输出容量由协商 MTU/深度推导，每个收发方法只执行一个有界请求，shutdown 按 LinkDown/Detach 排序且不在 Drop 中执行 I/O。TX 响应重新执行精确头、MTU、深度、连续描述符和 IPv4 校验。

安全 `XsnetTransport` 契约已把平台结果限定为 Success、具有权威未提交证明的 Rejected 和保守 Indeterminate；畸形成功响应也会毒化 handle。隔离 `no_std + alloc` Win32 transport 已通过 `x86_64-pc-windows-msvc` core/alloc 构建和交叉 Clippy，使用唯一接口、独占同步 handle、六个 ABI IOCTL、一个独立 identity IOCTL 和六个受限 `unsafe` 块。identity query 在同一 handle 上获取驱动 `NETADAPTER` 的 authoritative LUID，不按名称匹配。VM harness 已经真实打开同一设备、执行全部会话/Tx/Rx IOCTL 并证明 LUID 在 reopen 和 PnP restart 后稳定；但完整 Rust Agent 尚未在 Windows 链接或接入 runtime，空 TX/满 RX 的 Win32 错误仍没有权威 no-commit 映射，因此会话适配层继续隔离，不添加后台轮询或自动重试。完整边界见 `docs/WINDOWS_XSNET_TRANSPORT.md`。

## 6. 队列和资源上限

- 每方向最多 64 个待处理包和 1 MiB Agent 请求；
- 单包不超过协商 MTU，绝不信任描述符长度；
- 队列停止后不读取 NetAdapterCx ring，不完成新的成功数据请求；
- UMDF 首版只使用 NetAdapterCx 系统分配缓冲区，所有 owner/queue 对象由 WDF 父对象或明确引用拥有；
- 不做无限等待、无限重试、动态线程池或内核网络访问；
- backpressure 只阻塞虚拟 NIC 队列，不影响宿主其他网卡。

`include/xsnet_dataplane.h` 与 `src/dataplane.c` 已实现平台无关的固定槽队列模型：批次入队先完成全量规范编码、MTU 和容量检查，失败不部分入队；出队按调用方容量选择完整包，无法容纳首包时不消费；成功出队和 reset 清零完整槽。该模型只证明队列语义，不代表已接入 WDF direct I/O 或 NetAdapterCx ring。

NetAdapterCx packet queue 源码按官方 ring 所有权规则缓存 TX/RX ring collection 和虚拟地址 fragment 扩展。Windows 侧媒体类型为 Ethernet，私有 Agent ABI 为 raw IPv4：TX 验证并剥离 14 字节 Ethernet 头，RX 合成固定本地/对端地址与 IPv4 EtherType，再设置 Ethernet/IPv4 layout。同步完成后才推进正常 Begin/Next；发送取消只标记 completion range 并推进 packet `BeginIndex`，不得伪造 post `NextIndex` 或 fragment ownership。ring 索引、fragment 数、虚拟地址或容量异常会停止当前消费；packet queue callback 内不允许同步改变 link state，以免重入 NetAdapterCx stop/cancel。SetLink 只有在双向 packet queue 已启动时才能 link up；queue stop/cancel 会清空对应积压、退回 Attached，并由控制路径维护 link state。

## 7. 实机门禁状态

Windows 11 24H2 VM 已完成 WDK/Visual Studio 编译、INF/CAT、测试签名、clean install、真实设备句柄与会话/Tx/Rx IOCTL、PnP 热重启、standard Driver Verifier、UMDF/Application Verifier 和 clean uninstall。三轮 verifier-enabled 重启/数据面均通过，且没有新增 WDF、NDIS/LiveKernel、相关 WER 或错误事件。完整环境、哈希和时间见 `docs/WINDOWS_XSNET_VM_EVIDENCE.md`，因此 `BLK-001` 的驱动环境门禁已解除。

尚未完成并不得外推：完整 Rust Agent Windows 链接/runtime 接入、空 TX/满 RX 权威 no-commit 映射、SCM/Named Pipe/私有存储实机、IP Helper/DAD/路由与崩溃恢复、睡眠/唤醒、生产升级/回滚、Windows 10 和正式微软签名。它们继续由 `KI-016`、`KI-017`、`KI-018`、`KI-020` 与 `BLK-004` 跟踪。

## 8. 当前自动验证

```bash
make test-windows-xsnet-abi
```

该命令以 Release `-Werror` 和 ASan/UBSan Debug 两种配置编译并运行五组平台无关测试：解析器、会话状态机、有界包队列、确定性任意输入压力和生命周期交错。它只证明 ABI 长度、规范编码、批次边界、单 owner、版本/sequence、MTU/队列、状态转换、背压、环绕、清零和 harness 可达状态，不证明 Windows 驱动可运行。

```bash
make test-windows-xsnet-source
```

该命令检查 WDK 工程和 INF 的目标版本、安全指令、IOCTL 模式、源码生命周期调用，以及压力/生命周期测试注册与 Release 断言门禁。当前源码已包含 DriverEntry、DeviceAdd、file create/cleanup/close、串行控制队列、D0/release reset、NetAdapter 创建/start/stop 和 packet queue 生命周期骨架。

SetLink、同步 TX/RX direct-I/O 和 ring copy 继续由源码门禁与平台无关测试覆盖编码、状态、背压和清零语义；此外，`xsnet.vcxproj`、INF 与驱动 API 已在 WDK 10.0.26100、MSBuild、InfVerif 和 Windows 11 24H2 VM 中真实构建并运行。该实机结果只覆盖测试签名驱动及专用 SYSTEM harness，不等于完整 Windows Agent 或生产分发通过。
