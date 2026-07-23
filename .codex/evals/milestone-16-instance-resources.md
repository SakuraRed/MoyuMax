# EVAL DEFINITION：里程碑 16 实例资源内容管理（资源包/光影/数据包）

## Capability Evals

- [x] 三种资源类型本地导入：原子进入正确目标目录，清单可查出启用状态。
- [x] 同名文件拒绝导入且不覆盖；索引写入失败时不留半成品文件与索引行。
- [x] 启用/停用通过 `.disabled` 后缀切换，文件名与索引状态一致，rename 失败有补偿。
- [x] 数据包必须选择世界，只进入所选世界的 datapacks 目录。
- [x] 实例隔离：资源文件与索引只落在目标实例。

## Regression Evals

- [x] M1–M15 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M15 模组清单与更新流程在资源中心共存不回归。
- [x] 960×600 与 200% 放大下资源内容区无横向溢出、遮挡或文本贴边。

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

只有导入具备真实原子提交与失败补偿、启停状态与文件系统一致、数据包世界选择强制、实例隔离成立时，才可将本里程碑标记为 validated。只做界面开关不改文件、以覆盖代替拒绝、或索引与文件状态可漂移，均不算通过。

## 2026-07-23 验证报告

- Capability：5/5 PASS。
- Regression：3/3 PASS。
- `instance_resources_bdd`：7/7 PASS（资源包导入原子落位与索引、同名拒绝且不覆盖、索引失败移除已落位文件、启用/停用后缀切换与索引失败补偿回滚、数据包强制选择世界且只进入所选世界、两实例完全隔离、类型过滤与文件名/来源校验）。
- `cargo test --workspace`：128 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：57/57 PASS，其中 instance-resources 6/6 PASS（导入清单、同名拒绝、启停切换持久化、数据包世界选择、无世界阻止、960×600 与 200% 缩放）。本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：资源内容删除与回收站扩展、Modrinth 资源包在线搜索未实现（随后续里程碑）；导入经 tauri-plugin-dialog 2.7.2 原生选择器，浏览器 mock 以预置路径代替；不修改游戏 options.txt，游戏内选用由用户确认；schema 升至 v12（instance_resources 表，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
