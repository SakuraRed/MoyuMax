# 里程碑 24：内置 CLI（开发者模式）

## 目标

提供与 GUI 共用同一核心事务的内置命令行入口；CLI 默认关闭，在开发者设置中显式开启并提示风险；写操作支持 dry-run、版本化 JSON 输出与稳定退出码。

## 范围

1. CLI 入口：`moyumax-desktop.exe --cli <command>` 直接进入无窗口 CLI 模式（不启动 Tauri、不建托盘），Windows 下附加父控制台输出。
2. 命令集（v1）：
   - `instances list`：实例清单（读）。
   - `tasks list`：安装与内容任务清单（读）。
   - `tasks pause-all` / `tasks resume-all`：全局暂停/恢复（写，支持 `--dry-run`）。
   - `backups list [--instance <id>]`：备份清单（读）。
   - `backups create --instance <id>`：立即创建全量备份（写，支持 `--dry-run`）。
3. 输出：单行版本化 JSON 信封 `{schemaVersion, command, ok, data|error}`；稳定退出码 0 成功 / 2 用法错误 / 3 运行失败 / 4 CLI 未启用。
4. 安全边界：无账户/凭据命令、无永久删除命令、无安全设置修改命令；CLI 不得绕过 `cli_enabled` 设置。
5. 开发者设置：设置页新增开发者区，`cli_enabled` 持久化（默认关闭）与风险提示；GUI 开关立即生效。
6. 测试：CLI 执行函数层 BDD（启用门禁、读命令、写命令、dry-run 不落盘、退出码、JSON 信封）。

## 非目标

- 不做安装/启动游戏等长时命令（后续评估进度事件协议后单独设计）。
- 不做 shell 自动补全与 man 页。
- 不开放远程调用；CLI 只读本地状态库。

## 安全不变量

- CLI 与 GUI 走同一 AppService 事务；不单独实现写路径。
- 凭据字段绝不进入 CLI 输出（账户、令牌相关数据不在命令集内）。
- 未启用时拒绝一切命令（包括读命令）并返回明确指引。

## 验证

- Rust BDD：门禁、各命令输出与退出码、dry-run、信封版本。
- Playwright：开发者区开关渲染与风险提示、960×600 与 200% 缩放。
- 全工作区 Rust、Clippy、格式、Svelte、Vitest、Playwright、生产构建与 NSIS 构建通过。
