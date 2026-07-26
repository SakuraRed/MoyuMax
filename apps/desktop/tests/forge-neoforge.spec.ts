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

test("M12-FORGE-001 新建实例页可选择 Forge 与 NeoForge", async ({ page }) => {
  await seed(page);

  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await expect(page.getByRole("heading", { name: "安装第一个游戏" })).toBeVisible();

  await page.getByRole("radio", { name: /^Forge / }).click();
  await expect(page.getByRole("radio", { name: /^Forge / })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  const forgeGroup = page.getByRole("radiogroup", { name: "Forge 版本", exact: true });
  await expect(forgeGroup).toBeVisible();
  await expect(
    forgeGroup.getByRole("radio", { name: /58\.1\.20/ }),
  ).toHaveAttribute("aria-checked", "true");
  await forgeGroup.getByRole("radio", { name: "58.1.19", exact: true }).click();
  await expect(
    forgeGroup.getByRole("radio", { name: "58.1.19", exact: true }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 Forge",
  );

  await page.getByRole("radio", { name: /NeoForge / }).click();
  const neoforgeGroup = page.getByRole("radiogroup", { name: "NeoForge 版本", exact: true });
  await expect(neoforgeGroup).toBeVisible();
  await expect(
    neoforgeGroup.getByRole("radio", { name: /21\.8\.54/ }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(forgeGroup).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "实例名称" })).toHaveValue(
    "1.21.8 NeoForge",
  );

  await page.getByRole("button", { name: "查看安装信息" }).click();
  await expect(page.getByRole("heading", { name: "确认安装信息" })).toBeVisible();
  await expect(page.getByText("NeoForge 21.8.54", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();
  await expect(page.getByText("应用加载器", { exact: true })).toBeVisible();
});

test("M12-FORGE-001 Forge 与 NeoForge 选择在 960x600 与 200% 放大下不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await seed(page);

  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await page.getByRole("radio", { name: /NeoForge / }).click();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });
  const overflow = await page.evaluate(() => ({
    horizontal: document.documentElement.scrollWidth > window.innerWidth + 1,
    loaderCards: [...document.querySelectorAll<HTMLElement>(".loader-card")].some(
      (card) => card.scrollWidth > card.clientWidth + 1,
    ),
  }));
  expect(overflow.horizontal).toBe(false);
  expect(overflow.loaderCards).toBe(false);
});
