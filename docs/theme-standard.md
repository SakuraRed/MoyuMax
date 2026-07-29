# MoyuMax 主题包标准(v2)

> 版本：2.0 · 状态：生效
> 适用:MoyuMax ≥ 0.1.0。formatVersion=1 的纯配色包按附录 A 兼容导入。

## 1. 设计目标

主题包是**纯声明式**的视觉定制单元,满足:

- **跨版本支持**:`base` 层是稳定契约,应用新增页面与组件时自动获得主题基础样式,不需要主题作者跟进。
- **单独限制版本**:`overrides` 层按页面与版本范围单独生效,允许针对某个应用版本做深度定制,版本越界自动回落到 `base`。
- **后续更新直接适配**:新功能 UI 改动后,先由 `base` 套模板保证不崩;需要更好效果时,为对应版本补一条 override 即可。
- **安全**:无脚本、无远程资源、无任意 CSS。主题包经过结构化校验,越界内容直接拒绝。

## 2. 组合模型:基础声明 + 特殊样式

主题 = `base`(基础声明)+ `overrides[]`(特殊样式)。生效顺序:内置样式(moyu.css)→ `base.tokens` → `base.rules` → `overrides[].rules`(按声明顺序)。

### 2.1 base.tokens(跨版本令牌契约)

视觉令牌是主题与应用之间的**稳定契约**。应用保证这些令牌存在且语义不变;主题只覆盖取值,不发明新键。

| 令牌组 | 键 | 语义 |
|---|---|---|
| 背景 | `--bg-0 --bg-1 --bg-2 --bg-grad` | 窗口背景与渐变 |
| 面板 | `--glass --glass-strong --glass-border --glass-highlight --glass-blur` | 面板表面与描边 |
| 文字 | `--text-1 --text-2 --text-3` | 三级文字 |
| 强调 | `--accent --accent-ink --accent-soft` | 唯一强调色与其文字/柔底 |
| 语义 | `--ok --warn --danger --info` 及 `*-soft` | 仅状态用途 |
| 形状 | `--r` | 统一圆角基准 |
| 阴影 | `--shadow-1 --shadow-2` | 两级阴影 |
| 字体 | `--font --mono` | 字体栈(仅系统栈) |

未列出的令牌不得出现在包内(校验拒绝)。

### 2.2 base.rules(组件基础声明)

对**组件类**的基础样式。新组件默认继承,无需逐页适配。允许的声明属性见第 4 节清单;允许的选择器见第 5 节。

### 2.3 overrides[](特殊样式)

```json
{
  "name": "home-hero",
  "pages": ["home"],
  "appVersion": { "min": "0.1.0", "max": "0.2.x" },
  "rules": [ { "selector": ".hero-card", "declarations": { } } ]
}
```

- `pages`(可选):页面键清单,见第 6 节;缺省 = 全部页面。
- `appVersion`(可选):`min`/`max` 版本范围,semver 三段比较(`x` 通配末段);缺省 = 不限。
- 多条 override 命中同一选择器时,**后声明覆盖先声明**。

## 3. 包结构

```json
{
  "formatVersion": 2,
  "id": "animal-island",
  "name": "动物森友会",
  "author": "MoyuMax",
  "description": "奶油纸面与自然绿的动物森友会风格",
  "appVersion": { "min": "0.1.0" },
  "base": { "tokens": { }, "rules": [ ] },
  "overrides": [ ]
}
```

- `id`:小写字母/数字/连字符,1-32 字符,全局唯一(内置保留 `default-dark`、`default-light`、`animal-island`)。
- `name`/`author`/`description`:纯文本,各 ≤ 64 字符,禁止 URL 与控制字符。

## 4. 允许的声明属性(白名单)

颜色与表面:`color background background-color background-image(仅渐变) border border-color border-width border-style border-radius outline outline-color box-shadow text-shadow backdrop-filter -webkit-backdrop-filter filter opacity`
间距:`margin* padding* gap row-gap column-gap width min-width max-width height min-height max-height inset* top right bottom left`
文字:`font font-family font-size font-weight font-style line-height letter-spacing text-align text-decoration text-transform white-space word-break overflow-wrap`
布局(保守):`display flex flex-direction flex-wrap align-items align-content justify-content align-self justify-self place-items place-content grid-template-columns grid-column grid-row overflow overflow-x overflow-y`
其他:`transition transition-property transition-duration transition-timing-function animation(仅声明的 keyframes 名) transform cursor pointer-events user-select visibility content(仅文本) object-fit image-rendering aspect-ratio flex-grow flex-shrink flex-basis order`

值约束:不含 `url(`、`@import`、`expression`、`javascript:`;单值 ≤ 240 字符;颜色值建议引用令牌。

## 5. 允许的选择器(白名单)

- 仅类选择器及其组合:`.a`、`.a .b`、`.a.b`、`.a > .b`、`.a:hover`、`.a:focus-visible`、`.a:disabled`、`.a:active`、`.a:first-child`、`.a:last-child`、`.a:nth-child(n)`、`.a:not(.b)`。
- 允许 `.window` 作为根范围;引擎自动把每条规则限定在 `.window.tp-<id>` 下,页面级 override 再加 `[data-page="<key>"]`。
- 禁止:元素标签、ID、属性选择器(除引擎注入的 data-page)、通配 `*`、伪元素(`::before/::after` 允许,仅当 content 为文本)。

## 6. 页面键

`home instances instanceDetail resources tasks data accounts settings onboarding netplay backups crash`

应用窗口根元素携带 `data-page`(主导航键;实例详情为 `instanceDetail`,新建实例归入 `instances`,崩溃报告为 `crash`)。

## 7. 生效与回落

1. 包级 `appVersion` 不匹配 → 整包不生效并提示,不部分应用。
2. 某条 override 版本/页面不匹配 → 仅跳过该条。
3. 主题包被删除或失效 → 回落内置默认主题,不留残留样式(引擎单 `<style>` 节点整体替换)。

## 8. 制作流程(作者指南)

1. 复制 `docs/examples/theme-template.json` 起步。
2. 先定 `base.tokens`(推荐只改强调色与表面,不碰语义色)。
3. 跑通默认页后,再为要突出的页面加 overrides,每条写清 `name` 便于回顾。
4. 校验:设置 → 外观 → 导入主题包;失败信息会指出第一条越界。

## 附录 A:formatVersion=1 兼容

v1 纯配色包(colors 映射)导入时按表转换:`accent→--accent text→--text-1 text-2→--text-2 text-3→--text-3 bg-window→--bg-0 bg-app→--bg-1 bg-nav→--bg-1 surface→--glass-strong surface-2→--glass-strong border→--glass-border border-strong→--glass-border`,其余键忽略并提示。转换后为 v2 base.tokens,不含 rules 与 overrides。
