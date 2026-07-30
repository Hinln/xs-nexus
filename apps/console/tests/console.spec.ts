import { expect, test, type Page } from "@playwright/test";

import {
  emptySnapshotFixture,
  fulfillJson,
  mockAuthenticatedApi,
  sessionFixture,
  snapshotFixture,
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
