import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type Language = "zh-CN" | "zh-TW" | "en";

export interface OnboardingSelection {
  language: Language;
  dataDirectory: string;
  telemetryEnabled: boolean;
  updateChecksEnabled: boolean;
  natDetectionEnabled: boolean;
  instanceIsolationEnabled: boolean;
}

export interface BootstrapState {
  requiresOnboarding: boolean;
  defaultDataDirectory: string;
  defaults: OnboardingSelection;
  settings: OnboardingSelection | null;
}

export interface MoyuRuntime {
  getBootstrapState(): Promise<BootstrapState>;
  completeOnboarding(selection: OnboardingSelection): Promise<void>;
  skipOnboarding(): Promise<void>;
  minimizeWindow(): Promise<void>;
  toggleMaximizeWindow(): Promise<void>;
  closeWindow(): Promise<void>;
}

const BROWSER_STORAGE_KEY = "moyumax.browser.onboarding";

export function createRuntime(): MoyuRuntime {
  return Reflect.has(window, "__TAURI_INTERNALS__")
    ? createTauriRuntime()
    : createBrowserRuntime();
}

function createTauriRuntime(): MoyuRuntime {
  const currentWindow = getCurrentWindow();

  return {
    getBootstrapState: () => invoke<BootstrapState>("get_bootstrap_state"),
    completeOnboarding: (selection) =>
      invoke<void>("complete_onboarding", { selection }),
    skipOnboarding: () => invoke<void>("skip_onboarding"),
    minimizeWindow: () => currentWindow.minimize(),
    toggleMaximizeWindow: () => currentWindow.toggleMaximize(),
    closeWindow: () => currentWindow.close(),
  };
}

function createBrowserRuntime(): MoyuRuntime {
  const recommended = recommendedBrowserSelection();

  return {
    async getBootstrapState() {
      const serialized = window.localStorage.getItem(BROWSER_STORAGE_KEY);
      const settings = serialized
        ? (JSON.parse(serialized) as OnboardingSelection)
        : null;
      return {
        requiresOnboarding: settings === null,
        defaultDataDirectory: recommended.dataDirectory,
        defaults: recommended,
        settings,
      };
    },
    async completeOnboarding(selection) {
      window.localStorage.setItem(BROWSER_STORAGE_KEY, JSON.stringify(selection));
    },
    async skipOnboarding() {
      window.localStorage.setItem(
        BROWSER_STORAGE_KEY,
        JSON.stringify(recommended),
      );
    },
    async minimizeWindow() {},
    async toggleMaximizeWindow() {},
    async closeWindow() {},
  };
}

function recommendedBrowserSelection(): OnboardingSelection {
  return {
    language: "zh-CN",
    dataDirectory: "D:\\MoyuMax\\data",
    telemetryEnabled: false,
    updateChecksEnabled: true,
    natDetectionEnabled: false,
    instanceIsolationEnabled: true,
  };
}
