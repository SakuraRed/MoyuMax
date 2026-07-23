# 里程碑 26：发布资产（图标、SBOM、许可清单、关于页）

## 目标

构建产物携带正式图标；仓库提供可复现生成的 SBOM 与第三方许可清单；应用内"关于"区展示版本、许可证、来源与未签名开发构建声明；卸载默认保留全部个人数据的语义写入文档并锁定。

## 范围

1. 正式图标：程序化生成 MoyuMax 图标（品牌色圆角方块 + M 字形），产出多尺寸 ICO（安装器）与 PNG（托盘/界面），替换开发占位；构建管线引用同一份生成产物（`scripts/generate-icon.mjs` 可复现）。
2. SBOM：`scripts/generate-sbom.mjs` 从 Cargo.lock、cargo metadata 与 pnpm-lock 生成 `docs/SBOM.json`（CycloneDX 1.5 简式），CI/本地可复跑并与当前锁文件一致。
3. 许可清单：`docs/THIRD-PARTY-LICENSES.md` 由脚本生成（crate/package 名、版本、许可证、仓库），校验 GPL-3.0-only 兼容性（黑名单：GPL-2.0-only、AGPL、SSPL、专有未知）并在脚本中以退出码拦截。
4. 关于页：设置页关于区——版本号、GPL-3.0-only 许可证、源码仓库、SBOM/许可清单位置、未签名开发构建声明（不伪装正式发行）。
5. 卸载保留：文档与安装器契约继续锁定卸载不删除实例、存档、备份、账户与 JDK。

## 非目标

- 不实现 Authenticode 签名（需要用户证书，外部阻塞，如实标注）。
- 不实现卸载数据分类选择向导（正式版项）。
- 不生成安装包内的许可文本捆绑（在关于页与仓库提供入口）。

## 验证

- 脚本测试：图标 ICO/PNG 尺寸与格式校验；SBOM 覆盖全部锁文件条目；许可黑名单扫描通过。
- 安装器契约测试：打包产物引用正式图标。
- Playwright：关于区渲染、960×600 与 200% 缩放。
- 全工作区 Rust、Clippy、格式、Svelte、Vitest、Playwright、生产构建与 NSIS 构建通过。
