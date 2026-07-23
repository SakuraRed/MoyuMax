# EVAL DEFINITION：里程碑 26 发布资产

## Capability Evals

- [x] 图标脚本可复现产出多尺寸 ICO 与 PNG；打包与托盘引用正式图标。
- [x] SBOM 覆盖 Cargo.lock 与 pnpm-lock 全部依赖条目且与锁文件一致。
- [x] 许可清单脚本生成；GPL 兼容黑名单扫描退出码拦截生效。
- [x] 关于区展示版本、许可证、仓库与未签名声明。

## Regression Evals

- [x] M1–M25 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] 安装器契约测试继续通过（卸载不删除数据）。
- [x] 960×600 与 200% 放大下关于区无横向溢出。

## Deterministic Graders

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `node scripts/generate-icon.mjs --check`
- `node scripts/generate-sbom.mjs --check`
- `git diff --check`

## Completion Rule

只有图标真实进入打包、SBOM/许可与锁文件一致、黑名单拦截真实生效时，才可将本里程碑标记为 validated。贴图占位冒充正式资产、SBOM 漏报依赖、或黑名单形同虚设，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `node scripts/generate-icon.mjs`：产出 icon.ico（16/32/48/64/128/256 六尺寸）与 6 个 PNG（含 tray-icon.png 替换开发占位），`--check` PASS；128px 视觉复核通过。
- `node scripts/generate-sbom.mjs`：602 个依赖（Cargo + npm）全量许可证解析，`--check` 与锁文件一致，黑名单扫描通过；`docs/SBOM.json`（CycloneDX 1.5 简式）与 `docs/THIRD-PARTY-LICENSES.md` 已生成。
- 安装器契约测试扩展：打包图标声明存在且文件齐全、icon.ico ≥4 尺寸、托盘图标存在、`deleteAppDataOnUninstall` 非真（卸载默认保留实例/存档/备份/账户/JDK）。
- `cargo test --workspace`：167 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：99/99 PASS，其中 release-assets 2/2 PASS（关于区渲染、960×600 与 200% 缩放）；self-update 版本号选择器按区域修复。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍未签名，属外部阻塞；关于页已声明）。
- `git diff --check`：PASS。
- 范围说明：Authenticode 需用户证书（外部阻塞）；卸载数据分类向导与安装包内许可捆绑属正式版项。

状态：validated（范围限于本 eval 所列条目）。
