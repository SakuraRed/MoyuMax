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

export interface ManagedInstance {
  id: string;
  name: string;
  gameVersion: string;
  loaderKind: string;
  loaderVersion: string | null;
  rootDirectory: string;
  state: string;
}

export type RecycleItemKind = "instance";
export type RecycleItemState = "moving" | "ready" | "restoring" | "purging" | "failed";

export interface RecycleBinItem {
  id: string;
  kind: RecycleItemKind;
  subjectId: string;
  displayName: string;
  originalPath: string;
  recycledPath: string;
  originalState: string;
  sizeBytes: number;
  deletedAtUnixSeconds: number;
  expiresAtUnixSeconds: number;
  state: RecycleItemState;
}

export interface RecyclePurgeResult {
  itemId: string;
  releasedBytes: number;
  removedSubjects: number;
}

export type ModrinthSearchIndex = "relevance" | "downloads" | "follows" | "newest" | "updated";

export interface ModrinthSearchQuery {
  query: string;
  gameVersion: string;
  loader: string;
  index: ModrinthSearchIndex;
  offset: number;
  limit: number;
}

export interface ModrinthProjectSummary {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  downloads: number;
  clientSide: string;
  serverSide: string;
}

export interface ModrinthSearchPage {
  hits: ModrinthProjectSummary[];
  offset: number;
  limit: number;
  totalHits: number;
}

export type ContentDependencyKind = "required" | "optional" | "incompatible" | "embedded";

export interface ContentDependencyChoice {
  projectId: string | null;
  versionId: string | null;
  title: string;
  kind: ContentDependencyKind;
  requiredByProjectId: string;
}

export interface ContentFilePlan {
  url: string;
  filename: string;
  size: number;
  sha1: string;
  sha512: string;
}

export interface ContentPlanEntry {
  projectId: string;
  versionId: string;
  projectTitle: string;
  versionNumber: string;
  requiredByProjectId: string | null;
  file: ContentFilePlan;
}

export interface ContentInstallPlan {
  schemaVersion: number;
  instanceId: string;
  instanceName: string;
  gameVersion: string;
  loader: string;
  rootProjectId: string;
  entries: ContentPlanEntry[];
  optionalDependencies: ContentDependencyChoice[];
  incompatibleDependencies: ContentDependencyChoice[];
}

export interface ContentInstallPreview {
  id: string;
  plan: ContentInstallPlan;
}

export type ContentInstallStage =
  | "prepare"
  | "downloadFiles"
  | "verifyFiles"
  | "commitFiles"
  | "indexContent";

export interface ContentInstallTask {
  id: string;
  state: TaskState;
  currentStage: ContentInstallStage | null;
  plan: ContentInstallPlan;
  stagingDirectory: string;
  targetDirectory: string;
  sharedStoreDirectory: string;
  createdAtUnixSeconds: number;
  updatedAtUnixSeconds: number;
  progress: InstallTask["progress"];
}

export interface InstalledContent {
  id: string;
  instanceId: string;
  provider: "modrinth";
  projectId: string;
  versionId: string;
  projectTitle: string;
  versionNumber: string;
  fileName: string;
  relativePath: string;
  size: number;
  sha1: string;
  sha512: string;
  enabled: boolean;
  autoUpdateEnabled: boolean;
  installedAtUnixSeconds: number;
}

export type BackupTrigger = "preLaunch" | "postExit" | "manual";
export type BackupState = "staging" | "ready" | "skipped" | "failed";

export interface WorldBackupSummary {
  id: string;
  instanceId: string;
  instanceName: string;
  launchSessionId: string | null;
  trigger: BackupTrigger;
  state: BackupState;
  archivePath: string | null;
  worldCount: number;
  sourceBytes: number;
  archiveBytes: number;
  createdAtUnixSeconds: number;
  completedAtUnixSeconds: number | null;
  errorSummary: string | null;
}

export type LaunchSessionState =
  | "starting"
  | "running"
  | "completed"
  | "failed"
  | "stopped"
  | "interrupted";

export interface LaunchSession {
  id: string;
  instanceId: string;
  playerName: string;
  state: LaunchSessionState;
  startedAtUnixSeconds: number;
  endedAtUnixSeconds: number | null;
  exitCode: number | null;
  stdoutPath: string;
  stderrPath: string;
  errorSummary: string | null;
  preLaunchBackup?: WorldBackupSummary | null;
  postExitBackup?: WorldBackupSummary | null;
}

export type CrashCauseKind =
  | "outOfMemory"
  | "modConflict"
  | "javaRuntime"
  | "nativeCrash"
  | "launcherInterrupted"
  | "unknown";

export type CrashEvidenceKind =
  | "gameOutput"
  | "gameLog"
  | "gameCrashReport"
  | "nativeCrash"
  | "launcherLog"
  | "launchScript"
  | "environment";

export interface CrashEvidenceItem {
  kind: CrashEvidenceKind;
  bundleName: string;
  originalBytes: number;
  includedBytes: number;
  truncated: boolean;
}

export interface CrashReport {
  schemaVersion: number;
  id: string;
  launchSessionId: string;
  instanceId: string;
  createdAtUnixSeconds: number;
  cause: CrashCauseKind;
  title: string;
  summary: string;
  recommendations: string[];
  evidence: CrashEvidenceItem[];
  redactionSummary: string[];
}

export interface DiagnosticExportFile {
  bundleName: string;
  includedBytes: number;
  truncated: boolean;
}

export interface DiagnosticExportPreview {
  id: string;
  reportId: string;
  suggestedFileName: string;
  files: DiagnosticExportFile[];
  totalBytes: number;
  maximumEvidenceBytes: number;
  redactions: string[];
}

export interface DiagnosticExportResult {
  reportId: string;
  archivePath: string;
  archiveBytes: number;
  fileCount: number;
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
  searchModrinthMods(query: ModrinthSearchQuery): Promise<ModrinthSearchPage>;
  previewModrinthInstall(
    instanceId: string,
    projectId: string,
    selectedOptionalProjects: string[],
  ): Promise<ContentInstallPreview>;
  confirmContentPreview(previewId: string): Promise<ContentInstallTask>;
  getContentInstallTasks(): Promise<ContentInstallTask[]>;
  getInstalledContent(instanceId: string): Promise<InstalledContent[]>;
  retryContentTask(taskId: string): Promise<void>;
  resolveContentTaskRecovery(taskId: string, decision: RecoveryDecision): Promise<void>;
  listInstances(): Promise<ManagedInstance[]>;
  listRecycleBinItems(): Promise<RecycleBinItem[]>;
  recycleInstance(instanceId: string): Promise<RecycleBinItem>;
  restoreRecycleBinItem(itemId: string): Promise<ManagedInstance>;
  purgeRecycleBinItem(itemId: string): Promise<RecyclePurgeResult>;
  listWorldBackups(instanceId?: string): Promise<WorldBackupSummary[]>;
  startInstance(instanceId: string): Promise<LaunchSession>;
  stopInstance(instanceId: string): Promise<void>;
  listLaunchSessions(): Promise<LaunchSession[]>;
  listCrashReports(): Promise<CrashReport[]>;
  previewDiagnosticExport(reportId: string): Promise<DiagnosticExportPreview>;
  confirmDiagnosticExport(previewId: string): Promise<DiagnosticExportResult>;
  minimizeWindow(): Promise<void>;
  toggleMaximizeWindow(): Promise<void>;
  closeWindow(): Promise<void>;
}

const BROWSER_STORAGE_KEY = "moyumax.browser.onboarding";
const BROWSER_TASKS_KEY = "moyumax.browser.installTasks";
const BROWSER_INSTANCES_KEY = "moyumax.browser.instances";
const BROWSER_RECYCLE_BIN_KEY = "moyumax.browser.recycleBin";
const BROWSER_WORLD_BACKUPS_KEY = "moyumax.browser.worldBackups";
const BROWSER_LAUNCH_SESSIONS_KEY = "moyumax.browser.launchSessions";
const BROWSER_CRASH_REPORTS_KEY = "moyumax.browser.crashReports";
const BROWSER_CONTENT_TASKS_KEY = "moyumax.browser.contentTasks";
const BROWSER_INSTALLED_CONTENT_KEY = "moyumax.browser.installedContent";
const BROWSER_MODRINTH_OFFLINE_KEY = "moyumax.browser.modrinthOffline";
const browserPreviews = new Map<string, InstallSelection>();
const browserContentPreviews = new Map<string, ContentInstallPlan>();
const browserDiagnosticPreviews = new Map<string, string>();

interface BrowserRecycleEntry extends RecycleBinItem {
  instance: ManagedInstance;
}

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
    searchModrinthMods: (query) =>
      invoke<ModrinthSearchPage>("search_modrinth_mods", { query }),
    previewModrinthInstall: (instanceId, projectId, selectedOptionalProjects) =>
      invoke<ContentInstallPreview>("preview_modrinth_install", {
        instanceId,
        projectId,
        selectedOptionalProjects,
      }),
    confirmContentPreview: (previewId) =>
      invoke<ContentInstallTask>("confirm_content_preview", { previewId }),
    getContentInstallTasks: () =>
      invoke<ContentInstallTask[]>("get_content_install_tasks"),
    getInstalledContent: (instanceId) =>
      invoke<InstalledContent[]>("get_installed_content", { instanceId }),
    retryContentTask: (taskId) => invoke<void>("retry_content_task", { taskId }),
    resolveContentTaskRecovery: (taskId, decision) =>
      invoke<void>("resolve_content_task_recovery", { taskId, decision }),
    listInstances: () => invoke<ManagedInstance[]>("list_instances"),
    listRecycleBinItems: () =>
      invoke<RecycleBinItem[]>("list_recycle_bin_items"),
    recycleInstance: (instanceId) =>
      invoke<RecycleBinItem>("recycle_instance", { instanceId }),
    restoreRecycleBinItem: (itemId) =>
      invoke<ManagedInstance>("restore_recycle_bin_item", { itemId }),
    purgeRecycleBinItem: (itemId) =>
      invoke<RecyclePurgeResult>("purge_recycle_bin_item", { itemId }),
    listWorldBackups: (instanceId) =>
      invoke<WorldBackupSummary[]>("list_world_backups", {
        instanceId: instanceId ?? null,
      }),
    startInstance: (instanceId) =>
      invoke<LaunchSession>("start_instance", { instanceId }),
    stopInstance: (instanceId) => invoke<void>("stop_instance", { instanceId }),
    listLaunchSessions: () =>
      invoke<LaunchSession[]>("list_launch_sessions"),
    listCrashReports: () => invoke<CrashReport[]>("list_crash_reports"),
    previewDiagnosticExport: (reportId) =>
      invoke<DiagnosticExportPreview>("preview_diagnostic_export", { reportId }),
    confirmDiagnosticExport: (previewId) =>
      invoke<DiagnosticExportResult>("confirm_diagnostic_export", { previewId }),
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
    async searchModrinthMods(query) {
      if (window.localStorage.getItem(BROWSER_MODRINTH_OFFLINE_KEY) === "true") {
        throw new Error("无法连接 Modrinth：浏览器测试环境处于离线状态");
      }
      const hit: ModrinthProjectSummary = {
        projectId: "ROOT0001",
        slug: "continuity",
        title: "Continuity",
        description: "为方块纹理提供连续连接效果。",
        downloads: 42,
        clientSide: "required",
        serverSide: "optional",
      };
      const matches = `${hit.title} ${hit.description}`
        .toLocaleLowerCase()
        .includes(query.query.trim().toLocaleLowerCase());
      return {
        hits: matches ? [hit] : [],
        offset: query.offset,
        limit: query.limit,
        totalHits: matches ? 1 : 0,
      };
    },
    async previewModrinthInstall(instanceId, projectId, selectedOptionalProjects) {
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance || instance.loaderKind !== "fabric" || instance.state !== "ready") {
        throw new Error("目标实例不存在或不是可用的 Fabric 实例");
      }
      if (projectId !== "ROOT0001") throw new Error("浏览器测试来源中没有该项目");
      const optionalSelected = selectedOptionalProjects.includes("OPT00001");
      const plan: ContentInstallPlan = {
        schemaVersion: 1,
        instanceId,
        instanceName: instance.name,
        gameVersion: instance.gameVersion,
        loader: instance.loaderKind,
        rootProjectId: projectId,
        entries: [
          browserContentEntry(
            "DEP00001",
            "DEPVER01",
            "Fabric API",
            "fabric-api.jar",
            "ROOT0001",
          ),
          ...(optionalSelected
            ? [
                browserContentEntry(
                  "OPT00001",
                  "OPTVER01",
                  "Mod Menu",
                  "modmenu.jar",
                  "ROOT0001",
                ),
              ]
            : []),
          browserContentEntry(
            "ROOT0001",
            "ROOTVER1",
            "Continuity",
            "continuity.jar",
            null,
          ),
        ],
        optionalDependencies: [
          {
            projectId: "OPT00001",
            versionId: null,
            title: "Mod Menu",
            kind: "optional",
            requiredByProjectId: "ROOT0001",
          },
        ],
        incompatibleDependencies: [],
      };
      const id = crypto.randomUUID();
      browserContentPreviews.set(id, plan);
      return { id, plan };
    },
    async confirmContentPreview(previewId) {
      const plan = browserContentPreviews.get(previewId);
      if (!plan) throw new Error("内容安装预览已失效，请重新确认依赖");
      browserContentPreviews.delete(previewId);
      const id = crypto.randomUUID();
      const now = Math.floor(Date.now() / 1000);
      const totalBytes = plan.entries.reduce((total, entry) => total + entry.file.size, 0);
      const task: ContentInstallTask = {
        id,
        state: "queued",
        currentStage: "prepare",
        plan,
        stagingDirectory: `D:\\MoyuMax\\data\\.staging\\content\\${id}`,
        targetDirectory: `D:\\MoyuMax\\data\\instances\\${plan.instanceId}`,
        sharedStoreDirectory: "D:\\MoyuMax\\data\\store",
        createdAtUnixSeconds: now,
        updatedAtUnixSeconds: now,
        progress: {
          completedBytes: 0,
          totalBytes,
          currentItem: "等待执行",
          errorSummary: null,
        },
      };
      const tasks = browserContentTasks();
      tasks.push(task);
      window.localStorage.setItem(BROWSER_CONTENT_TASKS_KEY, JSON.stringify(tasks));
      return task;
    },
    async getContentInstallTasks() {
      return browserContentTasks();
    },
    async getInstalledContent(instanceId) {
      return browserInstalledContent().filter((entry) => entry.instanceId === instanceId);
    },
    async retryContentTask(taskId) {
      const tasks = browserContentTasks();
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task || task.state !== "failed") throw new Error("内容任务当前不能重试");
      task.state = "queued";
      task.currentStage = "prepare";
      task.progress.currentItem = "等待重试执行";
      task.progress.errorSummary = null;
      task.updatedAtUnixSeconds = Math.floor(Date.now() / 1000);
      window.localStorage.setItem(BROWSER_CONTENT_TASKS_KEY, JSON.stringify(tasks));
    },
    async resolveContentTaskRecovery(taskId, decision) {
      const tasks = browserContentTasks();
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task || task.state !== "awaitingRecovery") {
        throw new Error("内容任务当前不需要恢复确认");
      }
      task.state = decision === "resume" ? "queued" : "cancelled";
      task.updatedAtUnixSeconds = Math.floor(Date.now() / 1000);
      window.localStorage.setItem(BROWSER_CONTENT_TASKS_KEY, JSON.stringify(tasks));
    },
    async listInstances() {
      return browserInstances();
    },
    async listRecycleBinItems() {
      return browserRecycleEntries().map(({ instance: _instance, ...item }) => item);
    },
    async recycleInstance(instanceId) {
      const instances = browserInstances();
      const index = instances.findIndex((candidate) => candidate.id === instanceId);
      if (index < 0) throw new Error("实例不存在或已经在回收站中");
      if (
        browserLaunchSessions().some(
          (session) =>
            session.instanceId === instanceId &&
            ["starting", "running"].includes(session.state),
        )
      ) {
        throw new Error("实例仍在运行，请先停止游戏再移入回收站");
      }
      if (
        browserContentTasks().some(
          (task) =>
            task.plan.instanceId === instanceId &&
            !["completed", "cancelled", "failed"].includes(task.state),
        )
      ) {
        throw new Error("实例仍有未完成的内容任务，请先处理任务再移入回收站");
      }
      const managedInstance = instances[index];
      if (!managedInstance) throw new Error("实例不存在或已经在回收站中");
      instances.splice(index, 1);
      const now = Math.floor(Date.now() / 1000);
      const item: BrowserRecycleEntry = {
        id: `recycle-${crypto.randomUUID()}`,
        kind: "instance",
        subjectId: managedInstance.id,
        displayName: managedInstance.name,
        originalPath: managedInstance.rootDirectory,
        recycledPath: `${recommended.dataDirectory}\\.recycle\\instances\\${managedInstance.id}`,
        originalState: managedInstance.state,
        sizeBytes: 64 * 1024 * 1024,
        deletedAtUnixSeconds: now,
        expiresAtUnixSeconds: now + 30 * 24 * 60 * 60,
        state: "ready",
        instance: managedInstance,
      };
      window.localStorage.setItem(BROWSER_INSTANCES_KEY, JSON.stringify(instances));
      const entries = browserRecycleEntries();
      entries.unshift(item);
      window.localStorage.setItem(BROWSER_RECYCLE_BIN_KEY, JSON.stringify(entries));
      const { instance: _instance, ...summary } = item;
      return summary;
    },
    async restoreRecycleBinItem(itemId) {
      const entries = browserRecycleEntries();
      const index = entries.findIndex((candidate) => candidate.id === itemId);
      const entry = entries[index];
      if (!entry || entry.state !== "ready") {
        throw new Error("该回收站项目当前不能恢复");
      }
      entries.splice(index, 1);
      const restored: ManagedInstance = {
        ...entry.instance,
        rootDirectory: entry.originalPath,
        state: entry.originalState,
      };
      const instances = browserInstances();
      instances.push(restored);
      window.localStorage.setItem(BROWSER_INSTANCES_KEY, JSON.stringify(instances));
      window.localStorage.setItem(BROWSER_RECYCLE_BIN_KEY, JSON.stringify(entries));
      return restored;
    },
    async purgeRecycleBinItem(itemId) {
      const entries = browserRecycleEntries();
      const index = entries.findIndex((candidate) => candidate.id === itemId);
      const entry = entries[index];
      if (!entry || entry.state !== "ready") {
        throw new Error("该回收站项目当前不能永久删除");
      }
      entries.splice(index, 1);
      window.localStorage.setItem(BROWSER_RECYCLE_BIN_KEY, JSON.stringify(entries));
      return {
        itemId: entry.id,
        releasedBytes: entry.sizeBytes,
        removedSubjects: 1,
      };
    },
    async listWorldBackups(instanceId) {
      return browserWorldBackups().filter(
        (backup) => instanceId === undefined || backup.instanceId === instanceId,
      );
    },
    async startInstance(instanceId) {
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance || instance.state !== "ready") {
        throw new Error("实例不存在或当前不可启动");
      }
      const sessions = browserLaunchSessions();
      if (
        sessions.some(
          (session) =>
            session.instanceId === instanceId &&
            ["starting", "running"].includes(session.state),
        )
      ) {
        throw new Error("该实例已经在运行");
      }
      const now = Math.floor(Date.now() / 1000);
      const sessionId = crypto.randomUUID();
      const preLaunchBackup = createBrowserWorldBackup(
        instance,
        sessionId,
        "preLaunch",
      );
      const session: LaunchSession = {
        id: sessionId,
        instanceId,
        playerName: "MoyuMaxPlayer",
        state: "running",
        startedAtUnixSeconds: now,
        endedAtUnixSeconds: null,
        exitCode: null,
        stdoutPath: `${instance.rootDirectory}\\.minecraft\\logs\\moyumax\\browser.stdout.log`,
        stderrPath: `${instance.rootDirectory}\\.minecraft\\logs\\moyumax\\browser.stderr.log`,
        errorSummary: null,
        preLaunchBackup,
        postExitBackup: null,
      };
      sessions.unshift(session);
      window.localStorage.setItem(
        BROWSER_LAUNCH_SESSIONS_KEY,
        JSON.stringify(sessions),
      );
      return session;
    },
    async stopInstance(instanceId) {
      const sessions = browserLaunchSessions();
      const session = sessions.find(
        (candidate) =>
          candidate.instanceId === instanceId &&
          ["starting", "running"].includes(candidate.state),
      );
      if (!session) throw new Error("该实例当前没有可停止的游戏进程");
      session.state = "stopped";
      session.endedAtUnixSeconds = Math.floor(Date.now() / 1000);
      const instance = browserInstances().find(
        (candidate) => candidate.id === instanceId,
      );
      if (instance) {
        session.postExitBackup = createBrowserWorldBackup(
          instance,
          session.id,
          "postExit",
        );
      }
      window.localStorage.setItem(
        BROWSER_LAUNCH_SESSIONS_KEY,
        JSON.stringify(sessions),
      );
    },
    async listLaunchSessions() {
      return browserLaunchSessions();
    },
    async listCrashReports() {
      return browserCrashReports();
    },
    async previewDiagnosticExport(reportId) {
      const report = browserCrashReports().find((candidate) => candidate.id === reportId);
      if (!report) throw new Error("崩溃报告不存在");
      const id = crypto.randomUUID();
      browserDiagnosticPreviews.set(id, reportId);
      const files: DiagnosticExportFile[] = [
        { bundleName: "manifest.json", includedBytes: 1024, truncated: false },
        { bundleName: "report.json", includedBytes: 2048, truncated: false },
        ...report.evidence.map((item) => ({
          bundleName: item.bundleName,
          includedBytes: item.includedBytes,
          truncated: item.truncated,
        })),
      ].sort((left, right) => left.bundleName.localeCompare(right.bundleName));
      return {
        id,
        reportId,
        suggestedFileName: `MoyuMax-diagnostics-${reportId}.zip`,
        files,
        totalBytes: files.reduce((total, file) => total + file.includedBytes, 0),
        maximumEvidenceBytes: 512 * 1024,
        redactions: report.redactionSummary,
      };
    },
    async confirmDiagnosticExport(previewId) {
      const reportId = browserDiagnosticPreviews.get(previewId);
      if (!reportId) throw new Error("诊断导出预览已失效，请重新查看文件清单");
      const previewReport = browserCrashReports().find((report) => report.id === reportId);
      if (!previewReport) throw new Error("崩溃报告不存在");
      browserDiagnosticPreviews.delete(previewId);
      return {
        reportId,
        archivePath: `D:\\MoyuMax\\data\\diagnostics\\exports\\MoyuMax-diagnostics-${reportId}.zip`,
        archiveBytes: previewReport.evidence.reduce(
          (total, evidence) => total + evidence.includedBytes,
          3072,
        ),
        fileCount: previewReport.evidence.length + 2,
      };
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

function browserContentTasks(): ContentInstallTask[] {
  const serialized = window.localStorage.getItem(BROWSER_CONTENT_TASKS_KEY);
  return serialized ? (JSON.parse(serialized) as ContentInstallTask[]) : [];
}

function browserInstalledContent(): InstalledContent[] {
  const serialized = window.localStorage.getItem(BROWSER_INSTALLED_CONTENT_KEY);
  return serialized ? (JSON.parse(serialized) as InstalledContent[]) : [];
}

function browserContentEntry(
  projectId: string,
  versionId: string,
  projectTitle: string,
  filename: string,
  requiredByProjectId: string | null,
): ContentPlanEntry {
  return {
    projectId,
    versionId,
    projectTitle,
    versionNumber: "1.0.0+26.2",
    requiredByProjectId,
    file: {
      url: `https://cdn.modrinth.com/data/${projectId}/${filename}`,
      filename,
      size: projectId === "ROOT0001" ? 1_040_013 : 2_530_080,
      sha1: "1".repeat(40),
      sha512: "2".repeat(128),
    },
  };
}

function browserInstances(): ManagedInstance[] {
  const serialized = window.localStorage.getItem(BROWSER_INSTANCES_KEY);
  return serialized ? (JSON.parse(serialized) as ManagedInstance[]) : [];
}

function browserRecycleEntries(): BrowserRecycleEntry[] {
  const serialized = window.localStorage.getItem(BROWSER_RECYCLE_BIN_KEY);
  return serialized ? (JSON.parse(serialized) as BrowserRecycleEntry[]) : [];
}

function browserWorldBackups(): WorldBackupSummary[] {
  const serialized = window.localStorage.getItem(BROWSER_WORLD_BACKUPS_KEY);
  return serialized ? (JSON.parse(serialized) as WorldBackupSummary[]) : [];
}

function createBrowserWorldBackup(
  instance: ManagedInstance,
  sessionId: string,
  trigger: BackupTrigger,
): WorldBackupSummary {
  const backups = browserWorldBackups();
  const existing = backups.find(
    (backup) =>
      backup.launchSessionId === sessionId && backup.trigger === trigger,
  );
  if (existing) return existing;
  const now = Math.floor(Date.now() / 1000);
  const backup: WorldBackupSummary = {
    id: `backup-${crypto.randomUUID()}`,
    instanceId: instance.id,
    instanceName: instance.name,
    launchSessionId: sessionId,
    trigger,
    state: "ready",
    archivePath: `D:\\MoyuMax\\data\\backups\\instances\\${instance.id}\\${now}-${trigger}.zip`,
    worldCount: 1,
    sourceBytes: 8 * 1024 * 1024,
    archiveBytes: 2 * 1024 * 1024,
    createdAtUnixSeconds: now,
    completedAtUnixSeconds: now,
    errorSummary: null,
  };
  backups.unshift(backup);
  const retained = backups.filter((candidate) => candidate.instanceId !== instance.id);
  retained.push(
    ...backups
      .filter((candidate) => candidate.instanceId === instance.id)
      .slice(0, 20),
  );
  retained.sort(
    (left, right) =>
      right.createdAtUnixSeconds - left.createdAtUnixSeconds ||
      right.id.localeCompare(left.id),
  );
  window.localStorage.setItem(BROWSER_WORLD_BACKUPS_KEY, JSON.stringify(retained));
  return backup;
}

function browserLaunchSessions(): LaunchSession[] {
  const serialized = window.localStorage.getItem(BROWSER_LAUNCH_SESSIONS_KEY);
  return serialized ? (JSON.parse(serialized) as LaunchSession[]) : [];
}

function browserCrashReports(): CrashReport[] {
  const serialized = window.localStorage.getItem(BROWSER_CRASH_REPORTS_KEY);
  return serialized ? (JSON.parse(serialized) as CrashReport[]) : [];
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
