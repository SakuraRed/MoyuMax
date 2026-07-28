import { expect, test, type Page } from "@playwright/test";

const ONBOARDING = {
  language: "zh-CN",
  dataDirectory: "D:\\MoyuMax\\data",
  telemetryEnabled: false,
  updateChecksEnabled: true,
  natDetectionEnabled: false,
  instanceIsolationEnabled: true,
};

const FABRIC_API_VERSIONS = [
  {
    id: "FAPI0002",
    versionNumber: "0.91.0+1.21.8",
    versionType: "release",
    datePublished: "2026-07-01T00:00:00Z",
    gameVersions: ["1.21.8"],
    loaders: ["fabric", "quilt"],
  },
  {
    id: "FAPI0001",
    versionNumber: "0.90.0+1.21.8",
    versionType: "release",
    datePublished: "2026-06-01T00:00:00Z",
    gameVersions: ["1.21.8"],
    loaders: ["fabric"],
  },
];

async function seed(page: Page, options: { fabricApi?: boolean } = {}): Promise<void> {
  await page.goto("/");
  await page.evaluate(
    ({ onboarding, fabricApiVersions }) => {
      window.localStorage.clear();
      window.localStorage.setItem(
        "moyumax.browser.onboarding",
        JSON.stringify(onboarding),
      );
      if (fabricApiVersions) {
        window.localStorage.setItem(
          "moyumax.browser.modVersions",
          JSON.stringify(fabricApiVersions),
        );
      }
    },
    {
      onboarding: ONBOARDING,
      fabricApiVersions: options.fabricApi === false ? null : FABRIC_API_VERSIONS,
    },
  );
  await page.reload();
}

async function openInstallPage(page: Page): Promise<void> {
  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await expect(page.getByRole("heading", { name: "安装第一个游戏" })).toBeVisible();
}

test("PCL33-LATEST-001 最新卡快捷选中正式版与快照且不与分组重复", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  const latestCard = page.getByRole("group", { name: "最新" });
  await expect(
    latestCard.getByRole("radio", { name: /1\.21\.8/ }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(latestCard.getByRole("radio", { name: /25w30a/ })).toHaveCount(0);

  const versionGroups = page.getByRole("radiogroup", { name: "Minecraft 版本" });
  await expect(versionGroups.getByRole("radio", { name: /1\.21\.8/ })).toHaveCount(0);
  await expect(
    versionGroups.getByRole("radio", { name: /1\.21\.7/ }),
  ).toBeVisible();

  await page.getByRole("button", { name: /快照与测试版/ }).click();
  await expect(latestCard.getByRole("radio", { name: /25w30a/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /快照与测试版/ })).toHaveCount(0);

  await latestCard.getByRole("radio", { name: /25w30a/ }).click();
  await expect(
    latestCard.getByRole("radio", { name: /25w30a/ }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(
    page.getByText("快照是开发中的版本，可能不稳定", { exact: false }),
  ).toBeVisible();
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "25w30a 原版",
  );
});

test("PCL33-LATEST-002 远古版本显示兼容性提示条", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  await page.getByRole("button", { name: /远古版本/ }).click();
  await page.getByRole("radio", { name: /b1\.7\.3/ }).click();
  await expect(
    page.getByText("远古版本兼容性有限", { exact: false }),
  ).toBeVisible();
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "b1.7.3 原版",
  );
});

test("PCL32-FOLD-001 折叠卡展开选版本、切换互斥、清除回到原版", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  const fabricGroup = page.getByRole("radiogroup", { name: "Fabric Loader 版本" });
  await expect(fabricGroup).toBeVisible();
  await expect(
    fabricGroup.getByRole("radio", { name: /0\.16\.14/ }),
  ).toHaveAttribute("aria-checked", "true");

  await fabricGroup.getByRole("radio", { name: "0.16.13", exact: true }).click();
  await expect(page.getByRole("radio", { name: /^Fabric / })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 Fabric",
  );

  await page.getByRole("radio", { name: /^Forge / }).click();
  await expect(
    page.getByRole("radiogroup", { name: "Forge 版本", exact: true }),
  ).toBeVisible();
  await expect(fabricGroup).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 Forge",
  );

  await page.getByRole("button", { name: "清除选择" }).click();
  await expect(page.getByRole("radio", { name: /不安装/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 原版",
  );
});

test("PCL32-FAPI-001 Fabric 默认勾选 Fabric API，取消勾选出现红色提示", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  const toggle = page.getByRole("checkbox", { name: "附带安装 Fabric API" });
  await expect(toggle).toBeChecked();
  const versionGroup = page.getByRole("radiogroup", { name: "Fabric API" });
  await expect(
    versionGroup.getByRole("radio", { name: /0\.91\.0/ }),
  ).toHaveAttribute("aria-checked", "true");

  await toggle.uncheck();
  await expect(
    page.getByText("大多数 Fabric 模组需要 Fabric API，建议保留勾选。"),
  ).toBeVisible();
  await expect(versionGroup).toHaveCount(0);

  await toggle.check();
  await expect(
    page.getByText("大多数 Fabric 模组需要 Fabric API，建议保留勾选。"),
  ).toHaveCount(0);
});

test("PCL32-FAPI-002 Quilt 显示 Fabric API 卡但默认不勾选", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  await page.getByRole("radio", { name: /Quilt/ }).click();
  const toggle = page.getByRole("checkbox", { name: "附带安装 Fabric API" });
  await expect(toggle).not.toBeChecked();
  await expect(
    page.getByText("Quilt 可兼容 Fabric API", { exact: false }),
  ).toBeVisible();
  await expect(page.getByRole("radiogroup", { name: "Fabric API" })).toHaveCount(0);

  await toggle.check();
  await expect(page.getByRole("radiogroup", { name: "Fabric API" })).toBeVisible();
});

test("PCL32-FAPI-003 该 MC 版本无 Fabric API 时禁用勾选", async ({ page }) => {
  await page.goto("/");
  await page.evaluate((onboarding) => {
    window.localStorage.clear();
    window.localStorage.setItem(
      "moyumax.browser.onboarding",
      JSON.stringify(onboarding),
    );
    window.localStorage.setItem("moyumax.browser.modVersions", "[]");
  }, ONBOARDING);
  await page.reload();
  await openInstallPage(page);

  const toggle = page.getByRole("checkbox", { name: "附带安装 Fabric API" });
  await expect(toggle).toBeDisabled();
  await expect(
    page.locator("p.fabric-api-note", { hasText: "该 MC 版本暂无 Fabric API" }),
  ).toBeVisible();
});

test("PCL32-FAPI-004 安装完成后附带安装 Fabric API 到实例 mods", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  await expect(
    page.getByRole("checkbox", { name: "附带安装 Fabric API" }),
  ).toBeChecked();
  await page.getByRole("button", { name: "查看安装信息" }).click();
  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();

  await page.evaluate(() => {
    const tasks = JSON.parse(
      window.localStorage.getItem("moyumax.browser.installTasks") ?? "[]",
    ) as { state: string; plan: { instanceId: string; instanceName: string } }[];
    const task = tasks[tasks.length - 1]!;
    task.state = "completed";
    window.localStorage.setItem("moyumax.browser.installTasks", JSON.stringify(tasks));
    const instances = JSON.parse(
      window.localStorage.getItem("moyumax.browser.instances") ?? "[]",
    ) as Record<string, unknown>[];
    instances.push({
      id: task.plan.instanceId,
      name: task.plan.instanceName,
      gameVersion: "1.21.8",
      loaderKind: "fabric",
      loaderVersion: "0.16.14",
      rootDirectory: `D:\\MoyuMax\\data\\instances\\${task.plan.instanceId}`,
      state: "ready",
    });
    window.localStorage.setItem("moyumax.browser.instances", JSON.stringify(instances));
  });

  await expect(page.getByText("已附带安装 Fabric API")).toBeVisible();
  const resources = await page.evaluate(
    () =>
      JSON.parse(
        window.localStorage.getItem("moyumax.browser.instanceResources") ?? "[]",
      ) as { kind: string; displayName: string; relativePath: string }[],
  );
  expect(
    resources.some(
      (resource) =>
        resource.kind === "mod" &&
        resource.displayName === "P7dR8mSH" &&
        resource.relativePath.startsWith(".minecraft/mods/"),
    ),
  ).toBe(true);
});

test("PCL32-FAPI-005 Fabric API 附带安装失败不阻塞实例", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  await page.getByRole("button", { name: "查看安装信息" }).click();
  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();

  await page.evaluate(() => {
    const tasks = JSON.parse(
      window.localStorage.getItem("moyumax.browser.installTasks") ?? "[]",
    ) as { state: string }[];
    tasks[tasks.length - 1]!.state = "completed";
    window.localStorage.setItem("moyumax.browser.installTasks", JSON.stringify(tasks));
  });

  await expect(
    page.getByText("Fabric API 安装失败，可稍后在资源页重装", { exact: false }),
  ).toBeVisible();
  await expect(page.getByText("目标实例不存在", { exact: false })).toBeVisible();
});

test("PCL32-NAV-001 安装中切换页面后任务视图不丢失", async ({ page }) => {
  await seed(page);
  await openInstallPage(page);

  await page.getByRole("button", { name: "查看安装信息" }).click();
  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();

  // 切到首页再切回实例列表,经"新建实例"重新进入安装页:任务在服务端持续,
  // 视图应恢复排队态而不是回到配置页。
  await page.getByRole("button", { name: "首页", exact: true }).first().click();
  await page.getByRole("button", { name: "实例", exact: true }).first().click();
  await page.getByRole("button", { name: "新建实例" }).first().click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();
});
