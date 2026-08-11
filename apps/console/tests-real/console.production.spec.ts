import {
  expect,
  test,
  type APIRequestContext,
  type Page,
  type Request,
} from "@playwright/test";
import { generateKeyPairSync } from "node:crypto";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const managementPages = [
  { id: "dashboard", navigation: "首页", heading: "运行概览" },
  { id: "nodes", navigation: "节点", heading: "节点管理" },
  { id: "topology", navigation: "拓扑", heading: "网络拓扑" },
  { id: "networks", navigation: "网络", heading: "网络" },
  { id: "address-pools", navigation: "地址池", heading: "地址池" },
  { id: "tokens", navigation: "注册令牌", heading: "Enrollment Token" },
  { id: "groups", navigation: "分组与标签", heading: "分组与标签" },
  { id: "routes", navigation: "路由审批", heading: "子网路由审批" },
  { id: "relays", navigation: "Relay", heading: "Relay" },
  { id: "acl", navigation: "ACL", heading: "访问控制 ACL" },
  { id: "users", navigation: "用户权限", heading: "用户与权限" },
  { id: "audit", navigation: "审计日志", heading: "审计日志" },
  { id: "alerts", navigation: "安全告警", heading: "安全告警" },
  { id: "updates", navigation: "更新管理", heading: "更新管理" },
  { id: "settings", navigation: "系统设置", heading: "系统设置" },
  { id: "backup", navigation: "备份恢复", heading: "备份与恢复" },
] as const;

const viewports = [
  { name: "1920x1080", width: 1920, height: 1080 },
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1280x720", width: 1280, height: 720 },
  { name: "1024x768", width: 1024, height: 768 },
  { name: "768x1024", width: 768, height: 1024 },
  { name: "390x844", width: 390, height: 844 },
] as const;

const evidenceDirectory = resolve(requiredEnvironment("XS_CONSOLE_E2E_EVIDENCE_DIR"));
const screenshotDirectory = resolve(evidenceDirectory, "screenshots");
const networkName = "gate19-production-network";
const firstNodeName = "gate19-node-a";
const secondNodeName = "gate19-node-b";
const auditorUsername = "gate19-auditor";
const scenarioResults: Array<{ name: string; status: string }> = [];
const browserObservations: BrowserObservation[] = [];

interface SessionPayload {
  csrf_token: string;
  user: {
    username: string;
    role: string;
  };
}

interface HttpFailure {
  method: string;
  path: string;
  status: number;
}

interface RequestFailure {
  method: string;
  path: string;
  error: string;
}

interface BrowserObservation {
  label: string;
  consoleErrors: string[];
  pageErrors: string[];
  httpFailures: HttpFailure[];
  requestFailures: RequestFailure[];
}

test.describe.serial("真实生产 Console 完整矩阵", () => {
  test.afterEach(async ({}, testInfo) => {
    scenarioResults.push({
      name: testInfo.title,
      status: testInfo.status ?? "unknown",
    });
  });

  test.afterAll(async () => {
    await mkdir(evidenceDirectory, { recursive: true });
    const scenarioLines = [
      "scenario\tstatus",
      ...scenarioResults.map((result) => `${result.name}\t${result.status}`),
    ];
    await writeFile(
      resolve(evidenceDirectory, "scenario-matrix.tsv"),
      `${scenarioLines.join("\n")}\n`,
      "utf8",
    );
    await writeFile(
      resolve(evidenceDirectory, "browser-observations.json"),
      `${JSON.stringify(browserObservations, null, 2)}\n`,
      "utf8",
    );
  });

  test("真实 loading、登录、空状态、双提交和数据初始化", async ({ page, request }) => {
    test.setTimeout(180_000);
    const observation = observePage(page, "bootstrap-and-empty-state");
    const controllerPid = requiredPositiveInteger("XS_CONSOLE_E2E_CONTROLLER_PID");
    const adminPassword = await readPrivateCredential(
      requiredEnvironment("XS_CONSOLE_E2E_PASSWORD_FILE"),
    );
    const auditorPassword = await readPrivateCredential(
      requiredEnvironment("XS_CONSOLE_E2E_AUDITOR_PASSWORD_FILE"),
    );

    let controllerPaused = false;
    try {
      process.kill(controllerPid, "SIGSTOP");
      controllerPaused = true;
      await page.goto("/");
      await expect(page.getByRole("heading", { name: "正在加载" })).toBeVisible();
      await expect(page.getByText("正在验证安全会话")).toBeVisible();
      await saveScreenshot(page, "states", "loading-real-controller");
    } finally {
      if (controllerPaused) process.kill(controllerPid, "SIGCONT");
    }

    await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
    const rejectedLogin = page.waitForResponse(
      (response) => apiPath(response.url()) === "/v1/auth/login",
    );
    await page.getByLabel("用户名").fill(requiredEnvironment("XS_CONSOLE_E2E_USERNAME"));
    await page.getByLabel("密码").fill(`${adminPassword}x`);
    await page.getByRole("button", { name: "登录" }).click();
    expect((await rejectedLogin).status()).toBe(401);
    await expect(page.getByText("登录已失效，请重新登录")).toBeVisible();

    const session = await submitLogin(
      page,
      requiredEnvironment("XS_CONSOLE_E2E_USERNAME"),
      adminPassword,
    );
    expect(session.user.role).toBe("administrator");

    for (const target of managementPages) {
      await navigate(page, target.navigation);
      await expect(page.locator("#main-content h1")).toHaveText(target.heading);
      await expectNoDocumentOverflow(page, `initial ${target.id}`);
      await saveScreenshot(page, "initial-real-state", target.id);
    }

    await navigate(page, "网络");
    await expect(page.getByRole("heading", { name: "尚无网络" })).toBeVisible();
    await page.getByRole("button", { name: "创建网络" }).click();
    await page.getByLabel("网络名称").fill(networkName);
    await page.getByLabel("IPv4 地址池").fill("100.126.0.0/24");
    await page.getByLabel("预留地址数").fill("16");

    let networkCreateRequests = 0;
    const countNetworkCreate = (requestValue: Request) => {
      if (
        requestValue.method() === "POST" &&
        apiPath(requestValue.url()) === "/v1/admin/networks"
      ) {
        networkCreateRequests += 1;
      }
    };
    page.on("request", countNetworkCreate);
    const networkCreated = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        apiPath(response.url()) === "/v1/admin/networks",
    );
    await page.getByRole("button", { name: "确认创建" }).evaluate((button) => {
      (button as HTMLButtonElement).click();
      (button as HTMLButtonElement).click();
    });
    expect((await networkCreated).status()).toBe(201);
    await expect(page.getByRole("heading", { name: networkName })).toBeVisible();
    await page.waitForTimeout(200);
    page.off("request", countNetworkCreate);
    expect(networkCreateRequests).toBe(1);

    await navigate(page, "注册令牌");
    await page.getByRole("button", { name: "创建令牌" }).click();
    await page.getByLabel("最大使用次数").fill("2");
    await page.getByLabel("默认标签").fill("linux, gate19");
    const tokenCreated = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        apiPath(response.url()) === "/v1/admin/enrollment-tokens",
    );
    await page.getByRole("button", { name: "确认创建" }).click();
    expect((await tokenCreated).status()).toBe(201);
    const enrollmentToken = (
      await page.locator("code.secret-once").textContent()
    )?.trim();
    expect(enrollmentToken).toBeTruthy();

    await enrollNode(request, enrollmentToken ?? "", firstNodeName);
    await enrollNode(request, enrollmentToken ?? "", secondNodeName);
    await page.getByRole("button", { name: "我已保存并关闭" }).click();
    await refreshSnapshot(page);
    await navigate(page, "节点");
    await expect(page.getByText(firstNodeName, { exact: true })).toBeVisible();
    await expect(page.getByText(secondNodeName, { exact: true })).toBeVisible();

    await navigate(page, "用户权限");
    await page.getByRole("button", { name: "创建用户" }).click();
    await page.getByLabel("用户名").fill(auditorUsername);
    await page.getByLabel("显示名称").fill("Gate 19 审计员");
    await page.getByLabel("初始密码").fill(auditorPassword);
    await page.getByLabel("角色").selectOption("auditor");
    const userCreated = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        apiPath(response.url()) === "/v1/admin/users",
    );
    await page.getByRole("button", { name: "确认创建" }).click();
    expect((await userCreated).status()).toBe(201);
    await expect(page.getByText(`@${auditorUsername}`, { exact: true })).toBeVisible();

    await navigate(page, "审计日志");
    await expect(page.getByRole("table", { name: "最新审计事件" })).toBeVisible();
    await expect(page.getByRole("row")).not.toHaveCount(1);
    assertObservation(observation, [401]);
  });

  test("真实节点详情、ACL 与 400/403/404 API 边界", async ({ page }) => {
    const observation = observePage(page, "node-acl-and-api-boundary");
    const session = await loginAsAdministrator(page);

    await navigate(page, "节点");
    const detailButton = page
      .getByRole("row")
      .filter({ hasText: firstNodeName })
      .getByRole("button", { name: "查看详情" });
    await detailButton.focus();
    await detailButton.press("Enter");
    await expect(page.getByRole("dialog", { name: new RegExp(firstNodeName) })).toBeVisible();
    await expectAllInteractiveControlsNamed(page);
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(detailButton).toBeFocused();

    await navigate(page, "ACL");
    const sourceSelector = page.getByLabel("源节点");
    const destinationSelector = page.getByLabel("目标节点");
    const nodeValues = await sourceSelector.locator("option").evaluateAll((options) =>
      options.map((option) => (option as HTMLOptionElement).value),
    );
    expect(nodeValues).toHaveLength(2);
    await sourceSelector.selectOption(nodeValues[0]);
    await destinationSelector.selectOption(nodeValues[1]);
    await page.getByLabel("协议").selectOption("tcp");
    await page.getByLabel("目标端口").fill("443");
    const explanationResponse = page.waitForResponse(
      (response) => apiPath(response.url()).endsWith("/acl/explain"),
    );
    await page.getByRole("button", { name: "解释连接" }).click();
    expect((await explanationResponse).status()).toBe(200);
    await expect(page.locator(".decision strong")).toHaveText("拒绝");
    await expect(page.locator(".decision code")).toHaveText("default_deny");

    const malformed = await page.evaluate(async (csrfToken) => {
      const response = await fetch("/v1/admin/networks", {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
          "X-CSRF-Token": csrfToken,
        },
        body: "{",
      });
      return { status: response.status, body: (await response.json()) as unknown };
    }, session.csrf_token);
    expect(malformed).toEqual({
      status: 400,
      body: {
        error: {
          code: "invalid_request",
          message: "request validation failed",
        },
      },
    });

    const missingCsrf = await page.evaluate(async () => {
      const response = await fetch("/v1/admin/networks", {
        method: "POST",
        headers: { Accept: "application/json", "Content-Type": "application/json" },
        body: JSON.stringify({
          name: "gate19-csrf-must-fail",
          address_pool: "100.125.0.0/24",
          reserved_addresses: 16,
        }),
      });
      return response.status;
    });
    expect(missingCsrf).toBe(403);

    const missingEndpoint = await page.evaluate(async () => {
      const response = await fetch("/v1/admin/not-a-real-endpoint", {
        headers: { Accept: "application/json" },
      });
      return response.status;
    });
    expect(missingEndpoint).toBe(404);
    assertObservation(observation, [400, 401, 403, 404]);
  });

  test("真实并发页面拒绝过期配置版本", async ({ page, context }) => {
    const firstObservation = observePage(page, "stale-state-first-page");
    await loginAsAdministrator(page);
    await navigate(page, "更新管理");

    const secondPage = await context.newPage();
    const secondObservation = observePage(secondPage, "stale-state-second-page");
    await secondPage.goto("/#/updates");
    await expect(secondPage.getByRole("heading", { name: "更新管理" })).toBeVisible();

    const secondSelector = secondPage.getByLabel(`节点 ${firstNodeName} 的升级通道`);
    await secondSelector.selectOption("testing");
    secondPage.once("dialog", async (dialog) => dialog.accept());
    const acceptedUpdate = secondPage.waitForResponse(
      (response) => apiPath(response.url()).endsWith("/update-channel"),
    );
    await secondSelector
      .locator("xpath=..")
      .getByRole("button", { name: "保存" })
      .click();
    expect((await acceptedUpdate).status()).toBe(200);
    await expect(secondPage.getByText(/配置版本为 [0-9]+/)).toBeVisible();

    const firstSelector = page.getByLabel(`节点 ${firstNodeName} 的升级通道`);
    await firstSelector.selectOption("development");
    page.once("dialog", async (dialog) => dialog.accept());
    const rejectedUpdate = page.waitForResponse(
      (response) => apiPath(response.url()).endsWith("/update-channel"),
    );
    await firstSelector
      .locator("xpath=..")
      .getByRole("button", { name: "保存" })
      .click();
    expect((await rejectedUpdate).status()).toBe(409);
    await expect(page.getByText("资源已变化或与当前配置冲突")).toBeVisible();

    await page.reload();
    await expect(page.getByRole("heading", { name: "更新管理" })).toBeVisible();
    await expect(page.getByLabel(`节点 ${firstNodeName} 的升级通道`)).toHaveValue(
      "testing",
    );
    await secondPage.close();
    assertObservation(firstObservation, [401, 409]);
    assertObservation(secondObservation);
  });

  test("真实六视口页面、表格、模态框、键盘和可访问名称", async ({ page, browser }) => {
    test.setTimeout(360_000);
    const observation = observePage(page, "authenticated-visual-matrix");
    await loginAsAdministrator(page);

    for (const viewport of viewports) {
      const loginContext = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        colorScheme: "light",
        reducedMotion: "reduce",
      });
      const loginPage = await loginContext.newPage();
      const loginObservation = observePage(loginPage, `login-${viewport.name}`);
      await loginPage.goto("/");
      await expect(loginPage.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
      await expectNoDocumentOverflow(loginPage, `${viewport.name} login`);
      await expectAllInteractiveControlsNamed(loginPage);
      await saveScreenshot(loginPage, viewport.name, "login");
      assertObservation(loginObservation, [401]);
      await loginContext.close();

      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      for (const target of managementPages) {
        await navigate(page, target.navigation);
        await expect(page.locator("#main-content h1")).toHaveText(target.heading);
        await waitForStableRendering(page);
        await expectNoDocumentOverflow(page, `${viewport.name} ${target.id}`);
        if (viewport.name === "1440x900") {
          await expectAllInteractiveControlsNamed(page);
        }
        await saveScreenshot(page, viewport.name, target.id);
      }

      await navigate(page, "节点");
      const detailButton = page
        .getByRole("row")
        .filter({ hasText: firstNodeName })
        .getByRole("button", { name: "查看详情" });
      await detailButton.click();
      await expect(page.getByRole("dialog", { name: new RegExp(firstNodeName) })).toBeVisible();
      await expectNoDocumentOverflow(page, `${viewport.name} node-detail`);
      await expectAllInteractiveControlsNamed(page);
      await saveScreenshot(page, viewport.name, "node-detail");
      await page.getByRole("button", { name: "关闭节点详情" }).click();

      await page.goto("/#/missing-page");
      await expect(page.locator("#main-content h1")).toHaveText("页面不存在");
      await expectNoDocumentOverflow(page, `${viewport.name} not-found`);
      await saveScreenshot(page, viewport.name, "not-found");
    }

    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: "跳到主要内容" })).toBeFocused();
    await page.getByRole("link", { name: "跳到主要内容" }).press("Enter");
    await expect(page.locator("#main-content")).toBeFocused();

    await navigate(page, "节点");
    const tableShellOverflow = await page.locator(".table-shell").first().evaluate((element) =>
      getComputedStyle(element).overflowX,
    );
    expect(["auto", "scroll"]).toContain(tableShellOverflow);
    assertObservation(observation, [401]);
  });

  test("真实破坏性确认取消后不执行、确认后仅执行一次", async ({ page }) => {
    const observation = observePage(page, "destructive-confirmation");
    await loginAsAdministrator(page);
    await navigate(page, "节点");
    const row = page.getByRole("row").filter({ hasText: firstNodeName });
    const revokeButton = row.getByRole("button", { name: "吊销凭证" });
    let revokeRequests = 0;
    const countRevocations = (requestValue: Request) => {
      if (
        requestValue.method() === "POST" &&
        apiPath(requestValue.url()).endsWith("/revoke")
      ) {
        revokeRequests += 1;
      }
    };
    page.on("request", countRevocations);

    page.once("dialog", async (dialog) => {
      expect(dialog.message()).toContain("虚拟 IP 进入 1 小时冷却");
      await dialog.dismiss();
    });
    await revokeButton.click();
    expect(revokeRequests).toBe(0);
    await expect(revokeButton).toBeVisible();

    page.once("dialog", async (dialog) => dialog.accept());
    const revoked = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        apiPath(response.url()).endsWith("/revoke"),
    );
    await revokeButton.click();
    const revokedResponse = await revoked;
    expect(revokedResponse.status()).toBe(200);
    expect(await revokedResponse.json()).toMatchObject({
      network_id: expect.any(String),
      node_id_base64: expect.any(String),
      virtual_ip: expect.stringMatching(/^100[.]/),
      cooldown_until: expect.any(String),
      configuration_version: expect.any(Number),
    });
    await expect(row.locator(".status-pill", { hasText: "已吊销" })).toBeVisible();
    await expect(row.locator(".action-stack .muted")).toHaveText("已吊销");
    await expect(row.getByRole("button", { name: "吊销凭证" })).toHaveCount(0);
    expect(revokeRequests).toBe(1);
    page.off("request", countRevocations);
    assertObservation(observation, [401]);
  });

  test("真实审计员界面与服务端权限绕过拒绝", async ({ page }) => {
    const observation = observePage(page, "auditor-permission-boundary");
    const auditorPassword = await readPrivateCredential(
      requiredEnvironment("XS_CONSOLE_E2E_AUDITOR_PASSWORD_FILE"),
    );
    const session = await loginAs(page, auditorUsername, auditorPassword);
    expect(session.user.role).toBe("auditor");

    await navigate(page, "节点");
    await expect(page.getByRole("button", { name: "吊销凭证" })).toHaveCount(0);
    await navigate(page, "网络");
    await expect(page.getByRole("button", { name: "创建网络" })).toHaveCount(0);
    await navigate(page, "注册令牌");
    await expect(page.getByRole("button", { name: "创建令牌" })).toHaveCount(0);
    await navigate(page, "用户权限");
    await expect(page.getByRole("button", { name: "创建用户" })).toHaveCount(0);
    await expect(page.getByText(`@${auditorUsername}`, { exact: true })).toBeVisible();
    await navigate(page, "更新管理");
    await expect(page.getByRole("button", { name: "导入签名发布" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "配置灰度策略" })).toHaveCount(0);

    const bypass = await page.evaluate(async (csrfToken) => {
      const response = await fetch("/v1/admin/networks", {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
          "X-CSRF-Token": csrfToken,
        },
        body: JSON.stringify({
          name: "gate19-auditor-must-fail",
          address_pool: "100.124.0.0/24",
          reserved_addresses: 16,
        }),
      });
      return { status: response.status, body: (await response.json()) as unknown };
    }, session.csrf_token);
    expect(bypass).toEqual({
      status: 403,
      body: { error: { code: "forbidden", message: "permission denied" } },
    });

    const logoutResponsePromise = page.waitForResponse(
      (response) =>
        apiPath(response.url()) === "/v1/auth/logout" &&
        response.request().method() === "POST",
    );
    await page.getByRole("button", { name: "退出" }).click();
    expect((await logoutResponsePromise).status()).toBe(204);
    await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
    const sessionAfterLogout = await page.evaluate(async () =>
      (await fetch("/v1/auth/session", { headers: { Accept: "application/json" } })).status,
    );
    expect(sessionAfterLogout).toBe(401);
    assertObservation(observation, [401, 403]);
  });

  test("真实浏览器离线错误、旧数据替换与恢复", async ({ page, context }) => {
    const observation = observePage(page, "offline-error-and-recovery");
    await loginAsAdministrator(page);
    await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();

    await context.setOffline(true);
    try {
      await page.getByRole("button", { name: "刷新数据" }).click();
      await expect(page.getByRole("heading", { name: "数据读取失败" })).toBeVisible();
      await expect(page.getByText("无法连接 Controller")).toBeVisible();
      await saveScreenshot(page, "states", "offline-error");
    } finally {
      await context.setOffline(false);
    }

    await page.getByRole("button", { name: "重新加载" }).click();
    await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "数据读取失败" })).toHaveCount(0);
    expect(observation.requestFailures.length).toBeGreaterThan(0);
    expect(
      observation.requestFailures.every((failure) =>
        failure.path === "/v1/admin/console" || failure.path === "/v1/admin/users",
      ),
    ).toBe(true);
    assertObservation(observation, [401], true);
  });
});

function observePage(page: Page, label: string): BrowserObservation {
  const successfulNoContentRequests = new WeakSet<Request>();
  const pendingNoContentAborts = new Map<Request, RequestFailure>();
  const observation: BrowserObservation = {
    label,
    consoleErrors: [],
    pageErrors: [],
    httpFailures: [],
    requestFailures: [],
  };
  browserObservations.push(observation);
  page.on("console", (message) => {
    if (message.type() === "error") {
      observation.consoleErrors.push(classifyConsoleError(message.text()));
    }
  });
  page.on("pageerror", (error) => observation.pageErrors.push(error.name));
  page.on("response", (response) => {
    const requestValue = response.request();
    if (response.status() === 204) {
      successfulNoContentRequests.add(requestValue);
      const pendingFailure = pendingNoContentAborts.get(requestValue);
      if (pendingFailure !== undefined) {
        const index = observation.requestFailures.indexOf(pendingFailure);
        if (index !== -1) observation.requestFailures.splice(index, 1);
        pendingNoContentAborts.delete(requestValue);
      }
    }
    const path = apiPath(response.url());
    if (path.startsWith("/v1/") && response.status() >= 400) {
      observation.httpFailures.push({
        method: requestValue.method(),
        path,
        status: response.status(),
      });
    }
  });
  page.on("requestfailed", (requestValue) => {
    const path = apiPath(requestValue.url());
    if (path.startsWith("/v1/")) {
      const failure = {
        method: requestValue.method(),
        path,
        error: classifyNetworkError(requestValue.failure()?.errorText ?? "unknown"),
      };
      if (
        failure.error === "network_error_ERR_ABORTED" &&
        successfulNoContentRequests.has(requestValue)
      ) {
        return;
      }
      observation.requestFailures.push(failure);
      if (failure.error === "network_error_ERR_ABORTED") {
        pendingNoContentAborts.set(requestValue, failure);
      }
    }
  });
  return observation;
}

function assertObservation(
  observation: BrowserObservation,
  allowedStatuses: number[] = [],
  allowRequestFailures = false,
) {
  expect(observation.pageErrors).toEqual([]);
  expect(observation.httpFailures.filter((failure) => failure.status >= 500)).toEqual([]);
  expect(
    observation.httpFailures.filter(
      (failure) => !allowedStatuses.includes(failure.status),
    ),
  ).toEqual([]);
  expect(
    observation.consoleErrors.filter((error) => {
      const match = /^http_status_([0-9]{3})$/.exec(error);
      if (match) return !allowedStatuses.includes(Number(match[1]));
      if (allowRequestFailures && error.startsWith("network_error_")) return false;
      return true;
    }),
  ).toEqual([]);
  if (!allowRequestFailures) expect(observation.requestFailures).toEqual([]);
}

async function loginAsAdministrator(page: Page): Promise<SessionPayload> {
  return loginAs(
    page,
    requiredEnvironment("XS_CONSOLE_E2E_USERNAME"),
    await readPrivateCredential(requiredEnvironment("XS_CONSOLE_E2E_PASSWORD_FILE")),
  );
}

async function loginAs(
  page: Page,
  username: string,
  password: string,
): Promise<SessionPayload> {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "进入管理控制台" })).toBeVisible();
  return submitLogin(page, username, password);
}

async function submitLogin(
  page: Page,
  username: string,
  password: string,
): Promise<SessionPayload> {
  const loginResponse = page.waitForResponse(
    (response) => apiPath(response.url()) === "/v1/auth/login",
  );
  await page.getByLabel("用户名").fill(username);
  await page.getByLabel("密码").fill(password);
  await page.getByRole("button", { name: "登录" }).click();
  const response = await loginResponse;
  expect(response.status()).toBe(200);
  const session = (await response.json()) as SessionPayload;
  await expect(page.getByRole("heading", { name: "运行概览" })).toBeVisible();
  return session;
}

async function enrollNode(
  request: APIRequestContext,
  token: string,
  name: string,
) {
  const { publicKey } = generateKeyPairSync("ed25519");
  const exported = publicKey.export({ format: "jwk" });
  if (typeof exported.x !== "string") throw new Error("Ed25519 public key export failed");
  const response = await request.post("/v1/enroll", {
    data: {
      token,
      name,
      device_type: "linux",
      identity_public_key_base64: exported.x,
    },
  });
  expect(response.status()).toBe(201);
  const body = (await response.json()) as Record<string, unknown>;
  expect(typeof body.node_id_base64).toBe("string");
  expect(typeof body.virtual_ip).toBe("string");
}

async function refreshSnapshot(page: Page) {
  const response = page.waitForResponse(
    (candidate) => apiPath(candidate.url()) === "/v1/admin/console",
  );
  await page.getByRole("button", { name: "刷新数据" }).click();
  expect((await response).status()).toBe(200);
}

async function navigate(page: Page, navigation: string) {
  if ((page.viewportSize()?.width ?? 1440) <= 1080) {
    await page.getByRole("button", { name: "打开导航" }).click();
  }
  await page.getByRole("button", { name: navigation, exact: true }).click();
}

async function expectAllInteractiveControlsNamed(page: Page) {
  const controls = page.locator(
    "button:visible, a[href]:visible, input:visible, select:visible, textarea:visible",
  );
  const count = await controls.count();
  for (let index = 0; index < count; index += 1) {
    await expect(controls.nth(index)).toHaveAccessibleName(/\S/);
  }
}

async function expectNoDocumentOverflow(page: Page, context: string) {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow, `${context} horizontal overflow`).toBeLessThanOrEqual(1);
}

async function waitForStableRendering(page: Page) {
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
}

async function saveScreenshot(page: Page, group: string, name: string) {
  const directory = resolve(screenshotDirectory, group);
  await mkdir(directory, { recursive: true });
  await page.screenshot({
    path: resolve(directory, `${name}.png`),
    fullPage: true,
    animations: "disabled",
  });
}

function apiPath(url: string): string {
  try {
    return new URL(url).pathname;
  } catch {
    return "invalid-url";
  }
}

function classifyConsoleError(message: string): string {
  const status = /status of ([0-9]{3})/.exec(message);
  if (status) return `http_status_${status[1]}`;
  const networkError = /net::([A-Z_]+)/.exec(message);
  if (networkError) return `network_error_${networkError[1]}`;
  return "unclassified_console_error";
}

function classifyNetworkError(message: string): string {
  const networkError = /([A-Z_]{4,})/.exec(message);
  return networkError ? `network_error_${networkError[1]}` : "network_error_unknown";
}

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function requiredPositiveInteger(name: string): number {
  const raw = requiredEnvironment(name);
  if (!/^[1-9][0-9]*$/.test(raw)) throw new Error(`${name} is invalid`);
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) throw new Error(`${name} is invalid`);
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
