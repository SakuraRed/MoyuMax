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
  name: "启动测试",
  gameVersion: "26.2",
  loaderKind: "fabric",
  loaderVersion: "0.19.3",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
  state: "ready",
};

function runningSession() {
  return {
    id: "session-running",
    instanceId: INSTANCE.id,
    playerName: "MoyuMaxPlayer",
    state: "running",
    startedAtUnixSeconds: 1,
    endedAtUnixSeconds: null,
    exitCode: null,
    stdoutPath: "stdout.log",
    stderrPath: "stderr.log",
    errorSummary: null,
  };
}

function runningInstallTask() {
  return {
    id: "task-running",
    state: "running",
    currentStage: "downloadGameFiles",
    plan: {
      schemaVersion: 1,
      instanceId: INSTANCE.id,
      instanceName: INSTANCE.name,
      targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
      stages: [
        "prepare",
        "downloadGameFiles",
        "verifyFiles",
        "installGameEnvironment",
        "applyLoader",
        "commitChanges",
        "createRollbackPoint",
      ],
      estimatedDownloadBytes: 1024,
    },
    stagingDirectory: "D:\\MoyuMax\\data\\.staging\\install\\task-running",
    targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
    createdAtUnixSeconds: 1,
    updatedAtUnixSeconds: 1,
    progress: {
      completedBytes: 512,
      totalBytes: 1024,
      currentItem: "正在下载游戏文件",
      errorSummary: null,
    },
  };
}

async function seed(page: Page, entries: Record<string, unknown>): Promise<void> {
  await page.goto("/");
  await page.evaluate((seedEntries) => {
    window.localStorage.clear();
    window.localStorage.setItem(
      "moyumax.browser.onboarding",
      JSON.stringify(seedEntries.onboarding),
    );
    for (const [key, value] of Object.entries(seedEntries)) {
      if (key === "onboarding") continue;
      window.localStorage.setItem(
        key,
        typeof value === "string" ? value : JSON.stringify(value),
      );
    }
  }, { onboarding: ONBOARDING, ...entries });
  await page.reload();
}

async function readStorage(page: Page, key: string): Promise<string | null> {
  return page.evaluate(
    (storageKey) => window.localStorage.getItem(storageKey),
    key,
  );
}

test("UI-TRAY-001 首次关闭窗口询问最小化或退出并可记住选择", async ({ page }) => {
  await seed(page, {});

  await page.getByRole("button", { name: "关闭" }).click();
  const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("这是你第一次关闭主窗口，选择默认行为：")).toBeVisible();

  const minimize = dialog.getByRole("radio", { name: /最小化到系统托盘/ });
  const remember = dialog.getByRole("checkbox", { name: /记住本次选择/ });
  await expect(minimize).toBeChecked();
  await expect(remember).not.toBeChecked();

  await remember.check();
  await dialog.getByRole("button", { name: "确定" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await readStorage(page, "moyumax.browser.windowCloseBehavior")).toBe(
    "minimizeToTray",
  );
  expect(await readStorage(page, "moyumax.browser.windowState")).toBe("hidden");

  // 记住选择后再次关闭不再询问。
  await page.reload();
  await expect(page.getByRole("heading", { name: "从安装第一个游戏开始" })).toBeVisible();
  await page.getByRole("button", { name: "关闭" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await readStorage(page, "moyumax.browser.windowState")).toBe("hidden");
});

test("UI-TRAY-001 首次关闭选择退出且没有影响时直接退出", async ({ page }) => {
  await seed(page, {});

  await page.getByRole("button", { name: "关闭" }).click();
  const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
  await expect(dialog).toBeVisible();

  await dialog.getByRole("radio", { name: /退出 MoyuMax/ }).check();
  await dialog.getByRole("button", { name: "确定" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await readStorage(page, "moyumax.browser.windowState")).toBe("exited");
});

test("UI-TRAY-001 Esc 取消关闭询问且不改变任何状态", async ({ page }) => {
  await seed(page, {});

  await page.getByRole("button", { name: "关闭" }).click();
  const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
  await expect(dialog).toBeVisible();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await readStorage(page, "moyumax.browser.windowState")).toBeNull();
  expect(await readStorage(page, "moyumax.browser.windowCloseBehavior")).toBeNull();
});

test("UI-TRAY-004 游戏运行中退出前说明影响并在确认后安全退出", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.instances": [INSTANCE],
    "moyumax.browser.launchSessions": [runningSession()],
    "moyumax.browser.installTasks": [runningInstallTask()],
  });

  await page.getByRole("button", { name: "关闭" }).click();
  const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/「启动测试」正在运行/)).toBeVisible();

  await dialog.getByRole("radio", { name: /退出 MoyuMax/ }).check();
  await dialog.getByRole("button", { name: "确定" }).click();

  const impactDialog = page.getByRole("dialog", { name: "退出 MoyuMax" });
  await expect(impactDialog).toBeVisible();
  await expect(impactDialog.getByText(/退出将安全终止游戏/)).toBeVisible();
  await expect(impactDialog.getByText(/1 个任务正在进行或排队/)).toBeVisible();

  await impactDialog.getByRole("button", { name: "确认退出" }).click();
  await expect(impactDialog).toHaveCount(0);
  expect(await readStorage(page, "moyumax.browser.windowState")).toBe("exited");

  const sessions = JSON.parse(
    (await readStorage(page, "moyumax.browser.launchSessions")) ?? "[]",
  ) as { state: string; postExitBackup: unknown }[];
  expect(sessions[0]?.state).toBe("stopped");
  expect(sessions[0]?.postExitBackup).not.toBeNull();

  const tasks = JSON.parse(
    (await readStorage(page, "moyumax.browser.installTasks")) ?? "[]",
  ) as { state: string }[];
  expect(tasks[0]?.state).toBe("paused");
});

test("UI-TRAY-002 托盘唤醒恢复上次非敏感页面", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.startupKind": "wake",
    "moyumax.browser.shellState": { page: "tasks", scrollTop: 0 },
    "moyumax.browser.installTasks": [runningInstallTask()],
  });
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();
});

test("UI-TRAY-002 未知或敏感页面在唤醒时回退首页", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.startupKind": "wake",
    "moyumax.browser.shellState": { page: "settings", scrollTop: 0 },
  });
  await expect(page.getByRole("heading", { name: "从安装第一个游戏开始" })).toBeVisible();
});

test("UI-TRAY-002 冷启动不恢复历史页面", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.shellState": { page: "tasks", scrollTop: 0 },
  });
  await expect(page.getByRole("heading", { name: "从安装第一个游戏开始" })).toBeVisible();
});

test("UI-TASK-002 任务中心暂停全部并恢复全部任务", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.instances": [INSTANCE],
    "moyumax.browser.installTasks": [runningInstallTask()],
  });

  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();
  await expect(page.getByText("正在运行", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "暂停全部任务" }).click();
  await expect(page.getByText("已暂停", { exact: true }).first()).toBeVisible();
  await expect(page.getByText(/全部任务已暂停/)).toBeVisible();
  expect(await readStorage(page, "moyumax.browser.tasksPaused")).toBe("true");

  await page.getByRole("button", { name: "恢复全部任务" }).click();
  await expect(page.getByText("已排队", { exact: true }).first()).toBeVisible();
  expect(await readStorage(page, "moyumax.browser.tasksPaused")).toBe("false");
});

test("UI-TRAY-007 冷启动后首个可交互窗口立即可用", async ({ page }) => {
  await seed(page, {
    "moyumax.browser.instances": [INSTANCE],
  });
  await expect(page.getByRole("button", { name: "启动游戏" })).toBeVisible();
  await page.getByRole("button", { name: "启动游戏" }).click();
  await expect(page.getByText("正在运行", { exact: true })).toBeVisible();
});

test("UI-A11Y-001 关闭对话框在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page, {});

  await page.getByRole("button", { name: "关闭" }).click();
  const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
  await expect(dialog).toBeVisible();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  await expect(dialog.getByRole("radio", { name: /最小化到系统托盘/ })).toBeVisible();

  const geometry = await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>(".close-dialog");
    if (!root) throw new Error("close dialog is unavailable");
    return {
      horizontalOverflow: root.scrollWidth > root.clientWidth + 1,
      beyondViewport:
        root.getBoundingClientRect().right > window.innerWidth + 1 ||
        root.getBoundingClientRect().bottom > window.innerHeight + 1,
    };
  });
  expect(geometry.horizontalOverflow).toBe(false);
  expect(geometry.beyondViewport).toBe(false);
});
