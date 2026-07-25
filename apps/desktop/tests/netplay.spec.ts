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
  });
  await page.reload();
  await page.getByRole("button", { name: "联机", exact: true }).first().click();
});

test("NET-001 创建联机房间并管理生命周期", async ({ page }) => {
  await expect(page.locator(".sn-item", { hasText: "联机房间" })).toBeVisible();
  await expect(page.locator(".sn-item", { hasText: "NAT 类型检测" })).toBeVisible();

  await page.getByRole("textbox", { name: "房间号" }).fill("tundra-01");
  await page.getByRole("textbox", { name: "房间密码" }).fill("secret-12345");
  await page.getByRole("button", { name: "创建房间" }).click();

  await expect(page.locator(".netplay-room-name")).toHaveText("tundra-01");
  await expect(page.getByText("主机", { exact: true })).toBeVisible();
  await expect(page.getByText("10.144.144.1", { exact: false })).toBeVisible();
  await expect(page.getByText("房间已创建", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "离开房间" }).click();
  await expect(page.getByRole("button", { name: "创建房间" })).toBeVisible();
});

test("NET-002 房间密码过短显示可读错误", async ({ page }) => {
  await page.getByRole("textbox", { name: "房间号" }).fill("tundra-01");
  await page.getByRole("textbox", { name: "房间密码" }).fill("short");
  await page.getByRole("button", { name: "创建房间" }).click();
  await expect(page.getByRole("alert").getByText("房间密码必须是 8-64 位", { exact: false })).toBeVisible();
});

test("NET-003 加入者房间显示成员徽章", async ({ page }) => {
  await page.getByRole("textbox", { name: "房间号" }).fill("friend-01");
  await page.getByRole("textbox", { name: "房间密码" }).fill("secret-12345");
  await page.getByRole("button", { name: "加入房间" }).click();
  await expect(page.getByText("成员", { exact: true })).toBeVisible();
  await expect(page.getByText("自动分配（DHCP）", { exact: false })).toBeVisible();
});

test("NET-004 NAT 检测展示结果", async ({ page }) => {
  await page.locator(".sn-item", { hasText: "NAT 类型检测" }).click();
  await page.getByRole("button", { name: "检测 NAT 类型" }).click();
  await expect(page.getByText("203.0.113.55:54321")).toBeVisible();
  await expect(page.getByText("你在 NAT 之后", { exact: false })).toBeVisible();
});

test("NET-UI-001 联机页在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".settings-main *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
          element.clientWidth > 0 &&
          element.scrollWidth > element.clientWidth + 1,
      )
      .map((element) => element.className),
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
  expect(geometry.overflowingElements).toEqual([]);
});
