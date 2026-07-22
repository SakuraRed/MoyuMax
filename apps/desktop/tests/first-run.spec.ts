import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => window.localStorage.clear());
  await page.reload();
});

test("M1-FIRST-RUN-002 默认流程持久化并在刷新后进入首页", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "欢迎使用 MoyuMax" })).toBeVisible();

  await page.getByRole("button", { name: "下一步" }).click();
  await expect(page.getByRole("heading", { name: "数据位置" })).toBeVisible();
  await assertRegionsDoNotOverlap(page);

  await page.getByRole("button", { name: "下一步" }).click();
  await expect(page.getByRole("heading", { name: "隐私选择" })).toBeVisible();
  await assertRegionsDoNotOverlap(page);

  await page.getByRole("button", { name: "完成设置" }).click();
  await expect(page.getByRole("heading", { name: "一切就绪" })).toBeVisible();
  await assertRegionsDoNotOverlap(page);

  await page.getByRole("button", { name: "开始使用" }).click();
  await expect(page.getByRole("heading", { name: "从安装第一个游戏开始" })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("heading", { name: "从安装第一个游戏开始" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "欢迎使用 MoyuMax" })).toHaveCount(0);
});

test("UI-A11Y-001 在 960x600 和 200% 放大下主区域不重叠", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await page.reload();

  await assertRegionsDoNotOverlap(page);
  await expect(page.getByRole("button", { name: "下一步" })).toBeVisible();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await assertRegionsDoNotOverlap(page);
  await expect(page.getByRole("button", { name: "下一步" })).toBeVisible();

  const geometry = await page.evaluate(() => {
    const body = document.querySelector<HTMLElement>(".wizard-body");
    const actions = document.querySelector<HTMLElement>(".wizard-actions");
    if (!body || !actions) throw new Error("wizard layout is unavailable");
    return {
      bodyHasHorizontalOverflow: body.scrollWidth > body.clientWidth,
      actionsHasHorizontalOverflow: actions.scrollWidth > actions.clientWidth,
    };
  });

  expect(geometry.bodyHasHorizontalOverflow).toBe(false);
  expect(geometry.actionsHasHorizontalOverflow).toBe(false);
});

test("UI-A11Y-001 文本与卡片边缘保持可读内边距", async ({ page }) => {
  await expectElementPadding(page, ".wizard-card", { block: 18, inline: 22 });
  await expectElementPadding(page, ".choice-group", { block: 8, inline: 16 });

  const legendGap = await page.evaluate(() => {
    const legend = document.querySelector<HTMLElement>(".choice-section legend");
    const group = document.querySelector<HTMLElement>(".choice-group");
    if (!legend || !group) throw new Error("language group is unavailable");
    return group.getBoundingClientRect().top - legend.getBoundingClientRect().bottom;
  });
  expect(legendGap).toBeGreaterThanOrEqual(7);

  await page.getByRole("button", { name: "下一步" }).click();
  await expectElementPadding(page, ".choice-group", { block: 8, inline: 16 });

  await page.getByRole("button", { name: "下一步" }).click();
  await expectElementPadding(page, ".settings-panel", { block: 12, inline: 16 });

  await page.getByRole("button", { name: "完成设置" }).click();
  await expectElementPadding(page, ".summary-list > div", { block: 10, inline: 16 });
});

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

async function assertRegionsDoNotOverlap(
  page: import("@playwright/test").Page,
): Promise<void> {
  const geometry = await page.evaluate(() => {
    const rectangle = (selector: string) => {
      const element = document.querySelector<HTMLElement>(selector);
      if (!element) throw new Error(`missing layout region: ${selector}`);
      const bounds = element.getBoundingClientRect();
      return {
        top: bounds.top,
        right: bounds.right,
        bottom: bounds.bottom,
        left: bounds.left,
      };
    };

    return {
      titlebar: rectangle(".titlebar"),
      navigation: rectangle(".navrail"),
      topbar: rectangle(".topbar"),
      content: rectangle(".content"),
      statusbar: rectangle(".statusbar"),
      documentWidth: document.documentElement.scrollWidth,
      viewportWidth: window.innerWidth,
    };
  });

  expect(geometry.titlebar.bottom).toBeLessThanOrEqual(geometry.topbar.top);
  expect(geometry.navigation.right).toBeLessThanOrEqual(geometry.topbar.left);
  expect(geometry.topbar.bottom).toBeLessThanOrEqual(geometry.content.top);
  expect(geometry.content.bottom).toBeLessThanOrEqual(geometry.statusbar.top);
  expect(geometry.documentWidth).toBeLessThanOrEqual(geometry.viewportWidth);
}
