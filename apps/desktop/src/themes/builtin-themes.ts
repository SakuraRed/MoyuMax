import type { ThemePackV2 } from "../theme-engine";

/** 内置主题包注册表("default" 走 moyu.css 内置样式,不在此表)。 */
export const BUILTIN_THEME_PACKS: Record<string, ThemePackV2> = {
  "animal-island": {
    formatVersion: 2,
    id: "animal-island",
    name: "动物森友会",
    author: "MoyuMax",
    description: "奶油纸面、自然绿与大圆角的轻松风格,展示主题包定制能力",
    appVersion: { min: "0.1.0" },
    base: {
      tokens: {
        "--bg-0": "#f3ead6",
        "--bg-1": "#fdf8ec",
        "--bg-2": "#ecdfc2",
        "--bg-grad":
          "radial-gradient(1100px 640px at 88% -12%, rgba(127, 176, 105, 0.20), transparent 60%), radial-gradient(820px 560px at -12% 112%, rgba(214, 168, 96, 0.22), transparent 55%), linear-gradient(160deg, var(--bg-1), var(--bg-0) 72%)",
        "--glass": "rgba(255, 252, 243, 0.80)",
        "--glass-strong": "rgba(255, 252, 243, 0.95)",
        "--glass-border": "rgba(148, 122, 82, 0.20)",
        "--glass-highlight": "rgba(255, 255, 255, 0.66)",
        "--glass-blur": "blur(8px) saturate(110%)",
        "--text-1": "#4a3c28",
        "--text-2": "#77644a",
        "--text-3": "#a59272",
        "--accent": "#7fb069",
        "--accent-ink": "#fffef5",
        "--accent-soft": "rgba(127, 176, 105, 0.20)",
        "--ok": "#6fa96f",
        "--ok-soft": "rgba(111, 169, 111, 0.18)",
        "--warn": "#dfa04e",
        "--warn-soft": "rgba(223, 160, 78, 0.20)",
        "--danger": "#d97757",
        "--danger-soft": "rgba(217, 119, 87, 0.18)",
        "--info": "#6b9dc7",
        "--info-soft": "rgba(107, 157, 199, 0.20)",
        "--r": "16px",
        "--shadow-1": "0 6px 20px rgba(122, 96, 56, 0.14)",
        "--shadow-2": "0 14px 40px rgba(122, 96, 56, 0.18)",
        "--font": "\"Segoe UI\", \"Microsoft YaHei UI\", \"Microsoft YaHei\", system-ui, sans-serif",
        "--mono": "\"Cascadia Mono\", Consolas, monospace",
      },
      rules: [
        {
          selector: ".titlebar",
          declarations: {
            background: "rgba(255, 252, 243, 0.66)",
            "border-bottom": "1px solid rgba(148, 122, 82, 0.16)",
          },
        },
        {
          selector: ".navrail",
          declarations: {
            background: "rgba(255, 252, 243, 0.55)",
            "border-right": "1px solid rgba(148, 122, 82, 0.16)",
          },
        },
        {
          selector: ".nav-item.active",
          declarations: {
            background: "var(--accent-soft)",
            color: "#5d8f4a",
            "font-weight": "700",
          },
        },
        {
          selector: ".nav-item.active::before",
          declarations: { width: "4px", "border-radius": "999px" },
        },
        {
          selector: ".panel",
          declarations: {
            "border-radius": "18px",
            "box-shadow": "inset 0 1px 0 var(--glass-highlight), var(--shadow-1)",
          },
        },
        {
          selector: ".btn",
          declarations: { height: "38px", "font-weight": "700", "letter-spacing": "0.01em" },
        },
        {
          selector: ".btn.primary",
          declarations: {
            background: "linear-gradient(180deg, #93c47f, var(--accent))",
            "box-shadow": "0 3px 10px rgba(127, 176, 105, 0.35)",
          },
        },
        {
          selector: ".btn.primary:hover",
          declarations: { filter: "brightness(1.05)" },
        },
        {
          selector: ".tag",
          declarations: { "font-weight": "700" },
        },
        {
          selector: ".input",
          declarations: {
            background: "rgba(255, 255, 255, 0.72)",
            "border-color": "rgba(148, 122, 82, 0.24)",
          },
        },
        {
          selector: ".input:focus",
          declarations: { "border-color": "var(--accent)" },
        },
        {
          selector: ".modal",
          declarations: { "border-radius": "18px" },
        },
        {
          selector: ".switch.on",
          declarations: { background: "var(--accent)" },
        },
        {
          selector: ".tabs button.on",
          declarations: { color: "#5d8f4a", "font-weight": "700" },
        },
        {
          selector: ".tb-tool .dot",
          declarations: { width: "8px", height: "8px" },
        },
      ],
    },
    overrides: [
      {
        name: "首页 hero 草地感",
        pages: ["home"],
        appVersion: { min: "0.1.0" },
        rules: [
          {
            selector: ".hero-card",
            declarations: {
              "border-radius": "22px",
              "box-shadow":
                "inset 0 4px 0 rgba(127, 176, 105, 0.55), inset 0 1px 0 var(--glass-highlight), var(--shadow-1)",
            },
          },
          {
            selector: ".hero-cube",
            declarations: { "border-radius": "18px" },
          },
        ],
      },
      {
        name: "实例卡纸感",
        pages: ["instances"],
        appVersion: { min: "0.1.0" },
        rules: [
          {
            selector: ".inst-card",
            declarations: {
              "border-radius": "18px",
              transition: "transform 160ms ease, box-shadow 160ms ease",
            },
          },
          {
            selector: ".inst-card:hover",
            declarations: { transform: "translateY(-1px)", "box-shadow": "var(--shadow-2)" },
          },
        ],
      },
      {
        name: "任务卡柔和",
        pages: ["tasks"],
        appVersion: { min: "0.1.0" },
        rules: [
          {
            selector: ".task-card",
            declarations: { "border-radius": "18px" },
          },
        ],
      },
      {
        name: "设置导航纸面",
        pages: ["settings"],
        appVersion: { min: "0.1.0" },
        rules: [
          {
            selector: ".set-nav button.on",
            declarations: { color: "#5d8f4a", "font-weight": "700" },
          },
        ],
      },
    ],
  },
};
