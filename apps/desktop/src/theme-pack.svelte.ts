/**
 * 主题包应用与回落:单 <style> 节点整体替换;.window 携带 tp-<id> 作用域类。
 * 内置包直接编译,导入包经 runtime 读取 JSON 源(core 导入时已校验)。
 */

import type { MoyuRuntime } from "./runtime";
import { BUILTIN_THEME_PACKS } from "./themes/builtin-themes";
import { compileThemePack, parseThemePackSource, type ThemePackV2 } from "./theme-engine";

const STYLE_ELEMENT_ID = "moyumax-theme-pack";
let activePackId = $state("default");
let activeError = $state("");

export function themePackError(): string {
  return activeError;
}

export function activeThemePackId(): string {
  return activePackId;
}

export async function applyThemePack(runtime: MoyuRuntime, packId: string): Promise<void> {
  activeError = "";
  if (packId === "default") {
    injectCss("default", "");
    activePackId = "default";
    return;
  }
  const builtin = BUILTIN_THEME_PACKS[packId];
  let pack: ThemePackV2 | null = builtin ?? null;
  if (!pack) {
    try {
      const source = await runtime.readThemePackV2(packId);
      pack = parseThemePackSource(source);
    } catch (error) {
      activeError = error instanceof Error ? error.message : String(error);
      pack = null;
    }
  }
  if (!pack || pack.id !== packId) {
    activeError = activeError || `主题包 ${packId} 不可用,已回落默认主题`;
    injectCss("default", "");
    activePackId = "default";
    return;
  }
  injectCss(packId, compileThemePack(pack));
  activePackId = packId;
}

function injectCss(packId: string, css: string): void {
  document.getElementById(STYLE_ELEMENT_ID)?.remove();
  if (packId === "default" || css.trim() === "") return;
  const style = document.createElement("style");
  style.id = STYLE_ELEMENT_ID;
  style.textContent = css;
  document.head.appendChild(style);
}
