import { expect, test, type Page } from "@playwright/test";

const ONBOARDING = {
  language: "zh-CN",
  dataDirectory: "D:\\MoyuMax\\data",
  telemetryEnabled: false,
  updateChecksEnabled: true,
  natDetectionEnabled: false,
  instanceIsolationEnabled: true,
};

const INSTANCE = {
  id: "instance-id",
  name: "任务测试",
  gameVersion: "26.2",
  loaderKind: "fabric",
  loaderVersion: "0.19.3",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
  state: "ready",
};

function installTask(id: string, state: string, overrides: Record<string, unknown> = {}) {
  return {
    id,
    state,
    currentStage: "downloadGameFiles",
    plan: {
      schemaVersion: 1,
      instanceId: INSTANCE.id,
      instanceName: INSTANCE.name,
      targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
      stages: ["prepare", "downloadGameFiles"],
      estimatedDownloadBytes: 1024,
    },
    stagingDirectory: `D:\\MoyuMax\\data\\.staging\\install\\${id}`,
    targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
    createdAtUnixSeconds: 1,
    updatedAtUnixSeconds: 1,
    priority: 0,
    pausedBy: null,
    progress: {
      completedBytes: 512,
      totalBytes: 1024,
      currentItem: "正在下载游戏文件",
      errorSummary: null,
    },
    ...overrides,
  };
}

async function seed(page: Page, tasks: unknown[]): Promise<void> {
  await page.goto("/");
  await page.evaluate(
    ({ onboarding, instance, seededTasks }) => {
      window.localStorage.clear();
      window.localStorage.setItem(
        "moyumax.browser.onboarding",
        JSON.stringify(onboarding),
      );
      window.localStorage.setItem(
        "moyumax.browser.instances",
        JSON.stringify([instance]),
      );
      window.localStorage.setItem(
        "moyumax.browser.installTasks",
        JSON.stringify(seededTasks),
      );
    },
    { onboarding: ONBOARDING, instance: INSTANCE, seededTasks: tasks },
  );
  await page.reload();
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();
}

test("M14-TASK-001 单任务暂停与恢复", async ({ page }) => {
  await seed(page, [installTask("task-1", "running")]);

  await page.getByRole("button", { name: "暂停", exact: true }).click();
  await expect(page.getByText("已暂停", { exact: true }).first()).toBeVisible();

  await page.getByRole("button", { name: "恢复", exact: true }).click();
  await expect(page.getByText("已排队", { exact: true }).first()).toBeVisible();
});

test("M14-TASK-001 单独暂停的任务在恢复全部后保持暂停", async ({ page }) => {
  await seed(page, [
    installTask("task-user", "paused", { pausedBy: "user" }),
    installTask("task-global", "paused", { pausedBy: "global" }),
  ]);
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.tasksPaused", "true");
  });
  await page.reload();
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();

  await page.getByRole("button", { name: "恢复全部任务" }).click();
  const cards = page.locator(".task-card");
  await expect(cards.first()).toContainText("已暂停");
  await expect(cards.last()).toContainText("已排队");
});

test("M14-TASK-002 排队任务可调整优先级", async ({ page }) => {
  await seed(page, [installTask("task-priority", "queued")]);

  await expect(page.getByText("优先级 0")).toBeVisible();
  await page.getByRole("button", { name: "提高优先级" }).click();
  await expect(page.getByText("优先级 1")).toBeVisible();
  await page.getByRole("button", { name: "降低优先级" }).click();
  await expect(page.getByText("优先级 0")).toBeVisible();
});

test("M14-TASK-003 设置全局限速并显示状态", async ({ page }) => {
  await seed(page, []);

  await expect(page.getByText("不限速", { exact: true })).toBeVisible();
  await page.getByRole("textbox", { name: /限速/ }).fill("8");
  await page.getByRole("button", { name: "应用" }).click();
  await expect(page.getByText("当前限速：8 MiB/s", { exact: true })).toBeVisible();

  await page.reload();
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByText("当前限速：8 MiB/s", { exact: true })).toBeVisible();
});

test("UI-A11Y-001 任务控制区在 960x600 与 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page, [installTask("task-1", "running")]);

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  const overflow = await page.evaluate(() => ({
    horizontal: document.documentElement.scrollWidth > window.innerWidth + 1,
    bars: [...document.querySelectorAll<HTMLElement>(".task-limit-bar, .task-global-bar")].some(
      (element) => element.scrollWidth > element.clientWidth + 1,
    ),
  }));
  expect(overflow.horizontal).toBe(false);
  expect(overflow.bars).toBe(false);
});

test("M33-TASK-001 取消排队任务", async ({ page }) => {
  await seed(page, [installTask("task-cancel-queued", "queued")]);

  await page.getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.getByText("已取消", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "删除", exact: true })).toBeVisible();
});

test("M33-TASK-001 取消运行中任务", async ({ page }) => {
  await seed(page, [installTask("task-cancel-running", "running")]);

  await page.getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.getByText("已取消", { exact: true }).first()).toBeVisible();
});

test("M33-TASK-002 删除失败任务需二次确认", async ({ page }) => {
  await seed(page, [installTask("task-delete", "failed")]);

  await page.getByRole("button", { name: "删除", exact: true }).click();
  await page.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(page.getByText("没有任务")).toBeVisible();
});

test("M33-TASK-002 删除在确认前可撤回", async ({ page }) => {
  await seed(page, [installTask("task-keep", "failed")]);

  await page.getByRole("button", { name: "删除", exact: true }).click();
  await page.getByRole("button", { name: "取消", exact: true }).click();
  await expect(page.getByRole("button", { name: "删除", exact: true })).toBeVisible();
  await expect(page.getByText("失败", { exact: true }).first()).toBeVisible();
});

test("M33-TASK-002 非终态任务没有删除入口", async ({ page }) => {
  await seed(page, [installTask("task-active", "queued")]);

  await expect(page.getByRole("button", { name: "删除", exact: true })).toHaveCount(0);
});

test("M33-TASK-003 下载线程数保存并回读", async ({ page }) => {
  await seed(page, []);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "下载" }).click();
  await expect(page.getByRole("heading", { name: "下载" })).toBeVisible();

  const input = page.getByRole("spinbutton", { name: "下载线程数" });
  await expect(input).toHaveValue("24");
  await input.fill("16");
  await input.blur();
  await expect(page.getByText("下载线程数已保存，重启应用后生效。")).toBeVisible();

  await page.reload();
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "下载" }).click();
  await expect(page.getByRole("spinbutton", { name: "下载线程数" })).toHaveValue("16");
});

test("M33-TASK-003 下载线程数越界被拒绝", async ({ page }) => {
  await seed(page, []);
  await page.getByRole("button", { name: "设置", exact: true }).click();
  await page.locator(".sn-item", { hasText: "下载" }).click();

  const input = page.getByRole("spinbutton", { name: "下载线程数" });
  await input.fill("64");
  await input.blur();
  await expect(page.getByText("下载线程数必须是 1 到 32 的整数")).toBeVisible();
  const stored = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.downloadConcurrency"),
  );
  expect(stored).toBeNull();
});
