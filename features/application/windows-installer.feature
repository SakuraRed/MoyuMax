# language: zh-CN
@BUILD-WINDOWS-001 @M8
功能: Windows 开发预览安装包
  为了在不改变系统级配置的情况下人工审查 MoyuMax
  作为 Windows 用户
  我希望得到明确标记为预览版的传统每用户 EXE 安装器

  场景: 构建无需提权的 Windows 审查安装包
    假如 当前版本还没有配置 Authenticode 代码签名证书
    当 开发者构建 Windows 安装包
    那么 只应生成 NSIS EXE 安装器
    而且 安装模式应为 currentUser
    而且 版本和交付文件名应明确包含 preview 标记
