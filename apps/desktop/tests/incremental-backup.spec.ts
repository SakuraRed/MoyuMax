import { expect, test } from "@playwright/test";

const FULL_BACKUP = {
  id: "backup-full",
  instanceId: "instance-id",
  instanceName: "备份实例",
  launchSessionId: null,
  trigger: "manual",
  state: "ready",
  archivePath: "D:\\MoyuMax\\data\\backups\\instances\\instance-id\\1-manual.zip",
  worldCount: 2,
  sourceBytes: 8 * 1024 * 1024,
  archiveBytes: 2 * 1024 * 1024,
  createdAtUnixSeconds: 1784880000,
  completedAtUnixSeconds: 1784880001,
  errorSummary: null,
  kind: "full",
  baseBackupId: null,
};

const INCREMENTAL_BACKUP = {
  ...FULL_BACKUP,
  id: "backup-inc",
  trigger: "scheduled",
  archivePath: "D:\\MoyuMax\\data\\backups\\instances\\instance-id\\2-scheduled.zip",
  archiveBytes: 128 * 1024,
  createdAtUnixSeconds: 1784883600,
  kind: "incremental",
  baseBackupId: "backup-full",
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
          name: "备份实例",
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

test("M19-INC-001 备份时间线显示定时触发与增量类型徽章", async ({ page }) => {
  await page.evaluate((backups) => {
    window.localStorage.setItem(
      "moyumax.browser.worldBackups",
      JSON.stringify(backups),
    );
  }, [INCREMENTAL_BACKUP, FULL_BACKUP]);
  await page.reload();
  await page.getByRole("button", { name: "数据", exact: true }).click();

  const incremental = page.locator(".backup-row").filter({ hasText: "定时" });
  await expect(incremental).toBeVisible();
  await expect(incremental.getByText("增量", { exact: true })).toBeVisible();
  const full = page.locator(".backup-row").filter({ hasText: "手动" });
  await expect(full.getByText("完整", { exact: true })).toBeVisible();
});

test("M19-INC-002 备份设置持久化并校验边界", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("heading", { name: "世界备份" })).toBeVisible();

  const interval = page.getByRole("spinbutton", { name: "运行期间备份间隔（分钟）" });
  const keep = page.getByRole("spinbutton", { name: "每个实例保留备份数量" });
  await expect(interval).toHaveValue("30");
  await expect(keep).toHaveValue("20");

  await interval.fill("5");
  await interval.blur();
  await expect(page.getByText("运行期间每 5 分钟创建增量备份", { exact: true })).toBeVisible();
  await keep.fill("3");
  await keep.blur();
  await expect(page.getByText("每个实例最多保留 3 个备份", { exact: true })).toBeVisible();

  await page.reload();
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("spinbutton", { name: "运行期间备份间隔（分钟）" })).toHaveValue("5");
  await expect(page.getByRole("spinbutton", { name: "每个实例保留备份数量" })).toHaveValue("3");

  const intervalAgain = page.getByRole("spinbutton", { name: "运行期间备份间隔（分钟）" });
  await intervalAgain.fill("1441");
  await intervalAgain.blur();
  await expect(page.getByRole("alert").getByText("备份间隔必须在 0 到 1440 分钟之间", { exact: true })).toBeVisible();
  const keepAgain = page.getByRole("spinbutton", { name: "每个实例保留备份数量" });
  await keepAgain.fill("0");
  await keepAgain.blur();
  await expect(page.getByRole("alert").getByText("备份保留数量必须在 1 到 100 之间", { exact: true })).toBeVisible();
});

test("M19-INC-003 关闭定时备份的零间隔可保存", async ({ page }) => {
  await page.getByRole("button", { name: "设置" }).click();
  const interval = page.getByRole("spinbutton", { name: "运行期间备份间隔（分钟）" });
  await interval.fill("0");
  await interval.blur();
  await expect(page.getByText("已关闭运行期间定时备份", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.backupSettings") ?? "{}"),
  );
  expect(stored.intervalMinutes).toBe(0);
});

test("UI-BACKUP-002 备份设置与时间线在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.evaluate((backups) => {
    window.localStorage.setItem(
      "moyumax.browser.worldBackups",
      JSON.stringify(backups),
    );
  }, [INCREMENTAL_BACKUP, FULL_BACKUP]);
  await page.reload();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("heading", { name: "世界备份" })).toBeVisible();
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
