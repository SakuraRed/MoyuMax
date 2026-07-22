# EVAL DEFINITION：里程碑 1 首次运行

## Capability Evals

- [x] 新数据库返回需要首次运行引导及安全默认值。
- [x] 完成引导使用单一事务持久化，重新打开数据库后值一致。
- [x] UNC/SMB 路径被拒绝，未产生半完成状态。
- [x] 跳过引导保存安全默认值。
- [x] UI 可以仅使用键盘完成默认流程并进入首页空状态。
- [x] Rust 核心不依赖 Tauri，桌面层通过命令适配。

## Regression Evals

- [x] `cargo test --workspace` 通过，4 个 BDD 核心场景通过。
- [x] `cargo clippy --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm test` 通过，3/3 单元测试通过。
- [x] `pnpm build` 通过，`svelte-check` 为 0 错误、0 警告。
- [x] Playwright E2E 通过，3/3 场景覆盖持久化、缩放、重叠和容器内边距。
- [x] Tauri Debug 构建与真实启动通过，WebView2 正常建立且 SQLite 成功创建。
- [x] Tauri Release `--no-bundle` 构建通过。

## 人工复核

- [x] 与 `D:/Downloads/MoyuMax-mockups` 的欢迎、数据、隐私、完成及首页空状态完成浏览器视觉对照。
- [x] 960×600 下主操作仍可见，无横向滚动；200% 放大下无区域重叠。
- [ ] Windows 屏幕阅读器语义、焦点顺序和 200% 缩放需在后续专门设备测试中复核。

## 成功标准

Capability 和 Regression 确定性检查全部通过。性能只能在 Release 构建与目标基准设备上标记 validated。

## 2026-07-22 验证记录

- `corepack pnpm lint`：PASS。
- `corepack pnpm test`：PASS，3/3。
- `corepack pnpm --filter @moyumax/desktop test:e2e`：PASS，3/3。
- `corepack pnpm build`：PASS。
- `cargo fmt --all -- --check`：PASS。
- `cargo test --workspace`：PASS，4/4 核心 BDD 场景。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：PASS。
- Release 可执行文件：`target/release/moyumax-desktop.exe`，构建成功；尚未以目标基准设备数据标记性能 validated。
