import { expect, test } from "@playwright/test";

const NOW = Math.floor(Date.now() / 1000);

function shot(dayOffset: number, name: string, size = 1_800_000) {
  return {
    fileName: name,
    sizeBytes: size,
    takenAtUnixSeconds: NOW - dayOffset * 24 * 60 * 60,
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
          name: "回收实例",
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

test("M18-SHOT-001 截图清单与本周筛选", async ({ page }) => {
  await seedScreenshots(page, [
    shot(1, "2026-07-22_23.12.05.png"),
    shot(3, "2026-07-20_22.48.31.png"),
    shot(30, "2026-06-23_21.05.17.png"),
  ]);
  await page.getByRole("button", { name: "数据", exact: true }).click();

  await expect(page.getByRole("heading", { name: "截图" })).toBeVisible();
  await expect(page.locator(".screenshot-card")).toHaveCount(3);
  await expect(page.getByText("2026-07-22_23.12.05.png", { exact: true })).toBeVisible();
  await expectElementPadding(page, ".screenshot-card", { block: 16, inline: 20 });

  await page.getByRole("button", { name: "本周" }).click();
  await expect(page.locator(".screenshot-card")).toHaveCount(2);
  await expect(page.getByText("2026-06-23_21.05.17.png", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: /全部/ }).click();
  await expect(page.locator(".screenshot-card")).toHaveCount(3);
});

test("M18-SHOT-002 复制截图写入剪贴板", async ({ page }) => {
  await seedScreenshots(page, [shot(1, "2026-07-22_23.12.05.png")]);
  await page.getByRole("button", { name: "数据", exact: true }).click();

  await page.getByRole("button", { name: "截图 2026-07-22_23.12.05.png" }).click();
  await expect(page.getByText("已选 2026-07-22_23.12.05.png", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "复制" }).click();
  await expect(
    page.getByRole("status").getByText("已把「2026-07-22_23.12.05.png」复制到剪贴板", { exact: true }),
  ).toBeVisible();
  const clipboard = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.clipboardImage"),
  );
  expect(clipboard).toBe("2026-07-22_23.12.05.png");
});

test("M18-SHOT-003 删除截图进入回收站并可恢复", async ({ page }) => {
  await seedScreenshots(page, [shot(1, "2026-07-22_23.12.05.png")]);
  await page.getByRole("button", { name: "数据", exact: true }).click();

  await page.getByRole("button", { name: "截图 2026-07-22_23.12.05.png" }).click();
  await page.getByRole("button", { name: "删除", exact: true }).click();
  await page.getByRole("button", { name: "确认删除" }).click();
  await expect(
    page.getByRole("status").getByText("已把「2026-07-22_23.12.05.png」移入回收站，30 天内可恢复", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".screenshot-card")).toHaveCount(0);

  const binCard = page.locator(".recycle-card").filter({ hasText: "2026-07-22_23.12.05.png" });
  await expect(binCard).toBeVisible();
  await expect(binCard.getByText("截图", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "恢复“2026-07-22_23.12.05.png”" }).click();
  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
  await expect(page.locator(".screenshot-card")).toHaveCount(1);
});

test("M18-RES-001 资源删除进回收站并带索引恢复", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instanceResources",
      JSON.stringify([
        {
          id: "resource-1",
          instanceId: "instance-id",
          kind: "resourcepack",
          displayName: "faithful",
          fileName: "faithful.zip",
          relativePath: ".minecraft/resourcepacks/faithful.zip",
          size: 1024,
          sha256: "3".repeat(64),
          enabled: true,
          worldName: null,
          importedAtUnixSeconds: 1,
        },
      ]),
    );
  });
  await page.reload();
  await page.getByRole("button", { name: "资源", exact: true }).click();

  await page.getByRole("button", { name: "删除 faithful" }).click();
  await page.getByRole("button", { name: "确认删除" }).click();
  await expect(page.getByText("还没有导入资源包、光影或数据包", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "返回首页" }).click();
  await page.getByRole("button", { name: "数据", exact: true }).click();
  const binCard = page.locator(".recycle-card").filter({ hasText: "faithful" });
  await expect(binCard.getByText("资源内容", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "恢复“faithful”" }).click();

  await page.getByRole("button", { name: "资源", exact: true }).click();
  const row = page.locator(".installed-content-row").filter({ hasText: "faithful" });
  await expect(row).toBeVisible();
  await expect(row.getByRole("checkbox", { name: "faithful 启用开关" })).toBeChecked();
});

test("M18-WORLD-001 世界删除进回收站并可恢复", async ({ page }) => {
  const lastPlayed = NOW - 86400;
  await page.evaluate((played) => {
    window.localStorage.setItem(
      "moyumax.browser.worldDetails",
      JSON.stringify({
        "instance-id": [
          { name: "alpha", sizeBytes: 1024, lastPlayedUnixSeconds: played },
        ],
      }),
    );
  }, lastPlayed);
  await page.reload();
  await page.getByRole("button", { name: "数据", exact: true }).click();

  const row = page.locator(".backup-row").filter({ hasText: "alpha" });
  await row.getByRole("button", { name: "删除", exact: true }).click();
  await row.getByRole("button", { name: "确认删除" }).click();
  await expect(
    page.getByRole("status").getByText("已把世界「alpha」移入回收站，30 天内可恢复", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("这个实例还没有世界存档。", { exact: true })).toBeVisible();

  const binCard = page.locator(".recycle-card").filter({ hasText: "alpha" });
  await expect(binCard.getByText("世界", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "恢复“alpha”" }).click();
  await expect(page.locator(".backup-row").filter({ hasText: "alpha" })).toHaveCount(1);
});

test("UI-SHOT-001 截图区在 960x600 和 200% 放大下不发生横向溢出", async ({ page }) => {
  await seedScreenshots(page, [
    shot(1, "2026-07-22_23.12.05.png"),
    shot(2, "2026-07-21_22.48.31.png"),
    shot(3, "2026-07-20_21.05.17.png"),
  ]);
  await page.setViewportSize({ width: 960, height: 600 });
  await page.getByRole("button", { name: "数据", exact: true }).click();
  await page.getByRole("button", { name: "截图 2026-07-22_23.12.05.png" }).click();
  await expect(page.locator(".screenshot-actions")).toBeVisible();
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  const geometry = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    viewportWidth: window.innerWidth,
    overflowingElements: [...document.querySelectorAll<HTMLElement>(".data-content *")]
      .filter(
        (element) =>
          !element.classList.contains("sr-live") &&
          element.scrollWidth > element.clientWidth + 1,
      )
      .map((element) => ({
        tag: element.tagName,
        className: element.className,
        scrollWidth: element.scrollWidth,
        clientWidth: element.clientWidth,
      })),
  }));
  expect(geometry.scrollWidth).toBeLessThanOrEqual(geometry.viewportWidth);
  expect(geometry.overflowingElements).toEqual([]);
});

async function seedScreenshots(
  page: import("@playwright/test").Page,
  screenshots: { fileName: string; sizeBytes: number; takenAtUnixSeconds: number }[],
): Promise<void> {
  await page.evaluate((seeded) => {
    window.localStorage.setItem(
      "moyumax.browser.screenshots",
      JSON.stringify({ "instance-id": seeded }),
    );
  }, screenshots);
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
