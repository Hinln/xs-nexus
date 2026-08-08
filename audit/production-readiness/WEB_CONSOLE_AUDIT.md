# Web Console Audit

## Runtime

- 本地 `127.0.0.1:28081/` 返回 200，静态 `index.html` SHA-256 与生产 Console 镜像一致。
- `vpn.qinwen.co` 的 `/health/ready` 和 `/install` 可用，但 root/`index.html` 被反向代理到 Controller JSON 404，Console 不可用。
- `vpn.xiashikeji.cn` 的应用路径返回 525。
- 公网 API 缺少 HSTS 等安全头；错误 JSON 暴露详细反序列化信息。

## Fresh Playwright

- Playwright 1.62.1 容器 digest `sha256:dcc5531e97840b9b5e794f2814476b21571c5124a3fca2267d73041f56e7580e`。
- 10 项 E2E、2 项 visual 测试通过，生成 135 张新截图，覆盖六个要求分辨率和 normal/empty/loading/forbidden/error/large-data 状态。
- 测试源码对所有关键 API 使用 `page.route`；名为“真实登录后进入首页”的测试也使用 Mock。因此分类为 `SIMULATED_ONLY`，不得作为 Hard Gate 19 PASS。

## Result

`FAIL`。真实 Controller/DB/Redis/Console Playwright 和计划公网 Console 都未通过。

## Final Remediation Reassessment

- 新门禁启动真实临时 PostgreSQL、真实 Controller 和 Vite proxy，生成私有临时测试凭据后执行 Playwright；测试源码不调用 `page.route`。
- 验证真实登录、全部管理导航页、刷新 200、注销 204，且无 page error、5xx 或未解释网络失败。run `31270487478` 与 artifact `9025475853` 成功。
- 该门禁关闭“只有 Mock E2E”的内部缺口，但计划域名和正式公网 Console 路由仍未修复，生产仍运行旧 Console 镜像。

最终结果：Gate 19 从 `FAIL` 改善为 `PARTIAL`，仍不是生产 PASS。
