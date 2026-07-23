import { describe, expect, it } from "vitest";

import { isRestorablePage, sanitizeShellState } from "./shell-state";

describe("sanitizeShellState", () => {
  it("保留白名单页面与滚动位置", () => {
    expect(sanitizeShellState({ page: "tasks", scrollTop: 320 })).toEqual({
      page: "tasks",
      scrollTop: 320,
    });
  });

  it("数据损坏、未知页面与非法滚动值回退首页", () => {
    expect(sanitizeShellState(null)).toBeNull();
    expect(sanitizeShellState("tasks")).toBeNull();
    expect(sanitizeShellState({ page: "settings", scrollTop: 10 })).toBeNull();
    expect(sanitizeShellState({ scrollTop: 10 })).toBeNull();
  });

  it("非法滚动位置归零而不是拒绝页面", () => {
    expect(sanitizeShellState({ page: "home", scrollTop: -5 })).toEqual({
      page: "home",
      scrollTop: 0,
    });
    expect(sanitizeShellState({ page: "home", scrollTop: Number.NaN })).toEqual({
      page: "home",
      scrollTop: 0,
    });
  });

  it("敏感明文页面即使已持久化也回退首页", () => {
    const sensitivePages = new Set(["accounts-vault"]);
    expect(
      sanitizeShellState({ page: "accounts-vault", scrollTop: 10 }, { sensitivePages }),
    ).toBeNull();
    expect(
      sanitizeShellState({ page: "tasks", scrollTop: 10 }, { sensitivePages }),
    ).toEqual({ page: "tasks", scrollTop: 10 });
  });
});

describe("isRestorablePage", () => {
  it("只接受声明过的壳层页面", () => {
    for (const page of ["home", "install", "resources", "tasks", "data", "crash"]) {
      expect(isRestorablePage(page)).toBe(true);
    }
    expect(isRestorablePage("settings")).toBe(false);
    expect(isRestorablePage("loading")).toBe(false);
    expect(isRestorablePage("")).toBe(false);
  });
});
