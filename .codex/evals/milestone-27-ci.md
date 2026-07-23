# EVAL DEFINITION：里程碑 27 GitHub Actions Windows CI

## Capability Evals

- [x] `.github/workflows/ci.yml` 覆盖 Rust 静态检查与测试、前端 lint/test/build、Playwright、NSIS 构建与产物上传。
- [x] 工作流命令与本地验证门逐行一致；锁文件与固定版本语义保持（frozen-lockfile、精确工具链）。
- [x] YAML 解析有效；徽章与文档对照表存在。

## Regression Evals

- [x] M1–M26 全部 BDD、Vitest、Playwright 与 Rust 工作区测试在本机继续通过。

## Deterministic Graders

- YAML 解析（node 校验）。
- 本机全量验证门复跑（确认工作流未引入仓库状态回归）。

## Completion Rule

只有工作流真实覆盖全部验证门且与本地命令一致时，才可将本里程碑标记为 validated（YAML 层面）。首次推送后的绿色运行属推送后确认项，不在本里程碑内宣称。

## 2026-07-23 验证报告

- Capability：3/3 PASS（YAML 层面）。
- Regression：1/1 PASS。
- 结构校验：node 脚本确认 rust/frontend/bundle 三作业与全部关键命令存在（130 行）；命令与 `.continue-here.md` 第 10 节验证门逐行一致；CI 使用标准 1420 端口（干净环境无本机端口冲突）。
- 本机复跑：Rust 167 PASS、Vitest 19/19、Playwright 99/99（在 M26 状态上仅新增工作流与文档，无代码回归）。
- README：CI 徽章、验证门对照表、SBOM/许可清单链接。
- 范围说明：首次推送后的实际绿色运行由用户确认（GitHub Actions 无法在本机执行）；发布工作流（Release/标签）不在范围内。

状态：validated（YAML 层面；实际运行待推送后确认）。
