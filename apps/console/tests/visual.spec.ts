import { expect, test, type Page } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";

import {
  emptySnapshotFixture,
  fulfillJson,
  mockAuthenticatedApi,
  sessionFixture,
  stressSnapshotFixture,
} from "./fixtures";

const viewports = [
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1920x1080", width: 1920, height: 1080 },
  { name: "1280x720", width: 1280, height: 720 },
  { name: "1024x768", width: 1024, height: 768 },
  { name: "768x1024", width: 768, height: 1024 },
  { name: "390x844", width: 390, height: 844 },
] as const;

const pages = [
  ["dashboard", "首页", "运行概览"],
  ["nodes", "节点", "节点管理"],
  ["topology", "拓扑", "网络拓扑"],
  ["networks", "网络", "网络"],
  ["address-pools", "地址池", "地址池"],
  ["tokens", "注册令牌", "Enrollment Token"],
  ["groups", "分组与标签", "分组与标签"],
  ["routes", "路由审批", "子网路由审批"],
  ["relays", "Relay", "Relay"],
  ["acl", "ACL", "访问控制 ACL"],
  ["users", "用户权限", "用户与权限"],
  ["audit", "审计日志", "审计日志"],
  ["alerts", "安全告警", "安全告警"],
  ["updates", "更新管理", "更新管理"],
  ["settings", "系统设置", "系统设置"],
  ["backup", "备份恢复", "备份与恢复"],
] as const;

const outputRoot = resolve(process.cwd(), "../../artifacts/visual/m4.1");

test("@visual 固定视口页面截图与溢出检查", async ({ page }) => {
  test.setTimeout(300_000);
  const failures = monitorBrowser(page);

  for (const viewport of viewports) {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.unrouteAll({ behavior: "wait" });
    await page.route("**/v1/auth/session", (route) => fulfillJson(route, 401, { error: { code: "unauthorized" } }));
    await page.goto(`/?fixture=login-${viewport.name}`);
    await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
    await waitForStableRendering(page);
    await expectNoPageOverflow(page, `${viewport.name} login`);
    await saveScreenshot(page, viewport.name, "login", "normal");
    expectExpectedResponses(failures, 401, "/v1/auth/session");

    await page.unrouteAll({ behavior: "wait" });
    await mockAuthenticatedApi(page);
    await page.goto(`/?fixture=normal-${viewport.name}`);
    for (const [pageId, navigation, heading] of pages) {
      await navigate(page, viewport.width, navigation);
      await expect(page.locator("#main-content h1")).toHaveText(heading);
      await waitForStableRendering(page);
      await expectNoPageOverflow(page, `${viewport.name} ${pageId}`);
      await saveScreenshot(page, viewport.name, pageId, "normal");
    }

    await navigate(page, viewport.width, "节点");
    await page.getByRole("button", { name: "查看详情" }).first().click();
    await expect(page.getByRole("dialog", { name: /成都总部核心网关/ })).toBeVisible();
    await expectNoPageOverflow(page, `${viewport.name} node-detail`);
    await saveScreenshot(page, viewport.name, "node-detail", "normal");
    await page.getByRole("button", { name: "关闭节点详情" }).click();

    await page.goto(`/?fixture=normal-${viewport.name}#/missing-page`);
    await expect(page.locator("#main-content h1")).toHaveText("页面不存在");
    await waitForStableRendering(page);
    await expectNoPageOverflow(page, `${viewport.name} not-found`);
    await saveScreenshot(page, viewport.name, "not-found", "normal");
    expectClean(failures);
  }
});

test("@visual 桌面状态、大量数据和长 IPv6 矩阵", async ({ page }) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 1440, height: 900 });
  const failures = monitorBrowser(page);

  await mockAuthenticatedApi(page, { snapshot: emptySnapshotFixture() });
  await page.goto("/?fixture=empty");
  for (const [pageId, navigation, heading] of pages) {
    await navigate(page, 1440, navigation);
    await expect(page.locator("#main-content h1")).toHaveText(heading);
    await waitForStableRendering(page);
    await expectNoPageOverflow(page, `1440x900 ${pageId} empty`);
    await saveScreenshot(page, "1440x900", pageId, "empty");
  }
  expectClean(failures);

  await page.unrouteAll({ behavior: "wait" });
  await mockAuthenticatedApi(page, { snapshot: stressSnapshotFixture() });
  await page.goto("/?fixture=stress#/nodes");
  await expect(page.getByText("fd7a:115c:a1e0", { exact: false }).first()).toBeVisible();
  await expectNoPageOverflow(page, "1440x900 nodes stress");
  await saveScreenshot(page, "1440x900", "nodes", "large-data-long-ipv6");
  await navigate(page, 1440, "拓扑");
  await expectNoPageOverflow(page, "1440x900 topology stress");
  await saveScreenshot(page, "1440x900", "topology", "large-data-long-ipv6");
  expectClean(failures);

  await page.unrouteAll({ behavior: "wait" });
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture()));
  await page.route("**/v1/admin/console", async (route) => {
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 1_500));
    return fulfillJson(route, 200, stressSnapshotFixture());
  });
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  await page.goto("/?fixture=loading#/dashboard");
  await expect(page.getByRole("heading", { name: "正在加载" })).toBeVisible();
  await saveScreenshot(page, "1440x900", "global", "loading");
  expectClean(failures);

  await page.unrouteAll({ behavior: "wait" });
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture()));
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 403, { error: { code: "forbidden" } }));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  await page.goto("/?fixture=forbidden#/dashboard");
  await expect(page.getByRole("heading", { name: "没有访问权限" })).toBeVisible();
  await saveScreenshot(page, "1440x900", "global", "forbidden");
  expectExpectedResponses(failures, 403, "/v1/admin/console");

  await page.unrouteAll({ behavior: "wait" });
  await page.route("**/v1/auth/session", (route) => fulfillJson(route, 200, sessionFixture()));
  await page.route("**/v1/admin/console", (route) => fulfillJson(route, 503, { error: { code: "service_unavailable" } }));
  await page.route("**/v1/admin/users", (route) => fulfillJson(route, 200, []));
  await page.reload();
  await expect(page.getByRole("heading", { name: "数据读取失败" })).toBeVisible();
  await saveScreenshot(page, "1440x900", "global", "server-error");
  expectExpectedResponses(failures, 503, "/v1/admin/console");
  expectClean(failures);
});

async function navigate(page: Page, viewportWidth: number, navigation: string) {
  if (viewportWidth <= 1080) {
    await page.getByRole("button", { name: "打开导航" }).click();
  }
  await page.getByRole("button", { name: navigation, exact: true }).click();
}

async function waitForStableRendering(page: Page) {
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
}

async function saveScreenshot(page: Page, viewport: string, pageId: string, state: string) {
  const directory = resolve(outputRoot, viewport, pageId);
  await mkdir(directory, { recursive: true });
  await page.screenshot({
    path: resolve(directory, `${state}.png`),
    fullPage: true,
    animations: "disabled",
  });
}

async function expectNoPageOverflow(page: Page, context: string) {
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow, `${context} 页面横向溢出`).toBeLessThanOrEqual(1);
}

interface BrowserFailures {
  consoleErrors: string[];
  pageErrors: string[];
  failedResponses: Array<{ status: number; url: string }>;
}

function monitorBrowser(page: Page): BrowserFailures {
  const failures: BrowserFailures = { consoleErrors: [], pageErrors: [], failedResponses: [] };
  page.on("console", (message) => {
    if (message.type() === "error") failures.consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => failures.pageErrors.push(error.message));
  page.on("response", (response) => {
    if (response.status() >= 400) failures.failedResponses.push({ status: response.status(), url: response.url() });
  });
  return failures;
}

function expectExpectedResponses(failures: BrowserFailures, status: number, path: string) {
  expect(failures.failedResponses.length).toBeGreaterThan(0);
  expect(failures.failedResponses.every((response) => response.status === status && new URL(response.url).pathname === path)).toBe(true);
  expect(failures.pageErrors).toEqual([]);
  expect(failures.consoleErrors.every((error) => error.includes(`status of ${status}`))).toBe(true);
  failures.failedResponses.length = 0;
  failures.consoleErrors.length = 0;
}

function expectClean(failures: BrowserFailures) {
  expect(failures.failedResponses).toEqual([]);
  expect(failures.consoleErrors).toEqual([]);
  expect(failures.pageErrors).toEqual([]);
}
