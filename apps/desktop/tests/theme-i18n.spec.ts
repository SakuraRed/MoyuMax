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
          name: "外观实例",
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

test("M21-THEME-001 切换浅色主题立即生效并持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await expect(page.getByRole("heading", { name: "外观" })).toBeVisible();
  await expect(page.locator(".window")).toHaveAttribute("data-theme", "system");

  await page.getByRole("button", { name: "浅色", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-theme", "light");
  const background = await page.locator(".window").evaluate((element) =>
    getComputedStyle(element).getPropertyValue("--bg-app").trim(),
  );
  expect(background).toBe("#faf9f6");

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-theme", "light");
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByRole("group", { name: "界面主题" }).getByRole("button", { name: "跟随系统", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-theme", "system");
});

test("M21-THEME-002 深色主题不依赖系统设置", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByRole("button", { name: "深色", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-theme", "dark");
  const background = await page.locator(".window").evaluate((element) =>
    getComputedStyle(element).getPropertyValue("--bg-app").trim(),
  );
  expect(background).toBe("#1b1b1f");
});

test("M21-I18N-001 切换英文后界面文案立即变化并持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("heading", { name: "通用" })).toBeVisible();

  await page.getByRole("combobox", { name: "界面语言" }).selectOption({ label: "English" });
  await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "Settings" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "Home" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "Tasks" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "通用" })).toHaveCount(0);

  await page.reload();
  await expect(page.locator(".nav-item", { hasText: "Settings" })).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await page.locator(".sn-item", { hasText: "Appearance" }).click();
  await expect(page.getByRole("heading", { name: "Appearance" })).toBeVisible();
});

test("M21-I18N-002 切换繁体中文后界面文案变化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("combobox", { name: "界面语言" }).selectOption({ label: "繁體中文" });
  await expect(page.locator(".nav-item", { hasText: "首頁" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "任務" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "資料" })).toBeVisible();
  await expect(page.locator(".nav-item", { hasText: "設定" })).toBeVisible();
});

test("UI-THEME-001 外观设置区在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await expect(page.getByRole("heading", { name: "外观" })).toBeVisible();
  await expectElementPadding(page, ".panel.pad", { block: 18, inline: 20 });
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
