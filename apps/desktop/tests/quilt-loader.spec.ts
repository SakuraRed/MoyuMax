import { expect, test, type Page } from "@playwright/test";

const ONBOARDING = {
  language: "zh-CN",
  dataDirectory: "D:\\MoyuMax\\data",
  telemetryEnabled: false,
  updateChecksEnabled: true,
  natDetectionEnabled: false,
  instanceIsolationEnabled: true,
};

async function seed(page: Page): Promise<void> {
  await page.goto("/");
  await page.evaluate((onboarding) => {
    window.localStorage.clear();
    window.localStorage.setItem(
      "moyumax.browser.onboarding",
      JSON.stringify(onboarding),
    );
  }, ONBOARDING);
  await page.reload();
}

test("M11-QUILT-001 新建实例页可选择 Quilt 并完成安装预览", async ({ page }) => {
  await seed(page);

  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await expect(page.getByRole("heading", { name: "安装第一个游戏" })).toBeVisible();

  await page.getByRole("radio", { name: /Quilt/ }).click();
  await expect(page.getByRole("radio", { name: /Quilt/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );

  const versionGroup = page.getByRole("radiogroup", { name: /Quilt Loader 版本/ });
  await expect(versionGroup).toBeVisible();
  await expect(
    versionGroup.getByRole("radio", { name: /0\.30\.0/ }),
  ).toHaveAttribute("aria-checked", "true");
  await versionGroup.getByRole("radio", { name: "0.30.1-beta.1", exact: true }).click();
  await expect(
    versionGroup.getByRole("radio", { name: "0.30.1-beta.1", exact: true }),
  ).toHaveAttribute("aria-checked", "true");

  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 Quilt",
  );

  await page.getByRole("button", { name: "查看安装信息" }).click();
  await expect(page.getByRole("heading", { name: "确认安装信息" })).toBeVisible();
  await expect(page.getByText("Quilt 0.30.1-beta.1", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();

  await page.getByRole("button", { name: "返回首页" }).click();
  await expect(page.getByRole("button", { name: /Quilt.*1 个任务/ })).toBeVisible();
});

test("M11-QUILT-001 Quilt 选择在 960x600 与 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page);

  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await page.getByRole("radio", { name: /Quilt/ }).click();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  const overflow = await page.evaluate(() => ({
    horizontal:
      document.documentElement.scrollWidth > window.innerWidth + 1,
    loaderGrid: [...document.querySelectorAll<HTMLElement>(".loader-card")].some(
      (card) => card.scrollWidth > card.clientWidth + 1,
    ),
  }));
  expect(overflow.horizontal).toBe(false);
  expect(overflow.loaderGrid).toBe(false);
});
