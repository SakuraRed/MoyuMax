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
          name: "导航实例",
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

test("NAV-001 所有主导航按钮在任意页面都可点击", async ({ page }) => {
  for (const target of ["资源", "任务", "数据", "设置"]) {
    await page.getByRole("button", { name: target, exact: true }).click();
    for (const name of ["首页", "资源", "任务", "数据", "设置"]) {
      const button = page.getByRole("button", { name, exact: true });
      await expect(button).toBeEnabled();
    }
  }
});

test("NAV-002 导航账户按钮进入设置页账户区", async ({ page }) => {
  await page.getByRole("button", { name: "添加账户" }).click();
  await expect(page.getByRole("heading", { name: "账户" })).toBeVisible();
  await expect(page.getByRole("button", { name: "添加离线账户" })).toBeVisible();
});
