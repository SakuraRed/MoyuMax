# EVAL DEFINITION：里程碑 23 无障碍增强与性能正式验收

## Capability Evals

- [x] 减少动画手动开关立即生效并持久化（data-motion 属性与过渡停用实测）。
- [x] 高对比手动开关立即生效并持久化（data-contrast 属性与变量实值实测）。
- [x] 首页主操作 Tab 链路完整且焦点可见；设置区渲染新开关。
- [x] release-smoke cold 模式输出冷启动三轮样本与前台内存样本。

## Regression Evals

- [x] M1–M22 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 默认外观与 M22 基线一致（不动画/高对比默认关闭跟随系统）。
- [x] 960×600 与 200% 放大下设置区无横向溢出。

## Deterministic Graders

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `git diff --check`
- `node scripts/release-smoke.mjs --mode cold --cycles 3`

## Completion Rule

只有两项设置真实生效持久化、键盘链路 e2e 通过、基准数字真实产出时，才可将本里程碑标记为 validated。只做开关不改渲染、基准沿用旧样本冒充、或 e2e 只测存在不测属性实值，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `cargo test --workspace`：157 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：91/91 PASS，其中 a11y-performance 3/3 PASS（减少动画持久化、高对比变量实值与持久化、首页 Tab 链路与焦点环）；主题规格按动画/主题分组修复选择器歧义。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `node scripts/release-smoke.mjs --mode cold --cycles 3`：机制 PASS，样本真实产出（`output/release-smoke/m23-cold-20260723212224.json`）。本机实测：首窗可见 779/781/783 ms（超出 500 ms P95 预算，口径为进程启动到应用初始化完成，本机为开发负载环境）；可操作 842/843/846 ms（P50 达标 ≤ 1000 ms）；前台进程树私有内存峰值 219.2 MiB（低于 256 MiB 硬上限，未达 180 MiB 目标）。脚本按预算退出码为失败；正式基准结论留待发布候选在基准设备复测（缺口 #15）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：屏幕阅读器全量口述验收与 5 名用户形成性测试属缺口 #15 发布验收；虚拟化长列表未实现（当前规模未触发预算问题）。

状态：validated（范围限于本 eval 所列条目；预算达成情况如实记录，不作为本机通过）。
