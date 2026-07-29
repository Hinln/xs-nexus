# 拾枢（XS Nexus）Codex 无人值守开发文件包

本目录是一套用于让 Codex 在较少人工干预下，从空工作区持续完成“拾枢（XS Nexus）”自研内网互通系统开发、测试、修复、视觉验收和发布候选构建的工程控制文件。

它不是一条超长提示词的替代品，而是项目的长期事实来源、行为约束、执行计划和验收依据。

---

## 1. 文件用途

| 文件 | 用途 |
|---|---|
| `XS_Nexus_Codex_Development_Brief.md` | 产品范围、总体架构、核心协议与完整任务书 |
| `AGENTS.md` | Codex 在整个仓库必须遵守的永久规则 |
| `PLANS.md` | 执行计划的编写、维护和恢复规则 |
| `EXECUTION_PLAN.md` | 当前项目的实际阶段、里程碑和验收顺序 |
| `IMPLEMENT.md` | 无人值守实施循环、构建、测试和提交纪律 |
| `ACCEPTANCE.md` | 最终产品级验收条件 |
| `QA_MATRIX.md` | 网络、平台、故障、安装和性能测试矩阵 |
| `BUG_LOOP.md` | 缺陷发现、复现、修复和回归闭环 |
| `VISUAL_QA.md` | Web 控制台浏览器视觉和交互验收规则 |
| `SECURITY_REVIEW.md` | 身份、协议、驱动、供应链和主机安全审查 |
| `ENVIRONMENT.md` | 当前临时开发服务器、1Panel、NAS 和部署边界 |
| `.env.example` | 无真实密码的环境变量模板 |
| `RECOVERY_RUNBOOK.md` | 防止 SSH 失联、网络破坏和部署失败的恢复手册 |
| `RELEASE_CHECKLIST.md` | 发布候选版本的最终检查清单 |
| `PROGRESS.md` | 持续更新的项目状态和上下文恢复入口 |
| `DECISIONS.md` | 技术决策记录 |
| `KNOWN_ISSUES.md` | 已知问题和风险 |
| `BLOCKERS.md` | 外部阻塞和解除方法 |
| `FINAL_REPORT.md` | 最终真实交付报告模板 |
| `BOOTSTRAP_PROMPT.md` | 最后粘贴给 Codex 的唯一启动指令 |
| `apps/console/AGENTS.md` | 前端和视觉验收的目录级规则 |
| `crates/protocol/AGENTS.md` | 协议与密码学目录级规则 |
| `drivers/windows-xsnet/AGENTS.md` | Windows 驱动目录级安全规则 |

---

## 2. 使用步骤

1. 在云平台为临时开发服务器创建系统盘快照。
2. 将本文件包完整上传到服务器上的空项目目录，建议：
   ```bash
   mkdir -p /srv/xs-nexus
   cd /srv/xs-nexus
   ```
3. 不要把聊天中出现的服务器、数据库、Redis 或 NAS 明文密码复制进仓库文件。
4. 在仓库之外创建：
   ```bash
   install -d -m 700 /etc/xs-nexus
   install -m 600 .env.example /etc/xs-nexus/controller.env
   ```
5. 仅在 `/etc/xs-nexus/controller.env` 中填写真实连接信息。
6. 将 `BOOTSTRAP_PROMPT.md` 的全部内容粘贴给 Codex。
7. 允许 Codex自主完成普通开发、构建、测试和缺陷修复。
8. 遇到以下动作时保留人工门禁：
   - 安装 Windows 驱动；
   - 接入真实 NAS；
   - 接入日常使用的 Windows 电脑；
   - 修改生产 DNS；
   - 开放或收紧公网防火墙；
   - 正式驱动签名；
   - 使用离线更新签名私钥；
   - 将 Release Candidate 切换成生产服务。

---

## 3. 凭据规则

当前聊天中的所有密码都应视为临时凭据。

仓库中禁止出现：

- SSH 密码；
- NAS 密码；
- 数据库密码；
- Redis 密码；
- JWT 密钥；
- Cookie 密钥；
- Enrollment Token；
- 节点私钥；
- 发布签名私钥；
- 完整生产连接串；
- 任何可直接登录基础设施的秘密。

允许在仓库内出现：

- `.env.example` 中的占位符；
- 容器名称；
- 非秘密端口；
- 网络名称；
- 测试专用、无权限、可随时销毁的假数据。

开发完成后必须统一轮换所有临时密码。

---

## 4. “无人值守”的真实含义

Codex可以自主完成：

- 方案细化；
- 代码实现；
- 测试环境创建；
- 自动化测试；
- Bug 修复；
- 浏览器 QA；
- 文档更新；
- Git 提交；
- Release Candidate 构建。

Codex不得自主完成：

- 把未经安全审计的协议宣布为生产安全；
- 在真实 NAS 上执行破坏性操作；
- 在日常电脑上测试不稳定驱动；
- 关闭服务器安全机制；
- 删除 1Panel 数据或网络；
- 向公网暴露数据库；
- 使用真实生产签名私钥；
- 隐瞒未完成项或伪造测试结果。

---

## 5. 推荐的启动方式

把工作区作为 Git 仓库使用，并确保 Codex能够：

- 运行 Shell；
- 构建 Rust 和前端；
- 使用 Docker；
- 运行 Playwright；
- 创建 Linux network namespace；
- 读取日志；
- 生成截图；
- 提交 Git。

如果当前 Codex运行环境缺少上述权限，它必须在 `BLOCKERS.md` 留下证据，并继续完成不受影响的工作。
