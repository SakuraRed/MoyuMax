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
          id: "instance-backup",
          name: "备份世界",
          gameVersion: "1.21.8",
          loaderKind: "fabric",
          loaderVersion: "0.16.14",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-backup",
          state: "ready",
        },
      ]),
    );
  });
  await page.reload();
});

test("M8-BACKUP-001 游戏停止后会话摘要和数据页展示前后备份", async ({ page }) => {
  await page.getByRole("button", { name: "启动游戏" }).click();
  await expect(page.getByText("正在运行", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "停止游戏" }).click();

  await expect(page.getByText("世界备份：启动前 已备份 · 退出后 已备份", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "数据" }).click();
  await expect(page.getByRole("heading", { name: "世界备份" })).toBeVisible();
  await expect(page.getByText("启动前", { exact: true })).toBeVisible();
  await expect(page.getByText("退出后", { exact: true })).toBeVisible();
  await expect(page.getByText("1 个世界", { exact: false })).toHaveCount(2);
  await expect(page.getByText("已完成", { exact: true })).toHaveCount(2);
  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
});

test("UI-BACKUP-001 备份时间线在 960x600 和 200% 放大下无横向溢出", async ({ page }) => {
  await page.getByRole("button", { name: "启动游戏" }).click();
  await page.getByRole("button", { name: "停止游戏" }).click();
  await page.getByRole("button", { name: "数据" }).click();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByText("退出后", { exact: true })).toBeVisible();
  const geometry = await page.evaluate(() => ({
    documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".data-content *")]
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
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.overflowingElements).toEqual([]);
});
