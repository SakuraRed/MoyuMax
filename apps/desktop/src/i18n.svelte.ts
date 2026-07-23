import { en } from "./locales/en";
import { zhCN } from "./locales/zh-CN";
import { zhTW } from "./locales/zh-TW";

export type UiLanguage = "zh-CN" | "zh-TW" | "en";
export type UiTheme = "system" | "light" | "dark";

export const UI_LANGUAGES: { value: UiLanguage; label: string }[] = [
  { value: "zh-CN", label: "简体中文" },
  { value: "zh-TW", label: "繁體中文" },
  { value: "en", label: "English" },
];

export const UI_THEMES: { value: UiTheme; labelKey: string }[] = [
  { value: "system", labelKey: "appearance.theme.system" },
  { value: "light", labelKey: "appearance.theme.light" },
  { value: "dark", labelKey: "appearance.theme.dark" },
];

const dictionaries: Record<UiLanguage, Record<string, string>> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  en,
};

let language = $state<UiLanguage>("zh-CN");
let theme = $state<UiTheme>("system");

export function uiLanguage(): UiLanguage {
  return language;
}

export function uiTheme(): UiTheme {
  return theme;
}

export function applyUiPreferences(next: {
  language?: UiLanguage;
  theme?: UiTheme;
}): void {
  if (next.language) language = next.language;
  if (next.theme) theme = next.theme;
}

/** 按当前语言求值；缺键回退简体中文，再退化为键名（便于发现遗漏）。 */
export function t(key: string): string {
  return dictionaries[language][key] ?? zhCN[key] ?? key;
}
