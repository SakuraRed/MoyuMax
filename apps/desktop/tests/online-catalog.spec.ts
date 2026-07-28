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
  await page.getByRole("button", { name: "安装", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "确认安装到「测试实例」" })).toBeVisible();
});

test("M31-CAT-UI-002 在线整合包预览并安装", async ({ page }) => {
  await page.getByRole("button", { name: "整合包", exact: true }).click();
  await expect(page.getByText("CurseForge 整合包可从本地文件导入")).toBeVisible();

  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "安装", exact: true }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "整合包预览" })).toBeVisible();
  await expect(dialog.getByText("Tundra Adventures 1.0.0")).toBeVisible();
  await dialog.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("「Tundra Adventures」安装完成")).toBeVisible();
});

test("M31-CAT-UI-003 在线资源包安装到所选实例", async ({ page }) => {
  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "安装", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "安装 Continuity" })).toBeVisible();
  await dialog.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("已安装到", { exact: false })).toBeVisible();
});

test("M35-CAT-UI-001 资源包安装可挑选版本", async ({ page }) => {
  await page.getByRole("button", { name: "资源包", exact: true }).click();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "安装", exact: true }).click();

  const dialog = page.getByRole("dialog");
  const versionSelect = dialog.getByRole("combobox", { name: "下载版本" });
  await expect(versionSelect).toBeVisible();
  const options = await versionSelect.locator("option").allTextContents();
  expect(options.length).toBeGreaterThan(1);
  await versionSelect.selectOption({ index: 1 });
  await dialog.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("已安装到", { exact: false })).toBeVisible();
});

test("M31-CAT-UI-004 实例内容标签保留本地管理", async ({ page }) => {
  await page.getByRole("button", { name: "实例内容" }).click();
  await expect(page.getByRole("heading", { name: "本地已安装内容" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "资源内容" })).toBeVisible();
});

test("M31-CAT-UI-005 自由下载选择版本、文件名与路径", async ({ page }) => {
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "下载", exact: true }).first().click();

  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "下载 Continuity" })).toBeVisible();
  await expect(dialog.getByRole("combobox", { name: "下载版本" })).toBeVisible();
  await expect(dialog.getByRole("textbox", { name: "保存文件名" })).toHaveValue("continuity-3.0.2+26.2.jar");

  await dialog.getByRole("textbox", { name: "保存文件名" }).fill("continuity-custom.jar");
  await dialog.getByRole("button", { name: "下载", exact: true }).click();

  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("已下载到：", { exact: false })).toBeVisible();
  await expect(page.getByText("continuity-custom.jar", { exact: false })).toBeVisible();
});

test("M36-CAT-UI-001 版本选择按游戏版本分组并标注推荐", async ({ page }) => {
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "下载", exact: true }).first().click();

  const versionSelect = page.getByRole("dialog").getByRole("combobox", { name: "下载版本" });
  const groupLabels = await versionSelect.locator("optgroup").evaluateAll((groups) =>
    groups.map((group) => (group as HTMLOptGroupElement).label),
  );
  // 与实例(26.2)匹配的组置顶并标推荐,其余版本组全量保留
  expect(groupLabels[0]).toContain("26.2");
  expect(groupLabels[0]).toContain("推荐");
  expect(groupLabels).toContain("26.1");
});

test("M36-CAT-UI-002 选择自定义目录立即拉起目录选择器", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.pickedDirectory", "D:\\Mods\\custom");
  });
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page.getByRole("button", { name: "下载", exact: true }).first().click();

  const dialog = page.getByRole("dialog");
  await dialog.getByRole("radio", { name: "自定义目录" }).check();
  await expect(dialog.getByText("D:\\Mods\\custom", { exact: true })).toBeVisible();

  await dialog.getByRole("textbox", { name: "保存文件名" }).fill("continuity-path.jar");
  await dialog.getByRole("button", { name: "下载", exact: true }).click();
  await expect(page.getByText("continuity-path.jar", { exact: false })).toBeVisible();
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
