# EVAL DEFINITION：里程碑 22 全页面文案外置

## Capability Evals

- [x] ResourceCenter、TaskCenter、DataCenter、CrashCenter、GameInstall、Onboarding 全部用户可见文案经字典求值。
- [x] close-flow 退出影响描述等逻辑层文案经字典求值。
- [x] 英文/繁体下各页面关键文案真实渲染；静态扫描无未豁免硬编码中文。
- [x] 三语字典键集合一致（Vitest 强制）。

## Regression Evals

- [x] M1–M21 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 默认简体中文界面与 M21 基线一致。
- [x] 960×600 与 200% 放大下英文资源中心无横向溢出。

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

只有全部页面经字典求值、多语言渲染真实可见、扫描无未豁免残留时，才可将本里程碑标记为 validated。只做部分页面、切换后仍大面积中文、或键集合漂移，均不算通过。自定义背景与主题包不属于本里程碑。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- 字典：194 → 614 键，三语逐键一致（node 复验 614/614/614，无缺失无多余）；Vitest 19/19 PASS（键集合、非空/分隔键登记、占位符一致）。
- 静态扫描：ResourceCenter/TaskCenter/DataCenter/CrashCenter/GameInstall 组件中文字符零命中（仅注释）；豁免仅语言选项自名与纯数据（实例默认名）。
- `cargo test --workspace`：157 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm build`：PASS。
- Playwright：88/88 PASS，其中 full-i18n 5/5 PASS（英文资源/任务中心、繁体数据中心、英文安装向导、英文数据中心、960×600 与 200% 缩放）；英文长文案暴露 `.local-content-section` 头部溢出，已用 flex-wrap 修复并由 UI-I18N-002 锁定。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：核心返回的后端错误文案仍为中文（独立层，后续统一决策）；自定义背景/主题包、社区语言包未实现；时间戳格式随界面语言（zh-CN/zh-TW/en）渲染。

状态：validated（范围限于本 eval 所列条目）。
