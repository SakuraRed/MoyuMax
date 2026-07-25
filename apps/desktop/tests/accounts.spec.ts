import { expect, test } from "@playwright/test";

function accountEntry(overrides: Record<string, unknown> = {}) {
  return {
    id: crypto.randomUUID(),
    kind: "offline",
    username: "Steve_2026",
    playerUuid: crypto.randomUUID(),
    serverUrl: null,
    isDefault: false,
    sessionState: "valid",
    createdAtUnixSeconds: 1784880000,
    lastValidatedAtUnixSeconds: null,
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
          name: "账户实例",
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

test("M20-ACCT-001 创建离线账户并自动成为默认", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();
  await expect(page.getByRole("heading", { name: "账户" })).toBeVisible();
  await expect(page.getByText("还没有账户", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "添加离线账户" }).click();
  await page.getByRole("textbox", { name: "离线玩家名" }).fill("Steve_2026");
  await expectElementPadding(page, ".account-form", { block: 16, inline: 20 });
  await page.getByRole("button", { name: "创建离线账户" }).click();

  const row = page.locator(".backup-row").filter({ hasText: "Steve_2026" });
  await expect(row).toBeVisible();
  await expect(row.getByText("离线", { exact: true })).toBeVisible();
  await expect(row.getByText("默认", { exact: true })).toBeVisible();
  await expect(row.getByText("无法加入正版服务器", { exact: false })).toBeVisible();
});

test("M20-ACCT-002 外置登录添加账户且凭据错误可读", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();
  await page.getByRole("button", { name: "添加外置账户" }).click();
  await page.getByRole("textbox", { name: "外置账户用户名" }).fill("Alex@littleskin.cn");
  await page.getByRole("textbox", { name: "外置账户密码" }).fill("s3cret");
  await page.getByRole("button", { name: "登录并添加" }).click();

  const row = page.locator(".backup-row").filter({ hasText: "Alex" });
  await expect(row).toBeVisible();
  await expect(row.getByText("外置", { exact: true })).toBeVisible();
  await expect(row.getByText("令牌仅保存在本地", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "添加外置账户" }).click();
  await page.getByRole("textbox", { name: "外置账户用户名" }).fill("Bad@littleskin.cn");
  await page.getByRole("textbox", { name: "外置账户密码" }).fill("wrong");
  await page.getByRole("button", { name: "登录并添加" }).click();
  await expect(page.getByRole("alert").getByText("凭据无效或会话已过期", { exact: false })).toBeVisible();
  await expect(page.locator(".backup-row").filter({ hasText: "Bad" })).toHaveCount(0);
});

test("M20-ACCT-003 默认唯一且移除默认后最早剩余接任", async ({ page }) => {
  await seedAccounts(page, [
    accountEntry({ id: "acct-offline", username: "Steve_2026", isDefault: true }),
    accountEntry({
      id: "acct-authlib",
      kind: "authlib",
      username: "Alex",
      serverUrl: "https://littleskin.cn/api/yggdrasil",
    }),
  ]);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();

  const authlibRow = page.locator(".backup-row").filter({ hasText: "Alex" });
  await authlibRow.getByRole("button", { name: "设为默认" }).click();
  await expect(page.locator(".backup-row").filter({ hasText: "Alex" }).getByText("默认", { exact: true })).toBeVisible();
  await expect(page.locator(".backup-row").filter({ hasText: "Steve_2026" }).getByText("默认", { exact: true })).toHaveCount(0);

  await page.getByRole("button", { name: "移除账户 Alex" }).click();
  await authlibRow.getByRole("button", { name: "确认移除" }).click();
  await expect(page.locator(".backup-row").filter({ hasText: "Alex" })).toHaveCount(0);
  await expect(page.locator(".backup-row").filter({ hasText: "Steve_2026" }).getByText("默认", { exact: true })).toBeVisible();
});

test("M20-ACCT-004 会话过期的账户刷新失败并保留过期标记", async ({ page }) => {
  await seedAccounts(page, [
    accountEntry({
      id: "acct-expired",
      kind: "authlib",
      username: "Alex",
      serverUrl: "https://littleskin.cn/api/yggdrasil",
      isDefault: true,
      sessionState: "expired",
    }),
  ]);
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();

  await expect(page.getByText("会话已过期", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "刷新会话" }).click();
  await expect(page.getByRole("alert").getByText("请重新登录", { exact: false })).toBeVisible();
  await expect(page.getByText("会话已过期", { exact: true })).toBeVisible();
});

test("M20-ACCT-005 Microsoft 提供真实设备码登录入口", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();
  await expect(page.getByRole("button", { name: "添加 Microsoft 账户" })).toBeVisible();
  await expect(page.getByText("登录功能在后续里程碑提供", { exact: false })).toHaveCount(0);
});

test("UI-ACCT-001 账户区与添加表单在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await seedAccounts(page, [
    accountEntry({ id: "acct-offline", username: "Steve_2026", isDefault: true }),
  ]);
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "账户" }).click();
  await page.getByRole("button", { name: "添加外置账户" }).click();
  await expect(page.getByRole("textbox", { name: "外置账户用户名" })).toBeVisible();
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

async function seedAccounts(
  page: import("@playwright/test").Page,
  accounts: Record<string, unknown>[],
): Promise<void> {
  await page.evaluate((seeded) => {
    window.localStorage.setItem("moyumax.browser.accounts", JSON.stringify(seeded));
  }, accounts);
  await page.reload();
}

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
