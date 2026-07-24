# language: zh-CN
@M30 @accounts @microsoft
功能：Microsoft 设备码登录
  作为 MoyuMax 用户
  我希望用 Microsoft 账户登录启动器
  以便用正版身份启动游戏

  背景：
    假定 应用已完成首次引导
    并且 Microsoft 认证服务可用本地测试替身

  @M30-ACCT-001
  场景：设备码登录完整链路
    假如 Microsoft 测试替身先返回授权等待再返回成功
    当 用户发起 Microsoft 设备码登录
    那么 启动器展示用户码与验证地址
    并且 轮询遵守测试替身给定的间隔
    并且 登录成功后账户以 microsoft 类型入库
    并且 玩家名与 UUID 来自档案接口
    并且 MSA 刷新令牌与 MC 访问令牌只写入数据库
    并且 账户列表与事件负载不包含任何令牌

  @M30-ACCT-002
  场景：轮询期间用户取消
    假如 Microsoft 测试替身始终返回授权等待
    当 用户发起登录后在轮询期间取消
    那么 登录以已取消结束
    并且 不产生任何账户记录

  @M30-ACCT-003
  场景：用户拒绝授权
    假如 Microsoft 测试替身返回授权被拒绝
    当 用户发起 Microsoft 设备码登录
    那么 登录失败并提示用户拒绝了授权
    并且 不产生任何账户记录

  @M30-ACCT-004
  场景：账户未拥有 Minecraft
    假如 Microsoft 测试替身档案接口返回 404
    当 用户发起 Microsoft 设备码登录
    那么 登录失败并提示该账户未拥有 Minecraft
    并且 不产生任何账户记录

  @M30-ACCT-005
  场景：Xbox 业务错误映射为可读消息
    假如 Microsoft 测试替身 XSTS 返回无 Xbox 账户错误
    当 用户发起 Microsoft 设备码登录
    那么 登录失败并提示需要先在 xbox.com 创建档案

  @M30-ACCT-006
  场景：刷新轮换刷新令牌
    假如 已登录的 Microsoft 账户刷新令牌即将轮换
    当 用户刷新该账户会话
    那么 新刷新令牌与新 MC 令牌随事务入库
    并且 会话状态保持 valid

  @M30-ACCT-007
  场景：刷新令牌被吊销
    假如 Microsoft 测试替身刷新接口返回 invalid_grant
    当 用户刷新该账户会话
    那么 账户被标记为 expired
    并且 启动游戏被拒绝并提示重新登录

  @M30-ACCT-008
  场景：Microsoft 账户启动身份
    假如 默认账户是已登录的 Microsoft 账户
    当 解析启动身份
    那么 启动参数使用 msa 用户类型与 MC 访问令牌

  @M30-ACCT-009
  场景：临期 MC 令牌启动前自动刷新
    假如 已登录的 Microsoft 账户 MC 令牌剩余不足五分钟
    并且 Microsoft 测试替身刷新接口可用
    当 解析启动身份
    那么 启动参数使用换新后的 MC 访问令牌
    并且 新刷新令牌已持久化

  @M30-ACCT-010
  场景：slow_down 退避
    假如 Microsoft 测试替身先返回 slow_down 再返回成功
    当 用户发起 Microsoft 设备码登录
    那么 轮询间隔在原始间隔基础上增加五秒
