# Modrinth API 实现速查

## 官方来源

- API 总览与认证：<https://docs.modrinth.com/api/>
- 搜索项目：<https://docs.modrinth.com/api/operations/searchprojects/>
- 列出项目版本：<https://docs.modrinth.com/api/operations/getprojectversions/>
- 获取单个版本：<https://docs.modrinth.com/api/operations/getversion/>
- 生产 API：<https://api.modrinth.com/v2/>

核对日期为 2026-07-23。官方文档当时标识 Labrinth `v2.7.0/366f528`，OpenAPI 3.0.0。

## 客户端规则

- 大多数公开读取不需要令牌。首版搜索、项目、版本和文件解析不得要求用户登录 Modrinth。
- 每个请求必须使用唯一 `User-Agent`。MoyuMax 使用 `SakuraRed/MoyuMax/<version> (github.com/SakuraRed/MoyuMax)`。
- 读取 `X-Ratelimit-Limit`、`X-Ratelimit-Remaining` 和 `X-Ratelimit-Reset`，遇到限流不得忙循环。
- API v2 退役并返回 410 时必须显示 provider 需要升级，不得退化为解析网页。
- 长期索引使用不可变 `project_id` 和 `version_id`，不使用可能变化的 slug 作为数据库主键。

## 搜索契约

`GET /search` 使用以下 AND facets，每个 facet 内允许 OR：

```json
[
  ["project_type:mod"],
  ["versions:26.2"],
  ["categories:fabric"],
  ["client_side:required", "client_side:optional"]
]
```

首版排序支持 `relevance`、`downloads`、`newest` 和 `updated`。`limit` 不超过 100，界面默认 20。远程图标 URL 不下载、不渲染，避免远程媒体进入主题或首页性能路径。

## 兼容版本选择

`GET /project/{id}/version` 固定传入：

- `loaders=["fabric"]`
- `game_versions=["26.2"]`
- `include_changelog=false`

默认只选择 `status=listed`。优先级依次为 `release`、`beta`、`alpha`，同一通道选择最新 `date_published`。用户若要预发布版必须显式选择。

主文件选择顺序：

1. `primary=true` 且 `file_type` 为空。
2. 唯一一个 `file_type` 为空的文件。
3. 否则阻止自动安装并要求用户选择，绝不误装 `sources-jar`。

文件安装前同时校验 `size`、SHA-1 和 SHA-512。

## 依赖闭包

- `required`：自动加入同一安装事务。
- `optional`：只进入确认清单，默认不选中，由用户决定。
- `incompatible`：与已安装内容或本次闭包冲突时阻止确认，并列出项目与版本。
- `embedded`：记录来源，不重复下载为独立模组。

有 `version_id` 时使用指定版本并再次核对实例兼容性；只有 `project_id` 时按上述兼容版本规则选择。只有 `file_name` 且缺少可解析 ID 时不得猜测项目。依赖闭包按项目 ID 去重，检测循环并输出稳定的依赖在前顺序。

## 2026-07-23 实机样本

- Continuity：项目 `1IjD5062`，版本 `mgUN5Xz2`，`3.0.1+26.2`。
- 主文件：`continuity-3.0.1+26.2.jar`，大小 1,040,013 字节，SHA-1 `8fa8bc108e84158b0828a4aa59bf906d31676eec`。
- 必需依赖：Fabric API 项目 `P7dR8mSH`。
- 当时首个兼容 Fabric API：版本 `lVXlbH4w`，`0.155.2+26.2`，主文件大小 2,530,080 字节，SHA-1 `a2bf116a5beeb27245c1a36985aa05d729f78926`。

实机验收不得把这些版本号写死进产品逻辑；测试每次从官方兼容版本接口解析当前选择，并固定本次计划快照。
