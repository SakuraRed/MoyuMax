# language: zh-CN
@M12 @UI-INSTALL-001
功能: Forge 与 NeoForge 安装器处理器执行
  为了让模组玩家使用最广泛的两个加载器生态
  作为 MoyuMax 用户
  我希望新建实例时可以选择 Forge 或 NeoForge，由启动器安全完成安装器处理器

  场景: 选择兼容的 Forge 或 NeoForge 构建
    假如 用户选择一个 Minecraft 版本
    当 用户选择 Forge 或 NeoForge 加载器
    那么 系统应只列出与该版本兼容的构建并推荐最新版本

  场景: 安装器处理器按序执行并校验产物
    假如 一个 Forge 或 NeoForge 安装任务已下载安装器与依赖库
    当 任务进入应用加载器阶段
    那么 系统应按 install_profile 顺序执行客户端处理器
    而且 应跳过仅服务端的处理器
    当 处理器产物带有 _SHA 声明
    那么 只有校验通过的产物才能进入共享存储
    当 产物校验失败
    那么 暂存区回滚且实例与共享存储保持原状

  场景: 启动 Forge 或 NeoForge 实例
    假如 一个 Forge 或 NeoForge 实例已经安装完成
    当 用户启动游戏
    那么 系统应使用 version.json 声明的 mainClass 与参数
    而且 处理器产出的客户端 JAR 应在 classpath 中

  场景: NeoForge 模块路径占位符展开
    假如 一个 NeoForge 实例的运行时清单包含 ${library_directory} 占位符
    当 系统展开启动参数
    那么 占位符应替换为受管库目录与平台路径分隔符
    而且 任何未知占位符应直接报错而不是静默通过

  场景: 不支持的 install_profile 版本明确报错
    假如 一个安装器的 install_profile 不是 spec 1
    当 任务解析安装器
    那么 应返回明确的不支持错误而不是猜测执行
