import { describe, expect, it } from "vitest";

import type { ModrinthVersionSummary } from "./runtime";
import {
  buildVersionGroups,
  primaryGameVersion,
  versionGameTags,
  versionOptionLabel,
} from "./version-groups";

function version(
  id: string,
  versionNumber: string,
  gameVersions: string[],
  loaders: string[] = ["fabric"],
  datePublished = "2026-01-01",
  versionType = "release",
): ModrinthVersionSummary {
  return {
    id,
    versionNumber,
    versionType,
    datePublished,
    gameVersions,
    loaders,
    downloads: 0,
  };
}

describe("buildVersionGroups", () => {
  it("按 MC 精确版本分组,组降序、组内按发布日期降序", () => {
    const groups = buildVersionGroups(
      [
        version("a", "1.0", ["1.20.1"], ["fabric"], "2026-01-01"),
        version("b", "2.0", ["1.21.1"], ["fabric"], "2026-02-01"),
        version("c", "1.5", ["1.20.1"], ["fabric"], "2026-03-01"),
      ],
      { kind: "mod" },
    );
    expect(groups.map((group) => group.key)).toEqual(["1.21.1", "1.20.1"]);
    expect(groups[1]?.versions.map((entry) => entry.versionNumber)).toEqual(["1.5", "1.0"]);
  });

  it("跨版本文件同时出现在多个组", () => {
    const groups = buildVersionGroups([version("a", "1.0", ["1.21.1", "1.20.1"])], {
      kind: "mod",
    });
    expect(groups.map((group) => group.key)).toEqual(["1.21.1", "1.20.1"]);
  });

  it("多加载器模组按 加载器×版本 复合分组", () => {
    const groups = buildVersionGroups(
      [
        version("a", "1.0", ["1.21.1"], ["fabric"]),
        version("b", "1.0", ["1.21.1"], ["neoforge"]),
      ],
      { kind: "mod" },
    );
    expect(groups.map((group) => group.key).sort()).toEqual([
      "fabric 1.21.1",
      "neoforge 1.21.1",
    ]);
  });

  it("整合包版本只归属最高 MC 版本", () => {
    const multi = version("a", "1.0", ["1.20.1", "1.21.1", "1.21.4"]);
    expect(primaryGameVersion(multi)).toBe("1.21.4");
    const groups = buildVersionGroups([multi], { kind: "modpack" });
    expect(groups).toHaveLength(1);
    expect(groups[0]?.key).toBe("1.21.4");
  });

  it("快照与未知版本沉底", () => {
    const groups = buildVersionGroups(
      [
        version("a", "1.0", ["24w14a"]),
        version("b", "1.0", ["1.21.1"]),
        version("c", "1.0", ["未知"]),
      ],
      { kind: "mod" },
    );
    expect(groups.map((group) => group.key)).toEqual(["1.21.1", "__snapshot__", "__unknown__"]);
  });

  it("与目标实例匹配的组置顶并标记推荐", () => {
    const groups = buildVersionGroups(
      [
        version("a", "1.0", ["1.20.1"]),
        version("b", "2.0", ["1.21.1"]),
      ],
      { kind: "shader", target: { gameVersion: "1.20.1", loaderKind: "iris" } },
    );
    expect(groups[0]?.key).toBe("1.20.1");
    expect(groups[0]?.recommended).toBe(true);
    expect(groups[1]?.recommended).toBe(false);
  });

  it("整合包不做实例推荐", () => {
    const groups = buildVersionGroups([version("a", "1.0", ["1.21.1"])], {
      kind: "modpack",
      target: { gameVersion: "1.21.1", loaderKind: "neoforge" },
    });
    expect(groups[0]?.recommended).toBe(false);
  });
});

describe("versionOptionLabel / versionGameTags", () => {
  it("非 release 标注类型", () => {
    expect(versionOptionLabel(version("a", "1.0", ["1.21.1"], ["fabric"], "2026-01-01", "beta"))).toBe(
      "1.0 (beta)",
    );
    expect(versionOptionLabel(version("a", "1.0", ["1.21.1"]))).toBe("1.0");
  });

  it("游戏版本标签涵盖全部版本并按降序", () => {
    expect(versionGameTags(version("a", "1.0", ["1.20.1", "1.21.1", "24w14a"]))).toBe(
      "1.21.1、1.20.1、24w14a",
    );
  });
});
