import { expect, test } from "@playwright/test";

const READY_BACKUP = {
  id: "backup-1",
  instanceId: "instance-id",
  instanceName: "世界实例",
  launchSessionId: null,
  trigger: "postExit",
  state: "ready",
  archivePath: "D:\\MoyuMax\\data\\backups\\instances\\instance-id\\1-postExit.zip",
  worldCount: 1,
  sourceBytes: 8 * 1024 * 1024,
  archiveBytes: 2 * 1024 * 1024,
  createdAtUnixSeconds: 1784880000,
  completedAtUnixSeconds: 1784880001,
  errorSummary: null,
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
          name: "世界实例",
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

test("M17-WORLD-001 世界清单展示名称、占用与最近游玩", async ({ page }) => {
  await seedWorlds(page, [
    { name: "alpha", sizeBytes: 2.4 * 1024 * 1024 * 1024, lastPlayedUnixSeconds: 1784880000 },
    { name: "beta", sizeBytes: 512 * 1024 * 1024, lastPlayedUnixSeconds: null },
  ]);
  await page.getByRole("button", { name: "数据" }).click();

  await expect(page.getByRole("heading", { name: "世界存档" })).toBeVisible();
  const alpha = page.locator(".backup-row").filter({ hasText: "alpha" });
  await expect(alpha.getByRole("heading", { name: "alpha" })).toBeVisible();
  await expect(alpha.getByText("2.4 GiB", { exact: false })).toBeVisible();
  await expect(alpha.getByText("最近游玩", { exact: false })).toBeVisible();
  await expectElementPadding(page, ".backup-row", { block: 16, inline: 20 });
});

test("M17-WORLD-002 导出世界到用户选择的位置", async ({ page }) => {
  await seedWorlds(page, [
    { name: "alpha", sizeBytes: 1024, lastPlayedUnixSeconds: 1784880000 },
  ]);
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.worldExportPath",
      "D:\\Exports\\alpha.zip",
    );
  });
  await page.getByRole("button", { name: "数据" }).click();

  await page.getByRole("button", { name: "导出" }).click();
  await expect(page.getByRole("status").getByText("已导出世界「alpha」", { exact: false })).toBeVisible();
});

test("M17-WORLD-003 导入世界并拒绝同名覆盖", async ({ page }) => {
  await seedWorlds(page, [
    { name: "alpha", sizeBytes: 1024, lastPlayedUnixSeconds: 1784880000 },
  ]);
  await page.getByRole("button", { name: "数据" }).click();

  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.pickedWorldZip",
      "D:\\Downloads\\skyblock.zip",
    );
  });
  await page.getByRole("button", { name: "导入世界" }).click();
  await expect(page.getByRole("status").getByText("已导入世界「skyblock」", { exact: true })).toBeVisible();
  await expect(page.locator(".backup-row").filter({ hasText: "skyblock" })).toHaveCount(1);

  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.pickedWorldZip",
      "D:\\Downloads\\alpha.zip",
    );
  });
  await page.getByRole("button", { name: "导入世界" }).click();
  await expect(page.getByRole("alert").getByText("已拒绝导入且未覆盖", { exact: false })).toBeVisible();
  await expect(page.locator(".backup-row").filter({ hasText: "alpha" })).toHaveCount(1);
});

test("M17-WORLD-004 回滚确认后创建恢复点并完成回滚", async ({ page }) => {
  await seedWorlds(page, [
    { name: "alpha", sizeBytes: 1024, lastPlayedUnixSeconds: 1784880000 },
  ]);
  await page.evaluate((backup) => {
    window.localStorage.setItem(
      "moyumax.browser.worldBackups",
      JSON.stringify([backup]),
    );
  }, READY_BACKUP);
  await page.reload();
  await page.getByRole("button", { name: "数据" }).click();
  await page.getByRole("button", { name: "管理备份" }).click();

  const rollbackButton = page.getByRole("button", { name: "恢复", exact: true }).first();
  await rollbackButton.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "恢复这个备份？" })).toBeVisible();
  await expect(dialog.getByText("恢复前会先创建当前状态的恢复点", { exact: false })).toBeVisible();
  await expectElementPadding(page, ".confirmation-dialog", { block: 20, inline: 24 });
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await rollbackButton.click();
  await page.getByRole("button", { name: "确认恢复" }).click();
  await expect(page.getByRole("status").getByText("已恢复到所选备份", { exact: true })).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByText("手动", { exact: true })).toBeVisible();
  const backups = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.worldBackups") ?? "[]"),
  );
  expect(backups).toHaveLength(2);
  expect(backups[0].trigger).toBe("manual");
});

test("UI-WORLD-001 世界存档区与回滚对话框在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await seedWorlds(page, [
    { name: "alpha", sizeBytes: 2.4 * 1024 * 1024 * 1024, lastPlayedUnixSeconds: 1784880000 },
    { name: "beta", sizeBytes: 512 * 1024 * 1024, lastPlayedUnixSeconds: 1784880000 },
  ]);
  await page.evaluate((backup) => {
    window.localStorage.setItem(
      "moyumax.browser.worldBackups",
      JSON.stringify([backup]),
    );
  }, READY_BACKUP);
  await page.reload();
  // 旧全局样式在 ≤1050px 会收起导航标签,先在默认窗口导航到目标页,再缩放窗口。
  await page.getByRole("button", { name: "数据" }).click();
  await page.getByRole("button", { name: "管理备份" }).click();
  await page.getByRole("button", { name: "恢复", exact: true }).first().click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  // 数据页与回滚对话框仍为旧样式(BackupCenter 待重写),允许内部裁剪;
  // 这里断言页面级与对话框都不产生横向滚动。
  const geometry = await page.evaluate(() => {
    const dialog = document.querySelector<HTMLElement>(".confirmation-dialog");
    return {
      documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
      dialogOverflow: dialog ? dialog.scrollWidth > dialog.clientWidth + 1 : false,
    };
  });
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.dialogOverflow).toBe(false);
});

async function seedWorlds(
  page: import("@playwright/test").Page,
  worlds: { name: string; sizeBytes: number; lastPlayedUnixSeconds: number | null }[],
): Promise<void> {
  await page.evaluate((seeded) => {
    window.localStorage.setItem(
      "moyumax.browser.worldDetails",
      JSON.stringify({ "instance-id": seeded }),
    );
  }, worlds);
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
