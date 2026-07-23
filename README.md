# MoyuMax

[![CI](https://github.com/SakuraRed/MoyuMax/actions/workflows/ci.yml/badge.svg)](https://github.com/SakuraRed/MoyuMax/actions/workflows/ci.yml)

MoyuMax 是一款面向 Minecraft Java Edition 的开源、免费、安装式统一管理启动器。首发平台为 Windows 10 22H2 及以上的 x64 系统。

项目当前处于公开预览前的增量开发阶段。已经完成首次运行、当前推荐 Vanilla/Fabric 与 Azul Zulu 的真实安装执行、隔离实例启动、Modrinth 模组及必需依赖原子安装、异常退出后的本地崩溃报告和脱敏诊断包、实例本地回收站，以及游戏会话前后原子世界备份。首个公开预览仍缺少正式账户入口、其余加载器、完整任务控制、运行期间定时增量备份与世界回滚、三语 i18n 和内置 CLI，因此当前构建只标记为开发预览版。

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
- [游戏会话前后存档备份](docs/plans/MILESTONE-08-WORLD-BACKUP.md)
- [托盘生命周期与快速唤醒](docs/plans/MILESTONE-09-TRAY-LIFECYCLE.md)
- [下载来源统一与多线程加速](docs/plans/MILESTONE-10-DOWNLOAD-SOURCES.md)
- [Quilt 加载器安装与启动](docs/plans/MILESTONE-11-QUILT-LOADER.md)
- [Forge 与 NeoForge 安装器处理器执行](docs/plans/MILESTONE-12-FORGE-NEOFORGE.md)
- [Java 环境管理与删除/恢复闭环](docs/plans/MILESTONE-13-JAVA-ENVIRONMENT.md)
- [任务控制完整化](docs/plans/MILESTONE-14-TASK-CONTROL.md)
- [Modrinth 全加载器与按实例更新策略](docs/plans/MILESTONE-15-CONTENT-UPDATES.md)
- [实例资源内容管理](docs/plans/MILESTONE-16-INSTANCE-RESOURCES.md)
- [世界存档管理](docs/plans/MILESTONE-17-WORLD-MANAGEMENT.md)
- [截图管理与删除回收站统一](docs/plans/MILESTONE-18-SCREENSHOTS-RECYCLE.md)
- [运行期间增量备份与世界历史](docs/plans/MILESTONE-19-INCREMENTAL-BACKUP.md)
- [账户入口](docs/plans/MILESTONE-20-ACCOUNTS.md)
- [主题切换与 i18n 基础设施](docs/plans/MILESTONE-21-THEME-I18N.md)
- [全页面文案外置](docs/plans/MILESTONE-22-FULL-I18N.md)
- [无障碍增强与性能正式验收](docs/plans/MILESTONE-23-A11Y-PERFORMANCE.md)
- [内置 CLI（开发者模式）](docs/plans/MILESTONE-24-CLI.md)
- [启动器更新检查与安全下载](docs/plans/MILESTONE-25-SELF-UPDATE.md)
- [发布资产](docs/plans/MILESTONE-26-RELEASE-ASSETS.md)
- [GitHub Actions Windows CI](docs/plans/MILESTONE-27-CI.md)
- [下载来源统一与多线程加速计划](docs/plans/DOWNLOAD-SOURCES-AND-ACCELERATION.md)
- [技术栈决策](docs/architecture/ADR-0001-DESKTOP-STACK.md)

## CI 与验证门

GitHub Actions（`.github/workflows/ci.yml`）在干净 Windows 环境执行与本地一致的完整验证门：

| 作业 | 内容 |
|---|---|
| rust | `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings`、`cargo test --workspace` |
| frontend | `pnpm install --frozen-lockfile`、`pnpm lint`、`pnpm test`、`pnpm build`、Playwright e2e |
| bundle | 图标/SBOM 一致性校验、`pnpm tauri build`（x64 NSIS，未签名）与产物上传 |

## 许可

客户端以 `GPL-3.0-only` 发布，完整条款见 [LICENSE](LICENSE)。第三方组件清单见 [THIRD-PARTY-LICENSES](docs/THIRD-PARTY-LICENSES.md)，SBOM 见 [SBOM.json](docs/SBOM.json)（由 `node scripts/generate-sbom.mjs` 可复现生成）。
