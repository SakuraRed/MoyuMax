import { expect, test } from "@playwright/test";

const VALID_PACK = {
  formatVersion: 1,
  name: "苔原",
  author: "Moyu",
  colors: {
    "bg-app": "#101410",
    accent: "#7cc46c",
    text: "#f2f2ef",
  },
};

const VALID_PACK_V2 = {
  formatVersion: 2,
  id: "test-pack",
  name: "测试包",
  author: "Moyu",
  base: {
    tokens: {
      "--accent": "#3366ff",
    },
    rules: [
      {
        selector: ".panel",
        declarations: { "border-radius": "24px" },
      },
    ],
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
          name: "主题实例",
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

test("M28-THEME-001 纯色背景应用与持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("color");
  await page.getByRole("button", { name: "应用颜色" }).click();

  await expect(page.locator(".window")).toHaveAttribute("data-background", "color");
  const styled = await page.locator(".window").evaluate((element) =>
    element.getAttribute("style") ?? "",
  );
  expect(styled).toContain("--bg-window: #1b1b1f");

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-background", "color");
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("default");
  await expect(page.locator(".window")).toHaveAttribute("data-background", "default");
});

test("M28-THEME-002 图片背景在减少动画时降级", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.pickedBackgroundImage", "D:\\Pictures\\wall.png");
  });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("image");
  await page.getByRole("button", { name: "选择图片" }).click();

  await expect(page.locator(".window")).toHaveAttribute("data-background", "image");
  const styled = await page.locator(".window").evaluate((element) =>
    element.getAttribute("style") ?? "",
  );
  expect(styled).toContain("background-image");

  await page.locator(".sn-item", { hasText: "无障碍" }).click();
  await page.getByRole("group", { name: "动画偏好" }).getByRole("button", { name: "减少动画", exact: true }).click();
  const degraded = await page.locator(".window").evaluate((element) =>
    element.getAttribute("style") ?? "",
  );
  expect(degraded).not.toContain("background-image");

  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByRole("button", { name: "清除图片" }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-background", "default");
});

test("M28-THEME-003 主题包应用配色且高对比忽略", async ({ page }) => {
  await page.evaluate((pack) => {
    window.localStorage.setItem("moyumax.browser.themePackJson", JSON.stringify(pack));
    window.localStorage.setItem("moyumax.browser.pickedThemePack", "D:\\Themes\\tundra.json");
  }, VALID_PACK);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("themePack");
  await page.getByRole("button", { name: "导入主题包", exact: true }).click();

  await expect(page.locator(".window")).toHaveAttribute("data-background", "themePack");
  await expect(page.getByText("主题包「苔原」已应用", { exact: true })).toBeVisible();
  const styled = await page.locator(".window").evaluate((element) =>
    element.getAttribute("style") ?? "",
  );
  expect(styled).toContain("--accent: #7cc46c");

  await page.locator(".sn-item", { hasText: "无障碍" }).click();
  await page.getByRole("group", { name: "对比度偏好" }).getByRole("button", { name: "高对比", exact: true }).click();
  const degraded = await page.locator(".window").evaluate((element) =>
    element.getAttribute("style") ?? "",
  );
  expect(degraded).not.toContain("--accent: #7cc46c");

  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByRole("button", { name: "移除主题包" }).click();
  await expect(page.locator(".window")).toHaveAttribute("data-background", "default");
});

test("M28-THEME-004 恶意主题包被拒绝", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.themePackJson",
      JSON.stringify({
        formatVersion: 1,
        name: "evil",
        author: "evil",
        colors: { accent: "url(https://evil.example/x.css)" },
      }),
    );
    window.localStorage.setItem("moyumax.browser.pickedThemePack", "D:\\Themes\\evil.json");
  });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("themePack");
  await page.getByRole("button", { name: "导入主题包", exact: true }).click();

  await expect(page.getByRole("alert").getByText("#rrggbb", { exact: false })).toBeVisible();
  await expect(page.locator(".window")).toHaveAttribute("data-background", "default");
});

test("M28-THEME-V2-001 内置主题包切换与持久化", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  const packSelect = page.getByLabel("主题包选择");
  await packSelect.selectOption("animal-island");

  await expect(page.locator(".window")).toHaveAttribute("data-tp", "animal-island");
  const accent = await page
    .locator(".window")
    .evaluate((element) => getComputedStyle(element).getPropertyValue("--accent").trim());
  expect(accent.toLowerCase()).toBe("#7fb069");
  const panelRadius = await page
    .locator(".panel")
    .first()
    .evaluate((element) => getComputedStyle(element).borderRadius);
  expect(panelRadius).toBe("18px");

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-tp", "animal-island");

  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("主题包选择").selectOption("default");
  await expect(page.locator(".window")).toHaveAttribute("data-tp", "default");
});

test("M28-THEME-V2-002 主题包 v2 导入启用与持久化", async ({ page }) => {
  await page.evaluate((pack) => {
    window.localStorage.setItem("moyumax.browser.themePackJson", JSON.stringify(pack));
    window.localStorage.setItem("moyumax.browser.pickedThemePack", "D:\\Themes\\test-pack.json");
  }, VALID_PACK_V2);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByRole("button", { name: "导入主题包…" }).click();

  await expect(page.getByRole("status").getByText("已导入并启用「测试包」")).toBeVisible();
  const packSelect = page.getByLabel("主题包选择");
  await expect(packSelect.locator("option", { hasText: "测试包" })).toHaveCount(1);
  await expect(packSelect).toHaveValue("test-pack");
  await expect(page.locator(".window")).toHaveAttribute("data-tp", "test-pack");
  const accent = await page
    .locator(".window")
    .evaluate((element) => getComputedStyle(element).getPropertyValue("--accent").trim());
  expect(accent.toLowerCase()).toBe("#3366ff");

  await page.reload();
  await expect(page.locator(".window")).toHaveAttribute("data-tp", "test-pack");
});

test("UI-THEME-002 背景设置区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "外观" }).click();
  await page.getByLabel("背景", { exact: true }).selectOption("themePack");
  await expect(page.getByRole("button", { name: "导入主题包", exact: true })).toBeVisible();
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
