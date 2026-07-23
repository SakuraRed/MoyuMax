# EVAL DEFINITION：里程碑 11 Quilt 加载器安装与启动

## Capability Evals

- [x] Quilt 版本列表按游戏版本过滤，最新稳定版标记推荐；含 `-` 后缀版本不标记推荐。
- [x] 不在兼容列表中的 Quilt 版本被拒绝，不静默下载。
- [x] Quilt profile 经 SHA-256 校验入库；库文件经既有 SHA-1/大小校验后原子提交。
- [x] 启动参数使用 Quilt mainClass 与 loader 参数；实例加载器显示为 quilt。
- [x] 元数据失效不影响本地实例与启动。

## Regression Evals

- [x] M1–M10 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] Fabric 与 Vanilla 安装启动行为不变。
- [x] 960×600 与 200% 放大下新建实例页加载器区无横向溢出、遮挡或文本贴边。

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

只有 Quilt 实例能通过真实安装任务原子提交并以 Quilt mainClass 与参数启动、且 Fabric/Vanilla 回归全绿时，才可将本里程碑标记为 validated。仅在界面添加选项、伪造 profile、或复用 Fabric profile 冒充 Quilt，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：3/3 PASS。
- `quilt_loader_bdd`：5/5 PASS（稳定推荐标记、profile SHA-256 校验、不兼容版本拒绝且只查一次列表、安装原子提交 loader_kind=quilt 且 classpath 含 Quilt 库、启动参数含 Quilt mainClass 与 loader JVM 参数及 version_name）。
- `cargo test --workspace`：96 个非忽略测试 PASS；4 个联网或真实 Minecraft 昂贵测试保持 ignored。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：33/33 PASS，其中 quilt-loader 2/2 PASS（选择 Quilt、版本下拉、预览、任务入队、960×600 与 200% 几何）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 外部事实核查（2026-07-23 实测）：`meta.quiltmc.org/v3/versions/loader/{game}` 与 profile/json 端点可用且结构与 Fabric profile 同构（mainClass `org.quiltmc.loader.impl.launch.knot.KnotClient`）；Quilt 无官方稳定标记，按“版本号不含 `-`”判定稳定。
- 范围说明：Forge/NeoForge 需 installertools/binarypatcher/AutoRenamingTool 处理器执行，已调研确认 spec-1 install_profile 结构，属 M12；Quilt 实例的 Modrinth 模组安装仍沿用 M5 的 Fabric 限制，内容策略属后续里程碑。

状态：validated（范围限于本 eval 所列条目）。
