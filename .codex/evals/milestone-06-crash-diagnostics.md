# EVAL DEFINITION：里程碑 6 本地崩溃诊断

## Capability Evals

- [x] 失败与中断会话生成报告，正常退出和主动停止不生成报告。
- [x] 同一启动会话最多存在一份崩溃报告，重启恢复保持幂等。
- [x] 报告发现 stdout、stderr、`latest.log`、`debug.log`、Minecraft 崩溃报告、原生崩溃文本、启动器日志和脱敏启动脚本。
- [x] 本地规则能区分内存不足、模组冲突、Java 或原生崩溃、启动器中断和未知原因。
- [x] 公开 `report.json` 不包含原始证据路径或敏感值。
- [x] 导出预览在写文件前列出包内文件、大小限制和脱敏类别。
- [x] ZIP 使用受管暂存文件原子发布，失败不留下伪完成产物。
- [x] ZIP 内容脱敏玩家名、账户标识、用户目录、服务器地址、令牌、密码和 Authorization。
- [x] Tauri 不暴露绕过预览直接导出的 GUI 命令，也不存在上传命令。
- [x] 首页可进入声明式崩溃页，显示摘要、证据、建议、隐私说明和本地导出结果。
- [x] 960×600 与 200% 放大下无横向溢出、文本贴边或操作遮挡。

## Regression Evals

- [x] 现有启动、安装、内容任务和首次运行 BDD 全部通过。
- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo test --workspace` 通过。
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm lint`、`pnpm test`、`pnpm test:e2e` 和 `pnpm build` 通过。
- [x] Tauri Release 构建和主进程响应验证通过。

## Human Review

- [x] 导出预览文案明确说明“仅本地导出、不会自动上传”。
- [x] 崩溃页优先呈现通俗摘要，技术证据保持渐进披露。
- [x] 页面不声称已确定未知原因，也不诱导用户执行破坏性操作。

## 成功标准

只有异常退出能够自动形成持久、幂等、可解释的本地报告，用户在确认前能看到完整导出清单和脱敏范围，确认后生成的 ZIP 经自动检查不含测试注入的敏感值，同时现有启动闭环无回归，才可称“崩溃诊断基础功能可用”。

## EVAL REPORT：2026-07-23

- Capability：11/11 PASS。
- Regression：6/6 PASS。
- Human review：3/3 PASS。
- 核心 BDD：`crash_diagnostics_bdd` 5/5，启动生命周期 14/14。
- 前端 BDD：Playwright 13/13，其中崩溃诊断 2/2。
- 脱敏验证：自动解包 ZIP 并扫描玩家名、账户 UUID、用户或实例目录、域名端口、IP、令牌和密码，均未发现原值。
- 真实启动：保留实例副本进入 Minecraft 与 native 初始化后主动停止，会话为 `stopped`，无对应崩溃报告，脱敏启动脚本不含玩家名或实例路径。
- 发布构建：Tauri Release 构建通过，主进程启动后 `Responding=True`。
