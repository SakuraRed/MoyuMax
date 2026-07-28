import { expect, test } from "@playwright/test";

function seedBase() {
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
        name: "详情测试",
        gameVersion: "26.2",
        loaderKind: "fabric",
        loaderVersion: "0.19.3",
        rootDirectory: "D:\\MoyuMax\\data\\instances\\instance-id",
        state: "ready",
      },
    ]),
  );
}


test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(seedBase);
  await page.reload();
});

async function openServersTab(page: import("@playwright/test").Page): Promise<void> {
  await page.getByRole("button", { name: "实例", exact: true }).click();
  await page.getByRole("button", { name: /管理实例/ }).click();
  await page.locator(".tabs").getByRole("button", { name: "世界", exact: true }).click();
  await expect(page.getByText("服务器", { exact: true })).toBeVisible();
}

test("M34-SRV-001 空态、添加服务器与刷新后 MOTD 着色渲染", async ({ page }) => {
  await openServersTab(page);
  await expect(page.getByText("还没有添加服务器。", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "全部刷新" })).toBeDisabled();

  await page.getByRole("textbox", { name: "服务器名称输入" }).fill("大厅");
  await page.getByRole("textbox", { name: "服务器地址输入" }).fill("mc.example.com:25566");
  await page.getByRole("button", { name: "添加", exact: true }).click();

  const row = page.locator(".server-row").filter({ hasText: "大厅" });
  await expect(row).toBeVisible();
  await expect(row.getByText("mc.example.com:25566", { exact: true })).toBeVisible();
  await expect(row.getByText("尚未检测", { exact: true })).toBeVisible();
  await expect(page.locator(".toast").getByText("已添加服务器「大厅」", { exact: false })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceServers") ?? "{}"),
  );
  expect(stored["instance-id"]).toHaveLength(1);
  expect(stored["instance-id"][0].address).toBe("mc.example.com:25566");

  await page.getByRole("button", { name: "全部刷新" }).click();
  // mock MOTD 为 §aMoyuMax §7测试服务器,应按 § 码分段着色。
  await expect(page.locator(".server-motd .motd-ca")).toHaveText("MoyuMax ");
  await expect(page.locator(".server-motd .motd-c7")).toHaveText("测试服务器");
  await expect(row.getByText("3/20 在线 · 42 ms · 26.2", { exact: false })).toBeVisible();
});

test("M34-SRV-002 非法地址与空名称被拒绝且不写入", async ({ page }) => {
  await openServersTab(page);
  await page.getByRole("textbox", { name: "服务器名称输入" }).fill("大厅");
  await page.getByRole("textbox", { name: "服务器地址输入" }).fill("host:0");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.locator(".toast").getByText("端口必须在 1-65535 之间", { exact: false })).toBeVisible();
  await expect(page.locator(".server-row")).toHaveCount(0);

  await page.getByRole("textbox", { name: "服务器地址输入" }).fill("has space.com");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.locator(".toast").getByText("不能为空或包含空白", { exact: false })).toBeVisible();

  await page.getByRole("textbox", { name: "服务器名称输入" }).fill("   ");
  await page.getByRole("textbox", { name: "服务器地址输入" }).fill("mc.example.com");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.locator(".toast").getByText("服务器名称不能为空", { exact: false })).toBeVisible();

  const stored = await page.evaluate(() =>
    window.localStorage.getItem("moyumax.browser.instanceServers"),
  );
  expect(stored).toBeNull();
});

test("M34-SRV-003 编辑服务器名称与地址", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instanceServers",
      JSON.stringify({
        "instance-id": [
          { name: "大厅", address: "mc.example.com", icon: null, acceptTextures: null },
          { name: "生存", address: "10.0.0.8:25570", icon: null, acceptTextures: null },
        ],
      }),
    );
  });
  await page.reload();
  await openServersTab(page);

  const row = page.locator(".server-row").nth(1);
  await row.getByRole("button", { name: "编辑「生存」" }).click();
  // 编辑态行的名称/地址在输入框 value 中,不能用 hasText 过滤。
  const nameInput = row.getByRole("textbox", { name: "服务器名称输入" });
  await expect(nameInput).toHaveValue("生存");
  await nameInput.fill("生存二周目");
  await row.getByRole("textbox", { name: "服务器地址输入" }).fill("10.0.0.9");
  await row.getByRole("button", { name: "保存", exact: true }).click();

  await expect(page.locator(".toast").getByText("已更新服务器「生存二周目」", { exact: false })).toBeVisible();
  const updated = page.locator(".server-row").filter({ hasText: "生存二周目" });
  await expect(updated).toBeVisible();
  await expect(updated.getByText("10.0.0.9", { exact: true })).toBeVisible();
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceServers") ?? "{}"),
  );
  expect(stored["instance-id"][1]).toMatchObject({ name: "生存二周目", address: "10.0.0.9" });
  // 第一台不受影响。
  await expect(page.locator(".server-row").filter({ hasText: "大厅" })).toBeVisible();
});

test("M34-SRV-004 删除需要确认并持久化", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instanceServers",
      JSON.stringify({
        "instance-id": [
          { name: "大厅", address: "mc.example.com", icon: null, acceptTextures: null },
          { name: "生存", address: "10.0.0.8", icon: null, acceptTextures: null },
        ],
      }),
    );
  });
  await page.reload();
  await openServersTab(page);

  const row = page.locator(".server-row").filter({ hasText: "生存" });
  await row.getByRole("button", { name: "删除「生存」" }).click();
  await row.getByRole("button", { name: "取消", exact: true }).click();
  await expect(row).toBeVisible();

  await row.getByRole("button", { name: "删除「生存」" }).click();
  await row.getByRole("button", { name: "确认删除" }).click();
  await expect(page.locator(".toast").getByText("已删除服务器「生存」", { exact: false })).toBeVisible();
  await expect(page.locator(".server-row")).toHaveCount(1);
  const stored = await page.evaluate(() =>
    JSON.parse(window.localStorage.getItem("moyumax.browser.instanceServers") ?? "{}"),
  );
  expect(stored["instance-id"]).toHaveLength(1);
  expect(stored["instance-id"][0].name).toBe("大厅");
});

test("M34-SRV-005 离线服务器显示不可达且不阻塞其余", async ({ page }) => {
  await page.evaluate(() => {
    window.localStorage.setItem(
      "moyumax.browser.instanceServers",
      JSON.stringify({
        "instance-id": [
          { name: "大厅", address: "mc.example.com", icon: null, acceptTextures: null },
          { name: "死服", address: "dead.example.com", icon: null, acceptTextures: null },
        ],
      }),
    );
    window.localStorage.setItem(
      "moyumax.browser.offlineServers",
      JSON.stringify(["dead.example.com"]),
    );
  });
  await page.reload();
  await openServersTab(page);

  await page.getByRole("button", { name: "全部刷新" }).click();
  const deadRow = page.locator(".server-row").filter({ hasText: "死服" });
  await expect(deadRow.getByText("离线或不可达", { exact: true })).toBeVisible();
  const aliveRow = page.locator(".server-row").filter({ hasText: "大厅" });
  await expect(aliveRow.getByText("3/20 在线 · 42 ms · 26.2", { exact: false })).toBeVisible();

  // 单项刷新同样可用。
  await aliveRow.getByRole("button", { name: "刷新「大厅」" }).click();
  await expect(aliveRow.getByText("3/20 在线 · 42 ms · 26.2", { exact: false })).toBeVisible();
});
