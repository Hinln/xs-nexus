# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-29 10:14 UTC  
当前 Git 提交：`77e13b6`  
当前总状态：`IN_PROGRESS`  
当前里程碑：`M1.1 Controller 最小控制平面`

---

## 已完成

- M0.1 系统、资源、网络、防火墙、Docker、1Panel、服务和工具链基线；
- 隔离 namespace 中的真实 TUN、nftables 和 capability 验证；
- 外部 Secret 文件和 Git 忽略保护；
- Rust workspace、Controller、Relay、Agent、CLI 最小组件；
- React/Vite 控制台 workspace；
- 统一 Make、CI、秘密扫描、组件集成和网络能力脚本；
- M0.1 完整验证；
- 首个 Git 检查点 `c519e34`；
- M0.2 clean-room、总体架构、威胁模型和安全假设；
- XSP/1 字节级协议、密码学设计、测试向量和一致性验证；
- M0.2 全量验证证据 `/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`。

## 正在进行

- M1.1 Controller 最小控制平面：数据模型、迁移、Enrollment、IPAM、签名配置、审计和健康检查。

## 下一步

1. 定义 Controller 配置、错误模型和 PostgreSQL schema；
2. 实现一次性 Enrollment Token 的哈希存储和原子消费；
3. 实现节点注册、稳定 IPAM 和公钥身份绑定；
4. 实现带版本与签名的配置快照；
5. 实现健康检查、审计日志和基础控制连接；
6. 添加数据库集成测试并创建 M1.1 检查点。

## 下一条准确命令

```bash
sed -n '1,240p' apps/controller/Cargo.toml apps/controller/src/main.rs
```

## 最近测试

- 时间：2026-07-29 10:12 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：M0.2 全量验证命令集；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m0.2-20260729T101325Z/validate-m02.log`；
- 覆盖：格式、Clippy、TypeScript、构建、Rust/前端单测、组件集成、XSP/1 规范与向量、隔离 TUN/nftables、秘密扫描、npm 审计、Compose 外部网络。

## 当前失败

无未解决测试失败。

## 外部阻塞

- Windows 驱动真实测试需要 Windows 11 测试 VM、快照和 WDK；
- NAS 接入需要用户在 NAS 本地执行安装；
- 正式驱动签名需要外部签名流程；
- DNS 和生产防火墙变更需要人工批准；
- 现有 PostgreSQL、Redis 公网暴露整改需要用户批准修改 1Panel 或云防火墙，见 `BLK-005`。

## 当前风险

- 自研协议规范已冻结为 v1 草案，但尚未实现和独立审计；
- Windows 驱动尚未开发；
- 真实设备尚未接入；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器提示需要维护窗口重启；
- 临时凭据后续必须轮换。

## 恢复说明

```bash
cd /srv/xs-nexus
git status --short --branch
GIT_PAGER=cat git log --oneline -10
cat PROGRESS.md
./scripts/validate-m01.sh
```

然后读取：

- `AGENTS.md`
- `EXECUTION_PLAN.md`
- 当前目录级 `AGENTS.md`

## 临时服务

无项目临时服务。未启动 Compose 服务，隔离 namespace 测试退出后自动清理。

## 清理命令

```bash
make clean
```

该命令只删除项目 Rust 构建目录和控制台 `dist`，不操作 Docker、1Panel、网络或外部秘密。
