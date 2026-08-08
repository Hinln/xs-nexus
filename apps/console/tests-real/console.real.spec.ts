import { expect, test, type Page } from "@playwright/test";
import { readFile, stat } from "node:fs/promises";

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

test("真实 Controller 登录、导航、刷新与退出", async ({ page }) => {
  const username = requiredEnvironment("XS_CONSOLE_E2E_USERNAME");
  const password = await readPrivateCredential(requiredEnvironment("XS_CONSOLE_E2E_PASSWORD_FILE"));
  const browserErrors: string[] = [];
  const requestFailures: string[] = [];
  const serverFailures: string[] = [];

  page.on("pageerror", (error) => browserErrors.push(error.message));
  page.on("requestfailed", (request) => {
    requestFailures.push(`${request.method()} ${new URL(request.url()).pathname}: ${request.failure()?.errorText ?? "unknown"}`);
  });
  page.on("response", (response) => {
    const pathname = new URL(response.url()).pathname;
    if (pathname.startsWith("/v1/") && response.status() >= 500) {
      serverFailures.push(`${response.status()} ${pathname}`);
    }
  });

  await page.goto("/");
  await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();

  const loginResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname === "/v1/auth/login",
  );
  await page.getByLabel("用户名").fill(username);
  await page.getByLabel("密码").fill(password);
  await page.getByRole("button", { name: "登录" }).click();
  expect((await loginResponse).status()).toBe(200);
  await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();

  for (const [navigation, heading] of pages) {
    await page.getByRole("button", { name: navigation, exact: true }).click();
    await expect(page.locator("#main-content h1")).toHaveText(heading);
    await expectNoPageOverflow(page);
  }

  const refreshResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname === "/v1/admin/console",
  );
  await page.getByRole("button", { name: "刷新数据" }).click();
  expect((await refreshResponse).status()).toBe(200);

  const logoutResponse = page.waitForResponse((response) =>
    new URL(response.url()).pathname === "/v1/auth/logout",
  );
  await page.getByRole("button", { name: "退出" }).click();
  expect((await logoutResponse).status()).toBe(204);
  await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();

  expect(browserErrors).toEqual([]);
  expect(requestFailures).toEqual([]);
  expect(serverFailures).toEqual([]);
});

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

async function readPrivateCredential(path: string): Promise<string> {
  const metadata = await stat(path);
  if (!metadata.isFile() || (metadata.mode & 0o077) !== 0) {
    throw new Error("console E2E credential must be a private regular file");
  }
  const value = (await readFile(path, "utf8")).replace(/\r?\n$/, "");
  if (value.length < 12 || value.includes("\n") || value.includes("\r")) {
    throw new Error("console E2E credential is invalid");
  }
  return value;
}

async function expectNoPageOverflow(page: Page) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
}
