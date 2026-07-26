import { expect, test } from "@playwright/test";

function seedLogs() {
  const NOW = Math.floor(Date.now() / 1000);
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
        name: "日志测试",
        gameVersion: "26.2",
        loaderKind: "fabric",
        loaderVersion: "0.19.3",
        rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
        state: "ready",
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.launchSessions",
    JSON.stringify([
      {
        id: "session-running",
        instanceId: "instance-id",
        playerName: "MoyuMaxPlayer",
        state: "running",
        startedAtUnixSeconds: NOW,
        endedAtUnixSeconds: null,
        exitCode: null,
        stdoutPath: "D:\\MoyuMax\\data\\instances\\instance-id\\.minecraft\\logs\\moyumax\\session-running.stdout.log",
        stderrPath: "D:\\MoyuMax\\data\\instances\\instance-id\\.minecraft\\logs\\moyumax\\session-running.stderr.log",
        errorSummary: null,
        preLaunchBackup: null,
        postExitBackup: null,
      },
      {
        id: "session-old",
        instanceId: "instance-id",
        playerName: "MoyuMaxPlayer",
        state: "completed",
        startedAtUnixSeconds: NOW - 3600,
        endedAtUnixSeconds: NOW - 1800,
        exitCode: 0,
        stdoutPath: "D:\\MoyuMax\\data\\instances\\instance-id\\.minecraft\\logs\\moyumax\\session-old.stdout.log",
        stderrPath: "D:\\MoyuMax\\data\\instances\\instance-id\\.minecraft\\logs\\moyumax\\session-old.stderr.log",
        errorSummary: null,
        preLaunchBackup: null,
        postExitBackup: null,
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.launchLogs",
    JSON.stringify({
      "session-running": {
        stdout: "[Init] 运行中会话第一行\n[Init] 运行中会话第二行\n",
        stderr: "[Warn] 运行中会话警告\n",
      },
      "session-old": {
        stdout: "[Init] 历史会话第一行\n[Game] 历史会话结束\n",
        stderr: "",
      },
    }),
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(seedLogs);
  await page.reload();
});

test("UI-LOG-001 首页运行卡片直达日志副页并跟随新增输出", async ({ page }) => {
  await expect(page.getByText("正在运行", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "查看“日志测试”的游戏日志" }).click();

  // 默认选中最新会话并展示两个通道的已有内容。
  await expect(
    page.getByRole("heading", { name: "游戏日志" }),
  ).toBeVisible();
  const viewport = page.locator(".log-viewport");
  await expect(viewport).toContainText("运行中会话第一行");
  await expect(viewport).toContainText("运行中会话警告");

  // 自动滚动开关默认开启,可切换。
  const autoScroll = page.getByRole("checkbox", { name: "自动滚动" });
  await expect(autoScroll).toBeChecked();
  await autoScroll.uncheck();
  await expect(autoScroll).not.toBeChecked();
  await autoScroll.check();

  // 运行中会话每 2 秒尾部跟随:模拟游戏追加输出后新行自动出现。
  await page.evaluate(() => {
    const logs = JSON.parse(
      window.localStorage.getItem("moyumax.browser.launchLogs") ?? "{}",
    ) as Record<string, { stdout: string; stderr: string }>;
    const entry = logs["session-running"];
    if (entry) entry.stdout += "[Game] 世界已加载完成\n";
    window.localStorage.setItem(
      "moyumax.browser.launchLogs",
      JSON.stringify(logs),
    );
  });
  await expect(viewport).toContainText("世界已加载完成", { timeout: 6_000 });
});

test("UI-LOG-001 会话切换展示各自日志,结束进程按钮仅运行中可见", async ({
  page,
}) => {
  await page.getByRole("button", { name: "查看“日志测试”的游戏日志" }).click();
  const viewport = page.locator(".log-viewport");
  await expect(viewport).toContainText("运行中会话第一行");
  await expect(
    page.getByRole("button", { name: "结束进程" }),
  ).toBeVisible();

  // 切到已结束会话:一次性读完,无结束进程按钮。
  await page
    .getByRole("combobox", { name: "选择启动会话" })
    .selectOption("session-old");
  await expect(viewport).toContainText("历史会话第一行");
  await expect(viewport).toContainText("历史会话结束");
  await expect(viewport).not.toContainText("运行中会话");
  await expect(page.getByRole("button", { name: "结束进程" })).toHaveCount(0);

  // 切回运行中会话:按钮恢复,且不再重复历史会话内容。
  await page
    .getByRole("combobox", { name: "选择启动会话" })
    .selectOption("session-running");
  await expect(viewport).toContainText("运行中会话第一行");
  await expect(
    page.getByRole("button", { name: "结束进程" }),
  ).toBeVisible();
});

test("UI-LOG-001 结束进程按钮停止会话后消失", async ({ page }) => {
  await page.getByRole("button", { name: "查看“日志测试”的游戏日志" }).click();
  await expect(page.locator(".log-viewport")).toContainText("运行中会话第一行");

  await page.getByRole("button", { name: "结束进程" }).click();
  // 停止后下一次轮询读到 stopped 状态,按钮消失且保留已有日志。
  await expect(page.getByRole("button", { name: "结束进程" })).toHaveCount(0, {
    timeout: 6_000,
  });
  await expect(page.locator(".log-viewport")).toContainText("运行中会话第一行");
});

test("UI-LOG-001 复制全部与清空显示", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.getByRole("button", { name: "查看“日志测试”的游戏日志" }).click();
  const viewport = page.locator(".log-viewport");
  await expect(viewport).toContainText("运行中会话第一行");

  const copyButton = page.getByRole("button", { name: "复制全部" });
  await copyButton.click();
  await expect(page.getByRole("button", { name: "已复制" })).toBeVisible();
  const clipboard = await page.evaluate(() =>
    navigator.clipboard.readText(),
  );
  expect(clipboard).toContain("运行中会话第一行");
  expect(clipboard).toContain("运行中会话警告");

  // 清空只清显示不清文件:日志区回到空态提示。
  await page.getByRole("button", { name: "清空显示" }).click();
  await expect(viewport).toContainText("该会话还没有日志输出");
});

test("UI-LOG-001 实例详情导航进入日志副页,无会话实例显示空态", async ({
  page,
}) => {
  // 运行中卡片没有「管理」入口;移除运行中会话后从实例详情导航进入。
  await page.evaluate(() => {
    const sessions = JSON.parse(
      window.localStorage.getItem("moyumax.browser.launchSessions") ?? "[]",
    ) as { id: string }[];
    window.localStorage.setItem(
      "moyumax.browser.launchSessions",
      JSON.stringify(sessions.filter((session) => session.id !== "session-running")),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "管理“日志测试”" }).click();
  await page.getByRole("button", { name: "日志", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "游戏日志" }),
  ).toBeVisible();
  await expect(page.locator(".log-viewport")).toContainText("历史会话第一行");

  // 移除全部会话后显示空态。
  await page.evaluate(() => {
    window.localStorage.setItem("moyumax.browser.launchSessions", "[]");
    window.localStorage.setItem("moyumax.browser.launchLogs", "{}");
  });
  await page.reload();
  await page.getByRole("button", { name: "管理“日志测试”" }).click();
  await page.getByRole("button", { name: "日志", exact: true }).click();
  await expect(
    page.getByText("该实例还没有启动会话，启动一次游戏后即可查看日志。"),
  ).toBeVisible();
});
