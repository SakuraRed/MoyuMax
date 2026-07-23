# EVAL DEFINITION：里程碑 8 世界存档备份

## Capability Evals

- [x] 含世界的实例在进程创建前产生启动前原子 ZIP，内容与源文件一致。
- [x] 游戏正常退出、异常退出和用户停止后产生退出后原子 ZIP。
- [x] 启动会话关联并展示启动前与退出后备份状态。
- [x] 没有世界时记录 skipped，且不创建空归档或阻止启动。
- [x] 启动前备份失败会阻止进程创建并记录失败原因。
- [x] 启动器中断后为会话补写退出后备份，不产生重复记录。
- [x] 中断的临时归档在重启后清理并标记失败。
- [x] 每个实例只保留最近 20 个成功归档，清理不越过受管备份根。
- [x] 数据页显示本地备份时间线、世界数量、触发原因、状态和占用空间。

## Regression Evals

- [x] 安装、Modrinth 内容、启动、崩溃诊断和实例回收站 BDD 全部通过。
- [x] 托管 Java、共享基础文件、回收站和账户数据不进入备份或清理范围。
- [x] 游戏启动与停止仍使用数据库索引的受管 Java 和共享存储。
- [x] 960×600 和 200% 放大下备份时间线无横向溢出、遮挡或文本贴边。

## Deterministic Graders

- `cargo test -p moyumax-core --test world_backup_bdd`
- `cargo test -p moyumax-core --test launch_planning_bdd`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `git diff --check`

## Completion Rule

只有真实世界文件进入原子归档、会话前后状态可追踪、失败和中断不伪装成功、默认保留策略不越界，并且声明式界面能展示真实历史时，才可将本里程碑标记为 validated。只复制目录、只显示“已备份”文本或仅在浏览器内模拟均不算通过。

## 2026-07-23 验证报告

- Capability：9/9 PASS。
- Regression：4/4 PASS。
- `world_backup_bdd`：4/4 PASS。
- `launch_planning_bdd`：15/15 PASS。
- Vitest：6/6 PASS。
- Playwright：19/19 PASS，其中世界备份 2/2 PASS。
- `cargo test --workspace`：离线自动化测试全部 PASS；4 个需要联网或真实 Minecraft 的昂贵测试按声明保持 ignored，不计入本次实时验证。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：PASS。
- `cargo fmt --all -- --check`、Svelte 0 错误/0 警告、生产 Web 构建、`git diff --check`：PASS。
- 人工视觉检查：1365×768 截图对照 `D:/Downloads/MoyuMax-mockups`，卡片、行和标签均保留安全内边距；自动几何检查覆盖 960×600 与 200% 放大。

状态：validated，等待用户对 Windows 开发预览安装包进行人工审查。
