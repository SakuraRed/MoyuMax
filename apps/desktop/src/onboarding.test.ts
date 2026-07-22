import { describe, expect, test } from "vitest";

import {
  buildSelection,
  createOnboardingState,
  goBack,
  goForward,
  updateOnboardingDraft,
} from "./onboarding";
import type { OnboardingSelection } from "./runtime";

const defaults: OnboardingSelection = {
  language: "zh-CN",
  dataDirectory: "D:\\MoyuMax\\data",
  telemetryEnabled: false,
  updateChecksEnabled: true,
  natDetectionEnabled: false,
  instanceIsolationEnabled: true,
};

describe("M1 首次运行状态机", () => {
  test("M1-FIRST-RUN-001 使用后端安全默认值进入第一步", () => {
    const state = createOnboardingState(defaults);

    expect(state.step).toBe("language");
    expect(state.draft).toEqual(defaults);
  });

  test("M1-FIRST-RUN-002 按视觉顺序前进和返回", () => {
    const initial = createOnboardingState(defaults);
    const dataStep = goForward(initial);
    const privacyStep = goForward(dataStep);

    expect(dataStep.step).toBe("data");
    expect(privacyStep.step).toBe("privacy");
    expect(goBack(privacyStep).step).toBe("data");
    expect(goForward(privacyStep).step).toBe("done");
  });

  test("M1-FIRST-RUN-002 汇总提交用户确认的不可变快照", () => {
    const initial = createOnboardingState(defaults);
    const changed = updateOnboardingDraft(initial, {
      language: "en",
      dataDirectory: "E:\\Games\\MoyuMax",
      telemetryEnabled: true,
    });

    expect(buildSelection(changed)).toEqual({
      ...defaults,
      language: "en",
      dataDirectory: "E:\\Games\\MoyuMax",
      telemetryEnabled: true,
    });
    expect(initial.draft).toEqual(defaults);
  });
});
