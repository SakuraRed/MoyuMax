import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:1421",
    viewport: { width: 1280, height: 800 },
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: {
    command: "corepack pnpm dev:web",
    url: "http://127.0.0.1:1421",
    reuseExistingServer: true,
    // CI 冷启动(全量 svelte 编译)明显慢于本地,30s 会误判超时。
    timeout: 120_000,
  },
});
