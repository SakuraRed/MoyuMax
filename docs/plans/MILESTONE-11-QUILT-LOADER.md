# 里程碑 11：Quilt 加载器安装与启动

## 目标

用户新建实例时可选择 Quilt 加载器：系统按游戏版本列出兼容的 Quilt Loader 并推荐稳定版，安装与原子提交流程与 Fabric 完全一致，启动后使用 Quilt 的 mainClass 与参数。Forge/NeoForge 需要安装器处理器执行（installertools/binarypatcher/AutoRenamingTool），属下一里程碑，本里程碑不占位伪装。

## 已验证的外部事实（2026-07-23 实测）

- Quilt 元数据：`https://meta.quiltmc.org/v3/versions/loader/{game}` 返回 `[{loader:{version,maven,hashes}}]`；`https://meta.quiltmc.org/v3/versions/loader/{game}/{loader}/profile/json` 返回与 Fabric 同构的 profile（`id`、`mainClass`、`libraries[{name,url}]`、`arguments`）。
- Quilt 版本稳定判定：版本号不含 `-`（如 `0.30.0`）视为稳定，推荐第一个稳定项；含 `beta` 等后缀不标记推荐。
- BMCLAPI 路由（M10 已固定）：`meta.quiltmc.org` → `/quilt-meta`，`maven.quiltmc.org/repository/release` → `/maven`。
- Forge/NeoForge 安装器为 spec-1 install_profile：需执行客户端处理器（已调研确认，见交接 M12 节），不在本里程碑范围。

## 范围

1. `LoaderChoice`/`ResolvedLoader` 新增 Quilt 变体；`MetadataClient` 支持 Quilt 版本列表与 profile 解析（SHA-256 校验）。
2. 安装执行复用 M3 下载/校验/原子提交与 M10 来源路由；Quilt 库进入统一 classpath。
3. 启动复用 M4 参数展开：Quilt profile 的 `mainClass`、`id` 与 `arguments.jvm/game`。
4. 实例列表与首页展示 `quilt` 加载器；新建实例页开放 Quilt 选项与版本下拉。
5. 数据库 loader_kind 增加 `quilt` 映射，无需 schema 迁移。

## 非目标

- 不实现 Forge/NeoForge 安装器、处理器执行或二进制补丁（M12）。
- 不实现 Quilt 的整合包格式或插件系统；不接入 Quilt 专属元数据能力（如模组目录）。
- 不改变 Fabric/Vanilla 现有行为。

## 安全不变量

- Quilt profile 与库文件必须通过 SHA-256/SHA-1 校验；缺失校验值按既有规则报错。
- 未知启动占位符继续拒绝启动（launch.rs 既有语义）。
- 元数据失效不得影响本地实例与启动。

## 验证

- Rust BDD：Quilt 列表解析与稳定推荐、profile SHA 校验、非兼容版本拒绝、安装执行到可启动实例、启动参数含 Quilt mainClass 与 loader 参数。
- 回归：M2/M3/M4/M10 相关 BDD 全部通过。
- Playwright：新建实例页选择 Quilt 并完成安装预览与任务进入队列；960×600 与 200% 缩放。
- 全工作区 Rust、Clippy、格式、Svelte、Vitest、Playwright、生产构建与 NSIS 构建通过。
