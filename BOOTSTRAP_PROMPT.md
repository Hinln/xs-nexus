# 粘贴给 Codex 的唯一启动指令

进入本项目的无人值守自治开发模式。

首先完整读取并遵守：

- `00_README_FIRST.md`
- `AGENTS.md`
- `XS_Nexus_Codex_Development_Brief.md`
- `PLANS.md`
- `EXECUTION_PLAN.md`
- `IMPLEMENT.md`
- `ACCEPTANCE.md`
- `QA_MATRIX.md`
- `BUG_LOOP.md`
- `VISUAL_QA.md`
- `SECURITY_REVIEW.md`
- `ENVIRONMENT.md`
- `RECOVERY_RUNBOOK.md`
- `RELEASE_CHECKLIST.md`
- `PROGRESS.md`
- `DECISIONS.md`
- `KNOWN_ISSUES.md`
- `BLOCKERS.md`

这些文件共同构成本项目的长期事实来源。进入子目录时，还必须读取该目录中更严格的 `AGENTS.md`。

你不是只负责输出计划、架构建议或代码示例，而是负责从当前工作区状态持续实施、验证、修复和整理整个项目。

## 自治执行

持续执行：

1. 读取 `PROGRESS.md` 和 Git 状态；
2. 读取当前最早未完成的里程碑；
3. 检查现有代码、测试和历史，避免重复和功能回退；
4. 先建立或运行能够证明需求的测试；
5. 完成真实实现；
6. 运行格式化、静态分析、编译、单元测试、集成测试、端到端测试和适用的网络实验；
7. 失败时定位根因并修复，不得删除、跳过、屏蔽或弱化失败测试；
8. 更新 `PROGRESS.md`、`DECISIONS.md`、`KNOWN_ISSUES.md`、`BLOCKERS.md` 和相关文档；
9. 验证通过后创建范围单一、说明清晰的 Git 提交；
10. 立即进入下一里程碑；
11. 上下文较长时先把恢复信息完整写入 `PROGRESS.md`，然后继续。

普通技术选择不要向我询问。选择安全、稳定、可测试、可维护且符合项目规则的方案，并记录技术决策。

## 环境

项目在只安装了 1Panel 的临时 Linux 服务器开发。

Controller、Relay、Console 和普通后台服务使用 Docker Compose，并加入已经存在的外部 Docker 网络：

- `1panel-network`
- 子网 `172.18.0.0/16`

不得重建、删除、修改或接管该网络。

Linux Agent、TUN、Netlink、namespace、`nftables` 和 `tc` 优先在宿主机运行。不得为了方便给全部容器无限制特权。

数据库和 Redis 使用现有 1Panel 容器，通过仓库外的 `/etc/xs-nexus/controller.env` 注入秘密。不得把真实密码、Token、私钥或完整连接串写入 Git、日志、截图、状态文件、Docker 镜像或构建产物。

## 主机保护

在执行任何可能影响 SSH、默认路由、防火墙、Docker 网络或 1Panel 的操作前，必须：

1. 保存基线；
2. 准备自动回滚；
3. 保留 SSH 管理端口；
4. 限制变更范围；
5. 验证后再取消回滚。

禁止：

- 删除 1Panel 资源；
- 删除未知 Docker 卷；
- 执行全局 prune；
- 清空防火墙；
- 暴露数据库到公网；
- 修改无关网站；
- 让测试 TUN 接管无关流量。

## 禁止依赖

核心能力不得使用、调用、封装、复制或改名依赖：

- Tailscale；
- Headscale；
- WireGuard；
- Wintun；
- ZeroTier；
- NetBird；
- Nebula；
- OpenVPN；
- SoftEther；
- FRP；
- rathole；
- nps；
- coturn；
- TAP-Windows；
- 其他现成组网、VPN、穿透和中继产品。

允许使用公开标准、操作系统原生接口、通用框架和标准密码学库，但不得自行实现密码学原语。

## 开发顺序

严格按照 `EXECUTION_PLAN.md`：

1. 环境基线；
2. Clean-room、架构、协议和威胁模型；
3. Controller；
4. Linux Agent 和 TUN；
5. 两节点 XSP/1 真实加密链路；
6. NAT 探测和打洞；
7. 自研 Relay；
8. ACL、IPAM 和子网路由；
9. 控制台；
10. Playwright 和视觉验收；
11. 安装、升级、回滚和卸载；
12. Windows 驱动；
13. Bug 集中修复；
14. 安全、性能和稳定性复核；
15. Release Candidate。

不要在 Linux 核心链路、安装回滚和路由安全未通过前要求接入真实 NAS。

不要在 Windows 测试虚拟机、测试签名、Driver Verifier、安装卸载和无蓝屏验收通过前要求接入用户日常 Windows 电脑。

## 完成后的独立验收循环

功能实现完成后不得立即结束，必须自动执行：

第一轮：全量构建、测试、网络实验、安装、升级、卸载、异常恢复，建立 Bug 清单。

第二轮：逐项复现和修复。每项核心 Bug 先增加失败测试，再修复并全量回归。

第三轮：启动真实前后端和固定测试数据，使用 Playwright 遍历所有页面、状态和核心流程，在 `VISUAL_QA.md` 的分辨率下截图、检查和修复。

第四轮：以独立审查者视角检查协议、身份、权限、路由、Relay、驱动、更新供应链、日志脱敏和故障恢复。

第五轮：执行性能、资源泄漏、长时间运行、Controller 故障、Relay 故障、网络变化和路由残留测试。

只有满足 `ACCEPTANCE.md` 和 `RELEASE_CHECKLIST.md`，并如实完成 `FINAL_REPORT.md`，才允许标记 Release Candidate。

## 外部阻塞

Windows 测试机、真实 NAS、本地 Windows、DNS、正式驱动签名和生产防火墙属于人工门禁。

遇到外部阻塞：

1. 不得伪造完成；
2. 写入 `BLOCKERS.md`；
3. 附证据和解除步骤；
4. 完成所有不受影响的工作；
5. 仅在全部剩余工作都被不可绕过的外部条件阻塞时停止。

现在开始执行。不要停留在计划层面。
