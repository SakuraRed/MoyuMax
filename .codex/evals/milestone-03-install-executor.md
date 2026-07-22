# EVAL DEFINITION：里程碑 3 真实安装执行器

## Capability Evals

- [x] 支持 Range 的来源从现有 `.part` 长度续传。
- [x] 忽略 Range 的来源触发单文件清理并完整重下，不拼接内容。
- [x] 大小、SHA-1 或 SHA-256 不匹配时任务失败且不发布实例。
- [x] 已校验的共享文件不重复下载。
- [x] Mojang 资源索引展开为哈希寻址资源对象并全局去重。
- [x] Windows x64 原生库按规则选择并安全解包。
- [x] Fabric Maven 库与 Profile 被纳入同一实例运行时清单。
- [x] Azul JDK 安全解包，路径穿越、链接和异常展开量被拒绝。
- [x] 实例目录与数据库 `ready` 状态补偿式原子提交。
- [x] 运行中任务异常退出后等待恢复；确认后沿用原计划继续。
- [x] 任务中心显示真实阶段、已完成字节、总量或未知总量状态。

## Regression Evals

- [x] M1 4/4、M2 6/6 Rust BDD 场景继续通过。
- [x] 现有 6 个 Playwright 场景继续通过。
- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo test --workspace` 通过。
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm lint`、`pnpm test`、`pnpm test:e2e`、`pnpm build` 通过。
- [x] Tauri Release 构建与真实启动通过。

## 真实链路

- [x] 使用当前 Mojang、Fabric、Azul 元数据生成计划。
- [x] 安装当前推荐稳定版与推荐 Fabric 到全新数据目录。
- [ ] 中断至少一个大文件后继续，最终哈希一致。
- [x] 安装完成后实例记录为可启动且暂存区按策略清理。

## 2026-07-23 执行记录

- 当前 Mojang 推荐稳定版为 26.2，安装推荐 Fabric 与 Azul Zulu 25.0.4+7。
- 首次实机失败发现 Azul API `size=229857000`，CDN `Content-Length=229856681`，但文件 SHA-256 与 API 完全一致；安装快照现以 CDN 当前长度校准并继续强制 SHA-256。
- 第二次实机探针发现 Mojang 26.2 将原生库改为独立 Maven 分类器。进一步核对官方 JVM 参数后确认新版 x64 native JAR 必须保留在 classpath 供 LWJGL/JNA 自解包；执行器只排除 x86 与 ARM64，旧式 `natives/classifiers` 仍安全预解包。
- 最终完整实机测试通过，托管 `java.exe -version` 成功，任务暂存区已清理，符合 26.2 官方语义的成功实例保留在 `output/live-install/.tmp9GXxdd` 供启动链验证。
- 外部大文件的人为中断续传尚未执行；确定性 Range、忽略 Range、错误哈希与不覆盖共享文件回归均已通过。

## 成功标准

只有真实文件落盘、校验、环境部署和原子发布均有证据时，才能称“安装已实现”。仅生成计划或播放进度动画不算通过。
