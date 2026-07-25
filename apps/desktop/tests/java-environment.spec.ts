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
  id: "instance-a",
  name: "实例甲",
  gameVersion: "26.2",
  loaderKind: "fabric",
  loaderVersion: "0.19.3",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-a",
  state: "ready",
};

function readyEnvironment(referencing: { id: string; name: string }[]) {
  return {
    id: "env-21",
    distribution: "azulZulu",
    fullVersion: "21.0.12+8",
    architecture: "x64",
    homeDirectory: "D:\\MoyuMax\\data\\store\\java\\azul-zulu\\21.0.12+8\\x64",
    status: "ready",
    sizeBytes: 188 * 1024 * 1024,
    healthy: true,
    referencingInstances: referencing,
  };
}

async function seed(page: Page, environments: unknown[]): Promise<void> {
  await page.goto("/");
  await page.evaluate(
    ({ onboarding, instance, envs }) => {
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
        "moyumax.browser.javaEnvironments",
        JSON.stringify(envs),
      );
    },
    { onboarding: ONBOARDING, instance: INSTANCE, envs: environments },
  );
  await page.reload();
  await page.getByRole("button", { name: "设置" }).click();
  await page.locator(".sn-item", { hasText: "Java 环境" }).click();
  await expect(page.getByRole("heading", { name: "Java 环境" })).toBeVisible();
}

test("M13-JAVA-001 环境列表显示版本、大小、健康与引用实例", async ({ page }) => {
  await seed(page, [readyEnvironment([{ id: "instance-a", name: "实例甲" }])]);

  await expect(page.getByText("Azul Zulu 21.0.12+8")).toBeVisible();
  await expect(page.getByText("已就绪", { exact: true })).toBeVisible();
  await expect(page.getByText("1 个实例", { exact: true })).toBeVisible();
  await expect(page.getByText(/188\.0 MiB|179 MiB|188 MiB/)).toBeVisible();

  await page.getByRole("button", { name: "验证" }).click();
  await expect(page.getByText(/验证通过/)).toBeVisible();
});

test("M13-JAVA-002 删除被引用环境需确认并列出受影响实例", async ({ page }) => {
  await seed(page, [readyEnvironment([{ id: "instance-a", name: "实例甲" }])]);

  await page.getByRole("button", { name: "删除" }).click();
  const dialog = page.getByRole("dialog", { name: "删除 Java 环境" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(/「实例甲」/)).toBeVisible();
  await expect(dialog.getByText(/无法直接启动/)).toBeVisible();

  await dialog.getByRole("button", { name: /删除 Azul Zulu/ }).click();
  await expect(page.getByText(/已删除；受影响的 1 个实例/)).toBeVisible();
  await expect(page.getByText("已删除", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "一键恢复" })).toBeVisible();
});

test("M13-JAVA-002 取消删除不改变任何状态", async ({ page }) => {
  await seed(page, [readyEnvironment([{ id: "instance-a", name: "实例甲" }])]);

  await page.getByRole("button", { name: "删除" }).click();
  const dialog = page.getByRole("dialog", { name: "删除 Java 环境" });
  await dialog.getByRole("button", { name: "取消" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("已就绪", { exact: true })).toBeVisible();
});

test("M13-JAVA-003 一键恢复把环境标记回可用", async ({ page }) => {
  const deleted = {
    ...readyEnvironment([{ id: "instance-a", name: "实例甲" }]),
    status: "deleted",
    healthy: false,
    sizeBytes: 0,
  };
  await seed(page, [deleted]);

  await page.getByRole("button", { name: "一键恢复" }).click();
  await expect(page.getByText(/已恢复，引用实例已指向该环境/)).toBeVisible();
  await expect(page.getByText("已就绪", { exact: true })).toBeVisible();
});

test("M13-JAVA-004 设为实例环境并展示结果", async ({ page }) => {
  await seed(page, [readyEnvironment([])]);

  await page.getByRole("button", { name: "设为实例环境" }).click();
  await expect(page.getByRole("group", { name: "选择目标实例" })).toBeVisible();
  await page.getByRole("button", { name: "确认指派" }).click();
  await expect(page.getByText(/已为「实例甲」指派/)).toBeVisible();
});

test("UI-A11Y-001 Java 环境页在 960x600 与 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page, [readyEnvironment([{ id: "instance-a", name: "实例甲" }])]);

  await page.getByRole("button", { name: "删除" }).click();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  const dialog = page.getByRole("dialog", { name: "删除 Java 环境" });
  await expect(dialog).toBeVisible();
  const geometry = await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>(".confirmation-dialog");
    if (!root) throw new Error("dialog missing");
    return {
      overflow: root.scrollWidth > root.clientWidth + 1,
      beyond:
        root.getBoundingClientRect().right > window.innerWidth + 1 ||
        root.getBoundingClientRect().bottom > window.innerHeight + 1,
    };
  });
  expect(geometry.overflow).toBe(false);
  expect(geometry.beyond).toBe(false);
});
