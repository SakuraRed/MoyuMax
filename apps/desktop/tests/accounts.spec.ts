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

/** 导航底部账户卡直达顶级账户页。 */
async function openAccountsPage(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "账户", exact: true }).click();
  await expect(page.getByRole("heading", { name: "账户" })).toBeVisible();
}

test("M20-ACCT-001 创建离线账户并自动成为默认", async ({ page }) => {
  await openAccountsPage(page);
  await expect(page.getByText("还没有账户", { exact: false })).toBeVisible();

  // 无账户时添加菜单默认展开,直接可选离线类型。
  await page.getByRole("button", { name: "添加离线账户" }).click();
  await page.getByRole("textbox", { name: "离线玩家名" }).fill("Steve_2026");
  await expectElementPadding(page, ".acct-form", { block: 16, inline: 20 });
  await page.getByRole("button", { name: "创建离线账户" }).click();

  const row = page.locator(".acct-row").filter({ hasText: "Steve_2026" });
  await expect(row).toBeVisible();
  await expect(row.getByText("离线账户", { exact: true })).toBeVisible();
  await expect(row.getByText("默认", { exact: true })).toBeVisible();
  await expect(row.getByText("不能加入开启正版验证的服务器", { exact: false })).toBeVisible();
});

test("M20-ACCT-002 外置登录添加账户且凭据错误可读", async ({ page }) => {
  await openAccountsPage(page);
  await page.getByRole("button", { name: "添加 LittleSkin 账户" }).click();
  await page.getByRole("textbox", { name: "外置账户用户名" }).fill("Alex@littleskin.cn");
  await page.getByRole("textbox", { name: "外置账户密码" }).fill("s3cret");
  await page.getByRole("button", { name: "登录并添加" }).click();

  const row = page.locator(".acct-row").filter({ hasText: "Alex" });
  await expect(row).toBeVisible();
  await expect(page.getByText("第三方认证", { exact: true })).toBeVisible();
  await expect(row.getByText("LittleSkin", { exact: false })).toBeVisible();
  await expect(row.getByText("令牌仅保存在本地", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "添加账户" }).click();
  await page.getByRole("button", { name: "添加 LittleSkin 账户" }).click();
  await page.getByRole("textbox", { name: "外置账户用户名" }).fill("Bad@littleskin.cn");
  await page.getByRole("textbox", { name: "外置账户密码" }).fill("wrong");
  await page.getByRole("button", { name: "登录并添加" }).click();
  await expect(page.getByRole("alert").getByText("凭据无效或会话已过期", { exact: false })).toBeVisible();
  await expect(page.locator(".acct-row").filter({ hasText: "Bad" })).toHaveCount(0);
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
  await openAccountsPage(page);

  const authlibRow = page.locator(".acct-row").filter({ hasText: "Alex" });
  await authlibRow.getByRole("button", { name: "设为默认" }).click();
  await expect(page.locator(".acct-row").filter({ hasText: "Alex" }).getByText("默认", { exact: true })).toBeVisible();
  await expect(page.locator(".acct-row").filter({ hasText: "Steve_2026" }).getByText("默认", { exact: true })).toHaveCount(0);

  await page.getByRole("button", { name: "移除账户 Alex" }).click();
  await authlibRow.getByRole("button", { name: "确认移除" }).click();
  await expect(page.locator(".acct-row").filter({ hasText: "Alex" })).toHaveCount(0);
  await expect(page.locator(".acct-row").filter({ hasText: "Steve_2026" }).getByText("默认", { exact: true })).toBeVisible();
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
  await openAccountsPage(page);

  const row = page.locator(".acct-row").filter({ hasText: "Alex" });
  await expect(row.getByText("会话已过期", { exact: true })).toBeVisible();
  await expect(row.getByRole("button", { name: "重新登录" })).toBeVisible();
  await row.getByRole("button", { name: "刷新会话" }).click();
  await expect(page.getByRole("alert").getByText("请重新登录", { exact: false })).toBeVisible();
  await expect(row.getByText("会话已过期", { exact: true })).toBeVisible();
});

test("M20-ACCT-005 Microsoft 提供真实设备码登录入口", async ({ page }) => {
  await openAccountsPage(page);
  await expect(page.getByRole("button", { name: "添加 Microsoft 账户" })).toBeVisible();
  await expect(page.getByText("登录功能在后续里程碑提供", { exact: false })).toHaveCount(0);
});

test("M20-ACCT-006 本地保存密码先经风险确认且复选框默认不勾选", async ({ page }) => {
  await openAccountsPage(page);
  await page.getByRole("button", { name: "添加 LittleSkin 账户" }).click();
  await page.getByRole("textbox", { name: "外置账户用户名" }).fill("Alex@littleskin.cn");
  await page.getByRole("textbox", { name: "外置账户密码" }).fill("s3cret");
  await page.getByRole("checkbox", { name: "为该账户在本地保存密码(本地加密)" }).check();
  await page.getByRole("button", { name: "登录并添加" }).click();

  const dialog = page.getByRole("dialog", { name: "为「Alex@littleskin.cn」启用本地密码保存" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("默认只保存登录令牌", { exact: false })).toBeVisible();
  await expect(dialog.getByText("忘记主密码只能清除已保存密码", { exact: false })).toBeVisible();
  const confirm = dialog.getByRole("button", { name: "设置主密码并启用" });
  await expect(confirm).toBeDisabled();
  await dialog.getByRole("button", { name: "取消" }).click();
  await expect(dialog).toBeHidden();
  await expect(page.locator(".acct-row").filter({ hasText: "Alex" })).toHaveCount(0);

  await page.getByRole("button", { name: "登录并添加" }).click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole("checkbox", { name: "我已了解上述风险" }).check();
  await expect(confirm).toBeEnabled();
  await confirm.click();
  await expect(page.locator(".acct-row").filter({ hasText: "Alex" })).toBeVisible();
});

test("UI-ACCT-001 账户区与添加表单在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await seedAccounts(page, [
    accountEntry({ id: "acct-offline", username: "Steve_2026", isDefault: true }),
  ]);
  await page.setViewportSize({ width: 960, height: 600 });
  await openAccountsPage(page);
  await page.getByRole("button", { name: "添加账户" }).click();
  await page.getByRole("button", { name: "添加外置账户" }).click();
  await expect(page.getByRole("textbox", { name: "外置账户用户名" })).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  // 与同页 UI-SHOT/UI-WORLD/UI-BACKUP 一致:行内省略号属内部裁剪,
  // 断言页面级与内容容器都不产生横向滚动。
  const geometry = await page.evaluate(() => {
    const content = document.querySelector<HTMLElement>(".acct-content");
    return {
      documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
      containerOverflow: content ? content.scrollWidth > content.clientWidth + 1 : false,
    };
  });
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.containerOverflow).toBe(false);
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
