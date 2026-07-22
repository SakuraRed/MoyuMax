# EVAL DEFINITION：里程碑 5 Modrinth 模组安装

## Capability Evals

- [x] 搜索请求只返回目标实例兼容的客户端模组并保留项目 ID。
- [x] 兼容版本选择优先 listed release，绝不自动选择 sources JAR。
- [x] required 依赖自动纳入，optional 依赖默认不安装，incompatible 冲突阻断。
- [x] 指定版本依赖再次核对兼容性，项目依赖选择最新兼容版本。
- [x] 依赖循环与重复项目不会造成重复下载或无限递归。
- [x] 文件经过大小、SHA-1 和 SHA-512 校验。
- [x] 同名异哈希冲突不覆盖原文件。
- [x] 多文件发布或数据库写入失败时全部补偿回滚。
- [x] Tauri 内容任务与游戏安装任务共享同一后台并发槽。
- [x] UI 在确认前显示目标、必需、可选和冲突项目，并保持文本框体安全内边距。
- [x] 外网不可用时本地已安装内容列表和游戏启动不受影响。

## Regression Evals

- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo test --workspace` 通过。
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm lint`、`pnpm test`、`pnpm test:e2e`、`pnpm build` 通过。
- [x] 当前推荐版真实安装与真实启动探针继续通过。

## 真实链路

- [x] 官方搜索与版本接口返回 Minecraft 26.2 Fabric 兼容的 Continuity。
- [x] 计划自动解析 required Fabric API，且没有默认加入 optional 内容。
- [x] Continuity 与 Fabric API 文件通过大小、SHA-1 和 SHA-512 校验。
- [x] 两个文件原子进入保留实例并写入 `installed_content`。
- [x] 游戏启动日志证明 Fabric Loader 发现目标模组和依赖。
- [x] 测试完成后可从快照恢复保留实例，不污染里程碑 4 基线。

## 成功标准

只有用户选择一个 Modrinth 模组后，系统能从官方元数据解析全部必需依赖、展示确认、通过统一任务队列原子安装并在真实游戏启动中被 Fabric Loader 发现，才可称“模组安装可用”。只展示搜索结果、只下载单个 JAR 或手工复制依赖均不算通过。

## EVAL REPORT：2026-07-23

- Capability：11/11 PASS。
- Regression：5/5 PASS。
- 真实链路：6/6 PASS。
- 核心 BDD：`modrinth_content_bdd` 8/8，`content_install_executor_bdd` 5/5。
- 前端 BDD：Playwright 11/11，其中资源页 3/3，并覆盖 960×600、200% 放大。
- 真实命令：`cargo test -p moyumax-core --test live_content -- --ignored --nocapture`，动态搜索并解析 Continuity，安装 required Fabric API，在临时实例副本中启动后由 Fabric Loader 同时发现二者。
- 基线保护：真实测试只复制 `output/live-install/.tmp9GXxdd` 的数据库与实例目录；测试结束后副本自动删除，基线 `mods` 目录保持为空。
- 发布构建：`cargo build --release -p moyumax-desktop` 通过，Release 主进程启动后 `Responding=True`。
