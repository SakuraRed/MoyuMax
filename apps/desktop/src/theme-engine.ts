/**
 * 主题包标准 v2 引擎(docs/theme-standard.md):
 * 基础声明(tokens+rules)跨版本生效;特殊样式按页面与版本范围命中,
 * 全部规则限定在 .window.tp-<id> 作用域内,整体替换、可整体回落。
 */

export const APP_VERSION = "0.2.0";

export interface ThemeVersionRange {
  min?: string | null;
  max?: string | null;
}

export interface ThemeRule {
  selector: string;
  declarations: Record<string, string>;
}

export interface ThemeOverride {
  name: string;
  pages?: string[] | null;
  appVersion?: ThemeVersionRange | null;
  rules: ThemeRule[];
}

export interface ThemePackV2 {
  formatVersion: number;
  id: string;
  name: string;
  author: string;
  description?: string | null;
  appVersion?: ThemeVersionRange | null;
  base: {
    tokens: Record<string, string>;
    rules?: ThemeRule[];
  };
  overrides?: ThemeOverride[];
}

function compareSemver(left: string, right: string): number {
  const parse = (value: string) => value.split(".").map((part) => (part === "x" ? -1 : Number(part) || 0));
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < 3; index += 1) {
    const av = a[index] ?? 0;
    const bv = b[index] ?? 0;
    if (av === -1 || bv === -1) return 0;
    if (av !== bv) return av - bv;
  }
  return 0;
}

function versionInRange(version: string, range?: ThemeVersionRange | null): boolean {
  if (!range) return true;
  if (range.min && compareSemver(version, range.min) < 0) return false;
  if (range.max && compareSemver(version, range.max) > 0) return false;
  return true;
}

function renderRule(scope: string, rule: ThemeRule): string {
  const declarations = Object.entries(rule.declarations)
    .map(([property, value]) => `${property}: ${value}`)
    .join("; ");
  return `${scope} ${rule.selector} { ${declarations} }`;
}

/** 编译主题包为限定作用域的 CSS 文本;版本不匹配的 override 跳过。 */
export function compileThemePack(pack: ThemePackV2, appVersion: string = APP_VERSION): string {
  const scope = `.window[data-tp="${pack.id}"]`;
  const chunks: string[] = [];
  const tokenEntries = Object.entries(pack.base.tokens);
  if (tokenEntries.length > 0) {
    chunks.push(
      `${scope} { ${tokenEntries.map(([token, value]) => `${token}: ${value}`).join("; ")} }`,
    );
  }
  for (const rule of pack.base.rules ?? []) {
    chunks.push(renderRule(scope, rule));
  }
  for (const override of pack.overrides ?? []) {
    if (!versionInRange(appVersion, override.appVersion)) continue;
    const pages = override.pages?.filter((page) => page.trim() !== "");
    if (pages && pages.length > 0) {
      for (const page of pages) {
        for (const rule of override.rules) {
          chunks.push(renderRule(`${scope}[data-page="${page}"]`, rule));
        }
      }
    } else {
      for (const rule of override.rules) {
        chunks.push(renderRule(scope, rule));
      }
    }
  }
  return chunks.join("\n");
}

/** 解析主题包 JSON 源文本(core 导入时已做完整校验,这里仅解析)。 */
export function parseThemePackSource(source: string): ThemePackV2 {
  return JSON.parse(source) as ThemePackV2;
}
