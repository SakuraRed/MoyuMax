import { describe, expect, it } from "vitest";

import {
  describeExitImpact,
  impactRequiresConfirmation,
  routeCloseRequest,
} from "./close-flow";
import type { ExitImpact } from "./runtime";

function impact(overrides: Partial<ExitImpact> = {}): ExitImpact {
  return {
    runningSessions: [],
    activeInstallTasks: 0,
    activeContentTasks: 0,
    executingInstallTasks: 0,
    executingContentTasks: 0,
    pausedTasks: 0,
    ...overrides,
  };
}

describe("routeCloseRequest", () => {
  it("默认每次都打开首次关闭选择对话框", () => {
    expect(routeCloseRequest("ask", impact())).toBe("choice-dialog");
    expect(
      routeCloseRequest("ask", impact({ activeInstallTasks: 2 })),
    ).toBe("choice-dialog");
  });

  it("记住最小化到托盘后直接最小化,不检查影响", () => {
    expect(routeCloseRequest("minimizeToTray", impact())).toBe("minimize");
    expect(
      routeCloseRequest(
        "minimizeToTray",
        impact({
          runningSessions: [
            { sessionId: "s1", instanceId: "i1", instanceName: "生存" },
          ],
        }),
      ),
    ).toBe("minimize");
  });

  it("记住退出时无影响直接退出,有影响打开确认对话框", () => {
    expect(routeCloseRequest("exit", impact())).toBe("exit");
    expect(routeCloseRequest("exit", impact({ pausedTasks: 3 }))).toBe("exit");
    expect(routeCloseRequest("exit", impact({ activeContentTasks: 1 }))).toBe(
      "impact-dialog",
    );
    expect(
      routeCloseRequest(
        "exit",
        impact({
          runningSessions: [
            { sessionId: "s1", instanceId: "i1", instanceName: "生存" },
          ],
        }),
      ),
    ).toBe("impact-dialog");
  });
});

describe("impactRequiresConfirmation", () => {
  it("运行中游戏与活动任务需要确认,已暂停任务不需要", () => {
    expect(impactRequiresConfirmation(impact())).toBe(false);
    expect(impactRequiresConfirmation(impact({ pausedTasks: 2 }))).toBe(false);
    expect(impactRequiresConfirmation(impact({ activeInstallTasks: 1 }))).toBe(true);
    expect(
      impactRequiresConfirmation(
        impact({
          runningSessions: [
            { sessionId: "s1", instanceId: "i1", instanceName: "生存" },
          ],
        }),
      ),
    ).toBe(true);
  });
});

describe("describeExitImpact", () => {
  it("按影响类型生成用户可读清单", () => {
    const lines = describeExitImpact(
      impact({
        runningSessions: [
          { sessionId: "s1", instanceId: "i1", instanceName: "天空工厂" },
        ],
        activeInstallTasks: 1,
        activeContentTasks: 1,
        pausedTasks: 2,
      }),
    );
    expect(lines).toHaveLength(3);
    expect(lines[0]!.text).toContain("天空工厂");
    expect(lines[0]!.text).toContain("退出备份");
    expect(lines[0]!.danger).toBe(true);
    expect(lines[1]!.text).toContain("2 个任务");
    expect(lines[2]!.text).toContain("2 个任务已暂停");
  });
});
