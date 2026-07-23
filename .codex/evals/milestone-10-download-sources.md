# EVAL DEFINITION：里程碑 10 下载来源统一与多线程加速

## Capability Evals

- [x] 默认镜像优先：Modrinth/CurseForge 文件路由到 MCI Mirror，Minecraft 文件按 BMCLAPI 文档路由；任务详情记录真实来源。
- [x] 官方优先：Modrinth/Mojang 官方链路直连可用，官方失败时按策略回退镜像并记录；CurseForge 标记官方不可用且不发起直连。
- [x] 自定义源：失败不切换任何来源，任务给出可操作提示。
- [x] 分段下载：支持 Range 的大文件产生不重叠分段请求，清单持久化，合并后完整哈希一致才进入共享存储与原子提交。
- [x] 精准重试与降级：分段损坏只重下该分段；Range 被忽略安全降级单连接；ETag/长度变化废弃旧分段。
- [x] 暂停/重启复用有效分段，恢复先校验清单；与 M9 全局暂停语义一致。
- [x] 双任务共享连接预算时均能持续进展，单文件分段数有界。
- [x] 镜像离线时本地实例、内容、备份、回收站可用。
- [x] 受控限速基准：单连接未占满链路时分段模式中位吞吐 ≥ 单连接 1.8×。

## Regression Evals

- [x] M1–M9 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M3 单连接续传语义不变：无 Range 或小于阈值的文件行为与迁移前一致。
- [x] M5 Modrinth 内容安装真实链路（官方 API）保持默认回退路径可用。
- [x] 960×600 与 200% 放大下任务详情新增字段无横向溢出、遮挡或文本贴边。

## Deterministic Graders

- `cargo test -p moyumax-core --test download_sources_bdd`
- `cargo test -p moyumax-core --test segmented_download_bdd`（含受控限速基准，报告写入 `output/download-bench-latest.json`）
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
- `pnpm --filter @moyumax/desktop lint`
- `pnpm --filter @moyumax/desktop test`
- `pnpm --filter @moyumax/desktop test:e2e`
- `pnpm --filter @moyumax/desktop build`
- `git diff --check`

## Completion Rule

只有来源策略真实持久化并作用于每次下载决策、CurseForge 官方直连永不发起、分段清单可校验恢复、最终哈希校验与原子提交不被绕过、任务详情展示真实来源与降级原因、且基准数据来自受控实测时，才可将本里程碑标记为 validated。把镜像伪装成官方、自定义源静默切换、以单连接冒充分段、或基准来自理论推算，均不算通过。

## 2026-07-23 验证报告

- Capability：9/9 PASS。
- Regression：4/4 PASS。
- `download_sources_bdd`：7/7 PASS（策略持久化、镜像映射 12 组、官方优先顺序、Azul 无镜像单候选、CurseForge 官方不可用且不直连、自定义不切换与不支持的域名、镜像失败回退官方并记录两次尝试、任务来源详情持久化）。
- `segmented_download_bdd`：8/8 PASS（3 段并行不重叠合并、损坏分段精准重下、合并哈希失败不提交、Range 忽略降级单连接、ETag 变化废弃分段、暂停恢复复用分段、双任务公平与有界分段、基准）。
- 基准（写入 `output/download-bench-latest.json`）：受控 Range 源每连接限速 64 KiB/30ms，8 MiB 文件分 8 段，3 轮中位数 单连接 4.41s / 分段 1.50s，吞吐比 2.95×，满足 ≥1.8× 目标。
- `cargo test --workspace`：91 个非忽略测试 PASS；4 个联网或真实 Minecraft 昂贵测试保持 ignored。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：31/31 PASS，其中 download-sources 2/2 PASS（来源详情行与 960×600/200% 几何）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 外部事实核查（2026-07-23）：MCI Mirror `mod.mcimirror.top` Modrinth v2 兼容（search/version 实测）、文件路径 302 至官方 CDN 或镜像缓存且 Range 可用；BMCLAPI 路由表按 `bmclapidoc.bangbang93.com` 当前文档固定（piston-meta/data 同路径、/assets、/maven、/fabric-meta、/quilt-meta 等）。本机对部分 bmclapi2 GET 有 Cloudflare 挑战，真实探针留待发布候选重跑（live 测试保持 ignored）。
- 范围说明：设置页与“来源设置”界面未实现，策略经核心 API 持久化并默认镜像优先；CurseForge 目录能力未接入，仅落实官方不可用语义与 MCI Mirror 路径提示；全局限速与单任务暂停属任务控制里程碑。

状态：validated（范围限于本 eval 所列条目）。
