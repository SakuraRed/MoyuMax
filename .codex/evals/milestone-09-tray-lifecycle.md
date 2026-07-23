# EVAL DEFINITION：里程碑 9 托盘生命周期与快速唤醒

## Capability Evals

- [x] 首次关闭主窗口弹出最小化/退出选择，记住复选框默认不勾选，选择被持久化；记住后再次关闭不再询问。
- [x] 托盘菜单包含显示、最近实例快速启动（运行中标记）、活动任务只读摘要、暂停/恢复全部任务与退出；双击托盘图标显示窗口。
- [x] 最小化到托盘先隐藏保留界面，空闲 5 分钟后销毁 WebView；唤醒快速路径直接前置显示；销毁后唤醒重建窗口并恢复上次白名单页面与滚动位置；敏感页面回退首页；弹窗不重开。
- [x] 暂停全部任务停止调度并在分段边界中断下载，任务进入 `paused` 而非 `failed`，`.partial` 保留；恢复后续传完成且哈希一致；重启后暂停保持。
- [x] 退出前影响汇总覆盖运行中游戏与活动任务；确认退出先安全终止游戏并完成退出后备份，下载转为可恢复暂停；取消退出不改变运行状态。
- [x] 首个可交互窗口不被托盘初始化阻塞；托盘菜单与窗口界面展示同一数据库事实。
- [x] Release 冒烟给出快速唤醒 P95 ≤ 250 ms、WebView 销毁后后台私有内存 ≤ 80 MiB 目标 / 120 MiB 硬上限的采样证据，慢速重建唤醒有实测记录。

## Regression Evals

- [x] M1–M8 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 关闭对话框不影响既有窗口控制（最小化、最大化）；安装、启动、备份、回收站、诊断流程不变。
- [x] 960×600 与 200% 放大下关闭/退出对话框无横向溢出、遮挡或文本贴边。
- [x] 暂停语义不改变 M3 中断恢复语义：`running/committing` 在重启后仍进入 `awaiting_recovery`，`paused` 保持 `paused`。

## Deterministic Graders

- `cargo test -p moyumax-core --test tray_lifecycle_bdd`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `node scripts/release-smoke.mjs`（读取 `output/release-smoke/` 最新样本）
- `git diff --check`

## Completion Rule

只有关闭选择真实持久化并作用于所有关闭路径、托盘菜单来自同一数据库事实、窗口隐藏/销毁/重建真实发生、暂停任务以 `paused` 语义可续传恢复、退出保护先收敛游戏会话与备份再退出、且 Release 冒烟给出真实快速唤醒与销毁后内存样本时，才可将本里程碑标记为 validated。托盘菜单硬编码静态内容、暂停以失败伪装、后台超限时销毁缺失、或性能数字来自估算而非采样，均不算通过。

## 2026-07-23 验证报告

- Capability：7/7 PASS。
- Regression：4/4 PASS。
- `tray_lifecycle_bdd`：9/9 PASS（关闭行为、壳层状态、暂停标志重启保持、下载分段中断与续传、内容/安装任务暂停与重新入队、重启后 paused 与 awaiting_recovery 区分、退出影响汇总、最近实例排序与回收站排除）。
- 桌面单元测试：托盘菜单模型 2/2、任务协调器 3/3 PASS。
- Vitest：16/16 PASS（shell-state 5/5、close-flow 5/5）。
- Playwright：29/29 PASS，其中 tray-lifecycle 10/10 PASS。
- `cargo test --workspace`：76 个非忽略测试 PASS；4 个联网或真实 Minecraft 昂贵测试保持 ignored。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：PASS。
- `cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`、`pnpm build`：PASS。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- Release 冒烟（`node scripts/release-smoke.mjs`，证据 `output/release-smoke/m9-smoke-20260723054536.json`）：快速唤醒（隐藏→显示）样本 0/0/0/0/0/0/1/1/4/5 ms，P95 5 ms，满足 250 ms 预算；慢速唤醒（WebView 销毁后重建）325/332/334 ms，如实记录为信息项；WebView 销毁后后台私有内存平均 11.6 MiB、峰值 15.1 MiB，满足 80 MiB 目标与 120 MiB 硬上限。
- 首次尝试“最小化即销毁 WebView”时实测重建唤醒 329–359 ms 超过 250 ms 预算，按 ADR-0001 风险条目改为混合策略：隐藏保留界面 + 空闲 5 分钟销毁。两条路径均在上一条冒烟数据中有实测。
- 人工视觉检查：关闭选择对话框与退出影响对话框对照 `D:/Downloads/MoyuMax-mockups/pages/global.js` 的 global-close-dialog 截图核对（证据 `output/playwright/m9/close-dialog-*.png`）；960×600 与 200% 放大由 Playwright 几何断言覆盖。修复了对话框渲染在 `.window` 外导致设计令牌丢失的透明背景问题。
- 与 mockup 的两处有意偏差：记住复选框文案不含“设置 → 常规”引用（设置页尚未实现，见计划非目标）；退出选项副文案按真实退出语义改写（下载暂停可恢复、游戏安全终止并备份）。
- 托盘图标与菜单实机视觉未逐项截图，待用户对开发预览进行人工审查；托盘创建在全部冒烟与探针运行中无错误，菜单模型由单元测试固定。

状态：validated（范围限于本 eval 所列条目），等待用户对 Windows 开发预览安装包与托盘实机视觉进行人工审查。
