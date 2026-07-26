import { expect, test } from "@playwright/test";

function seedBase() {
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
        name: "详情测试",
        gameVersion: "26.2",
        loaderKind: "fabric",
        loaderVersion: "0.19.3",
        rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
        state: "ready",
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.javaEnvironments",
    JSON.stringify([
      {
        id: "env-21",
        distribution: "azulZulu",
        fullVersion: "21.0.12+8",
        architecture: "x64",
        homeDirectory: "D:\\MoyuMax\\data\\store\\java\\zulu\\env-21",
        status: "ready",
        sizeBytes: 190 * 1024 * 1024,
        healthy: true,
        referencingInstances: [{ id: "instance-id", name: "详情测试" }],
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.installedContent",
    JSON.stringify([
      {
        id: "content-1",
        instanceId: "instance-id",
        provider: "modrinth",
        projectId: "P1",
        versionId: "V1",
        projectTitle: "JEI 物品管理",
        versionNumber: "1.0.0",
        fileName: "jei.jar",
        relativePath: ".minecraft/mods/jei.jar",
        size: 2048,
        sha1: "1".repeat(40),
        sha512: "2".repeat(128),
        enabled: true,
        autoUpdateEnabled: false,
        installedAtUnixSeconds: 1,
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.instanceResources",
    JSON.stringify([
      {
        id: "resource-1",
        instanceId: "instance-id",
        kind: "resourcepack",
        displayName: "faithful",
        fileName: "faithful.zip",
        relativePath: ".minecraft/resourcepacks/faithful.zip",
        size: 1024,
        sha256: "3".repeat(64),
        enabled: true,
        worldName: null,
        importedAtUnixSeconds: 1,
      },
      {
        id: "resource-2",
        instanceId: "instance-id",
        kind: "shader",
        displayName: "complementary",
        fileName: "complementary.zip",
        relativePath: ".minecraft/shaderpacks/complementary.zip",
        size: 2048,
        sha256: "4".repeat(64),
        enabled: false,
        worldName: null,
        importedAtUnixSeconds: 1,
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.worldDetails",
    JSON.stringify({
      "instance-id": [
        { name: "世界甲", sizeBytes: 4096, lastPlayedUnixSeconds: 1760000000 },
      ],
    }),
  );
  window.localStorage.setItem(
    "moyumax.browser.screenshots",
    JSON.stringify({
      "instance-id": [
        {
          fileName: "2026-07-20_12.00.00.png",
          sizeBytes: 1024,
          takenAtUnixSeconds: 1760000000,
        },
      ],
    }),
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(seedBase);
  await page.reload();
});

async function openDetail(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "管理“详情测试”" }).click();
  await expect(page.getByRole("heading", { name: "概览" })).toBeVisible();
}

test("M33-DET-001 首页卡片进入详情并切换七个子页", async ({ page }) => {
  await openDetail(page);
  await expect(page.getByText("26.2", { exact: true })).toBeVisible();
  await expect(page.getByText("Fabric 0.19.3", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
  await expect(page.getByRole("button", { name: "移入回收站" })).toBeVisible();

  await page.locator(".settings-nav").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("heading", { name: "启动内存" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Java 环境" })).toBeVisible();

  await page.getByRole("button", { name: "Mod", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Mod", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "存档", exact: true }).click();
  await expect(page.getByRole("heading", { name: "存档", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "截图", exact: true }).click();
  await expect(page.getByRole("heading", { name: "截图", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await expect(page.getByRole("heading", { name: "资源包", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "光影", exact: true }).click();
  await expect(page.getByRole("heading", { name: "光影", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "返回首页" }).click();
  await expect(page.getByRole("heading", { name: "继续游戏" })).toBeVisible();
  await expect(page.getByRole("button", { name: "管理“详情测试”" })).toBeVisible();
});

test("M33-DET-002 启动内存保存、回读与非法值拒绝", async ({ page }) => {
  await openDetail(page);
  await page.locator(".settings-nav").getByRole("button", { name: "设置", exact: true }).click();

  const minInput = page.getByRole("textbox", { name: "最小内存 MiB" });
  const maxInput = page.getByRole("textbox", { name: "最大内存 MiB" });
  await expect(minInput).toHaveValue("512");
  await expect(maxInput).toHaveValue("2048");

  await minInput.fill("1024");
  await maxInput.fill("8192");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.locator(".toast").getByText("启动内存已保存", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.launchOptions") ?? "{}"),
  );
  expect(stored["instance-id"]).toEqual({ minimumMemoryMib: 1024, maximumMemoryMib: 8192 });

  await page.getByRole("button", { name: "返回首页" }).click();
  await openDetail(page);
  await page.locator(".settings-nav").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "最小内存 MiB" })).toHaveValue("1024");
  await expect(page.getByRole("textbox", { name: "最大内存 MiB" })).toHaveValue("8192");

  await page.getByRole("textbox", { name: "最小内存 MiB" }).fill("128");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.locator(".toast").getByText("内存设置必须满足", { exact: false })).toBeVisible();
  const afterInvalid = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.launchOptions") ?? "{}"),
  );
  expect(afterInvalid["instance-id"]).toEqual({ minimumMemoryMib: 1024, maximumMemoryMib: 8192 });
});

test("M33-DET-003 Mod 启停用与筛选", async ({ page }) => {
  await openDetail(page);
  await page.getByRole("button", { name: "Mod", exact: true }).click();
  const row = page.locator(".installed-content-row").filter({ hasText: "JEI 物品管理" });
  await expect(row).toBeVisible();

  const toggle = page.getByRole("checkbox", { name: "JEI 物品管理 启用开关" });
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect(row.getByText("已停用", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.installedContent") ?? "[]"),
  );
  expect(stored[0].enabled).toBe(false);

  await page.getByRole("button", { name: "停用", exact: true }).click();
  await expect(row).toBeVisible();
  await page.getByRole("button", { name: "启用", exact: true }).click();
  await expect(page.getByText("当前筛选下没有内容", { exact: false })).toBeVisible();
});

test("M33-DET-004 存档列表展示与回收站删除", async ({ page }) => {
  await openDetail(page);
  await page.getByRole("button", { name: "存档", exact: true }).click();
  const row = page.locator(".backup-row").filter({ hasText: "世界甲" });
  await expect(row).toBeVisible();
  await expect(row.getByText("最近游玩", { exact: false })).toBeVisible();
  await expect(row.getByRole("button", { name: "导出" })).toBeVisible();

  await row.getByRole("button", { name: "删除", exact: true }).click();
  await row.getByRole("button", { name: "确认删除" }).click();
  await expect(page.locator(".backup-row").filter({ hasText: "世界甲" })).toHaveCount(0);
  await expect(page.locator(".toast").getByText("已把世界「世界甲」移入回收站", { exact: false })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.worldDetails") ?? "{}"),
  );
  expect(stored["instance-id"]).toEqual([]);
});

test("M33-DET-005 截图点选与复制到剪贴板", async ({ page }) => {
  await openDetail(page);
  await page.getByRole("button", { name: "截图", exact: true }).click();
  await page.getByRole("button", { name: "截图 2026-07-20_12.00.00.png" }).click();
  await expect(page.getByText("已选 2026-07-20_12.00.00.png", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "复制", exact: true }).click();
  await expect(page.locator(".toast").getByText("已把「2026-07-20_12.00.00.png」复制到剪贴板", { exact: false })).toBeVisible();
  const copied = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.clipboardImage"),
  );
  expect(copied).toBe("2026-07-20_12.00.00.png");
});

test("M33-DET-006 资源包与光影按类型分开展示", async ({ page }) => {
  await openDetail(page);
  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await expect(page.locator(".installed-content-row").filter({ hasText: "faithful" })).toBeVisible();
  await expect(page.locator(".installed-content-row").filter({ hasText: "complementary" })).toHaveCount(0);

  await page.getByRole("button", { name: "光影", exact: true }).click();
  const shaderRow = page.locator(".installed-content-row").filter({ hasText: "complementary" });
  await expect(shaderRow).toBeVisible();
  await expect(page.locator(".installed-content-row").filter({ hasText: "faithful" })).toHaveCount(0);
  await expect(shaderRow.getByText("已停用", { exact: true })).toBeVisible();

  const toggle = page.getByRole("checkbox", { name: "complementary 启用开关" });
  await toggle.check();
  await expect(shaderRow.getByText("已启用", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceResources") ?? "[]"),
  );
  expect(stored.find((entry: { id: string }) => entry.id === "resource-2").enabled).toBe(true);
});

test("M33-DET-007 详情页内回收实例后优雅返回首页", async ({ page }) => {
  await openDetail(page);
  await page.getByRole("button", { name: "移入回收站" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "移入回收站" }).click();
  await expect(page.getByRole("heading", { name: "继续游戏" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "安装第一个游戏" })).toBeVisible();
});
