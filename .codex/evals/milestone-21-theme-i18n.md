# EVAL DEFINITION：里程碑 21 主题切换与 i18n 基础设施

## Capability Evals

- [x] system/light/dark 主题切换立即生效并持久化；light 不依赖系统媒体查询。
- [x] zh-CN/zh-TW/en 语言切换立即生效并持久化；缺键回退 zh-CN，不出现空白或键名。
- [x] 应用壳、首页、设置页文案全部经字典求值；设置页提供主题与语言选择。

## Regression Evals

- [x] M1–M20 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 默认（简体中文 + 跟随系统）视觉与 M20 基线一致。
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

## Completion Rule

只有主题与语言真实切换并持久化、字典覆盖本里程碑范围页面、缺键回退成立时，才可将本里程碑标记为 validated。只做选择器不落盘、语言切换需要手动刷新、或范围页面仍大面积硬编码，均不算通过。本里程碑不宣称全应用 i18n 完成。

## 2026-07-23 验证报告

- Capability：3/3 PASS。
- Regression：3/3 PASS。
- Vitest：19/19 PASS（新增 i18n.test 3/3：三语键集合完全一致 194/194/194、值非空与分隔键登记、插值占位符一致）。
- `cargo test --workspace`：157 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm build`：PASS。
- Playwright：83/83 PASS，其中 theme-i18n 5/5 PASS（浅色切换与持久化与 CSS 变量实值、深色不依赖系统、英文切换与持久化、繁体切换、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：文案外置范围为应用壳、首页与设置页（194 键三语一致）；资源/任务/数据/诊断/安装向导/关闭对话框以外的页面在 M22 继续外置（CloseDialog 已外置，close-flow.ts 的影响描述行属 M22）；自定义背景与主题包未实现。

状态：validated（范围限于本 eval 所列条目）。
