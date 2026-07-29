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
- 资源耗尽攻击。

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
