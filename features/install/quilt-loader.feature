# language: zh-CN
@M11 @UI-INSTALL-001
功能: Quilt 加载器安装与启动
  为了让习惯 Quilt 生态的用户获得与 Fabric 一致的安装体验
  作为 MoyuMax 用户
  我希望新建实例时可以选择兼容的 Quilt 加载器并直接启动

  场景: 选择兼容的 Quilt 加载器
    假如 用户选择一个 Minecraft 版本
    当 用户在新建实例页选择 Quilt 加载器
    那么 系统应只列出与该版本兼容的 Quilt Loader
    而且 应推荐最新的稳定版本

  场景: 安装 Quilt 实例
    假如 用户选择了兼容的 Quilt Loader 版本
    当 用户确认安装
    那么 Quilt Loader 及其依赖库应进入同一安装任务
    而且 全部文件通过校验后才原子提交实例
    当 元数据服务失效
    那么 本地实例和启动不受影响

  场景: 启动 Quilt 实例
    假如 一个 Quilt 实例已经安装完成
    当 用户启动游戏
    那么 系统应使用 Quilt profile 声明的 mainClass 与参数
    而且 实例的加载器应显示为 Quilt

  场景: 拒绝不兼容的 Quilt 版本
    假如 用户强制选择一个不在兼容列表中的 Quilt Loader
    当 系统解析安装请求
    那么 应返回不兼容错误而不是静默下载
