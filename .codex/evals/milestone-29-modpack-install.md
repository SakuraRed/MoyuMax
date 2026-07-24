# EVAL DEFINITION：里程碑 29 Modrinth 与 CurseForge 整合包安装与更新

## Capability Evals

- [x] 两种格式解析与预览（名称/版本/依赖/文件数与总大小）；非法包拒绝。
- [x] 安装：包依赖创建实例 + 全部文件哈希校验后原子提交；失败完整回滚；中断重启收敛。
- [x] 更新：按受管清单删除/替换，用户改动文件保留并提示；游戏或加载器不一致拒绝。
- [x] CF overrides 解包拒绝路径穿越；CF 文件经 MCI Mirror 解析且 SHA-1 校验。

## Regression Evals

- [x] M1–M28 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M2/M3 安装管线语义不变（整合包复用同一安装计划与执行器）。
- [x] 960×600 与 200% 放大下导入与更新区无横向溢出。

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

只有解析真实、哈希校验真实执行、失败完整回滚、更新保护用户改动时，才可将本里程碑标记为 validated。执行包内脚本、跳过哈希、或更新覆盖用户改动，均不算通过。

## 2026-07-24 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `modpack_bdd`：8/8 PASS（mrpack 解析预览与非法包拒绝、CF manifest 解析与 MCI Mirror 文件解析、安装哈希校验与原子提交、失败日志补偿回滚、中断重启收敛、更新删除/替换与用户改动保留、游戏/加载器不一致拒绝、CF overrides 路径穿越拒绝与 SHA-1 校验）。
- `cargo test --workspace`：180 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（19/19）、`pnpm build`：PASS。
- Playwright：110/110 PASS，其中 modpack-install 4/4 PASS（导入预览并确认安装、实例卡徽章与更新完成、导入失败可读错误、960×600 与 200% 缩放无横向溢出）。删除未使用常量后按同等配置在 1421 端口复跑该 spec 4/4 通过。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（`MoyuMax_0.1.0-preview.1_x64-setup.exe`，未签名，签名链路见交接 §7）。
- `git diff --check`、`node scripts/generate-sbom.mjs --check`（602 依赖一致）：PASS。
- 范围说明：整合包安装经 `install_modpack` 命令链式复用 M2/M3 安装计划与执行器创建游戏实例后再写入包文件，未改动既有安装管线语义；CF 文件仅经 MCI Mirror 解析，不接 CurseForge 官方源；包内脚本一律不执行（mrpack/CF 格式本身不含可执行脚本入口，导入侧也无任何执行路径）。

状态：validated（范围限于本 eval 所列条目）。
