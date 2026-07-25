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
          name: "命令行实例",
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

test("M24-CLI-001 开发者区开关渲染、风险提示与持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "开发者" }).click();
  await expect(page.getByRole("heading", { name: "开发者" })).toBeVisible();
  await expect(page.getByText("命令行会修改与图形界面相同的状态", { exact: false })).toBeVisible();

  const toggle = page.getByRole("checkbox", { name: "内置命令行（CLI）" });
  await expect(toggle).not.toBeChecked();
  await expect(page.getByText("moyumax-desktop.exe --cli instances list", { exact: true })).toHaveCount(0);
  await toggle.check();
  await expect(page.getByText("内置命令行已开启", { exact: true })).toBeVisible();
  await expect(page.getByText("moyumax-desktop.exe --cli instances list", { exact: true })).toBeVisible();

  await page.reload();
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "开发者" }).click();
  await expect(page.getByRole("checkbox", { name: "内置命令行（CLI）" })).toBeChecked();
  const stored = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.cliEnabled"),
  );
  expect(stored).toBe("true");
});

test("UI-CLI-001 开发者区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "开发者" }).click();
  await page.getByRole("checkbox", { name: "内置命令行（CLI）" }).check();
  await expect(page.getByText("moyumax-desktop.exe --cli instances list", { exact: true })).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".java-content *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
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
