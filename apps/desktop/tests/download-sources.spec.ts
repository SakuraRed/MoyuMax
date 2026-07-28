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
  name: "来源测试",
  gameVersion: "26.2",
  loaderKind: "fabric",
  loaderVersion: "0.19.3",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
  state: "ready",
};

function taskWithSourceDetail() {
  return {
    id: "task-source",
    state: "completed",
    currentStage: "createRollbackPoint",
    plan: {
      schemaVersion: 1,
      instanceId: INSTANCE.id,
      instanceName: INSTANCE.name,
      targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
      stages: ["prepare", "downloadGameFiles"],
      estimatedDownloadBytes: 1024,
    },
    stagingDirectory: "D:\\MoyuMax\\data\\.staging\\install\\task-source",
    targetDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
    createdAtUnixSeconds: 1,
    updatedAtUnixSeconds: 2,
    progress: {
      completedBytes: 1024,
      totalBytes: 1024,
      currentItem: "已完成",
      errorSummary: null,
      sourceDetail: {
        finalLabel: "Modrinth 官方",
        channel: "official",
        attempts: [
          {
            url: "https://mod.mcimirror.top/data/ABC/mod.jar",
            label: "MCI Mirror",
            channel: "mirror",
            outcome: { failed: { error: "连接超时" } },
          },
          {
            url: "https://cdn.modrinth.com/data/ABC/mod.jar",
            label: "Modrinth 官方",
            channel: "official",
            outcome: "success",
          },
        ],
        segmented: true,
        segmentCount: 4,
        degradedReason: null,
      },
    },
  };
}

function degradedTask() {
  const task = taskWithSourceDetail();
  return {
    ...task,
    id: "task-degraded",
    createdAtUnixSeconds: 3,
    updatedAtUnixSeconds: 4,
    progress: {
      ...task.progress,
      sourceDetail: {
        finalLabel: "BMCLAPI 镜像",
        channel: "mirror",
        attempts: [
          {
            url: "https://bmclapi2.bangbang93.com/v1/objects/abc/client.jar",
            label: "BMCLAPI 镜像",
            channel: "mirror",
            outcome: "success",
          },
        ],
        segmented: false,
        segmentCount: 0,
        degradedReason: "来源忽略 Range 分段请求,已降级为单连接续传",
      },
    },
  };
}

async function seed(page: Page): Promise<void> {
  await page.goto("/");
  await page.evaluate(
    ({ onboarding, instance, tasks }) => {
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
        JSON.stringify(tasks),
      );
    },
    {
      onboarding: ONBOARDING,
      instance: INSTANCE,
      tasks: [taskWithSourceDetail(), degradedTask()],
    },
  );
  await page.reload();
}

test("UI-DOWNLOAD-001 任务详情展示真实来源、回退记录与分段状态", async ({ page }) => {
  await seed(page);
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();

  await expect(
    page.getByText("来源:Modrinth 官方 · 已从 MCI Mirror 回退 · 4 个分段并行"),
  ).toBeVisible();
  await expect(
    page.getByText(/已降级单连接:来源忽略 Range 分段请求/),
  ).toBeVisible();
});

test("UI-A11Y-001 来源详情在 960x600 和 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page);
  await page.getByRole("button", { name: "任务", exact: true }).click();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  await expect(page.locator(".task-source").first()).toContainText("来源:Modrinth 官方");
  const overflow = await page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>(".task-source")].some(
      (element) => element.scrollWidth > element.clientWidth + 1,
    ),
  );
  expect(overflow).toBe(false);
});
