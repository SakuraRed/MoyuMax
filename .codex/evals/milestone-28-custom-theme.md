# EVAL DEFINITION：里程碑 28 自定义背景与纯数据主题包

## Capability Evals

- [x] 纯色/图片背景应用与持久化；图片压暗渲染；隐藏或减少动画时降级。
- [x] 主题包导入应用配色；非 JSON、未知字段、非颜色、URL 一律拒绝。
- [x] 高对比开启时忽略主题包配色；移除主题包回到默认。

## Regression Evals

- [x] M1–M27 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 默认外观与 M22 基线一致。
- [x] 960×600 与 200% 放大下背景设置区无横向溢出。

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

只有背景真实渲染并降级、主题包校验真实拦截、配色叠加与移除语义正确时，才可将本里程碑标记为 validated。只存设置不渲染、校验可被绕过、或主题包可夹带远程资源/脚本，均不算通过。

## 2026-07-23 验证报告

- Capability：3/3 PASS。
- Regression：3/3 PASS。
- `theme_bdd`：5/5 PASS（合法主题包解析、六类非法包拒绝（非 JSON/版本/未知键/非颜色/URL/空）、默认与纯色持久化与非法色拒绝、图片导入类型大小校验与读回、缺失图片背景拒绝）。
- `cargo test --workspace`：172 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：104/104 PASS，其中 custom-theme 5/5 PASS（纯色持久化、图片渲染与减少动画降级、主题包配色与高对比忽略、恶意包拒绝、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍未签名，属外部阻塞）。
- `git diff --check`、`node scripts/generate-sbom.mjs --check`：PASS。
- 范围说明：图片背景在窗口隐藏时随 WebView 销毁自动释放（无视频/动画背景，无需额外暂停逻辑）；主题包分享目录与视频背景未实现。

状态：validated（范围限于本 eval 所列条目）。
