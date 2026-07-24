import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => {
    window.localStorage.clear();
    window.localStorage.setItem(
      "moyumax.browser.onboarding",
      JSON.stringify({
        language: "zh-CN",
        dataDirectory: "D:\\MoyuMax\\data",
        telemetryEnabled: false,
        updateChecksEnabled: true,
        natDetectionEnabled: false,
        instanceIsolationEnabled: true,
      }),
    );
    window.localStorage.setItem(
      "moyumax.browser.instances",
      JSON.stringify([
        {
          id: "instance-id",
          name: "账户实例",
          gameVersion: "26.2",
          loaderKind: "fabric",
          loaderVersion: "0.19.3",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
          state: "ready",
        },
      ]),
    );
  });
  await page.reload();
});

test("M30-MS-001 设备码登录完成后账户入库", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("button", { name: "添加 Microsoft 账户" }).click();

  const panel = page.locator(".device-code-panel");
  await expect(panel).toBeVisible();
  await expect(panel.locator(".device-code")).toHaveText("AB12-CD34");
  await expect(panel.getByText("https://www.microsoft.com/link")).toBeVisible();
  await expect(panel.getByRole("button", { name: "复制用户码" })).toBeVisible();
  await expect(panel.getByRole("button", { name: "打开链接" })).toBeVisible();
  await expect(panel.getByRole("status")).toContainText("正在等待浏览器中的授权");

  await expect(panel).toHaveCount(0, { timeout: 10000 });
  await expect(page.getByRole("heading", { name: "Steve" })).toBeVisible();
  await expect(page.getByText("Microsoft", { exact: true }).first()).toBeVisible();
});

test("M30-MS-002 轮询期间取消登录", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.msLoginScenario", "pending");
  });
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("button", { name: "添加 Microsoft 账户" }).click();

  const panel = page.locator(".device-code-panel");
  await expect(panel).toBeVisible();
  await panel.getByRole("button", { name: "取消" }).click();

  await expect(panel).toHaveCount(0);
  await expect(page.getByText("还没有账户", { exact: false })).toBeVisible();
});

test("M30-MS-003 登录失败显示可读错误", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.msLoginScenario", "fail");
  });
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("button", { name: "添加 Microsoft 账户" }).click();

  await expect(page.locator(".device-code-panel")).toBeVisible();
  await expect(
    page.getByRole("alert").getByText("未拥有 Minecraft", { exact: false }),
  ).toBeVisible({ timeout: 10000 });
  await expect(page.locator(".device-code-panel")).toHaveCount(0);
});

test("UI-MS-001 设备码面板在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.msLoginScenario", "pending");
  });
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("button", { name: "添加 Microsoft 账户" }).click();
  await expect(page.locator(".device-code-panel")).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".device-code-panel *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
          element.clientWidth > 0 &&
          element.scrollWidth > element.clientWidth + 1,
      )
      .map((element) => ({
        tag: element.tagName,
        className: element.className,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
      })),
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
  expect(geometry.overflowingElements).toEqual([]);
});
