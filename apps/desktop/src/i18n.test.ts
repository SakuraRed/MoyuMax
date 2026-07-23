import { describe, expect, it } from "vitest";

import { en } from "./locales/en";
import { zhCN } from "./locales/zh-CN";
import { zhTW } from "./locales/zh-TW";

describe("i18n 字典", () => {
  it("三种语言的键集合完全一致", () => {
    const source = Object.keys(zhCN).sort();
    expect(Object.keys(zhTW).sort()).toEqual(source);
    expect(Object.keys(en).sort()).toEqual(source);
  });

  it("所有语言的值都非空；仅登记的分隔键允许边界空白", () => {
    // 这些键的值刻意携带前导/尾随空白作为行内分隔符，组件模板不再另加空格。
    const separatorKeys = new Set([
      "home.instance.latestSession",
      "home.instance.exitCode",
      "resources.files.worldSuffix",
      "data.worlds.lastPlayed",
      "crash.evidence.truncatedSuffix",
      "install.version.stableSuffix",
      "install.loader.recommendedSuffix",
      "install.queued.staging",
      "tasks.progress.totalKnown",
      "tasks.progress.totalUnknown",
    ]);
    for (const [key, value] of Object.entries(zhCN)) {
      expect(value.length, `zh-CN ${key} 为空`).toBeGreaterThan(0);
      if (!separatorKeys.has(key)) {
        expect(value, `zh-CN ${key} 含首尾空白`).toBe(value.trim());
      }
    }
    for (const [key, value] of Object.entries(zhTW)) {
      expect(value.length, `zh-TW ${key} 为空`).toBeGreaterThan(0);
    }
    for (const [key, value] of Object.entries(en)) {
      expect(value.length, `en ${key} 为空`).toBeGreaterThan(0);
    }
  });

  it("三语言保留相同的插值占位符", () => {
    const placeholder = /\{[a-z]+\}/g;
    for (const [key, value] of Object.entries(zhCN)) {
      const source = [...value.matchAll(placeholder)].map((match) => match[0]).sort();
      for (const [language, dictionary] of [
        ["zh-TW", zhTW],
        ["en", en],
      ] as const) {
        const translated = [...(dictionary[key] ?? "").matchAll(placeholder)]
          .map((match) => match[0])
          .sort();
        expect(translated, `${language} ${key} 占位符不一致`).toEqual(source);
      }
    }
  });
});
