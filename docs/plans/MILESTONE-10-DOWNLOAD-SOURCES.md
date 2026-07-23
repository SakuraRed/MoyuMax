# 里程碑 10：下载来源统一与多线程加速

## 目标

安装游戏、加载器与模组内容时，MoyuMax 按持久化的来源策略自动路由：默认内置镜像优先（Minecraft 文件走 BMCLAPI，Modrinth/CurseForge 走 MCI Mirror），可切官方源优先或自定义源；大文件在来源支持 HTTP Range 时进行有界多线程分段下载。来源切换、暂停、重启后只重下缺失或损坏的内容，最终文件仍经完整哈希校验与原子提交；任务详情记录真实来源、分段状态与降级原因。

## 已验证的外部事实（2026-07-23 实测）

- MCI Mirror：`api.modrinth.com` → `https://mod.mcimirror.top/modrinth`（v2 兼容，已实测 search/version）；`cdn.modrinth.com` → `https://mod.mcimirror.top`（同路径，302 至官方 CDN 或镜像缓存，Range 可用）；`api.curseforge.com` → `https://mod.mcimirror.top/curseforge`。
- BMCLAPI（按 `https://bmclapidoc.bangbang93.com/` 当前文档）：`launchermeta/launcher/piston-meta/piston-data`（同路径）→ `https://bmclapi2.bangbang93.com`；`resources.download.minecraft.net` → `/assets`；`libraries.minecraft.net` → `/maven`；`meta.fabricmc.net` → `/fabric-meta`；`maven.fabricmc.net` → `/maven`；`meta.quiltmc.org` → `/quilt-meta`；`authlib-injector.yushi.moe` → `/mirrors/authlib-injector`；Mojang java-runtime → `/v1/products/...`。
- Azul Zulu Java 下载无内置镜像，保持官方链路。
- 本机网络对部分 bmclapi2 GET 有 Cloudflare 挑战；真实探针测试标记为 live/ignored，发布候选时重跑。

## 范围

1. 来源策略引擎（核心 `source.rs`）：`mirror_first`（默认）、`official_first`、`custom` 三档持久化策略；按 URL 域名分类映射镜像候选；CurseForge 在 `official_first` 下标记官方不可用且不发起直连；自定义源只允许按策略替换基址，失败不切换任何来源。
2. 下载执行器按策略生成有序候选（镜像→官方 或 官方→镜像），逐个尝试并记录每次尝试的来源、URL 与结果；静默切换只发生在内置镜像与官方源之间；任务详情可查真实来源。
3. 大文件分段下载：单文件 ≥ 8 MiB 且来源支持 Range 时，分为最多 8 个不重叠分段并行下载；分段清单（来源、范围、已完成长度、ETag/Last-Modified）持久化在任务暂存区。
4. 恢复与降级：暂停与重启后复用有效分段并校验；Range 被忽略、分段交叉、总长度或对象证据变化时废弃并行方案，降级为单连接续传；分段损坏只重下该分段；最终合并后执行完整大小与计划哈希校验，通过才允许进入共享存储与原子提交。
5. 与 M9 暂停语义兼容：暂停全部任务在分段边界中断，恢复后继续未完成分段。
6. 任务详情：展示真实来源、分段/单连接模式、有效分段数与降级原因；托盘摘要保持只读精简。
7. 性能对照：本地受控 Range 源限速场景下分段模式与单连接对照至少 3 轮，单连接未占满链路时中位吞吐目标 ≥ 1.8×。

## 非目标

- 不实现设置页与“来源设置”界面；策略通过核心 API 持久化，界面随设置页里程碑交付。
- 不实现 CurseForge 目录搜索与内容安装；仅落实来源策略与官方不可用语义。
- 不实现单任务暂停、优先级、限速与压力感知并发（任务控制里程碑）；分段数只随文件大小与错误信号有界调整。
- 不实现全局限速；分段遵守统一连接预算。
- 不接入 P2P 或局域网缓存。

## 安全不变量

- 不把 MCI Mirror 表述为官方源；CurseForge 官方直连永不发起。
- 自定义源失败禁止静默切换；内置镜像与官方源之间的切换必须记录到任务详情。
- 不降低任何大小、哈希、ZIP 路径与原子提交校验；未通过最终校验的文件不得进入共享存储或实例。
- 分段只写入任务暂存区；不覆盖同名异哈希用户文件；远端对象证据变化时废弃旧分段。
- 镜像或元数据服务失效不得影响本地实例、内容索引、备份、回收站与游戏启动。
- 暂停/重启恢复不自动联网，沿用恢复确认语义。

## 验证

- Rust BDD（`download_sources_bdd.rs`、`segmented_download_bdd.rs`）：镜像路由、官方回退、CurseForge 官方不可用、自定义不切换、分段并行与合并校验、分段损坏精准重试、Range 忽略降级、对象变化废弃、暂停恢复复用分段、双任务公平。
- 回归：M3 单连接续传与 M5 内容安装 BDD 全部保持通过；官方链路 live 探针保持 ignored 可执行。
- Playwright：任务详情显示真实来源与降级原因；镜像离线时本地功能可用。
- 性能：受控限速本地源对照基准，报告中位吞吐倍数。
- 全工作区 Rust、Clippy、格式、Svelte、Vitest、Playwright、生产构建与 NSIS 构建通过。
