# MoyuMax 项目协作规则

## 语言与证据

- 与用户沟通及项目文档默认使用简体中文，不使用表情或类表情符号。
- `implemented`、`validated`、`completed` 必须严格区分。
- 完成声明必须附文件、命令、测试、运行或发布证据。

## 架构边界

- `crates/moyumax-core` 是不依赖 Tauri 的本地领域与应用核心。
- `apps/desktop/src-tauri` 只负责桌面生命周期、系统集成和命令适配。
- `apps/desktop/src` 只通过版本化运行时接口访问核心，不直接写数据库或受管目录。
- GUI、CLI 和后台任务最终必须复用同一核心事务与状态模型。

## 开发流程

- 行为变更先在 `features/` 写 BDD 示例，再写失败测试，然后实现最小代码。
- Rust 执行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings` 和 `cargo test --workspace`。
- 前端执行 `pnpm lint`、`pnpm test` 和 `pnpm build`。
- 视觉变更必须对照 `D:/Downloads/MoyuMax-mockups`，并在浏览器或桌面应用中验证。
- 依赖使用精确版本；Node 包管理器固定为 `pnpm@10.26.2`，提交 lockfile。

## 数据与安全

- 受管数据写入必须具备事务、原子提交或可验证回滚路径。
- 默认离线、默认实例隔离、默认不上报诊断、默认不开放公网监听。
- 不记录令牌、密码或未脱敏启动命令。
- 禁止任意脚本式整合包、远程主题资源和启动器擅自安装模组。

## Git

- 提交身份为 `SakuraRed <86900315+SakuraRed@users.noreply.github.com>`。
- 提交必须使用 SSH 签名并包含 DCO `Signed-off-by`。
- 不提交构建产物、运行数据、数据库、凭据或浏览器临时文件。
