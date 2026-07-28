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

/** 新入口:导航「实例」进入列表页,再点实例卡片进入详情。 */
async function openDetail(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "实例", exact: true }).click();
  await page.getByRole("button", { name: /管理实例/ }).click();
  await expect(page.locator(".tabs")).toBeVisible();
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
}

async function openDetailTab(
  page: import("@playwright/test").Page,
  tabName: string,
): Promise<void> {
  await page.locator(".tabs").getByRole("button", { name: tabName, exact: true }).click();
}

test("M33-DET-001 实例列表进入详情,六个页签结构完整", async ({ page }) => {
  await openDetail(page);

  // 六页签:概览/内容/世界/截图/日志/设置。
  const tabs = page.locator(".tabs button");
  await expect(tabs).toHaveCount(6);
  await expect(tabs.nth(0)).toHaveText("概览");
  await expect(tabs.nth(1)).toHaveText("内容");
  await expect(tabs.nth(2)).toHaveText("世界");
  await expect(tabs.nth(3)).toHaveText("截图");
  await expect(tabs.nth(4)).toHaveText("日志");
  await expect(tabs.nth(5)).toHaveText("设置");

  // 概览:hero 卡与实例信息。
  await expect(page.locator(".hero-card")).toContainText("Minecraft 26.2 · Fabric 0.19.3");
  await expect(page.locator(".hero-card")).toContainText("1 个模组");
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
  await expect(page.locator(".kv-row").nth(0)).toContainText("Minecraft 26.2");
  await expect(page.getByText("临时切换仅对本次启动生效", { exact: false })).toBeVisible();

  // 内容:自动更新提示条与模组行。
  await openDetailTab(page, "内容");
  await expect(page.getByText("内容自动更新默认关闭", { exact: false })).toBeVisible();
  await expect(page.locator(".list-row").filter({ hasText: "JEI 物品管理" })).toBeVisible();

  // 世界:世界行与备份时间线。
  await openDetailTab(page, "世界");
  await expect(page.locator(".world-row").filter({ hasText: "世界甲" })).toBeVisible();
  await expect(page.getByText("备份时间线", { exact: true })).toBeVisible();

  // 截图:截图卡片。
  await openDetailTab(page, "截图");
  await expect(page.getByRole("button", { name: "截图 2026-07-20_12.00.00.png" })).toBeVisible();

  // 日志:无会话空态。
  await openDetailTab(page, "日志");
  await expect(
    page.getByText("该实例还没有启动会话，启动一次游戏后即可查看日志。"),
  ).toBeVisible();

  // 设置:Java/内存设置行与回收入口。
  await openDetailTab(page, "设置");
  await expect(page.getByText("Java 环境", { exact: true }).first()).toBeVisible();
  await expect(page.getByText("内存分配", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "移入回收站" })).toBeVisible();

  // 标题栏返回按钮回到实例列表。
  await page.getByRole("button", { name: "返回" }).click();
  await expect(page.getByRole("button", { name: "管理实例「详情测试」" })).toBeVisible();
});

test("M33-DET-002 启动内存跟随全局与自定义切换", async ({ page }) => {
  await openDetail(page);
  await openDetailTab(page, "设置");

  // 默认跟随全局,展示全局自动分配摘要,不出现输入框
  await expect(page.getByText("当前生效：全局自动分配 512-4096 MiB")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "最小内存 MiB" })).toHaveCount(0);

  // 切到自定义,用当前生效值预填
  await page.getByRole("button", { name: "自定义" }).click();
  const minInput = page.getByRole("textbox", { name: "最小内存 MiB" });
  const maxInput = page.getByRole("textbox", { name: "最大内存 MiB" });
  await expect(minInput).toHaveValue("512");
  await expect(maxInput).toHaveValue("4096");

  await minInput.fill("1024");
  await maxInput.fill("8192");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.locator(".toast").getByText("启动内存已保存", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.launchOptions") ?? "{}"),
  );
  expect(stored["instance-id"]).toEqual({ minimumMemoryMib: 1024, maximumMemoryMib: 8192 });

  await page.getByRole("button", { name: "返回" }).click();
  await openDetail(page);
  await openDetailTab(page, "设置");
  await expect(page.getByRole("textbox", { name: "最小内存 MiB" })).toHaveValue("1024");
  await expect(page.getByRole("textbox", { name: "最大内存 MiB" })).toHaveValue("8192");

  await page.getByRole("textbox", { name: "最小内存 MiB" }).fill("128");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.locator(".toast").getByText("内存设置必须满足", { exact: false })).toBeVisible();
  const afterInvalid = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.launchOptions") ?? "{}"),
  );
  expect(afterInvalid["instance-id"]).toEqual({ minimumMemoryMib: 1024, maximumMemoryMib: 8192 });

  // 切回跟随全局,清除实例覆盖
  await page.getByRole("button", { name: "跟随全局" }).click();
  await expect(page.locator(".toast").getByText("已切换为跟随全局", { exact: true })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "最小内存 MiB" })).toHaveCount(0);
  const cleared = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.launchOptions") ?? "{}"),
  );
  expect(cleared["instance-id"]).toBeUndefined();

  await page.getByRole("button", { name: "返回" }).click();
  await openDetail(page);
  await openDetailTab(page, "设置");
  await expect(page.getByText("当前生效：全局自动分配 512-4096 MiB")).toBeVisible();
});

test("M33-DET-003 模组启停用开关", async ({ page }) => {
  await openDetail(page);
  await openDetailTab(page, "内容");
  const row = page.locator(".list-row").filter({ hasText: "JEI 物品管理" });
  await expect(row).toBeVisible();

  const toggle = page.getByRole("switch", { name: "JEI 物品管理 启用开关" });
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.installedContent") ?? "[]"),
  );
  expect(stored[0].enabled).toBe(false);

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
});

test("M33-DET-004 世界列表展示与回收站删除", async ({ page }) => {
  await openDetail(page);
  await openDetailTab(page, "世界");
  const row = page.locator(".world-row").filter({ hasText: "世界甲" });
  await expect(row).toBeVisible();
  await expect(row).toContainText("4.0 KiB");

  // 选中世界后出现删除入口,删除走两段确认。
  await row.click();
  await page.getByRole("button", { name: "删除", exact: true }).click();
  await page.getByRole("button", { name: "确认删除" }).click();
  await expect(page.locator(".world-row").filter({ hasText: "世界甲" })).toHaveCount(0);
  await expect(page.locator(".toast").getByText("已把世界「世界甲」移入回收站", { exact: false })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.worldDetails") ?? "{}"),
  );
  expect(stored["instance-id"]).toEqual([]);
});

test("M33-DET-005 截图点选与复制到剪贴板", async ({ page }) => {
  await openDetail(page);
  await openDetailTab(page, "截图");
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
  await openDetailTab(page, "内容");
  await expect(page.getByText("资源包", { exact: true })).toBeVisible();
  await expect(page.getByText("光影", { exact: true })).toBeVisible();

  const packRow = page.locator(".list-row").filter({ hasText: "faithful" });
  const shaderRow = page.locator(".list-row").filter({ hasText: "complementary" });
  await expect(packRow).toBeVisible();
  await expect(shaderRow).toBeVisible();
  await expect(packRow.getByRole("switch", { name: "faithful 启用开关" })).toHaveAttribute("aria-checked", "true");
  await expect(shaderRow.getByRole("switch", { name: "complementary 启用开关" })).toHaveAttribute("aria-checked", "false");

  await shaderRow.getByRole("switch", { name: "complementary 启用开关" }).click();
  await expect(shaderRow.getByRole("switch", { name: "complementary 启用开关" })).toHaveAttribute("aria-checked", "true");
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceResources") ?? "[]"),
  );
  expect(stored.find((entry: { id: string }) => entry.id === "resource-2").enabled).toBe(true);
});

test("M33-DET-007 设置页签内回收实例后优雅返回实例列表", async ({ page }) => {
  await openDetail(page);
  await openDetailTab(page, "设置");
  await page.getByRole("button", { name: "移入回收站" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "移入回收站" }).click();
  // 实例被回收后退回实例列表空态。
  await expect(page.getByRole("button", { name: "新建实例" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: /管理实例/ })).toHaveCount(0);
});
