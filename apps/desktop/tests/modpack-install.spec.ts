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
          name: "Tundra Adventures 1.0.0",
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
});

test("M29-PACK-001 导入整合包预览并确认安装", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.pickedModpackFile", "D:\\Packs\\tundra.mrpack");
  });
  await page.getByRole("button", { name: "实例", exact: true }).first().click();
  await page.getByRole("button", { name: "新建实例" }).click();
  await expect(page.getByRole("heading", { name: "整合包" })).toBeVisible();

  await page.getByRole("button", { name: "导入整合包…" }).click();
  await expect(page.getByRole("heading", { name: "Tundra Adventures" })).toBeVisible();
  await expect(page.getByText("Modrinth", { exact: true })).toBeVisible();
  await expect(page.getByText("版本 1.0.0 · Minecraft 26.2 · fabric 0.19.3 · 42 个文件", { exact: false })).toBeVisible();
  await expectElementPadding(page, ".modpack-preview", { block: 16, inline: 20 });

  await page.getByRole("button", { name: "确认安装" }).click();
  await expect(page.getByText("整合包安装完成", { exact: true })).toBeVisible();
  await expect(page.getByText("「Tundra Adventures」1.0.0 已安装 42 个受管文件", { exact: true })).toBeVisible();
});

test("M29-PACK-002 实例卡显示整合包徽章并完成更新", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.modpacks",
      JSON.stringify({
        "instance-id": {
          provider: "modrinth",
          packName: "Tundra Adventures",
          packVersion: "1.0.0",
          gameVersion: "26.2",
          loaderKind: "fabric",
          managedFiles: [],
          installedAtUnixSeconds: 1,
        },
      }),
    );
    window.localStorage.setItem(
      "moyumax.browser.modpackUpdateReport",
      JSON.stringify({
        packName: "Tundra Adventures",
        fromVersion: "1.0.0",
        toVersion: "2.0.0",
        addedFiles: 3,
        replacedFiles: 10,
        deletedFiles: 1,
        keptUserModified: ["config/user.cfg"],
      }),
    );
    window.localStorage.setItem("moyumax.browser.pickedModpackFile", "D:\\Packs\\tundra-2.mrpack");
  });
  await page.reload();

  await page.getByRole("button", { name: "实例", exact: true }).first().click();
  await page.getByRole("button", { name: "管理实例「Tundra Adventures 1.0.0」" }).click();
  await page.locator(".tabs").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByText("Tundra Adventures 1.0.0 · Modrinth", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "更新整合包" }).click();
  await expect(
    page.getByRole("status").getByText("「Tundra Adventures」已从 1.0.0 更新到 2.0.0；以下已改动文件保留未覆盖：config/user.cfg", { exact: true }),
  ).toBeVisible();
});

test("M29-PACK-003 整合包导入失败显示可读错误", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.removeItem("moyumax.browser.modpackPreview");
    window.localStorage.setItem("moyumax.browser.pickedModpackFile", "D:\\Packs\\broken.mrpack");
  });
  await page.getByRole("button", { name: "实例", exact: true }).first().click();
  await page.getByRole("button", { name: "新建实例" }).click();
  await page.getByRole("button", { name: "导入整合包…" }).click();
  await expect(page.getByRole("alert").getByText("modrinth.index.json 或 manifest.json", { exact: false })).toBeVisible();
});

test("UI-PACK-001 整合包导入区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.pickedModpackFile", "D:\\Packs\\tundra.mrpack");
  });
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "实例", exact: true }).first().click();
  await page.getByRole("button", { name: "新建实例" }).click();
  await page.getByRole("button", { name: "导入整合包…" }).click();
  await expect(page.getByRole("heading", { name: "Tundra Adventures" })).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".install-content *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
          // 文本框内部滚动自身内容（如只读路径），不属于布局横向溢出
          !(element instanceof HTMLInputElement) &&
          !(element instanceof HTMLTextAreaElement) &&
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

async function expectElementPadding(
  page: import("@playwright/test").Page,
  selector: string,
  minimum: { block: number; inline: number },
): Promise<void> {
  const spacing = await page.locator(selector).first().evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      top: Number.parseFloat(style.paddingTop),
      right: Number.parseFloat(style.paddingRight),
      bottom: Number.parseFloat(style.paddingBottom),
      left: Number.parseFloat(style.paddingLeft),
    };
  });

  expect(spacing.top).toBeGreaterThanOrEqual(minimum.block);
  expect(spacing.right).toBeGreaterThanOrEqual(minimum.inline);
  expect(spacing.bottom).toBeGreaterThanOrEqual(minimum.block);
  expect(spacing.left).toBeGreaterThanOrEqual(minimum.inline);
}
