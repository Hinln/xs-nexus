# PROGRESS.md — 当前项目状态

最后更新时间：2026-07-29 09:58 UTC  
当前 Git 提交：首个 M0.1 检查点待创建  
当前总状态：`IN_PROGRESS`  
当前里程碑：`M0.1 仓库与环境基线（验证通过，检查点待创建）`

---

## 已完成

- 完整读取启动指令和全部项目规则；
- 保存系统、资源、网络、路由、防火墙、Docker、1Panel、服务和工具链基线；
- 确认 `1panel-network` 为现有 bridge 网络，子网为 `172.18.0.0/16`；
- 确认项目 Compose 只以外部网络方式引用 `1panel-network`；
- 在隔离 namespace 中实际验证 TUN、nftables 和网络 capability；
- 安装并记录 Rust、Node、C/C++、CMake、Make、ripgrep 和 ShellCheck 工具链；
- 初始化 Git `main` 分支；
- 创建 Rust workspace、四个最小组件、React/Vite 控制台和统一 Make 入口；
- 创建秘密扫描、组件集成和隔离网络验证脚本；
- 生成环境基线报告并记录现有数据库公网暴露风险；
- M0.1 完整验证通过。

## 正在进行

- 创建第一个范围单一的 Git 检查点。

## 下一步

1. 暂存仓库文件并再次检查秘密和差异；
2. 创建 M0.1 Git 检查点；
3. 将 M0.1 标记为 `PASSED` 并记录提交哈希；
4. 立即进入 M0.2 Clean-room、架构和威胁模型。

## 下一条准确命令

```bash
git add . && git diff --cached --check && make security-check
```

## 最近测试

- 时间：2026-07-29 09:57 UTC；
- 环境：Ubuntu 26.04 LTS，Linux 7.0.0-1008-gcp，x86_64；
- 命令：`./scripts/validate-m01.sh`；
- 结果：通过；
- 证据：`/srv/xs-nexus/artifacts/qa/m0.1-20260729T095256Z/validate-m01.log`；
- 覆盖：格式、Clippy、TypeScript、构建、Rust/前端单测、组件集成、隔离 TUN/nftables、秘密扫描、npm 审计、Compose 外部网络、主机残留检查。

## 当前失败

无未解决测试失败。首次执行中的 TypeScript CSS 类型、TUN 接口名长度和秘密扫描误报均已按根因修复，原始失败日志保留在最近测试证据目录。

## 外部阻塞

- Windows 驱动真实测试需要 Windows 11 测试 VM、快照和 WDK；
- NAS 接入需要用户在 NAS 本地执行安装；
- 正式驱动签名需要外部签名流程；
- DNS 和生产防火墙变更需要人工批准；
- 现有 PostgreSQL、Redis 公网暴露整改需要用户批准修改 1Panel 或云防火墙，见 `BLK-005`。

## 当前风险

- 自研协议尚未实现和审计；
- Windows 驱动尚未开发；
- 真实设备尚未接入；
- PostgreSQL、Redis 现有公网端口仍可达；
- 服务器提示需要维护窗口重启；
- 临时凭据后续必须轮换。

## 恢复说明

```bash
cd /srv/xs-nexus
git status --short --branch
git log --oneline -10
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
