import { expect, test } from "@playwright/test";

function resourceEntry(overrides: Record<string, unknown> = {}) {
  return {
    id: "resource-1",
    instanceId: "instance-id",
    kind: "resourcepack",
    displayName: "faithful",
    fileName: "faithful.zip",
    relativePath: ".minecraft/resourcepacks/faithful.zip",
    size: 1024,
    sha256: "3".repeat(64),
    enabled: true,
    worldName: null,
    importedAtUnixSeconds: 1,
    ...overrides,
  };
}

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
          name: "资源测试",
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

test("M16-RES-001 导入资源包后出现在资源内容清单", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.pickedResourceFile",
      "D:\\Downloads\\faithful.zip",
    );
  });
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();
  await expect(page.getByRole("heading", { name: "资源内容" })).toBeVisible();
  await expect(page.getByText("还没有导入资源包、光影或数据包", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "导入资源包" }).click();
  const row = page.locator(".installed-content-row").filter({ hasText: "faithful" });
  await expect(row).toBeVisible();
  await expect(row.getByText("资源包 · faithful.zip", { exact: true })).toBeVisible();
  await expect(page.getByRole("checkbox", { name: "faithful 启用开关" })).toBeChecked();
  await expectElementPadding(page, ".installed-content-row", { block: 16, inline: 20 });
});

test("M16-RES-002 同名资源拒绝导入且不覆盖清单", async ({ page }) => {
  const existing = resourceEntry();
  await page.evaluate((entry) => {
    window.localStorage.setItem(
      "moyumax.browser.instanceResources",
      JSON.stringify([entry]),
    );
    window.localStorage.setItem(
      "moyumax.browser.pickedResourceFile",
      "D:\\Downloads\\faithful.zip",
    );
  }, existing);
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  await page.getByRole("button", { name: "导入资源包" }).click();
  await expect(page.getByText("已拒绝导入且未覆盖", { exact: false })).toBeVisible();
  await expect(page.locator(".installed-content-row").filter({ hasText: "faithful" })).toHaveCount(1);
});

test("M16-RES-003 启用与停用切换并持久化", async ({ page }) => {
  const existing = resourceEntry();
  await page.evaluate((entry) => {
    window.localStorage.setItem(
      "moyumax.browser.instanceResources",
      JSON.stringify([entry]),
    );
  }, existing);
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  const toggle = page.getByRole("checkbox", { name: "faithful 启用开关" });
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect(page.getByText("已停用", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceResources") ?? "[]"),
  );
  expect(stored[0].enabled).toBe(false);
  await toggle.check();
  await expect(page.getByText("已启用", { exact: true })).toBeVisible();
});

test("M16-RES-004 数据包必须选择世界后导入", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instanceWorlds",
      JSON.stringify({ "instance-id": ["world-a", "world-b"] }),
    );
    window.localStorage.setItem(
      "moyumax.browser.pickedResourceFile",
      "D:\\Downloads\\tweaks.zip",
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  await page.getByRole("button", { name: "导入数据包" }).click();
  const worldSelect = page.locator(".datapack-import-form select");
  await expect(worldSelect).toBeVisible();
  await worldSelect.selectOption("world-b");
  await expectElementPadding(page, ".datapack-import-form", { block: 16, inline: 20 });
  await page.getByRole("button", { name: "选择文件并导入" }).click();

  const row = page.locator(".installed-content-row").filter({ hasText: "tweaks" });
  await expect(row).toBeVisible();
  await expect(row.getByText("数据包 · 世界 world-b · tweaks.zip", { exact: true })).toBeVisible();
});

test("M16-RES-005 没有世界时数据包导入被阻止并说明原因", async ({ page }) => {
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();
  await page.getByRole("button", { name: "导入数据包" }).click();
  await expect(page.getByText("这个实例还没有世界", { exact: false })).toBeVisible();
  await expect(page.locator(".datapack-import-form")).toHaveCount(0);
});

test("UI-RES-001 资源内容区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  const seeded = [
    resourceEntry(),
    resourceEntry({
      id: "resource-2",
      kind: "shader",
      displayName: "complementary",
      fileName: "complementary.zip",
      relativePath: ".minecraft/shaderpacks/complementary.zip",
      enabled: false,
    }),
    resourceEntry({
      id: "resource-3",
      kind: "datapack",
      displayName: "tweaks",
      fileName: "tweaks.zip",
      relativePath: ".minecraft/saves/world-b/datapacks/tweaks.zip",
      worldName: "world-b",
    }),
  ];
  await page.evaluate((entries) => {
    window.localStorage.setItem(
      "moyumax.browser.instanceResources",
      JSON.stringify(entries),
    );
    window.localStorage.setItem(
      "moyumax.browser.instanceWorlds",
      JSON.stringify({ "instance-id": ["world-a", "world-b"] }),
    );
  }, seeded);
  await page.reload();
  // 旧全局样式在 ≤1050px 会收起导航标签,先在默认窗口导航并打开导入表单,再缩放窗口。
  await page.getByRole("button", { name: "资源", exact: true }).click();
  await page.getByRole("button", { name: "实例内容" }).click();
  await page.getByRole("button", { name: "导入数据包" }).click();
  await expect(page.locator(".datapack-import-form")).toBeVisible();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  // 资源页仍为旧样式(ResourceCenter 待重写),允许内部裁剪;
  // 这里断言页面级与容器都不产生横向滚动。
  const geometry = await page.evaluate(() => {
    const content = document.querySelector<HTMLElement>(".resource-content");
    return {
      documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
      containerOverflow: content ? content.scrollWidth > content.clientWidth + 1 : false,
    };
  });
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.containerOverflow).toBe(false);
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
