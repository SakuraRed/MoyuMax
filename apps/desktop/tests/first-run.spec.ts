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
  await expect(page.getByRole("heading", { name: "这里还空着" })).toBeVisible();

  await page.reload();
  await expect(page.getByRole("heading", { name: "这里还空着" })).toBeVisible();
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
  await expectContentInset(page, ".choice", ".choice-copy", {
    top: 16,
    right: 20,
    bottom: 16,
  });
  await expectElementPadding(page, ".choice-copy strong em", { block: 5, inline: 12 });

  const legendGap = await page.evaluate(() => {
    const legend = document.querySelector<HTMLElement>(".choice-section legend");
    const group = document.querySelector<HTMLElement>(".choice-group");
    if (!legend || !group) throw new Error("language group is unavailable");
    return group.getBoundingClientRect().top - legend.getBoundingClientRect().bottom;
  });
  expect(legendGap).toBeGreaterThanOrEqual(7);

  await page.getByRole("button", { name: "下一步" }).click();
  await expectContentInset(page, ".choice", ".choice-copy", {
    top: 16,
    right: 20,
    bottom: 16,
  });

  await page.getByRole("button", { name: "下一步" }).click();
  await expectContentInset(page, ".setting-row", ".setting-row > span:first-child", {
    top: 16,
    bottom: 16,
    left: 20,
  });
  await expectElementPadding(page, ".setting-row strong em", { block: 5, inline: 12 });

  await page.getByRole("button", { name: "完成设置" }).click();
  await expectElementPadding(page, ".summary-list > div", { block: 16, inline: 20 });
});

test("M2-INSTALL-001 默认配置生成可恢复安装任务", async ({ page }) => {
  await completeDefaultOnboarding(page);

  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await expect(page.getByRole("heading", { name: "安装第一个游戏" })).toBeVisible();
  await expect(page.getByRole("radio", { name: /1\.21\.8/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(page.getByRole("radio", { name: /Fabric/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expectContentInset(page, ".install-choice-row", ".choice-copy", {
    top: 16,
    right: 20,
    bottom: 16,
  });
  await expectElementPadding(page, ".install-form-card", { block: 20, inline: 24 });
  await assertDocumentHasNoHorizontalOverflow(page);

  await page.getByRole("button", { name: "查看安装信息" }).click();
  await expect(page.getByRole("heading", { name: "确认安装信息" })).toBeVisible();
  await expect(page.getByText("Azul Zulu 21.0.12+8 · x64")).toBeVisible();
  await expect(page.getByText("安装游戏环境", { exact: true })).toBeVisible();
  await expectElementPadding(page, ".install-summary > div", { block: 16, inline: 20 });
  await expectElementPadding(page, ".stage-preview li", { block: 14, inline: 16 });

  await page.getByRole("button", { name: "开始安装" }).click();
  await expect(page.getByRole("heading", { name: "安装任务已进入队列" })).toBeVisible();
  await expect(page.getByText("等待调度", { exact: true })).toBeVisible();
  await expect(page.getByText("安装游戏环境", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "返回首页" }).click();
  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "任务中心" })).toBeVisible();
  await expect(page.getByText("1.21.8 Fabric", { exact: false })).toBeVisible();

  await page.reload();
  await expect(page.getByText("1.21.8 Fabric", { exact: false })).toBeVisible();
});

test("M2-INSTALL-001 安装页在 960x600 与 200% 放大下不遮挡主操作", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await completeDefaultOnboarding(page);
  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await expect(page.getByRole("button", { name: "查看安装信息" })).toBeVisible();

  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByRole("button", { name: "查看安装信息" })).toBeVisible();
  await assertDocumentHasNoHorizontalOverflow(page);
});

test("M2-INSTALL-006 用户拒绝恢复时任务标记取消", async ({ page }) => {
  await completeDefaultOnboarding(page);
  await page.getByRole("button", { name: "安装第一个游戏" }).click();
  await page.getByRole("button", { name: "查看安装信息" }).click();
  await page.getByRole("button", { name: "开始安装" }).click();
  await page.getByRole("button", { name: "返回首页" }).click();

  await page.evaluate(() => {
    const key = "moyumax.browser.installTasks";
    const tasks = JSON.parse(window.localStorage.getItem(key) ?? "[]") as Array<{
      state: string;
    }>;
    if (!tasks[0]) throw new Error("missing browser install task");
    tasks[0].state = "awaitingRecovery";
    window.localStorage.setItem(key, JSON.stringify(tasks));
  });
  await page.reload();

  await page.getByRole("button", { name: "任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "发现未完成的安装" })).toBeVisible();
  await page.getByRole("button", { name: "放弃并清理临时文件" }).click();
  await expect(page.getByText("已取消", { exact: true })).toBeVisible();
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

async function expectContentInset(
  page: import("@playwright/test").Page,
  containerSelector: string,
  contentSelector: string,
  minimum: Partial<Record<"top" | "right" | "bottom" | "left", number>>,
): Promise<void> {
  const inset = await page.locator(containerSelector).first().evaluate(
    (container, selector) => {
      const content = container.querySelector<HTMLElement>(selector);
      if (!content) throw new Error(`missing content region: ${selector}`);
      const containerBounds = container.getBoundingClientRect();
      const contentBounds = content.getBoundingClientRect();
      return {
        top: contentBounds.top - containerBounds.top,
        right: containerBounds.right - contentBounds.right,
        bottom: containerBounds.bottom - contentBounds.bottom,
        left: contentBounds.left - containerBounds.left,
      };
    },
    contentSelector,
  );

  for (const [side, expected] of Object.entries(minimum)) {
    expect(inset[side as keyof typeof inset]).toBeGreaterThanOrEqual(expected);
  }
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

async function completeDefaultOnboarding(
  page: import("@playwright/test").Page,
): Promise<void> {
  await page.getByRole("button", { name: "下一步" }).click();
  await page.getByRole("button", { name: "下一步" }).click();
  await page.getByRole("button", { name: "完成设置" }).click();
  await page.getByRole("button", { name: "开始使用" }).click();
}

async function assertDocumentHasNoHorizontalOverflow(
  page: import("@playwright/test").Page,
): Promise<void> {
  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
}
