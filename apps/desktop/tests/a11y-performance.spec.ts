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
          name: "验收实例",
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

test("M23-A11Y-001 减少动画手动开关立即生效并持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-motion", "system");

  await page.getByRole("button", { name: "减少动画", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-motion", "reduce");

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-motion", "reduce");
  await page.getByRole("button", { name: "设置" }).click();
  await page.getByRole("group", { name: "动画偏好" }).getByRole("button", { name: "跟随系统", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-motion", "system");
});

test("M23-A11Y-002 高对比手动开关立即生效并持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-contrast", "standard");
  const before = await page.locator(".window").evaluate((element) => {
    const style = getComputedStyle(element);
    return { border: style.getPropertyValue("--border").trim(), text: style.getPropertyValue("--text").trim() };
  });

  await page.getByRole("button", { name: "高对比", exact: true }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-contrast", "high");
  const after = await page.locator(".window").evaluate((element) => {
    const style = getComputedStyle(element);
    return { border: style.getPropertyValue("--border").trim(), text: style.getPropertyValue("--text").trim() };
  });
  expect(after.border).toBe(after.text);
  expect(after.border).not.toBe(before.border);

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-contrast", "high");
});

test("M23-A11Y-003 首页主操作全部可键盘到达且焦点可见", async ({ page }) => {
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
  const focused: { role: string; name: string }[] = [];
  for (let index = 0; index < 24; index += 1) {
    await page.keyboard.press("Tab");
    const current = await page.evaluate(() => {
      const element = document.activeElement as HTMLElement | null;
      if (!element || element === document.body) return null;
      const role = element.getAttribute("role") ?? element.tagName.toLowerCase();
      const name = element.getAttribute("aria-label") ?? element.textContent?.trim() ?? "";
      const outlineWidth = getComputedStyle(element).outlineWidth;
      return { role, name, outlineWidth };
    });
    if (current && !focused.some((entry) => entry.role === current.role && entry.name === current.name)) {
      focused.push(current);
      if (current.role === "button") {
        expect(Number.parseFloat(current.outlineWidth)).toBeGreaterThan(0);
      }
    }
  }
  const names = focused.map((entry) => entry.name).join("|");
  expect(names).toContain("启动游戏");
  expect(names).toContain("安装");
  expect(focused.some((entry) => entry.name === "资源")).toBe(true);
  expect(focused.some((entry) => entry.name === "任务")).toBe(true);
  expect(focused.some((entry) => entry.name === "设置")).toBe(true);
});
