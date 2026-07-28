import { expect, test } from "@playwright/test";

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
          id: "crash-instance",
          name: "崩溃诊断测试",
          gameVersion: "1.21.8",
          loaderKind: "fabric",
          loaderVersion: "0.16.14",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\crash-instance",
          state: "ready",
        },
      ]),
    );
    window.localStorage.setItem(
      "moyumax.browser.launchSessions",
      JSON.stringify([
        {
          id: "failed-session",
          instanceId: "crash-instance",
          playerName: "MoyuMaxPlayer",
          state: "failed",
          startedAtUnixSeconds: 1,
          endedAtUnixSeconds: 2,
          exitCode: 1,
          stdoutPath: "D:\\Users\\Private\\latest.log",
          stderrPath: "D:\\Users\\Private\\stderr.log",
          errorSummary: "游戏进程退出码：1",
        },
      ]),
    );
    window.localStorage.setItem(
      "moyumax.browser.crashReports",
      JSON.stringify([
        {
          schemaVersion: 1,
          id: "crash-failed-session",
          launchSessionId: "failed-session",
          instanceId: "crash-instance",
          createdAtUnixSeconds: 2,
          cause: "outOfMemory",
          title: "游戏可用内存不足",
          summary: "游戏进程返回退出码 1。最后输出包含 Java 内存不足证据；这不等于存档已经损坏。",
          recommendations: [
            "关闭占用大量内存的程序后再试；不要直接把全部物理内存分配给游戏。",
            "检查最近加入的高分辨率资源包、光影或大型模组。",
          ],
          evidence: [
            {
              kind: "gameOutput",
              bundleName: "game/last-output.log",
              originalBytes: 8192,
              includedBytes: 4096,
              truncated: false,
            },
            {
              kind: "gameLog",
              bundleName: "game/latest.log",
              originalBytes: 4096,
              includedBytes: 3072,
              truncated: false,
            },
            {
              kind: "launcherLog",
              bundleName: "moyumax/launcher.log",
              originalBytes: 512,
              includedBytes: 512,
              truncated: false,
            },
            {
              kind: "launchScript",
              bundleName: "moyumax/launch-redacted.cmd.txt",
              originalBytes: 2048,
              includedBytes: 1536,
              truncated: false,
            },
          ],
          redactionSummary: [
            "玩家名称与账户标识替换为占位符。",
            "用户目录和实例绝对路径替换为占位符。",
            "IP、域名端口和显式服务器地址替换为占位符。",
            "令牌、密码、Authorization 和类似凭据字段替换为占位符。",
            "每个文本证据最多保留最后 512 KiB。",
          ],
        },
      ]),
    );
  });
  await page.reload();
});

test("M6-CRASH-001 用户从异常会话进入崩溃页并在预览后导出", async ({ page }) => {
  await expect(page.getByText("上次异常退出", { exact: false })).toBeVisible();
  await page.getByRole("button", { name: "查看诊断" }).click();

  await expect(page.getByRole("heading", { name: "崩溃诊断" })).toBeVisible();
  await expect(page.getByText("游戏可用内存不足", { exact: true })).toBeVisible();
  await expect(page.getByText("这不等于存档已经损坏", { exact: false })).toBeVisible();
  await expect(page.getByText("game/last-output.log", { exact: true })).toBeVisible();
  await expect(page.getByText("预览文件清单后导出到本地 ZIP", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "确认并导出到本地" })).toHaveCount(0);

  await page.getByRole("button", { name: "预览诊断包" }).click();
  await expect(page.getByRole("heading", { name: "导出前隐私检查" })).toBeVisible();
  await expect(page.getByText("manifest.json", { exact: true })).toBeVisible();
  await expect(page.getByText("用户目录和实例绝对路径替换为占位符。", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "确认并导出到本地" }).click();
  await expect(page.getByText("诊断包已保存在本地", { exact: true }).first()).toBeVisible();
  await expect(page.getByText(/MoyuMax-diagnostics-crash-failed-session\.zip/)).toBeVisible();
});

test("UI-CRASH-001 崩溃页在 960x600 和 200% 放大下无横向溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "查看诊断" }).click();
  await page.getByRole("button", { name: "预览诊断包" }).click();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByRole("button", { name: "确认并导出到本地" })).toBeVisible();
  const geometry = await page.evaluate(() => ({
    documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>("main.content *")]
      .filter(
        (element) =>
          element.scrollWidth > element.clientWidth + 1,
      )
      .map((element) => ({
        tag: element.tagName,
        className: element.className,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
      })),
  }));
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.overflowingElements).toEqual([]);
});

test("UI-CRASH-002 崩溃诊断面板保留安全内边距", async ({ page }) => {
  await page.getByRole("button", { name: "查看诊断" }).click();

  const padding = await page.locator(".panel.pad").first().evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      top: Number.parseFloat(style.paddingTop),
      inline: Number.parseFloat(style.paddingLeft),
    };
  });
  expect(padding.top).toBeGreaterThanOrEqual(18);
  expect(padding.inline).toBeGreaterThanOrEqual(20);
});
