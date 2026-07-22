# EVAL DEFINITION：里程碑 2 安装计划与持久化队列

## Capability Evals

- [x] 官方版本目录成功解析，推荐稳定版来自 `latest.release` 而不是硬编码。
- [x] 官方版本详情转换为带可信 SHA-1、大小与资源对象总量的下载清单。
- [x] 默认 Fabric 选择只使用服务端返回的兼容 Loader 版本。
- [x] 游戏文件与托管 Java 出现在同一个版本化安装计划中。
- [x] 已存在的相同 Java 完整构建被复用，不生成重复安装动作。
- [x] 安装任务、阶段和计划快照在 SQLite 重新打开后保持一致。
- [x] 运行中任务重启后进入等待恢复确认，不自动恢复。
- [x] 用户拒绝恢复时只清理任务暂存区，并保留已提交数据和任务历史。
- [x] 暂存任务不会提前产生可启动实例。
- [x] 在线目录不可用时返回本地缓存并标记来源。
- [x] 首页可以进入声明式新建实例页并创建真实持久化任务。
- [x] 重新加载后首页仍显示持久化任务，并可进入任务中心。

## Regression Evals

- [x] 首次运行 4 个 Rust BDD 场景继续通过。
- [x] 首次运行、安装和排版 6 个 Playwright 场景通过。
- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo test --workspace` 通过，首次运行 4/4、安装计划 6/6。
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm lint`、`pnpm test`、`pnpm test:e2e` 与 `pnpm build` 通过。
- [x] 显式在线探针访问 Mojang、Fabric 与 Azul 官方服务并生成同一解析请求。
- [x] Tauri Release `--no-bundle` 构建与真实启动通过。

## 人工复核

- [x] 与 `inst-new-default`、`inst-new-confirm` 和任务中心相关 mockups 完成浏览器视觉对照。
- [x] 960×600、200% 文本缩放下核心操作可达且无横向滚动。
- [ ] 屏幕阅读器能够朗读版本、推荐状态、Loader 兼容性、Java 结果和任务阶段。

## 成功标准

本里程碑只在真实元数据、数据库重启和浏览器行为均有证据时标记 validated。下载执行器未完成前不得宣称游戏已经安装。

## 2026-07-22 验证记录

- `cargo test --workspace`：PASS；M1 4/4、M2 6/6，在线测试默认忽略。
- `cargo test -p moyumax-core --test live_metadata -- --ignored --nocapture`：PASS；Mojang、Fabric、Azul 真实在线链路 1/1。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：PASS。
- `cargo fmt --all -- --check`：PASS。
- `corepack pnpm lint`：PASS，0 错误、0 警告。
- `corepack pnpm test`：PASS，6/6。
- `corepack pnpm test:e2e`：PASS，6/6。
- `corepack pnpm build`：PASS。
- `corepack pnpm --filter @moyumax/desktop tauri build --no-bundle`：PASS。
- Release 真实启动：根进程响应正常，WebView2 子进程建立，SQLite `user_version=2`，新增 6 张领域表可见。
- Release 前台进程树私有内存：215.2 MiB；低于 256 MiB 硬上限，但高于 180 MiB 目标，标记为 WARNING，后续需继续优化。
- 视觉证据：`output/playwright/m2-install-configure.png`、`output/playwright/m2-install-confirm.png`（本地忽略产物）。
