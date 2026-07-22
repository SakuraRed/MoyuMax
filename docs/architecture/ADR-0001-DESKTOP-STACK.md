# ADR-0001：Windows 桌面技术栈

## 状态

接受，用于首个可运行原型；在里程碑 1 的 Release 基准后复核。

## 背景

MoyuMax 首发 Windows 10 22H2 x64，后续需要 macOS/Linux。现有 mockups 已使用 HTML、CSS 和 JavaScript 完整表达 103 个界面状态。启动器必须满足快速唤醒、低后台占用、完整离线和统一核心等约束。

基准硬门槛包括：首个可见窗口 P95 不超过 500 ms，冷启动完全可操作 P95 不超过 2 秒，托盘唤醒 P95 不超过 250 ms；前台所有相关进程总私有内存硬上限 256 MiB，UI 卸载后后台硬上限 120 MiB。

## 决策

首个原型采用：

- Rust 作为与 UI 无关的领域、存储、任务和未来 CLI 核心。
- Tauri 2 作为桌面壳与 Windows 集成层。
- Svelte 5、TypeScript 和 CSS 作为编译型声明式 WebView 界面。
- SQLite 作为本地权威状态库。

`moyumax-core` 不得依赖 Tauri；Tauri 命令只是适配器。浏览器开发适配器只用于 UI 开发与测试，正式桌面构建必须调用 Rust 核心。

## 理由

- 现有 mockups 可直接转译设计令牌和结构，视觉偏差与重复劳动较少。
- Rust 核心可在未来桌面平台和 CLI 中复用，运行时不要求用户安装 Node.js 或 .NET。
- Svelte 将组件编译为定向 DOM 更新，不引入 React 虚拟 DOM；声明式组件、CSS Grid/Flex 约束与视觉回归共同降低排版错位和状态漂移风险。
- SQLite 能从第一天建立数据库迁移、事务和单一事实来源。

## 代价与风险

- Windows WebView2 是主要内存和冷启动风险，必须用实际 Release 构建测量，不能以浏览器开发模式代替。
- Tauri 的窗口销毁与托盘重建策略需要专门验证，避免隐藏窗口后 WebView 继续占用高内存。
- WebView2 兼容性不能等同于 Windows ARM64 正式支持。

## 复核条件

里程碑 1 完成后，在规定基准设备执行 Release 测量。若经过构建优化后仍超过任何硬上限，或托盘销毁/重建无法满足体验，应在扩展 UI 页面前制作 Avalonia/.NET 对照原型并重新决策。
