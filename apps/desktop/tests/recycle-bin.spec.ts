import { expect, test } from "@playwright/test";

const instance = {
  id: "instance-recycle",
  name: "生存世界",
  gameVersion: "1.21.8",
  loaderKind: "fabric",
  loaderVersion: "0.16.14",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-recycle",
  state: "ready",
};

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate((managedInstance) => {
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
      JSON.stringify([managedInstance]),
    );
  }, instance);
  await page.reload();
});

test("M7-RECYCLE-001 实例经确认进入回收站并可从数据页恢复", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
  await page.getByRole("button", { name: "将“生存世界”移入回收站" }).click();

  const dialog = page.getByRole("dialog", { name: "将“生存世界”移入回收站？" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "取消" })).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(dialog.getByRole("button", { name: "移入回收站" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(dialog.getByRole("button", { name: "取消" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
  await page.getByRole("button", { name: "将“生存世界”移入回收站" }).click();
  await expect(dialog.getByText("保留 30 天", { exact: false })).toBeVisible();
  await expect(dialog.getByText("托管 Java 不会被删除", { exact: false })).toBeVisible();
  await dialog.getByRole("button", { name: "移入回收站" }).click();

  await expect(page.getByRole("heading", { name: "这里还空着" })).toBeVisible();
  await page.getByRole("button", { name: "数据" }).click();
  await expect(page.getByRole("heading", { name: "数据与回收站" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
  await expect(page.getByText("30 天后到期", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "恢复“生存世界”" }).click();
  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "首页" }).click();
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
});

test("M7-RECYCLE-002 永久删除前展示空间与不可恢复说明", async ({ page }) => {
  await page.getByRole("button", { name: "将“生存世界”移入回收站" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "移入回收站" }).click();
  await page.getByRole("button", { name: "数据" }).click();
  await page.getByRole("button", { name: "永久删除“生存世界”" }).click();

  const dialog = page.getByRole("dialog", { name: "永久删除“生存世界”？" });
  await expect(dialog.getByRole("button", { name: "取消" })).toBeFocused();
  await expect(dialog.getByText("1 个实例", { exact: false })).toBeVisible();
  await expect(dialog.getByText("64.0 MiB", { exact: false })).toBeVisible();
  await expect(dialog.getByText("无法恢复", { exact: false })).toBeVisible();
  await dialog.getByRole("button", { name: "永久删除" }).click();

  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
});

test("UI-RECYCLE-001 数据页在 960x600 和 200% 放大下无横向溢出", async ({ page }) => {
  await page.getByRole("button", { name: "将“生存世界”移入回收站" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "移入回收站" }).click();
  await page.getByRole("button", { name: "数据" }).click();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByRole("button", { name: "恢复“生存世界”" })).toBeVisible();
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
