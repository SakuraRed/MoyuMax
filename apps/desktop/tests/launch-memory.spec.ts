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
  });
  await page.reload();
});

async function openMemorySettings(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "内存" }).click();
  await expect(page.getByRole("heading", { name: "启动内存" })).toBeVisible();
}

test("M33-MEM-001 全局内存默认自动分配并展示当前取值", async ({ page }) => {
  await openMemorySettings(page);

  await expect(page.getByRole("radio", { name: "自动分配（推荐）" })).toBeChecked();
  await expect(page.getByText("当前将分配 512-4096 MiB")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "全局最小内存 MiB" })).toHaveCount(0);
});

test("M33-MEM-002 全局自定义保存、回读与恢复自动", async ({ page }) => {
  await openMemorySettings(page);

  // 切到自定义,用当前自动分配值预填
  await page.getByRole("radio", { name: "自定义" }).click();
  const minInput = page.getByRole("textbox", { name: "全局最小内存 MiB" });
  const maxInput = page.getByRole("textbox", { name: "全局最大内存 MiB" });
  await expect(minInput).toHaveValue("512");
  await expect(maxInput).toHaveValue("4096");

  await minInput.fill("1024");
  await maxInput.fill("8192");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(
    page.locator(".java-notice").getByText("全局启动内存已保存", { exact: true }),
  ).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.globalLaunchPreference") ?? "null"),
  );
  expect(stored).toEqual({ mode: "custom", minMib: 1024, maxMib: 8192 });

  // 回读:离开设置页再进入,自定义选择与取值保持
  await page.getByRole("button", { name: "首页", exact: true }).click();
  await openMemorySettings(page);
  await expect(page.getByRole("radio", { name: "自定义" })).toBeChecked();
  await expect(page.getByRole("textbox", { name: "全局最小内存 MiB" })).toHaveValue("1024");
  await expect(page.getByRole("textbox", { name: "全局最大内存 MiB" })).toHaveValue("8192");

  // 非法值拒绝且不写入
  await page.getByRole("textbox", { name: "全局最小内存 MiB" }).fill("128");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(
    page.locator(".error-block").getByText("内存设置必须满足", { exact: false }),
  ).toBeVisible();
  const afterInvalid = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.globalLaunchPreference") ?? "null"),
  );
  expect(afterInvalid).toEqual({ mode: "custom", minMib: 1024, maxMib: 8192 });

  // 切回自动分配,清除全局自定义
  await page.getByRole("radio", { name: "自动分配（推荐）" }).click();
  await expect(
    page.locator(".java-notice").getByText("已切换为自动分配", { exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("textbox", { name: "全局最小内存 MiB" })).toHaveCount(0);
  const reverted = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.globalLaunchPreference") ?? "null"),
  );
  expect(reverted).toEqual({ mode: "auto" });
});
