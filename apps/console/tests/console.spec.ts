import { expect, test, type Page } from "@playwright/test";

import {
  emptySnapshotFixture,
  fulfillJson,
  mockAuthenticatedApi,
  sessionFixture,
  snapshotFixture,
  updatePoliciesFixture,
  updateReleasesFixture,
} from "./fixtures";

test("真实登录后进入首页", async ({ page }) => {
  let authenticated = false;
  await page.route("**/v1/auth/session", (route) =>
    fulfillJson(route, authenticated ? 200 : 401, authenticated ? sessionFixture() : { error: { code: "unauthorized" } }),
  );
  await page.route("**/v1/auth/login", (route) => {
    authenticated = true;
    return fulfillJson(route, 200, sessionFixture());
  });
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 200, snapshotFixture));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  const errors = trackBrowserErrors(page);

  await page.goto("/");
  await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
  expect(errors).not.toEqual([]);
  expect(errors.every((error) => error.includes("status of 401"))).toBe(true);
  errors.length = 0;
  await page.getByLabel("用户名").fill("admin");
  await page.getByLabel("密码").fill("fixture-password-not-sent-anywhere");
  await page.getByRole("button", { name: "登录" }).click();
  await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("所有管理页面可通过主导航访问", async ({ page }) => {
  await mockAuthenticatedApi(page);
  const errors = trackBrowserErrors(page);
  await page.goto("/");
  const pages = [
    ["首页", "运行概览"],
    ["节点", "节点管理"],
    ["拓扑", "网络拓扑"],
    ["网络", "网络"],
    ["地址池", "地址池"],
    ["注册令牌", "Enrollment Token"],
    ["分组与标签", "分组与标签"],
    ["路由审批", "子网路由审批"],
    ["Relay", "Relay"],
    ["ACL", "访问控制 ACL"],
    ["用户权限", "用户与权限"],
    ["审计日志", "审计日志"],
    ["安全告警", "安全告警"],
    ["更新管理", "更新管理"],
    ["系统设置", "系统设置"],
    ["备份恢复", "备份与恢复"],
  ] as const;
  for (const [navigation, heading] of pages) {
    await page.getByRole("button", { name: navigation, exact: true }).click();
    await expect(page.locator("#main-content h1")).toHaveText(heading);
    await expectNoPageOverflow(page);
  }
  expect(errors).toEqual([]);
});

test("Relay 页面展示身份签名指标与 24 小时窗口", async ({ page }) => {
  await mockAuthenticatedApi(page);
  await page.goto("/#/relays");
  await expect(page.getByRole("heading", { name: "Relay" })).toBeVisible();
  await expect(page.getByText("签名指标新鲜")).toBeVisible();
  await expect(page.getByText("2", { exact: true })).toBeVisible();
  await expect(page.getByText("12.0 MiB / 12.0 MiB")).toBeVisible();
  await expect(page.getByText("17,990 / 10")).toBeVisible();
  await expect(page.getByText("6.50 ms")).toBeVisible();
});

test("空数据、无权限和服务错误均有明确状态", async ({ page }) => {
  await mockAuthenticatedApi(page, { snapshot: emptySnapshotFixture() });
  await page.goto("/#/nodes");
  await expect(page.getByRole("heading", { name: "尚无节点" })).toBeVisible();

  await page.unrouteAll({ behavior: "wait" });
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture()));
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 403, { error: { code: "forbidden" } }));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  await page.reload();
  await expect(page.getByRole("heading", { name: "没有访问权限" })).toBeVisible();

  await page.unrouteAll({ behavior: "wait" });
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture()));
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 503, { error: { code: "service_unavailable" } }));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  await page.reload();
  await expect(page.getByRole("heading", { name: "数据读取失败" })).toBeVisible();
  await expect(page.getByText("Controller 暂时无法完成请求")).toBeVisible();
});

test("审计员界面不提供写操作", async ({ page }) => {
  await mockAuthenticatedApi(page, { role: "auditor" });
  await page.goto("/#/nodes");
  await expect(page.getByRole("heading", { name: "节点管理" })).toBeVisible();
  await expect(page.getByRole("button", { name: "吊销凭证" })).toHaveCount(0);
  await page.getByRole("button", { name: "网络", exact: true }).click();
  await expect(page.getByRole("button", { name: "创建网络" })).toHaveCount(0);
  await page.getByRole("button", { name: "更新管理", exact: true }).click();
  await expect(page.getByRole("button", { name: "导入签名发布" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "配置灰度策略" })).toHaveCount(0);
  await expect(page.getByLabel(/的升级通道/)).toHaveCount(0);
});

test("更新页导入公开签名材料并以代次保护灰度策略", async ({ page }) => {
  await mockAuthenticatedApi(page);
  let importedBody: Record<string, unknown> | null = null;
  let policyBody: Record<string, unknown> | null = null;
  await page.unroute("**/v1/admin/update-releases");
  await page.route("**/v1/admin/update-releases", (route) => {
    if (route.request().method() === "POST") {
      importedBody = route.request().postDataJSON() as Record<string, unknown>;
      return fulfillJson(route, 201, { ...updateReleasesFixture[0], id: "70000000-0000-4000-8000-000000000002", version: "0.3.0" });
    }
    return fulfillJson(route, 200, updateReleasesFixture);
  });
  await page.route("**/v1/admin/networks/*/update-policies/*/*/*", (route) => {
    policyBody = route.request().postDataJSON() as Record<string, unknown>;
    return fulfillJson(route, 200, {
      ...updatePoliciesFixture[0],
      generation: 4,
      rollout_basis_points: 5000,
      paused: false,
    });
  });

  await page.goto("/#/updates");
  await expect(page.getByRole("heading", { name: "已验证发布" })).toBeVisible();
  await page.getByRole("button", { name: "导入签名发布" }).click();
  await page.getByLabel("发布清单").setInputFiles({
    name: "xs-nexus-0.3.0.manifest",
    mimeType: "text/plain",
    buffer: Buffer.from("schema_version=1\nproduct=xs-nexus\nversion=0.3.0\n"),
  });
  await page.getByLabel("Ed25519 签名").setInputFiles({
    name: "xs-nexus-0.3.0.manifest.sig",
    mimeType: "application/octet-stream",
    buffer: Buffer.alloc(64, 7),
  });
  await page.getByLabel("HTTPS 归档地址").fill("https://updates.example.test/xs-nexus-0.3.0-x86_64-unknown-linux-gnu.tar.gz");
  await page.getByRole("button", { name: "验证并导入" }).click();
  await expect(page.getByText("已导入不可变发布 0.3.0（x86_64）")).toBeVisible();
  expect(importedBody).not.toBeNull();
  expect(String(importedBody?.manifest_base64)).not.toContain("=");
  expect(String(importedBody?.signature_base64)).toHaveLength(86);

  await page.getByRole("button", { name: "配置灰度策略" }).click();
  await page.getByLabel("灰度比例（%）").fill("50");
  page.once("dialog", async (dialog) => {
    expect(dialog.message()).toContain("50% 节点");
    await dialog.accept();
  });
  await page.getByRole("button", { name: "确认影响并保存" }).click();
  await expect(page.getByText("策略已保存为第 4 代")).toBeVisible();
  expect(policyBody).toMatchObject({
    expected_generation: 3,
    release_id: updateReleasesFixture[0].id,
    minimum_version: "0.1.0",
    rollout_basis_points: 5000,
    paused: false,
  });
});

test("更新页通过签名配置切换节点通道并保护配置版本", async ({ page }) => {
  await mockAuthenticatedApi(page);
  let channelBody: Record<string, unknown> | null = null;
  await page.route("**/v1/admin/networks/*/nodes/*/update-channel", (route) => {
    channelBody = route.request().postDataJSON() as Record<string, unknown>;
    return fulfillJson(route, 200, {
      network_id: snapshotFixture.networks[0].id,
      node_id_base64: snapshotFixture.nodes[0].node_id_base64,
      update_channel: "testing",
      configuration_version: 13,
    });
  });

  await page.goto("/#/updates");
  const channelSelect = page.getByLabel(/成都总部核心网关.*的升级通道/);
  await channelSelect.selectOption("testing");
  page.once("dialog", async (dialog) => {
    expect(dialog.message()).toContain("测试通道");
    await dialog.accept();
  });
  await channelSelect.locator("xpath=..").getByRole("button", { name: "保存" }).click();
  await expect(page.getByText(/配置版本为 13/)).toBeVisible();
  expect(channelBody).toEqual({
    expected_configuration_version: 12,
    update_channel: "testing",
  });
});

test("更新页覆盖加载、空数据、无权限与服务错误", async ({ page }) => {
  await mockAuthenticatedApi(page, { snapshot: emptySnapshotFixture() });
  await page.unroute("**/v1/admin/update-releases");
  await page.route("**/v1/admin/update-releases", async (route) => {
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 300));
    return fulfillJson(route, 200, []);
  });
  await page.goto("/#/updates");
  await expect(page.getByRole("heading", { name: "正在加载" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "尚无签名发布" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "尚无更新策略" })).toBeVisible();

  await page.unroute("**/v1/admin/update-releases");
  await page.route("**/v1/admin/update-releases", (route) => fulfillJson(route, 403, { error: { code: "forbidden" } }));
  await page.reload();
  await expect(page.getByRole("heading", { name: "没有访问权限" })).toBeVisible();

  await page.unroute("**/v1/admin/update-releases");
  await page.route("**/v1/admin/update-releases", (route) => fulfillJson(route, 503, { error: { code: "service_unavailable" } }));
  await page.reload();
  await expect(page.getByRole("heading", { name: "数据读取失败" })).toBeVisible();
  await expect(page.getByText("Controller 暂时无法完成请求")).toBeVisible();
});

test("键盘可到达跳转链接和主导航", async ({ page }) => {
  await mockAuthenticatedApi(page);
  await page.goto("/");
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name: "跳到主要内容" })).toBeFocused();
  await page.getByRole("link", { name: "跳到主要内容" }).press("Enter");
  await expect(page.locator("#main-content")).toBeFocused();
});

test("节点详情支持键盘关闭并恢复焦点", async ({ page }) => {
  await mockAuthenticatedApi(page);
  await page.goto("/#/nodes");
  const detailButton = page.getByRole("button", { name: "查看详情" }).first();
  await detailButton.focus();
  await detailButton.press("Enter");
  await expect(page.getByRole("dialog", { name: /成都总部核心网关/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(detailButton).toBeFocused();

  await page.goto("/#/missing-page");
  await expect(page.getByRole("heading", { name: "页面不存在" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "无法找到页面" })).toBeVisible();
});

function trackBrowserErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  return errors;
}

async function expectNoPageOverflow(page: Page) {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);
}
