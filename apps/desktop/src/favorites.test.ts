import { describe, expect, it } from "vitest";

import {
  isFavorite,
  listFavorites,
  toggleFavorite,
  type FavoriteProjectInput,
} from "./favorites.svelte";

function entry(projectId: string): FavoriteProjectInput {
  return {
    projectId,
    slug: projectId.toLowerCase(),
    title: `项目 ${projectId}`,
    iconUrl: null,
    type: "mod",
  };
}

describe("favorites store", () => {
  it("切换收藏后加入列表并再次切换后移除", () => {
    expect(isFavorite("T001")).toBe(false);
    expect(toggleFavorite(entry("T001"))).toBe(true);
    expect(isFavorite("T001")).toBe(true);
    expect(toggleFavorite(entry("T001"))).toBe(false);
    expect(isFavorite("T001")).toBe(false);
  });

  it("新收藏排在最前并记录时间戳", () => {
    toggleFavorite(entry("T002"));
    toggleFavorite(entry("T003"));
    const mine = listFavorites().filter((candidate) =>
      ["T002", "T003"].includes(candidate.projectId),
    );
    expect(mine.map((candidate) => candidate.projectId)).toEqual(["T003", "T002"]);
    expect(mine[0]?.addedAtUnixSeconds).toBeGreaterThan(0);
    toggleFavorite(entry("T002"));
    toggleFavorite(entry("T003"));
  });

  it("重复收藏同一项目不产生重复条目", () => {
    toggleFavorite(entry("T004"));
    toggleFavorite(entry("T004"));
    toggleFavorite(entry("T004"));
    const mine = listFavorites().filter((candidate) => candidate.projectId === "T004");
    expect(mine).toHaveLength(1);
    toggleFavorite(entry("T004"));
    expect(isFavorite("T004")).toBe(false);
  });
});
