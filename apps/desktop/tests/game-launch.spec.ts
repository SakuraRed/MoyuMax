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
          name: "启动测试",
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

test("UI-LAUNCH-001 首页可以启动和停止本地实例", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "启动测试" })).toBeVisible();
  await expect(page.getByText("Minecraft 26.2", { exact: false })).toBeVisible();
  await expect(page.getByText("Fabric 0.19.3", { exact: false })).toBeVisible();

  const heroPadding = await page.locator(".hero-card").evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      top: Number.parseFloat(style.paddingTop),
      right: Number.parseFloat(style.paddingRight),
      bottom: Number.parseFloat(style.paddingBottom),
      left: Number.parseFloat(style.paddingLeft),
    };
  });
  expect(heroPadding).toEqual({ top: 26, right: 28, bottom: 26, left: 28 });
  await expect(page.locator(".hero-card .tag.ok")).toContainText("可启动");

  const startButton = page.getByRole("button", { name: "启动游戏" });
  await expect(startButton).toBeFocused();
  await startButton.click();
  await expect(page.locator(".hero-card").getByText("正在运行", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "安全终止" })).toBeVisible();

  await page.getByRole("button", { name: "安全终止" }).click();
  await expect(page.getByText("已停止", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
});

test("UI-LAUNCH-001 实例首页在 960x600 和 200% 放大下不重叠", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.reload();
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    cardOverflow: [...document.querySelectorAll<HTMLElement>(".hero-card")].some(
      (card) => card.scrollWidth > card.clientWidth,
    ),
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
  expect(geometry.cardOverflow).toBe(false);
});

test("UI-LAUNCH-002 整合包安装中显示徽标并禁用启动", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.modpackInstalling", JSON.stringify(["instance-id"]));
  });
  await page.reload();
  await expect(page.getByText("整合包文件安装中，完成后才能启动", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeDisabled();
});
