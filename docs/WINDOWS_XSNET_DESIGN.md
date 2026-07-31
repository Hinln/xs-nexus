# xsnet Windows 驱动设计

状态：M6.1 进行中；ABI 与会话状态机已在 Linux/Clang 下验证  
实机边界：尚无 WDK 构建、测试签名包、Windows VM、Driver Verifier 或蓝屏结论

## 1. 平台选择

首个可安装实现选择 UMDF 2.33 + NetAdapterCx 2.5，目标 Windows 11 24H2 x86_64，与 `QA_MATRIX.md` 的 Windows 11 LTSC 2024 强制门禁一致。UMDF NetAdapterCx 从 Windows 11 24H2 开始支持 Ethernet，可使用系统分配数据缓冲区并降低首版内核攻击面。

微软版本表同时把 Windows 10 2004 的 NetAdapterCx 2.0 标为仅支持 MBBCx，这与任务书的 Windows 10 + NetAdapterCx Ethernet 组合冲突。当前不向 Windows 10 分发、不声称兼容；独立实现或范围决策由 `KI-016` 跟踪，不能用 Windows 11 结果代替。

依据仅限微软官方文档：

- [User-mode NetAdapterCx](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/user-mode-netcx)
- [NetAdapterCx version overview](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/netadaptercx-version-overview)
- [Porting NDIS miniport drivers to NetAdapterCx](https://learn.microsoft.com/en-us/windows-hardware/drivers/netcx/porting-ndis-miniport-drivers-to-netadaptercx)
- [Windows security model for driver developers](https://learn.microsoft.com/en-us/windows-hardware/drivers/driversecurity/windows-security-model)
- [Failure to Check the Size of Buffers](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/failure-to-check-the-size-of-buffers)

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

## 6. 队列和资源上限

- 每方向最多 64 个待处理包和 1 MiB Agent 请求；
- 单包不超过协商 MTU，绝不信任描述符长度；
- 队列停止后不读取 NetAdapterCx ring，不完成新的成功数据请求；
- UMDF 首版只使用 NetAdapterCx 系统分配缓冲区，所有 owner/queue 对象由 WDF 父对象或明确引用拥有；
- 不做无限等待、无限重试、动态线程池或内核网络访问；
- backpressure 只阻塞虚拟 NIC 队列，不影响宿主其他网卡。

`include/xsnet_dataplane.h` 与 `src/dataplane.c` 已实现平台无关的固定槽队列模型：批次入队先完成全量规范编码、MTU 和容量检查，失败不部分入队；出队按调用方容量选择完整包，无法容纳首包时不消费；成功出队和 reset 清零完整槽。该模型只证明队列语义，不代表已接入 WDF direct I/O 或 NetAdapterCx ring。

## 7. 未完成门禁

以下项目在获得 `BLK-001` 环境前不得标记通过：

- WDK/Visual Studio 的 UMDF 2.33 + NetAdapterCx 2.5 编译；
- INF、CAT、测试签名和驱动安装；
- Windows 11 24H2 队列收发和 Linux 互通；
- 取消、Agent crash、设备移除、睡眠/唤醒和网络切换；
- 反复安装/升级/卸载和失败回滚；
- Driver Verifier、崩溃转储和蓝屏结果。

正式微软签名继续由 `BLK-004` 阻塞。编译成功也不能替代 VM 或 Driver Verifier 证据。

## 8. 当前自动验证

```bash
make test-windows-xsnet-abi
```

该命令以 Release `-Werror` 和 ASan/UBSan Debug 两种配置编译并运行平台无关解析器、会话状态机与有界包队列，只证明 ABI 长度、规范编码、批次边界、单 owner、版本/sequence、MTU/队列、状态转换、背压、环绕和清零，不证明 Windows 驱动可运行。

```bash
make test-windows-xsnet-source
```

该命令检查 WDK 工程和 INF 的目标版本、安全指令、IOCTL 模式以及源码中的生命周期调用。当前源码已包含 DriverEntry、DeviceAdd、file create/cleanup/close、串行控制队列、D0/release reset、NetAdapter 创建/start/stop 和 packet queue 生命周期骨架。

SetLink、TX 和 RX IOCTL 当前显式返回 `STATUS_NOT_SUPPORTED`，queue advance 不消费 ring，因此 adapter 始终保持 disconnected。这是未完成数据面前的失败关闭状态，不是可工作的驱动或收发证据。`xsnet.vcxproj` 属性名、INF 和全部 API 仍必须在 WDK 10.0.26100、MSBuild、InfVerif 和 Windows 11 24H2 VM 中真实验证。
