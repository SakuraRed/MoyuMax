# EVAL DEFINITION：里程碑 13 Java 环境管理与删除/恢复闭环

## Capability Evals

- [x] 环境清单显示发行版、完整补丁版本、架构、位置、大小、健康状态与引用实例。
- [x] 删除被引用环境列出受影响实例并要求确认；确认后环境文件移除、墓碑保留；重启收敛中断删除。
- [x] 删除后启动受影响实例明确报错且不自动重装；一键恢复获取同线最新补丁并更新引用后可启动。
- [x] 设为实例环境校验主版本一致；数据库与磁盘运行时清单同步；不一致时启动拒绝。
- [x] 健康验证如实反映环境文件缺失；墓碑身份不被新安装复用。

## Regression Evals

- [x] M1–M12 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M2 Java 自动选择与去重、M4 启动语义不变。
- [x] 960×600 与 200% 放大下环境页与确认对话框无横向溢出、遮挡或文本贴边。

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

只有删除/墓碑/恢复真实作用于文件与数据库引用、启动缺失有明确指引、设环境主版本校验与双端同步成立、且普通安装路径无回归时，才可将本里程碑标记为 validated。静默重装已删构建、删除触碰受管目录外文件、或只在界面陈列而不改变核心状态，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：3/3 PASS。
- `java_environment_bdd`：6/6 PASS（清单含大小/健康/引用、引用删除需确认且墓碑保留引用记录、删除后启动明确报错并指引恢复且不自动装回、中断删除重启收敛、设环境主版本校验与数据库/磁盘双端同步、恢复替换墓碑并重接引用后可启动）。
- `cargo test --workspace`：109 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：41/41 PASS，其中 java-environment 6/6 PASS（列表、删除确认、取消、恢复、指派、960×600 与 200% 几何）。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：设置页骨架当前只承载 Java 环境组，其他设置分组随各自里程碑进入；恢复获取同线最新补丁（精确补丁可能被替代，如实展示恢复后版本）；外部注册的受管目录外环境位置不会被删除清理。

状态：validated（范围限于本 eval 所列条目）。
