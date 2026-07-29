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
    window.localStorage.setItem(
      "moyumax.browser.instances",
      JSON.stringify([
        {
          id: "instance-id",
          name: "测试实例",
          gameVersion: "26.2",
          loaderKind: "fabric",
          loaderVersion: "0.19.3",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
          state: "ready",
        },
      ]),
    );
    // 浏览器 mock 层的 CurseForge Key（独立种子键，非真实密钥）。
    window.localStorage.setItem("moyumax.browser.curseforgeApiKey", "mock-cf-key");
  });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).first().click();
});

/** 目录工具行切换到 CurseForge 来源。 */
async function switchToCurseforge(page: Page): Promise<void> {
  await page
    .getByRole("group", { name: "目录来源" })
    .getByRole("button", { name: "CurseForge", exact: true })
    .click();
}

test("CF-CAT-001 来源切换到 CurseForge 浏览并搜索", async ({ page }) => {
  await expect(page.getByText("目录由 Modrinth 提供", { exact: false })).toBeVisible();

  await switchToCurseforge(page);
  await expect(page.getByText("目录由 CurseForge 官方 API 提供")).toBeVisible();
  // 默认热门浏览：mock 目录返回 Sodium 与 JEI。
  await expect(page.getByText("Sodium", { exact: true })).toBeVisible();
  await expect(page.getByText("Just Enough Items (JEI)", { exact: true })).toBeVisible();

  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("sodium");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(page.getByText("Sodium", { exact: true })).toBeVisible();
  await expect(page.getByText("Just Enough Items (JEI)", { exact: true })).toBeHidden();

  // 切回 Modrinth 来源恢复原有目录。
  await page
    .getByRole("group", { name: "目录来源" })
    .getByRole("button", { name: "Modrinth", exact: true })
    .click();
  await expect(page.getByText("目录由 Modrinth 提供", { exact: false })).toBeVisible();
});

test("CF-CAT-002 CurseForge 详情副视图与单文件安装到实例", async ({ page }) => {
  await switchToCurseforge(page);
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("sodium");
  await page.getByRole("button", { name: "搜索", exact: true }).click();

  await page.getByRole("button", { name: "详情", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Sodium" })).toBeVisible();
  await expect(page.getByText("CurseForge 官方 API", { exact: true })).toBeVisible();
  // 文件列表按 MC 版本分组，两个 mock 文件都在。
  await expect(page.getByText("0.6.2+26.2")).toBeVisible();
  await expect(page.getByText("0.6.1+26.1")).toBeVisible();

  // 详情右栏安装到实例：模组走版本确认（单文件安装，不做依赖闭包解析）。
  await page.getByRole("button", { name: "安装到 测试实例", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "安装 Sodium" })).toBeVisible();
  await expect(dialog.getByText("不做依赖闭包解析", { exact: false })).toBeVisible();
  await dialog.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("已安装到", { exact: false })).toBeVisible();

  const resources = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceResources") ?? "[]"),
  );
  expect(resources.some((entry: { kind: string }) => entry.kind === "mod")).toBe(true);
});

test("CF-CAT-003 CurseForge 整合包预览并安装", async ({ page }) => {
  await switchToCurseforge(page);
  await page.getByRole("button", { name: "整合包", exact: true }).click();
  await expect(page.getByText("Sodium", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "安装", exact: true }).first().click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "整合包预览" })).toBeVisible();
  await expect(dialog.getByText("All the Mods 10 3.2.1")).toBeVisible();
  await dialog.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("「All the Mods 10」安装完成")).toBeVisible();

  const modpacks = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.modpacks") ?? "{}"),
  );
  expect(modpacks["instance-id"]?.provider).toBe("curseforge");
  expect(modpacks["instance-id"]?.packName).toBe("All the Mods 10");
});

test("CF-CAT-004 CurseForge 自由下载：无校验值文件如实提示按大小校验", async ({ page }) => {
  await switchToCurseforge(page);
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("sodium");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "下载", exact: true }).first().click();

  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "下载 Sodium" })).toBeVisible();
  // 默认选中最新正式版（带 sha1）：不出现大小校验提示。
  await expect(dialog.getByText("按文件大小校验", { exact: false })).toBeHidden();

  // 切到无校验值的 beta 文件：提示出现。
  await dialog.getByRole("button", { name: "下载版本" }).click();
  const panel = dialog.getByRole("listbox", { name: "下载版本" });
  await panel.getByRole("option", { name: /26\.1/ }).click();
  await panel.getByRole("option", { name: /0\.6\.1/ }).click();
  await expect(dialog.getByText("来源未提供校验值，下载后按文件大小校验。")).toBeVisible();

  await dialog.getByRole("button", { name: "下载", exact: true }).click();
  await expect(page.getByText("已下载到：", { exact: false })).toBeVisible();
});

test("CF-KEY-001 未配置时如实报错；设置页保存/测试/清除 Key", async ({ page }) => {
  // 未配置 Key：CurseForge 来源如实报错并给出去向提示。
  await page.evaluate(() => window.localStorage.removeItem("moyumax.browser.curseforgeApiKey"));
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).first().click();
  await switchToCurseforge(page);
  const errorBlock = page.getByRole("alert").first();
  await expect(errorBlock).toBeVisible();
  await errorBlock.getByText("技术细节", { exact: false }).click();
  await expect(page.getByText("未配置 CurseForge API Key", { exact: false })).toBeVisible();

  // 设置 → 来源：保存 Key 后官方源标记启用。
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "来源" }).click();
  await expect(page.getByText("官方源不可用")).toBeVisible();
  await page.getByLabel("CurseForge API Key").fill("mock-cf-key");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByText("已保存到本机。")).toBeVisible();
  await expect(page.getByText("官方源已启用")).toBeVisible();

  // 测试 Key 有效；清除后回到未配置态。
  await page.getByRole("button", { name: "测试", exact: true }).click();
  await expect(page.getByText("Key 有效（Minecraft）。")).toBeVisible();
  await page.getByRole("button", { name: "清除", exact: true }).click();
  await expect(page.getByText("已清除", { exact: false })).toBeVisible();
  await expect(page.getByText("官方源不可用")).toBeVisible();
});
