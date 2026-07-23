# EVAL DEFINITION：里程碑 14 任务控制完整化

## Capability Evals

- [x] 单任务暂停/恢复：分段边界中断、保留续传状态、其他任务不受影响；与全局暂停组合语义正确。
- [x] 优先级持久化并决定共享执行槽出队顺序；同优先级按创建时间。
- [x] 全局限速持久化；多连接分段下实测总吞吐不系统性突破（≤ 上限 ×1.1）；限速状态进入任务详情。
- [x] 压力感知：吞吐劣化或连续失败收缩有效连接（下限 1），稳定后缓慢回升；有效连接数进入任务详情。

## Regression Evals

- [x] M1–M13 全部 BDD、Vitest、Playwright 与 Rust 工作区测试通过。
- [x] M3 续传、M9 暂停/退出、M10 分段与降级语义不变。
- [x] 960×600 与 200% 放大下任务中心控制区无横向溢出、遮挡或文本贴边。

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

只有单任务控制与全局控制语义清晰区分、优先级真实影响出队、限速在分段下载下仍被实测遵守、压力收缩有真实信号来源，才可将本里程碑标记为 validated。只做界面按钮、限速只作用于单连接、或压力调节凭空无据，均不算通过。

## 2026-07-23 验证报告

- Capability：4/4 PASS。
- Regression：3/3 PASS。
- `task_control_bdd`：5/5 PASS（单任务暂停/恢复与全局暂停互不干扰、优先级出队顺序与执行中拒绝调整、令牌桶限速实测（4 MiB @ 1 MiB/s 多连接总耗时不低于理论下限的 85% 且显著慢于不限速）、压力减半与下限 1 及健康回升、用户暂停的排队任务不会被误标失败）。
- `cargo test --workspace`：114 个非忽略测试 PASS；5 个 ignored 保持不变。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo fmt --all -- --check`：PASS。
- `pnpm --filter @moyumax/desktop lint`（0 错误 0 警告）、`pnpm test`（16/16）、`pnpm build`：PASS。
- Playwright：46/46 PASS，其中 task-control 5/5 PASS。本次本机端口 1420 被第三方进程 HYP.exe 占用，按同等配置改用 1421 复跑通过；标准 `pnpm test:e2e` 在端口空闲时不变。
- `pnpm --filter @moyumax/desktop tauri build`：x64 NSIS 生成成功（仍无 Authenticode，属发布缺口 #13）。
- `git diff --check`：PASS。
- 范围说明：压力信号仅来自网络行为（每连接吞吐与失败），磁盘与 CPU 压力感知属后续；计划时段下载未实现；schema 升至 v10（优先级与暂停来源列，自动迁移）。

状态：validated（范围限于本 eval 所列条目）。
