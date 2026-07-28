import { expect, test, type Page } from "@playwright/test";

// 详情文件列表种子：覆盖两个游戏版本与两个加载器，验证分组/折叠/筛选。
const MOD_VERSIONS = [
  {
    id: "VER001",
    versionNumber: "3.1.0+26.2",
    versionType: "release",
    datePublished: "2026-06-20T10:00:00Z",
    gameVersions: ["26.2"],
    loaders: ["fabric"],
    downloads: 120_000,
  },
  {
    id: "VER002",
    versionNumber: "3.0.2+26.2",
    versionType: "release",
    datePublished: "2026-06-18T10:00:00Z",
    gameVersions: ["26.1", "26.2"],
    loaders: ["fabric", "forge"],
    downloads: 340_000,
  },
  {
    id: "VER003",
    versionNumber: "2.9.0+26.1",
    versionType: "beta",
    datePublished: "2026-05-02T10:00:00Z",
    gameVersions: ["26.1"],
    loaders: ["forge"],
    downloads: 8_000,
  },
];

async function seedBase(page: Page, options?: { instances?: string }) {
  await page.goto("/");
  await page.evaluate((opts) => {
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
      opts.instances ??
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
    window.localStorage.setItem("moyumax.browser.modVersions", opts.modVersions);
  }, { instances: options?.instances, modVersions: JSON.stringify(MOD_VERSIONS) });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).first().click();
}

async function openContinuityDetail(page: Page) {
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  const card = page.locator(".res-row").filter({ hasText: "Continuity" });
  await card.getByRole("button", { name: "详情", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Continuity" })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await seedBase(page);
});

test("M32-DET-UI-001 进入详情并返回后保留搜索状态", async ({ page }) => {
  await openContinuityDetail(page);

  await expect(page.getByText("连续性 (Continuity)", { exact: true })).toBeVisible();
  await expect(page.getByText("34.2M 次下载")).toBeVisible();
  await expect(page.getByText("Modrinth（下载默认走 MCI Mirror 内置镜像）", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "打开 Modrinth 源站" })).toBeVisible();
  await expect(page.getByRole("button", { name: "MCMOD 百科" })).toBeVisible();
  await expect(page.getByRole("button", { name: "复制名称" })).toBeVisible();
  await expect(page.getByRole("button", { name: "复制链接" })).toBeVisible();
  await expect(page.getByRole("button", { name: "收藏 Continuity" })).toBeVisible();

  await page.getByRole("button", { name: "返回结果列表" }).click();
  await expect(page.getByRole("searchbox", { name: "搜索在线资源" })).toHaveValue("continuity");
  await expect(
    page.locator(".res-row").filter({ hasText: "Continuity" }),
  ).toBeVisible();
});

test("M32-DET-UI-002 文件按游戏版本分组折叠且所选版本自动展开", async ({ page }) => {
  await openContinuityDetail(page);

  // 默认跟随所选实例（26.2）：仅一组且自动展开
  const selectedGroup = page.getByRole("button", { name: /Minecraft 26\.2/ });
  await expect(selectedGroup).toHaveAttribute("aria-expanded", "true");
  await expect(selectedGroup.getByText("所选版本")).toBeVisible();
  await expect(page.getByText("3.1.0+26.2", { exact: true })).toBeVisible();
  await expect(page.getByText("2.9.0+26.1", { exact: true })).toHaveCount(0);

  // 切到全部版本后出现第二组，默认折叠；点击后展开（fabric 筛选下 26.1 组仅 3.0.2+26.2）
  await page.getByRole("button", { name: "全部版本", exact: true }).click();
  const otherGroup = page.getByRole("button", { name: /Minecraft 26\.1/ });
  await expect(otherGroup).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator(".detail-file-row")).toHaveCount(2);
  await otherGroup.click();
  await expect(otherGroup).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator(".detail-file-row")).toHaveCount(3);
});

test("M32-DET-UI-003 详情内加载器筛选 chip 过滤文件", async ({ page }) => {
  await openContinuityDetail(page);
  await page.getByRole("button", { name: "全部版本", exact: true }).click();

  // 默认跟随实例加载器 fabric：forge-only 的 2.9.0+26.1 不可见
  await page.getByRole("button", { name: /Minecraft 26\.1/ }).click();
  await expect(page.getByText("2.9.0+26.1", { exact: true })).toHaveCount(0);

  await page.getByRole("button", { name: "Forge", exact: true }).click();
  await expect(page.getByText("2.9.0+26.1 (beta)", { exact: true })).toBeVisible();
  await expect(page.getByText("3.1.0+26.2", { exact: true })).toHaveCount(0);
});

test("M32-DET-UI-004 有实例时模组详情下载进入安装计划", async ({ page }) => {
  await openContinuityDetail(page);
  const row = page.locator(".detail-file-row").filter({ hasText: "3.1.0+26.2" });
  await row.getByRole("button", { name: "安装", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "确认安装到「测试实例」" })).toBeVisible();
});

test("M32-DET-UI-005 无实例时详情下载跳过版本选择直接落盘", async ({ page }) => {
  await seedBase(page, { instances: "[]" });
  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  const card = page.locator(".res-row").filter({ hasText: "Continuity" });
  await card.getByRole("button", { name: "详情", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Continuity" })).toBeVisible();

  await page.getByRole("button", { name: /Minecraft 26\.2/ }).click();
  const row = page.locator(".detail-file-row").filter({ hasText: "3.1.0+26.2" });
  await row.getByRole("button", { name: "下载", exact: true }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("textbox", { name: "保存文件名" })).toHaveValue(
    "continuity-3.1.0+26.2.zip",
  );
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.pickedDirectory", "D:\\MoyuMax\\data\\downloads");
  });
  await dialog.getByRole("button", { name: "选择目录…" }).click();
  await dialog.getByRole("button", { name: "下载", exact: true }).click();

  await expect(page.getByText("已下载到：", { exact: false })).toBeVisible();
  const downloaded = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.downloadedFiles") ?? "[]"),
  );
  expect(downloaded).toEqual([
    { path: "D:\\MoyuMax\\data\\downloads/continuity-3.1.0+26.2.zip", versionId: "VER001" },
  ]);
});

test("M32-DET-UI-006 整合包详情下载进入安装预览", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.modpackPreview",
      JSON.stringify({
        provider: "modrinth",
        name: "Tundra Adventures",
        version: "1.0.0",
        gameVersion: "26.2",
        loaderKind: "fabric",
        loaderVersion: "0.19.3",
        fileCount: 42,
        totalBytes: 96 * 1024 * 1024,
      }),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).first().click();
  await page.getByRole("button", { name: "整合包", exact: true }).click();
  const card = page.locator(".res-row").filter({ hasText: "Continuity" });
  await card.getByRole("button", { name: "详情", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Continuity" })).toBeVisible();

  await page.getByRole("button", { name: /Minecraft 26\.2/ }).click();
  const row = page.locator(".detail-file-row").filter({ hasText: "3.1.0+26.2" });
  await row.getByRole("button", { name: "安装", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "整合包预览" })).toBeVisible();
  await expect(dialog.getByText("Tundra Adventures 1.0.0")).toBeVisible();
});

test("M32-DET-UI-007 复制链接后按钮进入已复制状态", async ({ page }) => {
  await openContinuityDetail(page);
  const copyButton = page.getByRole("button", { name: "复制链接" });
  await copyButton.click();
  await expect(page.getByRole("button", { name: "已复制", exact: true })).toBeVisible();
});

test("M32-FAV-UI-001 结果卡收藏后出现在收藏子页并可取消", async ({ page }) => {
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  const card = page.locator(".res-row").filter({ hasText: "Continuity" });
  const star = card.getByRole("button", { name: "收藏 Continuity" });
  await star.click();
  await expect(star).toHaveAttribute("aria-pressed", "true");

  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.favorites") ?? "[]"),
  );
  expect(stored).toHaveLength(1);
  expect(stored[0]).toMatchObject({ projectId: "ROOT0001", slug: "continuity", type: "mod" });

  await page.getByRole("button", { name: "收藏", exact: true }).click();
  await expect(page.getByRole("heading", { name: "模组", exact: true })).toBeVisible();
  const row = page.locator(".favorites-row").filter({ hasText: "Continuity" });
  await expect(row.getByText("连续性 (Continuity)", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "取消收藏" }).click();
  await expect(page.getByText("还没有收藏的资源。")).toBeVisible();
  await expect(page.getByRole("button", { name: "去搜索" })).toBeVisible();
});

test("M32-FAV-UI-002 收藏行内下载进入资源详情", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.favorites",
      JSON.stringify([
        {
          projectId: "ROOT0002",
          slug: "lithium",
          title: "Lithium",
          iconUrl: null,
          type: "mod",
          addedAtUnixSeconds: 1784000000,
        },
      ]),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).first().click();
  await page.getByRole("button", { name: "收藏", exact: true }).click();

  const row = page.locator(".favorites-row").filter({ hasText: "Lithium" });
  await expect(row.getByText("锂 (Lithium)", { exact: true })).toBeVisible();
  await row.getByRole("button", { name: "下载", exact: true }).click();

  await expect(page.getByRole("heading", { name: "Lithium" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "文件列表" })).toBeVisible();
  await expect(page.getByText("3.1.0+26.2", { exact: true })).toBeVisible();
});

test("UI-DET-001 资源详情在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await openContinuityDetail(page);
  await page.getByRole("button", { name: "全部版本", exact: true }).click();
  await page.getByRole("button", { name: /Minecraft 26\.1/ }).click();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".resource-content *")]
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
