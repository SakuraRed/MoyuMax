import { expect, test } from "@playwright/test";

function installedEntry(projectId: string, title: string, fileName: string) {
  return {
    id: `installed-${projectId}`,
    instanceId: "instance-id",
    provider: "modrinth",
    projectId,
    versionId: `${projectId}-V1`,
    projectTitle: title,
    versionNumber: "1.0.0+26.2",
    fileName,
    relativePath: `.minecraft/mods/${fileName}`,
    size: 1040013,
    sha1: "1".repeat(40),
    sha512: "2".repeat(128),
    enabled: true,
    autoUpdateEnabled: false,
    installedAtUnixSeconds: 1,
  };
}

function updateEntry(projectId: string, title: string, fileName: string) {
  return {
    instanceId: "instance-id",
    projectId,
    projectTitle: title,
    currentVersionId: `${projectId}-V1`,
    currentVersionNumber: "1.0.0+26.2",
    latestVersionId: `${projectId}-V2`,
    latestVersionNumber: "2.0.0+26.2",
    file: {
      url: `https://cdn.modrinth.com/data/${projectId}/${fileName}`,
      filename: fileName,
      size: 1040013,
      sha1: "1".repeat(40),
      sha512: "2".repeat(128),
    },
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
          name: "更新测试",
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

test("M15-UPDATE-001 检查更新后逐项触发更新并进入统一任务队列", async ({ page }) => {
  await seedContent(page, [
    installedEntry("ROOT0001", "Continuity", "continuity.jar"),
    installedEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ], [
    updateEntry("ROOT0001", "Continuity", "continuity.jar"),
    updateEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ]);
  await page.getByRole("button", { name: "资源" }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByText("2 项可用更新", { exact: true })).toBeVisible();
  const continuityRow = page.locator(".content-update-panel .installed-content-row").filter({ hasText: "Continuity" });
  await expect(continuityRow.getByText("1.0.0+26.2 → 2.0.0+26.2", { exact: true })).toBeVisible();
  await expectElementPadding(page, ".auto-update-toggle", { block: 16, inline: 20 });
  await expectElementPadding(page, ".content-update-panel .installed-content-row", { block: 16, inline: 20 });
  await expect(page.getByRole("button", { name: "全部更新" })).toHaveCount(0);

  await continuityRow.getByRole("button", { name: "更新" }).click();
  await expect(page.getByText("更新任务已加入队列", { exact: true })).toBeVisible();
  await expect(page.locator(".content-update-panel .installed-content-row")).toHaveCount(1);
  const tasks = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.contentTasks") ?? "[]"),
  );
  expect(tasks).toHaveLength(1);
  expect(tasks[0].plan.isUpdate).toBe(true);
  expect(tasks[0].plan.entries).toHaveLength(1);
  expect(tasks[0].plan.entries[0].projectId).toBe("ROOT0001");
});

test("M15-UPDATE-002 开启按实例自动更新后提供全部更新入口", async ({ page }) => {
  await seedContent(page, [
    installedEntry("ROOT0001", "Continuity", "continuity.jar"),
    installedEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ], [
    updateEntry("ROOT0001", "Continuity", "continuity.jar"),
    updateEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ]);
  await page.getByRole("button", { name: "资源" }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  const toggle = page.getByRole("checkbox", { name: "按实例自动更新策略" });
  await expect(toggle).not.toBeChecked();
  await expect(page.getByText("开启后可一键安装全部可用更新。")).toBeVisible();
  await toggle.check();
  await expect(toggle).toBeChecked();

  await page.getByRole("button", { name: "检查更新" }).click();
  await page.getByRole("button", { name: "全部更新" }).click();
  await expect(page.getByText("更新任务已加入队列", { exact: true })).toBeVisible();
  const tasks = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.contentTasks") ?? "[]"),
  );
  expect(tasks).toHaveLength(1);
  expect(tasks[0].plan.isUpdate).toBe(true);
  expect(tasks[0].plan.entries).toHaveLength(2);
  const flags = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.contentAutoUpdate") ?? "{}"),
  );
  expect(flags["instance-id"]).toBe(true);
});

test("M15-UPDATE-003 没有可用更新时明确提示且默认只提示不下载", async ({ page }) => {
  await seedContent(page, [installedEntry("ROOT0001", "Continuity", "continuity.jar")], []);
  await page.getByRole("button", { name: "资源" }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByText("已安装内容均为最新兼容版本。", { exact: true })).toBeVisible();
  const tasks = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.contentTasks") ?? "[]"),
  );
  expect(tasks).toHaveLength(0);
});

test("M15-LOADER-001 Quilt 与 Forge 实例同样出现在内容管理中", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instances",
      JSON.stringify([
        {
          id: "quilt-instance",
          name: "Quilt 实例",
          gameVersion: "26.2",
          loaderKind: "quilt",
          loaderVersion: "0.29.0",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\quilt-instance",
          state: "ready",
        },
        {
          id: "forge-instance",
          name: "Forge 实例",
          gameVersion: "26.2",
          loaderKind: "forge",
          loaderVersion: "60.0.0",
          rootDirectory: "D:\\MoyuMax\\data\\instances\\forge-instance",
          state: "ready",
        },
      ]),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源" }).click();
  await page.getByRole("button", { name: "实例内容" }).click();

  const options = page.locator(".resource-instance-field select option");
  await expect(options).toHaveCount(2);
  await expect(options.nth(0)).toContainText("Quilt");
  await expect(options.nth(1)).toContainText("Forge");
  await expect(page.getByText("先安装一个 Fabric、Quilt、Forge 或 NeoForge 游戏实例", { exact: false })).toHaveCount(0);
});

test("UI-UPDATE-001 更新面板在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await seedContent(page, [
    installedEntry("ROOT0001", "Continuity", "continuity.jar"),
    installedEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ], [
    updateEntry("ROOT0001", "Continuity", "continuity.jar"),
    updateEntry("DEP00001", "Fabric API", "fabric-api.jar"),
  ]);
  // 旧全局样式在 ≤1050px 会收起导航标签,先在默认窗口导航并触发更新检查,再缩放窗口。
  await page.getByRole("button", { name: "资源" }).click();
  await page.getByRole("button", { name: "实例内容" }).click();
  await page.getByRole("checkbox", { name: "按实例自动更新策略" }).check();
  await page.getByRole("button", { name: "检查更新" }).click();
  await expect(page.getByRole("button", { name: "全部更新" })).toBeVisible();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  // 资源页仍为旧样式(ResourceCenter 待重写),允许内部裁剪;
  // 这里断言页面级与容器都不产生横向滚动。
  const geometry = await page.evaluate(() => {
    const content = document.querySelector<HTMLElement>(".resource-content");
    return {
      documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
      containerOverflow: content ? content.scrollWidth > content.clientWidth + 1 : false,
    };
  });
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.containerOverflow).toBe(false);
});

async function seedContent(
  page: import("@playwright/test").Page,
  installed: ReturnType<typeof installedEntry>[],
  updates: ReturnType<typeof updateEntry>[],
): Promise<void> {
  await page.evaluate(
    ({ installed, updates }) => {
      window.localStorage.setItem("moyumax.browser.installedContent", JSON.stringify(installed));
      window.localStorage.setItem("moyumax.browser.contentUpdates", JSON.stringify(updates));
    },
    { installed, updates },
  );
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
