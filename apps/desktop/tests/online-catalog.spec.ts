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
          name: "测试实例",
          gameVersion: "26.2",
          loaderKind: "fabric",
          loaderVersion: "0.19.3",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
          state: "ready",
        },
      ]),
    );
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
});

test("M31-CAT-UI-001 在线目录默认显示并可按类型搜索模组", async ({ page }) => {
  await expect(page.getByRole("button", { name: "在线目录" })).toBeVisible();
  await expect(page.getByRole("button", { name: "实例内容" })).toBeVisible();
  await expect(page.getByRole("button", { name: "模组", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "整合包", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "光影", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "资源包", exact: true })).toBeVisible();

  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(page.getByText("Continuity", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "查看安装计划" }).click();
  await expect(page.getByRole("heading", { name: "安装计划预览" })).toBeVisible();
});

test("M31-CAT-UI-002 在线整合包预览并安装", async ({ page }) => {
  await page.getByRole("button", { name: "整合包", exact: true }).click();
  await expect(page.getByText("CurseForge 整合包可从本地文件导入")).toBeVisible();

  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "安装", exact: true }).click();

  await expect(page.getByRole("heading", { name: "整合包预览" })).toBeVisible();
  await expect(page.getByText("Tundra Adventures 1.0.0")).toBeVisible();
  await page.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("「Tundra Adventures」安装完成")).toBeVisible();
});

test("M31-CAT-UI-003 在线资源包安装到所选实例", async ({ page }) => {
  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "安装", exact: true }).click();
  await expect(page.getByText("已安装到", { exact: false })).toBeVisible();
});

test("M31-CAT-UI-004 实例内容标签保留本地管理", async ({ page }) => {
  await page.getByRole("button", { name: "实例内容" }).click();
  await expect(page.getByRole("heading", { name: "本地已安装内容" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "资源内容" })).toBeVisible();
});

test("UI-CAT-001 在线目录在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(page.getByText("Continuity", { exact: true })).toBeVisible();
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
