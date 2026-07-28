import { expect, test, type Page } from "@playwright/test";

const instance = {
  id: "instance-recycle",
  name: "生存世界",
  gameVersion: "1.21.8",
  loaderKind: "fabric",
  loaderVersion: "0.16.14",
  rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-recycle",
  state: "ready",
};

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate((managedInstance) => {
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
      JSON.stringify([managedInstance]),
    );
  }, instance);
  await page.reload();
});

/** 实例页 → 批量管理 → 勾选 → 批量条删除,打开删除确认弹窗。
 *  注意:卡片管理按钮(.card-hit)覆盖层挡住 pick 的鼠标点击(InstanceGallery 已知遮挡问题),
 *  这里走键盘聚焦 + Enter 的真实可用路径。 */
async function openBatchDeleteDialog(page: Page, name: string) {
  await page.getByRole("button", { name: "实例", exact: true }).click();
  await page.getByRole("button", { name: "批量管理" }).click();
  await page.getByRole("button", { name: `选择实例「${name}」` }).focus();
  await page.keyboard.press("Enter");
  await page.getByRole("button", { name: "删除", exact: true }).click();
  return page.getByRole("dialog", { name: "删除 1 个实例？" });
}

test("M7-RECYCLE-001 实例经确认进入回收站并可从数据页恢复", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
  const dialog = await openBatchDeleteDialog(page, "生存世界");

  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "取消" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("button", { name: "选择实例「生存世界」" })).toBeVisible();

  await page.getByRole("button", { name: "删除", exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("保留 30 天", { exact: false })).toBeVisible();
  await dialog.getByRole("button", { name: "删除 1 个实例" }).click();

  await expect(page.getByRole("heading", { name: "还没有实例" })).toBeVisible();
  await page.getByRole("button", { name: "数据", exact: true }).click();
  await expect(page.getByRole("heading", { name: "数据与回收站" })).toBeVisible();
  await expect(page.getByText("生存世界")).toBeVisible();
  await expect(page.getByText("剩 30 天", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "恢复“生存世界”" }).click();
  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "首页", exact: true }).click();
  await expect(page.getByRole("heading", { name: "生存世界" })).toBeVisible();
});

test("M37-DATA-001 存储空间展示实例占用与磁盘余量", async ({ page }) => {
  await page.getByRole("button", { name: "数据" }).click();
  const storage = page.getByLabel("存储空间");
  await expect(storage).toBeVisible();
  await expect(storage.getByText("MoyuMax 占用", { exact: true })).toBeVisible();
  await expect(storage.getByText("磁盘剩余 58.0 GiB · 共 96.0 GiB", { exact: false })).toBeVisible();
  const legend = storage.locator(".legend");
  await expect(legend).toContainText("实例");
  await expect(legend).toContainText("64.0 MiB");
});

test("M7-RECYCLE-002 永久删除前展示空间与不可恢复说明", async ({ page }) => {
  const deleteDialog = await openBatchDeleteDialog(page, "生存世界");
  await deleteDialog.getByRole("button", { name: "删除 1 个实例" }).click();
  await page.getByRole("button", { name: "数据", exact: true }).click();
  await page.getByRole("button", { name: "永久删除“生存世界”" }).click();

  const dialog = page.getByRole("dialog", { name: "永久删除回收站项目" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "取消" })).toBeFocused();
  await expect(dialog.getByText("即将永久删除 1 个项目", { exact: false })).toBeVisible();
  await expect(dialog.getByText("64.0 MiB", { exact: false }).first()).toBeVisible();
  await expect(dialog.getByText("此操作不可恢复", { exact: false })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: "永久删除“生存世界”" }).click();
  await dialog.getByRole("button", { name: "永久删除 1 个项目" }).click();
  await expect(page.getByText("回收站为空", { exact: true })).toBeVisible();
});

test("UI-RECYCLE-001 数据页在 960x600 和 200% 放大下无横向溢出", async ({ page }) => {
  const deleteDialog = await openBatchDeleteDialog(page, "生存世界");
  await deleteDialog.getByRole("button", { name: "删除 1 个实例" }).click();
  await page.getByRole("button", { name: "数据", exact: true }).click();
  await page.setViewportSize({ width: 960, height: 600 });
  await page.evaluate(() => {
    document.documentElement.style.zoom = "2";
  });

  await expect(page.getByRole("button", { name: "恢复“生存世界”" })).toBeVisible();
  // 与同页 UI-SHOT/UI-WORLD/UI-BACKUP 一致:单元格省略号属内部裁剪,
  // 断言页面级与内容容器都不产生横向滚动。
  const geometry = await page.evaluate(() => {
    const content = document.querySelector<HTMLElement>(".data-content");
    return {
      documentOverflow: document.documentElement.scrollWidth > window.innerWidth,
      containerOverflow: content ? content.scrollWidth > content.clientWidth + 1 : false,
    };
  });
  expect(geometry.documentOverflow).toBe(false);
  expect(geometry.containerOverflow).toBe(false);
});
