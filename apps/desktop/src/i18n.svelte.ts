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

export type UiMotion = "system" | "reduce";
export type UiContrast = "standard" | "high";

export const UI_MOTIONS: { value: UiMotion; labelKey: string }[] = [
  { value: "system", labelKey: "appearance.motion.system" },
  { value: "reduce", labelKey: "appearance.motion.reduce" },
];

export const UI_CONTRASTS: { value: UiContrast; labelKey: string }[] = [
  { value: "standard", labelKey: "appearance.contrast.standard" },
  { value: "high", labelKey: "appearance.contrast.high" },
];

const dictionaries: Record<UiLanguage, Record<string, string>> = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  en,
};

let language = $state<UiLanguage>("zh-CN");
let theme = $state<UiTheme>("system");
let motion = $state<UiMotion>("system");
let contrast = $state<UiContrast>("standard");

export function uiLanguage(): UiLanguage {
  return language;
}

export function uiTheme(): UiTheme {
  return theme;
}

export function uiMotion(): UiMotion {
  return motion;
}

export function uiContrast(): UiContrast {
  return contrast;
}

export function applyUiPreferences(next: {
  language?: UiLanguage;
  theme?: UiTheme;
  motion?: UiMotion;
  contrast?: UiContrast;
}): void {
  if (next.language) language = next.language;
  if (next.theme) theme = next.theme;
  if (next.motion) motion = next.motion;
  if (next.contrast) contrast = next.contrast;
}

/** 按当前语言求值；缺键回退简体中文，再退化为键名（便于发现遗漏）。 */
export function t(key: string): string {
  return dictionaries[language][key] ?? zhCN[key] ?? key;
}
