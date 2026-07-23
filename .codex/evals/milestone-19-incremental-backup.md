# EVAL DEFINITION：里程碑 19 运行期间增量备份与世界历史

## Capability Evals

- [x] 定时增量备份只含变化/新增文件并记录删除清单；无基准索引时回退全量。
- [x] 回滚到增量点与当时 saves 逐文件一致，删除的文件不重现。
- [x] 保留数量可配置；清理级联链段，恢复链不悬空。
- [x] 间隔配置持久化；会话结束后调度不再产生备份。

## Regression Evals

- [x] M1–M18 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M8 全量备份、M17 回滚恢复点语义不变。
- [x] 960×600 与 200% 放大下备份设置与时间线无横向溢出。

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

只有增量内容真实按差异裁剪、链式回滚逐文件一致、清理不悬空、调度随会话结束停止时，才可将本里程碑标记为 validated。增量实为全量改名、回滚忽略删除清单、或调度脱离会话生命周期，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `incremental_backup_bdd`：7/7 PASS（增量只含变化与新增并记录删除清单、无基准回退全量、链式回滚逐文件一致且删除不重现/后续新增不提前、无变化不产生备份、保留清理不悬空且归档文件删除、间隔与保留配置持久化与边界、间隔为 0 调度立即退出）。
- `cargo test --workspace`：150 个非忽略测试 PASS（连续 4 轮全量复跑均 PASS）；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：72/72 PASS，其中 incremental-backup 4/4 PASS（类型徽章与定时标签、设置持久化与边界、零间隔关闭、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：差异检测按文件大小 + 纳秒级修改时间（与 rsync 同语义），同尺寸同时间刻的极端改写可能漏检，不引入逐文件哈希权衡；定时备份 launch_session_id 存 NULL 以保留会话触发唯一槽；基准选择按 rowid 插入序；调度循环只做会话生命周期内的尽力而为备份，启动器退出后不补做；schema 升至 v14（world_backups 增加 kind 与 base_backup_id，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
