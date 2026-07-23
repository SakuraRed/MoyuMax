# EVAL DEFINITION：里程碑 17 世界存档管理

## Capability Evals

- [x] 按实例世界清单：名称、占用、最近游玩时间与文件系统一致。
- [x] 世界导出 ZIP 完整可读，可再导入到另一个实例（同名拒绝不覆盖）。
- [x] 回滚先创建恢复点备份，saves 恢复到备份状态；恢复点失败则中止且 saves 不变。
- [x] 解压校验拒绝路径穿越；导入识别两种布局。
- [x] 回滚交换中断后重启收敛，saves 完整可用。

## Regression Evals

- [x] M1–M16 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M8 备份创建、保留策略与中断恢复语义不变。
- [x] 960×600 与 200% 放大下世界存档区与回滚对话框无横向溢出。

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

只有回滚具备真实恢复点与可验证补偿、解压校验真实生效、导入不覆盖、中断收敛不丢目录时，才可将本里程碑标记为 validated。只替换目录不留恢复点、校验可被穿越绕过、或中断后需要手工清理，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：3/3 PASS。
- `worlds_management_bdd`：8/8 PASS（世界清单与文件系统一致、导出可再导入另一实例、同名拒绝不覆盖、回滚恢复 saves 且恢复点捕获回滚前状态并清理旧目录、恢复点失败中止且 saves 不变、交换中断重启恢复原 saves、路径穿越条目拒绝、根级 level.dat 布局导入）。
- `cargo test --workspace`：136 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：62/62 PASS，其中 world-management 5/5 PASS（清单、导出、导入与同名拒绝、回滚确认对话框与恢复点、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：回滚单位为整个 saves（与 M8 备份粒度一致），按世界粒度回滚未实现；世界删除与回收站扩展随 M18 统一设计；定时增量备份属缺口 #8 后续；schema 保持 v12 不变。

状态：validated（范围限于本 eval 所列条目）。
