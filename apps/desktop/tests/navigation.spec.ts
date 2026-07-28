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

test("NAV-002 导航账户按钮进入账户页", async ({ page }) => {
  await page.getByRole("button", { name: "账户", exact: true }).click();
  await expect(page.getByRole("button", { name: "添加账户" })).toBeVisible();
});

test("NAV-003 全局搜索定位实例与页面", async ({ page }) => {
  await page.getByRole("button", { name: "全局搜索" }).click();
  const dialog = page.getByRole("dialog", { name: "全局搜索" });
  await expect(dialog).toBeVisible();

  await dialog.getByRole("textbox").fill("导航实例");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "概览", exact: true })).toBeVisible();

  await page.keyboard.press("Control+k");
  await page.getByRole("dialog", { name: "全局搜索" }).getByRole("textbox").fill("实例");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "全局搜索" })).toHaveCount(0);
});
