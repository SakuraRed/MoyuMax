import { expect, test, type Page } from "@playwright/test";

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
});

async function openDownloadSettings(page: Page): Promise<void> {
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "下载" }).click();
  await expect(page.getByRole("heading", { name: "下载" })).toBeVisible();
}

test("PROXY-001 默认跟随系统,切换直连立即持久化并回读", async ({ page }) => {
  await openDownloadSettings(page);

  const proxySelect = page.getByRole("combobox", { name: "代理", exact: true });
  await expect(proxySelect).toHaveValue("system");
  await expect(
    page.getByText("保存后新发起的查询与下载立即生效", { exact: false }),
  ).toBeVisible();

  await proxySelect.selectOption("direct");
  await expect(
    page.locator(".java-notice").getByText("代理设置已保存", { exact: false }),
  ).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.proxyPreference") ?? "null"),
  );
  expect(stored).toEqual({ mode: "direct" });

  // 回读:离开设置页再进入,选择保持
  await page.getByRole("button", { name: "首页", exact: true }).click();
  await openDownloadSettings(page);
  await expect(page.getByRole("combobox", { name: "代理", exact: true })).toHaveValue("direct");
});

test("PROXY-002 自定义代理非法地址拒绝且不写入", async ({ page }) => {
  await openDownloadSettings(page);

  await page.getByRole("combobox", { name: "代理", exact: true }).selectOption("custom");
  const urlInput = page.getByRole("textbox", { name: "代理地址" });
  await expect(urlInput).toBeVisible();

  // 空地址:拒绝
  await page.getByRole("button", { name: "保存自定义代理" }).click();
  await expect(
    page.getByRole("alert").getByText("http://、https:// 或 socks5h://", { exact: false }),
  ).toBeVisible();

  // 缺少协议:拒绝
  await urlInput.fill("127.0.0.1:10808");
  await page.getByRole("button", { name: "保存自定义代理" }).click();
  await expect(
    page.getByRole("alert").getByText("http://、https:// 或 socks5h://", { exact: false }),
  ).toBeVisible();

  // 不支持的协议:拒绝
  await urlInput.fill("ftp://127.0.0.1:10808");
  await page.getByRole("button", { name: "保存自定义代理" }).click();
  await expect(
    page.getByRole("alert").getByText("http://、https:// 或 socks5h://", { exact: false }),
  ).toBeVisible();

  const stored = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.proxyPreference"),
  );
  expect(stored).toBeNull();
});

test("PROXY-003 自定义代理合法地址保存并回读", async ({ page }) => {
  await openDownloadSettings(page);

  await page.getByRole("combobox", { name: "代理", exact: true }).selectOption("custom");
  const urlInput = page.getByRole("textbox", { name: "代理地址" });
  await urlInput.fill("http://127.0.0.1:10808");
  await page.getByRole("button", { name: "保存自定义代理" }).click();
  await expect(
    page.locator(".java-notice").getByText("代理设置已保存", { exact: false }),
  ).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.proxyPreference") ?? "null"),
  );
  expect(stored).toEqual({ mode: "custom", url: "http://127.0.0.1:10808" });

  // 回读:离开设置页再进入,模式与地址保持
  await page.getByRole("button", { name: "首页", exact: true }).click();
  await openDownloadSettings(page);
  await expect(page.getByRole("combobox", { name: "代理", exact: true })).toHaveValue("custom");
  await expect(page.getByRole("textbox", { name: "代理地址" })).toHaveValue(
    "http://127.0.0.1:10808",
  );
});
