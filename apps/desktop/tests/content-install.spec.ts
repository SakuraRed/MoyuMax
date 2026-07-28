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
          name: "内容测试",
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

test("M5-INSTALL-001 用户确认目标模组和必需依赖后进入统一任务队列", async ({ page }) => {
  await page.getByRole("button", { name: "资源" }).click();
  await expect(page.getByRole("button", { name: "在线目录" })).toBeVisible();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("Continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();

  const result = page.locator(".res-row").filter({ hasText: "Continuity" });
  await expect(result).toBeVisible();
  await expect(result.locator("img")).toHaveCount(0);
  await expectElementPadding(page, ".res-row", { block: 12, inline: 14 });
  await result.getByRole("button", { name: "安装", exact: true }).click();

  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "确认安装到「内容测试」" })).toBeVisible();
  await expect(dialog.getByText("Fabric API", { exact: true })).toBeVisible();
  await expect(dialog.getByText("必需", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("checkbox", { name: /Mod Menu/ })).not.toBeChecked();
  await expectElementPadding(page, ".modal", { block: 20, inline: 24 });
  await expectElementPadding(page, ".install-line", { block: 9, inline: 0 });

  await dialog.getByRole("button", { name: "仍要安装" }).click();
  await expect(page.getByText("安装任务已加入队列", { exact: true })).toBeVisible();
  await expectElementPadding(page, ".content-queued", { block: 20, inline: 24 });
  await page.getByRole("button", { name: "查看任务中心" }).click();
  await expect(page.getByText("安装「Continuity」", { exact: true })).toBeVisible();
  await expect(page.getByText("下载文件", { exact: true })).toBeVisible();
  await expectElementPadding(page, ".task-card", { block: 20, inline: 24 });
  await expectElementPadding(page, ".task-state", { block: 5, inline: 12 });
});

test("M5-OFFLINE-001 远程搜索失败时本地内容列表保持可用", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.modrinthOffline", "true");
    window.localStorage.setItem(
      "moyumax.browser.installedContent",
      JSON.stringify([
        {
          id: "installed-id",
          instanceId: "instance-id",
          provider: "modrinth",
          projectId: "ROOT0001",
          versionId: "ROOTVER1",
          projectTitle: "Continuity",
          versionNumber: "3.0.1+26.2",
          fileName: "continuity.jar",
          relativePath: ".minecraft/mods/continuity.jar",
          size: 1040013,
          sha1: "1".repeat(40),
          sha512: "2".repeat(128),
          enabled: true,
          autoUpdateEnabled: false,
          installedAtUnixSeconds: 1,
        },
      ]),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源" }).click();

  await page.getByRole("button", { name: "实例内容", exact: true }).click();
  await expect(page.getByText("Continuity", { exact: true })).toBeVisible();
  await expect(page.getByText("自动更新关闭", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "在线目录" }).click();
  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("Continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(page.getByText("搜索失败", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "实例内容", exact: true }).click();
  await expect(page.getByText("Continuity", { exact: true })).toBeVisible();
});

test("UI-MOD-001 资源页在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "资源" }).click();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await page.getByRole("searchbox", { name: "搜索在线资源" }).fill("Continuity");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await expect(page.locator(".res-row").first()).toBeVisible();
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
