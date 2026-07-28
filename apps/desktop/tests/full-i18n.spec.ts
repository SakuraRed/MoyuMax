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
          name: "多语言实例",
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

test("M22-I18N-001 英文界面渲染资源中心与任务中心", async ({ page }) => {
  await switchToEnglish(page);
  await page.getByRole("button", { name: "Resources", exact: true }).click();
  await expect(page.getByRole("button", { name: "Online catalog" })).toBeVisible();
  await page.getByRole("button", { name: "Instance content" }).click();
  await expect(page.getByRole("heading", { name: "Locally installed content" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Check for updates" })).toBeVisible();

  await page.getByRole("button", { name: "Tasks", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Task Center" })).toBeVisible();
  await expect(page.getByText("No tasks", { exact: true })).toBeVisible();
});

test("M22-I18N-002 繁体中文界面渲染数据中心", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("combobox", { name: "界面语言" }).selectOption({ label: "繁體中文" });
  await expect(page.locator(".nav-item", { hasText: "資料" })).toBeVisible();

  await page.getByRole("button", { name: "首頁", exact: true }).click();
  await page.getByRole("button", { name: "資料", exact: true }).click();
  await expect(page.getByRole("heading", { name: "資料與資源回收筒" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "世界存檔" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "資源回收筒", exact: true })).toBeVisible();
});

test("M22-I18N-003 英文界面渲染安装向导", async ({ page }) => {
  await switchToEnglish(page);
  await page.getByRole("button", { name: "Instances", exact: true }).click();
  await page.getByRole("button", { name: "New instance" }).click();
  await expect(page.getByRole("heading", { name: "Install your first game" })).toBeVisible();
});

test("M22-I18N-004 英文界面渲染数据中心", async ({ page }) => {
  await switchToEnglish(page);
  await expect(page.locator(".nav-item", { hasText: "Data" })).toBeVisible();
  await page.getByRole("button", { name: "Data", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Data & Recycle Bin" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "World saves" })).toBeVisible();
});

test("UI-I18N-002 英文资源中心在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await switchToEnglish(page);
  await page.getByRole("button", { name: "Resources", exact: true }).click();
  await expect(page.getByRole("button", { name: "Online catalog" })).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  // 页面级横向不溢出,且主操作在放大后仍可见可用
  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
  await expect(page.getByRole("button", { name: "Online catalog" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Online catalog" })).toBeEnabled();
  await page.getByRole("button", { name: "Instance content" }).click();
  await expect(page.getByRole("heading", { name: "Locally installed content" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Check for updates" })).toBeVisible();
});

async function switchToEnglish(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("combobox", { name: "界面语言" }).selectOption({ label: "English" });
  await expect(page.locator(".nav-item", { hasText: "Settings" })).toBeVisible();
  // 设置页只放行首页导航，先回到首页再跳转其他页面。
  await page.getByRole("button", { name: "Home", exact: true }).click();
}
