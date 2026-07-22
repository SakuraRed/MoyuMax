import { describe, expect, test } from "vitest";

import {
  defaultInstanceName,
  formatBytes,
  installStageLabel,
  recommendedFabricLoader,
  recommendedVersion,
} from "./installation";

describe("安装默认值", () => {
  test("使用官方推荐稳定版而不是写死列表第一项", () => {
    const selected = recommendedVersion([
      {
        id: "snapshot",
        releaseType: "snapshot",
        releaseTime: "",
        metadataUrl: "https://example.invalid/snapshot",
        metadataSha1: "1",
        recommended: false,
      },
      {
        id: "release",
        releaseType: "release",
        releaseTime: "",
        metadataUrl: "https://example.invalid/release",
        metadataSha1: "2",
        recommended: true,
      },
    ]);
    expect(selected?.id).toBe("release");
  });

  test("使用兼容列表中标记推荐的 Fabric Loader", () => {
    expect(
      recommendedFabricLoader([
        { version: "0.16.13", stable: true, recommended: false },
        { version: "0.16.14", stable: true, recommended: true },
      ])?.version,
    ).toBe("0.16.14");
  });

  test("名称、空间和任务阶段使用用户语言", () => {
    expect(defaultInstanceName("1.21.8", { kind: "fabric", version: "0.16.14" })).toBe(
      "1.21.8 Fabric",
    );
    expect(formatBytes(1_934_524_416)).toBe("1.8 GiB");
    expect(installStageLabel("installGameEnvironment")).toBe("安装游戏环境");
  });
});
