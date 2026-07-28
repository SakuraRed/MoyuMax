import { expect, test, type Page } from "@playwright/test";

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

async function openSourceSettings(page: Page): Promise<void> {
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "来源" }).click();
  await expect(page.getByRole("heading", { name: "来源" })).toBeVisible();
}

test("SRC-001 默认内置镜像优先,切换官方优先并持久化", async ({ page }) => {
  await openSourceSettings(page);

  await expect(page.getByRole("radio", { name: /内置镜像优先/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.getByRole("radio", { name: /官方源优先/ }).click();
  await expect(page.locator(".java-notice").getByText("下载源已保存", { exact: false })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.sourcePolicy") ?? "null"),
  );
  expect(stored).toEqual({ kind: "officialFirst" });

  // 回读:离开设置页再进入,选择保持
  await page.getByRole("button", { name: "首页", exact: true }).click();
  await openSourceSettings(page);
  await expect(page.getByRole("radio", { name: /官方源优先/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
});

test("SRC-002 自定义源校验与保存回读", async ({ page }) => {
  await openSourceSettings(page);

  await page.getByRole("radio", { name: /自定义源/ }).click();
  const minecraftInput = page.getByRole("textbox", { name: "Minecraft 镜像基址" });
  await expect(minecraftInput).toBeVisible();
  // 自定义源的限制说明常驻可见
  await expect(page.getByText("自定义源失败后不会切换到其他来源", { exact: false })).toBeVisible();

  // 两个基址都为空:拒绝
  await page.getByRole("button", { name: "保存自定义源" }).click();
  await expect(page.getByRole("alert").getByText("至少填写一个基址", { exact: false })).toBeVisible();

  // 非 https:拒绝
  await minecraftInput.fill("http://insecure.example.com");
  await page.getByRole("button", { name: "保存自定义源" }).click();
  await expect(page.getByRole("alert").getByText("https://", { exact: false })).toBeVisible();

  // 合法:保存并回读
  await minecraftInput.fill("https://bmclapi2.bangbang93.com");
  await page.getByRole("button", { name: "保存自定义源" }).click();
  await expect(page.locator(".java-notice").getByText("下载源已保存", { exact: false })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.sourcePolicy") ?? "null"),
  );
  expect(stored).toEqual({
    kind: "custom",
    minecraftBase: "https://bmclapi2.bangbang93.com",
    modrinthBase: null,
  });

  await page.getByRole("button", { name: "首页", exact: true }).click();
  await openSourceSettings(page);
  await expect(page.getByRole("radio", { name: /自定义源/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByRole("textbox", { name: "Minecraft 镜像基址" })).toHaveValue(
    "https://bmclapi2.bangbang93.com",
  );
});

test("SRC-003 平台来源说明如实标注 CurseForge 与 Modrinth", async ({ page }) => {
  await openSourceSettings(page);

  await expect(page.getByText("平台来源说明", { exact: true })).toBeVisible();
  await expect(page.getByText("官方源不可用", { exact: true })).toBeVisible();
  await expect(page.getByText("默认经 MCI Mirror 提供", { exact: false })).toBeVisible();
  await expect(page.getByText("官方源可用", { exact: true })).toBeVisible();
});
