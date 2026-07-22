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

export type CatalogSource = "network" | "cache";
export type GameReleaseType = "release" | "snapshot" | "oldBeta" | "oldAlpha" | "unknown";

export interface GameVersionSummary {
  id: string;
  releaseType: GameReleaseType;
  releaseTime: string;
  metadataUrl: string;
  metadataSha1: string;
  recommended: boolean;
}

export interface VersionCatalog {
  latestRelease: string;
  latestSnapshot: string;
  versions: GameVersionSummary[];
  fetchedAtUnixSeconds: number;
  source: CatalogSource;
}

export interface FabricLoaderSummary {
  version: string;
  stable: boolean;
  recommended: boolean;
}

export type LoaderChoice =
  | { kind: "vanilla" }
  | { kind: "fabric"; version: string };

export type InstanceIsolation = "full" | "sharedBase" | "disabled";

export interface InstallSelection {
  instanceName: string;
  gameVersion: GameVersionSummary;
  loader: LoaderChoice;
  isolation: InstanceIsolation;
}

export interface InstallPreview {
  id: string;
  instanceName: string;
  gameVersion: string;
  loaderName: string;
  loaderVersion: string | null;
  javaDistribution: "azulZulu";
  javaVersion: string;
  javaArchitecture: "x64";
  isolation: InstanceIsolation;
  estimatedDownloadBytes: number;
}

export type InstallStage =
  | "prepare"
  | "downloadGameFiles"
  | "verifyFiles"
  | "installGameEnvironment"
  | "applyLoader"
  | "commitChanges"
  | "createRollbackPoint";

export type TaskState =
  | "queued"
  | "running"
  | "committing"
  | "paused"
  | "awaitingRecovery"
  | "failed"
  | "completed"
  | "cancelled";

export type RecoveryDecision = "resume" | "discard";

export interface InstallTask {
  id: string;
  state: TaskState;
  currentStage: InstallStage | null;
  plan: {
    schemaVersion: number;
    instanceId: string;
    instanceName: string;
    targetDirectory: string;
    stages: InstallStage[];
    estimatedDownloadBytes: number;
  };
  stagingDirectory: string;
  targetDirectory: string;
  createdAtUnixSeconds: number;
  updatedAtUnixSeconds: number;
  progress: {
    completedBytes: number;
    totalBytes: number | null;
    currentItem: string | null;
    errorSummary: string | null;
  };
}

export interface MoyuRuntime {
  getBootstrapState(): Promise<BootstrapState>;
  completeOnboarding(selection: OnboardingSelection): Promise<void>;
  skipOnboarding(): Promise<void>;
  getGameVersionCatalog(): Promise<VersionCatalog>;
  getFabricLoaders(gameVersion: string): Promise<FabricLoaderSummary[]>;
  previewInstall(selection: InstallSelection): Promise<InstallPreview>;
  confirmInstallPreview(previewId: string): Promise<InstallTask>;
  getInstallTasks(): Promise<InstallTask[]>;
  resolveInstallTaskRecovery(taskId: string, decision: RecoveryDecision): Promise<void>;
  retryInstallTask(taskId: string): Promise<void>;
  minimizeWindow(): Promise<void>;
  toggleMaximizeWindow(): Promise<void>;
  closeWindow(): Promise<void>;
}

const BROWSER_STORAGE_KEY = "moyumax.browser.onboarding";
const BROWSER_TASKS_KEY = "moyumax.browser.installTasks";
const browserPreviews = new Map<string, InstallSelection>();

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
    getGameVersionCatalog: () =>
      invoke<VersionCatalog>("get_game_version_catalog"),
    getFabricLoaders: (gameVersion) =>
      invoke<FabricLoaderSummary[]>("get_fabric_loaders", { gameVersion }),
    previewInstall: (selection) =>
      invoke<InstallPreview>("preview_install", { selection }),
    confirmInstallPreview: (previewId) =>
      invoke<InstallTask>("confirm_install_preview", { previewId }),
    getInstallTasks: () => invoke<InstallTask[]>("get_install_tasks"),
    resolveInstallTaskRecovery: (taskId, decision) =>
      invoke<void>("resolve_install_task_recovery", { taskId, decision }),
    retryInstallTask: (taskId) => invoke<void>("retry_install_task", { taskId }),
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
    async getGameVersionCatalog() {
      return browserVersionCatalog();
    },
    async getFabricLoaders() {
      return [
        { version: "0.16.14", stable: true, recommended: true },
        { version: "0.16.13", stable: true, recommended: false },
      ];
    },
    async previewInstall(selection) {
      const id = crypto.randomUUID();
      browserPreviews.set(id, selection);
      return {
        id,
        instanceName: selection.instanceName,
        gameVersion: selection.gameVersion.id,
        loaderName: selection.loader.kind === "fabric" ? "Fabric" : "原版",
        loaderVersion:
          selection.loader.kind === "fabric" ? selection.loader.version : null,
        javaDistribution: "azulZulu",
        javaVersion: "21.0.12+8",
        javaArchitecture: "x64",
        isolation: selection.isolation,
        estimatedDownloadBytes: 1_934_524_416,
      };
    },
    async confirmInstallPreview(previewId) {
      const selection = browserPreviews.get(previewId);
      if (!selection) throw new Error("安装预览已失效，请返回重新确认");
      browserPreviews.delete(previewId);
      const now = Math.floor(Date.now() / 1000);
      const id = crypto.randomUUID();
      const stages: InstallStage[] = [
        "prepare",
        "downloadGameFiles",
        "verifyFiles",
        "installGameEnvironment",
        "applyLoader",
        "commitChanges",
        "createRollbackPoint",
      ];
      const task: InstallTask = {
        id,
        state: "queued",
        currentStage: "prepare",
        plan: {
          schemaVersion: 1,
          instanceId: crypto.randomUUID(),
          instanceName: selection.instanceName,
          targetDirectory: `D:\\MoyuMax\\data\\instances\\${id}`,
          stages,
          estimatedDownloadBytes: 1_934_524_416,
        },
        stagingDirectory: `D:\\MoyuMax\\data\\.staging\\install\\${id}`,
        targetDirectory: `D:\\MoyuMax\\data\\instances\\${id}`,
        createdAtUnixSeconds: now,
        updatedAtUnixSeconds: now,
        progress: {
          completedBytes: 0,
          totalBytes: 1_934_524_416,
          currentItem: "等待执行",
          errorSummary: null,
        },
      };
      const tasks = browserInstallTasks();
      tasks.push(task);
      window.localStorage.setItem(BROWSER_TASKS_KEY, JSON.stringify(tasks));
      return task;
    },
    async getInstallTasks() {
      return browserInstallTasks();
    },
    async resolveInstallTaskRecovery(taskId, decision) {
      const tasks = browserInstallTasks();
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task || task.state !== "awaitingRecovery") {
        throw new Error("任务当前不需要恢复确认");
      }
      task.state = decision === "resume" ? "queued" : "cancelled";
      task.updatedAtUnixSeconds = Math.floor(Date.now() / 1000);
      window.localStorage.setItem(BROWSER_TASKS_KEY, JSON.stringify(tasks));
    },
    async retryInstallTask(taskId) {
      const tasks = browserInstallTasks();
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task || task.state !== "failed") throw new Error("任务当前不能重试");
      task.state = "queued";
      task.currentStage = "prepare";
      task.progress.currentItem = "等待重试执行";
      task.progress.errorSummary = null;
      task.updatedAtUnixSeconds = Math.floor(Date.now() / 1000);
      window.localStorage.setItem(BROWSER_TASKS_KEY, JSON.stringify(tasks));
    },
    async minimizeWindow() {},
    async toggleMaximizeWindow() {},
    async closeWindow() {},
  };
}

function browserInstallTasks(): InstallTask[] {
  const serialized = window.localStorage.getItem(BROWSER_TASKS_KEY);
  return serialized ? (JSON.parse(serialized) as InstallTask[]) : [];
}

function browserVersionCatalog(): VersionCatalog {
  const release: GameVersionSummary = {
    id: "1.21.8",
    releaseType: "release",
    releaseTime: "2026-07-17T12:00:00+00:00",
    metadataUrl: "https://piston-meta.mojang.com/v1/packages/release.json",
    metadataSha1: "1111111111111111111111111111111111111111",
    recommended: true,
  };
  return {
    latestRelease: release.id,
    latestSnapshot: "25w30a",
    versions: [
      release,
      {
        ...release,
        id: "1.21.7",
        releaseTime: "2026-06-30T12:00:00+00:00",
        recommended: false,
      },
      {
        ...release,
        id: "1.20.1",
        releaseTime: "2023-06-12T12:00:00+00:00",
        recommended: false,
      },
    ],
    fetchedAtUnixSeconds: Math.floor(Date.now() / 1000),
    source: "network",
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
