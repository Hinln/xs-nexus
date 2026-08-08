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
