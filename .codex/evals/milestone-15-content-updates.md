# EVAL DEFINITION：里程碑 15 Modrinth 全加载器与按实例更新策略

## Capability Evals

- [x] Quilt/Forge/NeoForge 实例可安装 Modrinth 模组与必需依赖（原子事务，失败回滚）。
- [x] 默认关闭时更新检查只提示不下载，不产生任务或修改。
- [x] 更新替换先备份旧文件，失败完整回滚；中断任务重启后自动恢复旧文件再进入恢复确认。
- [x] 按实例自动更新开关持久化；开启后更新仍需用户明确触发。

## Regression Evals

- [x] M1–M14 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M5 Fabric 安装与依赖语义、M3/M5 中断恢复语义不变。
- [x] 960×600 与 200% 放大下更新区无横向溢出、遮挡或文本贴边。

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

只有全加载器安装真实生效、更新替换带真实恢复点与回滚、中断收敛正确、默认关闭语义不破坏时，才可将本里程碑标记为 validated。只做界面提示、更新不留恢复点、或以删除代替回滚，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `content_updates_bdd`：7/7 PASS（Quilt/Forge/NeoForge 实例接受 Modrinth 计划且不支持加载器仍被拒绝、更新检查只读元数据不产生任务不修改文件、更新原子替换并保留逐项启用/自动更新标志且成功后清理恢复点、索引写入失败从恢复点完整放回旧文件且不留半成品、提交阶段中断重启后先放回旧文件再进入恢复确认并可放弃、按实例开关持久化与全部更新单任务及空选择/未安装拒绝）。
- `cargo test --workspace`：121 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：51/51 PASS，其中 content-updates 5/5 PASS（更新清单、逐项更新、全部更新入口、空更新提示、全加载器实例选择、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：更新恢复点是事务级快照（`.moyumax/snapshots/content-update-{task}/`），成功或回滚后即清理，不做长期版本留存；按内容锁定/忽略更新沿用既有字段未接入界面；真实 Modrinth 更新探针留待发布候选执行；schema 升至 v11（实例内容自动更新开关列，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
