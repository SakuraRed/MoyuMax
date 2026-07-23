// 一次性视觉证据采集:关闭对话框两种形态(首次选择 / 退出影响)。
import { chromium } from "@playwright/test";
import { mkdirSync } from "node:fs";

mkdirSync("../../output/playwright/m9", { recursive: true });
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });

await page.goto("http://127.0.0.1:1420");
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
await page.getByRole("button", { name: "关闭" }).click();
await page.getByRole("dialog", { name: "关闭 MoyuMax" }).waitFor();
await page.screenshot({ path: "../../output/playwright/m9/close-dialog-choice.png" });

// 退出影响形态:种入运行中实例与任务。
await page.evaluate(() => {
  window.localStorage.setItem(
    "moyumax.browser.instances",
    JSON.stringify([
      {
        id: "instance-id",
        name: "启动测试",
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
        startedAtUnixSeconds: 1,
        endedAtUnixSeconds: null,
        exitCode: null,
        stdoutPath: "stdout.log",
        stderrPath: "stderr.log",
        errorSummary: null,
      },
    ]),
  );
  window.localStorage.setItem(
    "moyumax.browser.installTasks",
    JSON.stringify([
      {
        id: "task-running",
        state: "running",
        currentStage: "downloadGameFiles",
        plan: {
          schemaVersion: 1,
          instanceId: "instance-id",
          instanceName: "启动测试",
          targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
          stages: ["prepare", "downloadGameFiles"],
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
      },
    ]),
  );
});
await page.keyboard.press("Escape");
await page.getByRole("button", { name: "关闭" }).click();
const dialog = page.getByRole("dialog", { name: "关闭 MoyuMax" });
await dialog.waitFor();
await dialog.getByRole("radio", { name: /退出 MoyuMax/ }).check();
await dialog.getByRole("button", { name: "确定" }).click();
await page.getByRole("dialog", { name: "退出 MoyuMax" }).waitFor();
await page.screenshot({ path: "../../output/playwright/m9/close-dialog-impact.png" });

await browser.close();
console.log("saved ../../output/playwright/m9/close-dialog-*.png");
