# MoyuMax

MoyuMax 是一款面向 Minecraft Java Edition 的开源、免费、安装式统一管理启动器。首发平台为 Windows 10 22H2 及以上的 x64 系统。

项目当前处于公开预览前的增量开发阶段。已经完成首次运行、当前推荐 Vanilla/Fabric 与 Azul Zulu 的真实安装执行、隔离实例启动、Modrinth 模组及必需依赖原子安装、异常退出后的本地崩溃报告和脱敏诊断包，以及实例删除、恢复与永久删除的本地回收站闭环。首个公开预览仍缺少正式账户入口、其余加载器、完整任务控制、存档备份与世界级数据管理、三语 i18n 和内置 CLI，因此当前构建不标记为公开预览版。

## 技术骨架

- Rust：本地领域核心、SQLite 状态、事务和未来 CLI。
- Tauri 2：Windows 桌面生命周期与系统集成。
- Svelte 5 + TypeScript：编译型声明式 WebView 界面，避免命令式 DOM 状态漂移。
- BDD + TDD：`features/` 定义行为，Rust 与 TypeScript 测试实现自动验收。

## 开发

要求：Rust 1.96.0、Node.js 22、pnpm 10.26.2、Windows WebView2。

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm test
corepack pnpm test:e2e
corepack pnpm build
cargo test --workspace
corepack pnpm --filter @moyumax/desktop tauri dev
```

## 文档

- [UI/UX 需求书](docs/UI-UX-REQUIREMENTS.md)
- [首个垂直切片计划](docs/plans/MILESTONE-01-FIRST-RUN.md)
- [安装计划与持久化队列](docs/plans/MILESTONE-02-INSTALL-PLANNING.md)
- [真实安装执行器](docs/plans/MILESTONE-03-INSTALL-EXECUTOR.md)
- [真实游戏启动](docs/plans/MILESTONE-04-GAME-LAUNCH.md)
- [Modrinth 内容安装](docs/plans/MILESTONE-05-MODRINTH-CONTENT.md)
- [本地崩溃诊断](docs/plans/MILESTONE-06-CRASH-DIAGNOSTICS.md)
- [实例内置回收站](docs/plans/MILESTONE-07-INSTANCE-RECYCLE-BIN.md)
- [技术栈决策](docs/architecture/ADR-0001-DESKTOP-STACK.md)

## 许可

客户端以 `GPL-3.0-only` 发布，完整条款见 [LICENSE](LICENSE)。第三方许可清单将在引入可分发第三方组件时同步维护。
