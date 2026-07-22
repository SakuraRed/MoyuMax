# EVAL DEFINITION：里程碑 4 真实游戏启动

## Capability Evals

- [x] 当前推荐 Minecraft 与 Fabric 运行时清单可生成无残留占位符的 Windows x64 命令。
- [x] 本地离线身份名称与 UUID 跨启动稳定。
- [x] Java、classpath、资源索引或日志配置缺失时不创建进程。
- [x] Mojang、默认用户 JVM 与 Fabric 参数按规则正确合并。
- [x] 新版 x64 native JAR 保留在 classpath，错误架构不进入命令。
- [x] stdout、stderr、退出码和会话状态持久化。
- [x] 非零退出标记失败，主动停止标记已停止。
- [x] 同一实例拒绝重复运行。
- [x] Tauri 命令立即返回并在后台监控游戏进程。
- [x] 首页可以启动和停止已安装实例并看到真实状态。

## Regression Evals

- [x] `cargo fmt --all -- --check` 通过。
- [x] `cargo test --workspace` 通过。
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- [x] `pnpm lint`、`pnpm test`、`pnpm test:e2e`、`pnpm build` 通过。
- [x] 里程碑 3 当前推荐版真实安装探针继续通过。

## 真实链路

- [x] 使用 `output/live-install/.tmp9GXxdd` 中的 26.2 Fabric 实例生成启动命令。
- [x] 托管 Azul Java 成功进入 Fabric/Minecraft 主类。
- [x] 游戏完成首次 native 自解包并产生可辨认启动日志。
- [x] 用户停止游戏后会话、退出状态和日志保持一致。

## 成功标准

只有真实托管 Java 进程使用受管文件进入 Minecraft/Fabric 主类，并且进程与日志生命周期可查询，才可称“游戏可启动”。只生成命令文本或执行 `java -version` 不算通过。

## 2026-07-23 验证记录

### Code-Based Grader

- `cargo test -p moyumax-core --test launch_planning_bdd`：14/14，通过。覆盖参数展开、关键文件阻断、x64 native 精确筛选、受管目录索引、重复启动、主动停止、重启恢复、命令脱敏、停止通道关闭和日志初始化失败。
- `cargo test -p moyumax-desktop --lib`：1/1，通过。停止请求只路由到对应实例且不可重复消费。
- `pnpm --dir apps/desktop test:e2e`：8/8，通过。其中 2 个 `UI-LAUNCH-001` 场景覆盖实例首页、默认键盘焦点、启动、停止、18×20px 卡片内边距、960×600 和 200% 放大。
- `corepack pnpm tauri build --no-bundle`：通过，生成 `target/release/moyumax-desktop.exe`；隔离启动后进程保持响应。

### 真实运行 Grader

- `MOYUMAX_LIVE_RESULT_FILE=output/live-install-result-20260723.json cargo test -p moyumax-core --test live_install -- --ignored --nocapture`：1/1，通过，用时 694.48 秒。
- 安装结果：Minecraft 26.2、Fabric 0.19.3、Azul Zulu 25.0.4+7；实例为 `ready`，任务为 `completed`，暂存区已清理。
- `MOYUMAX_LIVE_INSTALL_ROOT=output/live-install/.tmp9GXxdd cargo test -p moyumax-core --test live_launch -- --ignored --nocapture`：1/1，通过。
- 实例：Minecraft 26.2、Fabric Loader 0.19.3、Azul Zulu 25.0.4+7、Windows x64。
- 最新 stdout 包含 `Loading Minecraft 26.2 with Fabric Loader 0.19.3`；stderr 为空。
- `natives/lwjgl/3.4.1-snapshot/x64` 包含 `lwjgl.dll`、`glfw.dll`、`OpenAL.dll`、`shaderc.dll` 等实际解包文件。
- 测试通过显式停止信号结束游戏，并断言会话状态为 `stopped`。

### 当前状态

里程碑 4 的游戏启动能力、桌面协调器、实例首页、真实安装回归和真实游戏启动均为 PASS。本里程碑状态为 VALIDATED，可以形成签名提交并进入 Modrinth 内容安装里程碑。
