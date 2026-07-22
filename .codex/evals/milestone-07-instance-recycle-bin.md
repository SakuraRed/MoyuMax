# EVAL DEFINITION：里程碑 7 实例内置回收站

## Capability Evals

- [x] 未运行实例可以移入同一数据位置内的受管回收站，并从活动实例列表隐藏。
- [x] 回收记录包含原位置、回收位置、占用空间、删除时间和精确 30 天保留期限。
- [x] 回收站实例可在原位置未被占用时恢复，存档和关联数据库索引保持。
- [x] 正在运行的实例拒绝回收，源目录和数据库不变化。
- [x] 原位置冲突时拒绝恢复，不覆盖任一侧内容。
- [x] 永久删除只清除回收对象和实例级索引，不清除托管 Java 或共享仓库。
- [x] 中断的回收、恢复或永久删除在重新打开数据库后收敛到单一状态。
- [x] 首页删除确认和数据页恢复闭环使用真实运行时接口，不伪造状态。
- [x] 永久删除确认展示对象数量、占用空间和不可恢复说明。

## Regression Evals

- [x] 当前推荐 Fabric 实例安装和启动行为保持通过。
- [x] Modrinth 内容任务与本地内容索引行为保持通过。
- [x] 崩溃报告、诊断导出和历史会话行为保持通过。
- [x] 托管 Java 去重与删除实例不删除 Java 的既有约束保持通过。
- [x] 960×600 与 200% 放大下无横向溢出、遮挡或文本贴边。

## Deterministic Graders

- `cargo test -p moyumax-core --test recycle_bin_bdd`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `git diff --check`

## Completion Rule

只有真实文件目录、SQLite 索引、托管 Java 保留、桌面命令和声明式数据页共同形成可恢复闭环，且中断与冲突不会覆盖数据时，才可将本里程碑标记为 validated。只从列表隐藏、只改数据库状态或直接调用系统回收站均不算通过。

## Evidence

- `recycle_bin_bdd`：7/7，通过真实临时目录和 SQLite 覆盖回收、恢复、冲突、永久删除、Java 与 Modrinth 索引保留，以及移动、恢复、删除三种中断收敛。
- Rust 全工作区测试：通过；既有安装、启动、内容与崩溃诊断回归保持通过，联网实机探针继续按设计标记为 ignored。
- Clippy：`--workspace --all-targets --all-features -- -D warnings` 通过；`cargo fmt --all -- --check` 通过。
- Playwright：17/17；其中回收站 3/3，覆盖删除确认、数据页恢复、永久删除确认和 960×600 下 200% 放大。
- Svelte：0 错误、0 警告；Vitest 6/6；前端生产构建通过。
- 浏览器视觉复核：`output/playwright/m7-data-recycle.png` 与 `output/playwright/m7-purge-confirm.png`。
- Release：`cargo build --release -p moyumax-desktop` 通过；使用 `MOYUMAX_STATE_DIR` 与 `MOYUMAX_DATA_DIR` 隔离启动后主进程 `Responding=True`，隔离数据库为 `user_version=7` 且包含 `recycle_bin_items`。

结论：Capability 与 Regression 确定性检查全部 PASS，本里程碑状态为 VALIDATED。
