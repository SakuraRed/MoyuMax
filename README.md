# MoyuMax

MoyuMax 是一款面向 Minecraft Java Edition 的开源、免费、安装式统一管理启动器。首发平台为 Windows 10 22H2 及以上的 x64 系统。

项目当前处于首个可运行里程碑开发阶段。首个垂直切片覆盖首次运行引导、本地持久化和首页空状态，后续逐步接入实例安装、Java 环境、任务队列和模组生态。

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
corepack pnpm build
cargo test --workspace
corepack pnpm --filter @moyumax/desktop tauri dev
```

## 文档

- [UI/UX 需求书](docs/UI-UX-REQUIREMENTS.md)
- [首个垂直切片计划](docs/plans/MILESTONE-01-FIRST-RUN.md)
- [技术栈决策](docs/architecture/ADR-0001-DESKTOP-STACK.md)

## 许可

客户端以 `GPL-3.0-only` 发布，完整条款见 [LICENSE](LICENSE)。第三方许可清单将在引入可分发第三方组件时同步维护。
