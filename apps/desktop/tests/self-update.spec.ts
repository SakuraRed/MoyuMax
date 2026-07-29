import { expect, test } from "@playwright/test";

const NEW_RELEASE = {
  tag: "v0.2.0",
  name: "0.2.0",
  notes: "新功能与修复",
  pageUrl: "https://github.com/SakuraRed/MoyuMax/releases/tag/v0.2.0",
  minAppVersion: "0.1.0",
  installer: {
    name: "MoyuMax_0.2.0_x64-setup.exe",
    url: "https://example.com/setup.exe",
    size: 1048576,
    sha256: "abc123",
  },
};

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
          name: "更新实例",
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

test("M25-UPDATE-001 已是最新时不产生下载入口", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "更新" }).click();
  await expect(page.getByRole("heading", { name: "启动器更新" })).toBeVisible();
  await expect(page.getByLabel("启动器更新").getByText("0.2.0", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByText("已是最新版本", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /下载安装包/ })).toHaveCount(0);
});

test("M25-UPDATE-002 发现新版本并经校验下载", async ({ page }) => {
  await page.evaluate((release) => {
    window.localStorage.setItem("moyumax.browser.latestRelease", JSON.stringify(release));
  }, NEW_RELEASE);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "更新" }).click();

  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByText("v0.2.0", { exact: true })).toBeVisible();
  await expect(page.getByText("新功能与修复", { exact: true })).toBeVisible();
  await expect(page.getByText("该发布要求最低可升级版本 0.1.0", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: /下载安装包/ }).click();
  await expect(page.getByText("安装包已通过校验；请自行运行完成安装", { exact: true })).toBeVisible();
  await expect(page.getByText("MoyuMax_0.2.0_x64-setup.exe", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "打开所在位置" }).click();
  const opened = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.openedLocation"),
  );
  expect(opened).toContain("MoyuMax_0.2.0_x64-setup.exe");
});

test("M25-UPDATE-003 校验失败显示可读错误", async ({ page }) => {
  await page.evaluate((release) => {
    window.localStorage.setItem("moyumax.browser.latestRelease", JSON.stringify(release));
    window.localStorage.setItem("moyumax.browser.updateDownloadFails", "true");
  }, NEW_RELEASE);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "更新" }).click();

  await page.getByRole("button", { name: "检查更新" }).click();
  await page.getByRole("button", { name: /下载安装包/ }).click();
  await expect(page.getByRole("alert").getByText("SHA-256 校验失败", { exact: false })).toBeVisible();
});

test("UI-UPDATE-002 更新区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await page.evaluate((release) => {
    window.localStorage.setItem("moyumax.browser.latestRelease", JSON.stringify(release));
  }, NEW_RELEASE);
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "更新" }).click();
  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByText("v0.2.0", { exact: true })).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".java-content *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
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
