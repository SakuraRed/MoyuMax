import { expect, test } from "@playwright/test";

function seedBase() {
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
        name: "导出测试",
        gameVersion: "26.2",
        loaderKind: "fabric",
        loaderVersion: "0.19.3",
        rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
        state: "ready",
      },
    ]),
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(seedBase);
  await page.reload();
});

async function openExport(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "实例", exact: true }).click();
  await page.getByRole("button", { name: "管理实例「导出测试」" }).click();
  await expect(page.getByRole("heading", { name: "导出测试" })).toBeVisible();
  await page.getByLabel("实例管理").getByRole("button", { name: "设置", exact: true }).click();
  await expect(page.getByText("导出整合包", { exact: true })).toBeVisible();
}

test("M30-EXP-001 导出表单渲染并带默认值", async ({ page }) => {
  await openExport(page);

  await expect(page.getByRole("textbox", { name: "整合包名" })).toHaveValue("导出测试");
  await expect(page.getByRole("textbox", { name: "整合包版本号" })).toHaveValue("1.0.0");
  await expect(page.getByRole("checkbox", { name: "包含游戏设置" })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "包含资源包" })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "包含光影包" })).toBeChecked();
  await expect(page.getByRole("checkbox", { name: "包含服务器列表" })).not.toBeChecked();
  await expect(page.getByRole("checkbox", { name: "包含截图" })).not.toBeChecked();
});

test("M30-EXP-002 导出成功显示报告并记录选项", async ({ page }) => {
  await openExport(page);
  await page.getByRole("checkbox", { name: "包含服务器列表" }).check();
  await page.getByRole("checkbox", { name: "包含光影包" }).uncheck();

  await page.getByRole("button", { name: "开始导出" }).click();

  await expect(page.locator(".toast").getByText("整合包导出完成", { exact: true })).toBeVisible();
  await expect(page.getByText("产物路径", { exact: true })).toBeVisible();
  await expect(page.getByText("导出测试-1.0.0.mrpack", { exact: false })).toBeVisible();
  await expect(page.getByText("4.0 KiB", { exact: true })).toBeVisible();

  const record = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.lastModpackExport"),
  );
  const parsed = JSON.parse(record ?? "{}");
  expect(parsed.options).toMatchObject({
    name: "导出测试",
    version: "1.0.0",
    includeConfig: true,
    includeResourcePacks: true,
    includeShaders: false,
    includeServers: true,
    includeScreenshots: false,
  });
  expect(parsed.destinationPath).toContain("导出测试-1.0.0.mrpack");
});

test("M30-EXP-003 名称非法字符在默认文件名中被过滤", async ({ page }) => {
  await openExport(page);
  await page.getByRole("textbox", { name: "整合包名" }).fill('a/b:c*?d"|');
  await page.getByRole("textbox", { name: "整合包版本号" }).fill("2.0");

  await page.getByRole("button", { name: "开始导出" }).click();

  await expect(page.locator(".toast").getByText("整合包导出完成", { exact: true })).toBeVisible();
  const record = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.lastModpackExport"),
  );
  const parsed = JSON.parse(record ?? "{}");
  const fileName = String(parsed.destinationPath).split(/[\\/]/).pop() ?? "";
  expect(fileName).toBe("a-b-c--d---2.0.mrpack");
  expect(fileName).not.toMatch(/[\\/:*?"<>|]/);
});

test("M30-EXP-004 名称或版本为空时拒绝导出", async ({ page }) => {
  await openExport(page);
  await page.getByRole("textbox", { name: "整合包名" }).fill("   ");

  await page.getByRole("button", { name: "开始导出" }).click();

  await expect(page.locator(".toast").getByText("整合包名与版本号不能为空", { exact: true })).toBeVisible();
  const record = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.lastModpackExport"),
  );
  expect(record).toBeNull();
});
