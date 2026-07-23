# EVAL DEFINITION：里程碑 12 Forge 与 NeoForge 安装器处理器执行

## Capability Evals

- [x] Forge/NeoForge 版本列表按游戏版本过滤并推荐最新构建；安装器下载入暂存区。
- [x] spec-1 install_profile 解析：客户端处理器按序执行，server 侧跳过，spec 不符明确报错。
- [x] Maven 坐标（含 classifier 与 @ext）解析与下载；占位符白名单展开，未知占位符报错。
- [x] 处理器产物经 `_SHA`（提供时）与存在性校验后进入共享存储；失败回滚不污染实例与共享存储。
- [x] PATCHED 客户端 JAR 进入 classpath；Forge/NeoForge mainClass 与参数启动。
- [x] NeoForge `${library_directory}`、`${classpath_separator}` 正确展开。
- [x] 生产处理器运行器使用托管 Java；真实处理器全链路有 live 探针（ignored）。

## Regression Evals

- [x] M1–M11 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] Fabric/Quilt/Vanilla 安装启动行为不变。
- [x] 960×600 与 200% 放大下新建实例页与任务详情无横向溢出、遮挡或文本贴边。

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

只有 Forge 与 NeoForge 安装任务真实执行处理器链、产物经校验后原子提交、实例以各自 mainClass 与参数启动、且假运行器测试与真实探针边界清楚区分时，才可将本里程碑标记为 validated。不执行处理器直接搬运安装器内容、伪造 PATCHED、跳过 `_SHA` 校验、或以 Fabric 流程冒充，均不算通过。

## 2026-07-23 验证报告

- Capability：7/7 PASS。
- Regression：3/3 PASS。
- `forge_neoforge_bdd`：7/7 PASS（Maven 坐标解析含 classifier/@ext、spec 0 拒绝、Forge 处理器链端到端（客户端 2 个处理器、server 侧跳过、占位符全展开、PATCHED 经 SHA-1 校验进入共享存储与 classpath）、PATCHED SHA 不匹配回滚且不污染共享存储、NeoForge 六处理器链与启动模块占位符展开、BMCLAPI Forge/NeoForge 列表推荐与解析）。
- `cargo test --workspace`：103 个非忽略测试 PASS；5 个 ignored（含新增 `live_forge_install` 真实处理器探针，发布候选前执行）。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：35/35 PASS，其中 forge-neoforge 2/2 PASS（双加载器选择、版本下拉、预览入队、960×600 与 200% 几何）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 外部事实核查（2026-07-23 实测安装器）：Forge `1.21.8-58.1.20` 与 NeoForge `21.8.54` 均为 spec-1 install_profile；Forge 客户端链为 DOWNLOAD_MOJMAPS/FART/binarypatcher，NeoForge 为 MCP_DATA/DOWNLOAD_MOJMAPS/MERGE_MAPPING/jarsplitter/AutoRenamingTool/binarypatcher；BINPATCH 内嵌于安装器 data/；Forge `:client` 库条目 URL 为空，由 PATCHED 产出；NeoForge 启动需 `${library_directory}` 与 `${classpath_separator}` 展开。
- 范围说明：spec 0/更老格式明确报错；服务端处理器不执行；处理器运行器生产为托管 Java 子进程，BDD 使用确定性假运行器，真实处理器全链路为 ignored live 探针；Modrinth 模组安装仍限 Fabric 实例。

状态：validated（范围限于本 eval 所列条目），真实处理器探针 `live_forge_install` 留待发布候选执行。


