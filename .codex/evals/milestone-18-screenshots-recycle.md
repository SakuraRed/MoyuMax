# EVAL DEFINITION：里程碑 18 截图管理与删除回收站统一

## Capability Evals

- [x] 截图清单与筛选（全部/本周）与文件系统一致；复制写入系统剪贴板；打开本地位置可用。
- [x] 截图/资源/世界删除全部进入回收站事务模型，与实例共用补偿与重启收敛。
- [x] 资源删除与恢复不丢索引行；停用资源以 `.disabled` 实际文件名正确往返。
- [x] 恢复在原位置被占用时拒绝且不覆盖；永久删除对所有类型开放且需逐项确认。

## Regression Evals

- [x] M1–M17 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M7 实例回收/恢复/清理语义不变。
- [x] 960×600 与 200% 放大下截图区与删除确认无横向溢出。

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

只有三类删除与实例共用同一回收站事务路径、恢复不覆盖、资源索引无损往返、截图复制真实写剪贴板时，才可将本里程碑标记为 validated。各写一套删除逻辑、恢复可覆盖、或复制只是复制文件路径，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `screenshots_recycle_bdd`：7/7 PASS（截图清单与文件系统一致、截图删除/恢复往返、占用拒绝不覆盖、停用资源删除后索引与 `.disabled` 文件名无损往返、世界删除/恢复、全类型永久删除、中断移动重启收敛）。
- `cargo test --workspace`：143 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：68/68 PASS，其中 screenshots-recycle 6/6 PASS（清单与筛选、复制、截图/资源/世界删除恢复、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：截图复制经 tauri-plugin-clipboard-manager 2.3.2（图片解码走 `Image.fromBytes`）；截图区不提供缩略图（资产协议与懒加载缩略图随后续）；按世界筛选截图不可行（文件名无世界信息）；schema 升至 v13（回收站重建去除 subject_id 唯一约束并新增 payload 列，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
