# 里程碑 12：Forge 与 NeoForge 安装器处理器执行

## 目标

用户新建实例时可选择 Forge 或 NeoForge：系统按游戏版本列出兼容构建，下载官方安装器，按 spec-1 `install_profile.json` 依次执行客户端处理器（映射合并、改名、二进制补丁），产出带校验的客户端 JAR，原子提交实例并以加载器的 mainClass 与参数启动。处理器用托管 Java 执行，全部产物先写暂存区、校验后再进入共享存储。

## 已验证的外部事实（2026-07-23 实测安装器）

- Forge `1.21.8-58.1.20`：`install_profile.json` spec 1；`data.BINPATCH=/data/client.lzma`（内嵌）；`PATCHED=[net.minecraftforge:forge:1.21.8-58.1.20:client]` 带 `PATCHED_SHA`；处理器为 installertools、ForgeAutoRenamingTool（客户端）、binarypatcher；`version.json` mainClass `net.minecraftforge.bootstrap.ForgeBootstrap`，`net.minecraftforge:forge:...:client` 库条目 URL 为空（由处理器产出）。
- NeoForge `21.8.54`：`install_profile.json` spec 1；BINPATCH 内嵌于 `data/client.lzma`；客户端处理器链 MCP_DATA、DOWNLOAD_MOJMAPS、MERGE_MAPPING、jarsplitter、AutoRenamingTool、binarypatcher；`version.json` mainClass `cpw.mods.bootstraplauncher.BootstrapLauncher`，JVM 参数含 `${library_directory}`、`${classpath_separator}` 模块路径占位符。
- 版本列表：BMCLAPI `/forge/minecraft/:id`、`/neoforge/list/:mcversion` 实测可用；安装器经 Maven 或 BMCLAPI 302 下载。
- 数据值形态：`[group:artifact:version[:classifier][@ext]]` Maven 引用、`/data/...` 安装器内路径、工作目录相对路径、`_SHA` 十六进制校验。
- 处理器只执行 `sides` 含 `client` 或无 `sides` 的条目，顺序与 profile 一致。

## 范围

1. 加载器目录：Forge/NeoForge 版本列表（BMCLAPI 为主，官方 Maven 元数据为回退），推荐最新构建。
2. 安装器事务：安装器 JAR 下载入暂存区，解包 `install_profile.json` 与 `version.json`，下载 version.json 游戏库与处理器库（沿用 M3/M10 下载与来源路由）。
3. 处理器引擎：Maven 坐标解析、占位符展开、处理器计划（Main-Class 读取自 JAR manifest）、按序执行、`_SHA` 与存在性校验；server 侧处理器跳过。
4. 运行时清单：PATCHED JAR 进入共享存储并加入 classpath；mainClass/arguments 取自 `version.json`；NeoForge 的 `${library_directory}` 与 `${classpath_separator}` 启动占位符展开。
5. 新建实例页开放 Forge/NeoForge 选项与版本下拉；实例显示加载器。
6. 处理器运行器可注入：生产用托管 Java 子进程，测试用确定性假运行器；真实处理器全链路探针保持 live/ignored。

## 非目标

- 不执行服务器侧处理器，不支持服务端安装。
- 不实现 Forge/NeoForge 模组目录扩展；Modrinth 模组仍限 Fabric 实例（内容策略属后续里程碑）。
- 不支持 spec 0 或更老的 install_profile 格式，遇到即明确报错。
- 不实现处理器下载内容绕过校验；任何产物不带 `_SHA` 时只校验存在性与非空并在任务详情如实记录。

## 安全不变量

- 任何处理器产物在写入共享存储前必须通过 `_SHA`（提供时）与存在性校验；校验失败回滚暂存，实例与共享存储保持原状。
- 占位符白名单展开；未知占位符直接报错（沿用 launch.rs 语义）。
- 处理器子进程只使用托管 Java；不执行安装器内的任意脚本入口，只执行 profile 声明的处理器 JAR。
- 失败事务不产生半成品实例；安装器、处理器库与产物只写任务暂存区与受管共享存储。

## 验证

- Rust BDD：Maven 坐标解析、占位符展开、spec 版本拒绝、处理器顺序与 server 跳过、假运行器端到端（Forge 与 NeoForge 双链路）、`_SHA` 校验失败回滚、NeoForge 启动占位符展开、PATCHED 进入 classpath 且可启动。
- 真实探针（live/ignored）：真实 Forge 或 NeoForge 客户端安装到可启动实例，发布候选前重跑。
- Playwright：新建实例页 Forge/NeoForge 选择与版本下拉、预览与任务入队、960×600 与 200% 缩放。
- 全工作区 Rust、Clippy、格式、Svelte、Vitest、Playwright、生产构建与 NSIS 构建通过。
