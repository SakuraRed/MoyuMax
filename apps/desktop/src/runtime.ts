import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  | { kind: "fabric"; version: string }
  | { kind: "quilt"; version: string }
  | { kind: "forge"; version: string }
  | { kind: "neoforge"; version: string };

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

export type SourceChannel = "mirror" | "official" | "custom";

export type SourcePolicy =
  | { kind: "mirrorFirst" }
  | { kind: "officialFirst" }
  | { kind: "custom"; minecraftBase: string | null; modrinthBase: string | null };

export type SourceAttemptOutcome = "success" | { failed: { error: string } };

export interface SourceAttempt {
  url: string;
  label: string;
  channel: SourceChannel;
  outcome: SourceAttemptOutcome;
}

export interface TaskSourceDetail {
  finalLabel: string;
  channel: SourceChannel;
  attempts: SourceAttempt[];
  segmented: boolean;
  segmentCount: number;
  degradedReason: string | null;
  effectiveConnections?: number;
}

export interface TaskProgress {
  completedBytes: number;
  totalBytes: number | null;
  currentItem: string | null;
  errorSummary: string | null;
  sourceDetail?: TaskSourceDetail | null;
}

export type TaskKind = "install" | "content";

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
  priority: number;
  pausedBy: string | null;
  progress: TaskProgress;
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

export type RecycleItemKind = "instance" | "screenshot" | "resource" | "world";
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
  payload: string | null;
}

export interface InstanceScreenshot {
  fileName: string;
  sizeBytes: number;
  takenAtUnixSeconds: number;
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
  /** 更新计划：允许同名异哈希替换，替换前旧文件移入实例快照区。 */
  isUpdate: boolean;
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
  priority: number;
  pausedBy: string | null;
  progress: TaskProgress;
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

export interface ContentUpdateInfo {
  projectId: string;
  projectTitle: string;
  currentVersionId: string;
  currentVersionNumber: string;
  latestVersionId: string;
  latestVersionNumber: string;
  file: ContentFilePlan;
}

export type InstanceResourceKind = "resourcepack" | "shader" | "datapack";

export interface InstanceResource {
  id: string;
  instanceId: string;
  kind: InstanceResourceKind;
  displayName: string;
  fileName: string;
  relativePath: string;
  size: number;
  sha256: string;
  enabled: boolean;
  worldName: string | null;
  importedAtUnixSeconds: number;
}

export interface InstanceWorldInfo {
  name: string;
  sizeBytes: number;
  lastPlayedUnixSeconds: number | null;
}

export type BackupTrigger = "preLaunch" | "postExit" | "manual" | "scheduled";
export type BackupState = "staging" | "ready" | "skipped" | "failed";
export type BackupKind = "full" | "incremental";

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
  kind: BackupKind;
  baseBackupId: string | null;
}

export interface WorldBackupSettings {
  intervalMinutes: number;
  keepCount: number;
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

export type JavaEnvironmentStatus =
  | "planned"
  | "installing"
  | "ready"
  | "missing"
  | "failed"
  | "deleted";

export interface ReferencingInstance {
  id: string;
  name: string;
}

export interface JavaEnvironment {
  id: string;
  distribution: "azulZulu";
  fullVersion: string;
  architecture: "x64";
  homeDirectory: string;
  status: JavaEnvironmentStatus;
  sizeBytes: number;
  healthy: boolean;
  referencingInstances: ReferencingInstance[];
}

export type JavaDeleteOutcome =
  | { kind: "deleted"; filesRemoved: boolean }
  | { kind: "requiresConfirmation"; instances: ReferencingInstance[] };

export type WindowCloseBehavior = "ask" | "minimizeToTray" | "exit";
export type WindowCloseAction = "minimize" | "exit";

export interface WindowCloseResolution {
  action: WindowCloseAction;
  remember: boolean;
}

export interface ExitImpactSession {
  sessionId: string;
  instanceId: string;
  instanceName: string;
}

export interface ExitImpact {
  runningSessions: ExitImpactSession[];
  activeInstallTasks: number;
  activeContentTasks: number;
  executingInstallTasks: number;
  executingContentTasks: number;
  pausedTasks: number;
}

export type WindowStartupKind = "cold" | "wake";

export type PendingIntent =
  | { kind: "quickLaunch"; instanceId: string }
  | { kind: "exitRequested" };

/** 持久化的壳层页面与滚动位置;page 的合法性由 shell-state.ts 校验。 */
export interface ShellStateSnapshot {
  page: string;
  scrollTop: number;
}

export interface MoyuRuntime {
  getBootstrapState(): Promise<BootstrapState>;
  completeOnboarding(selection: OnboardingSelection): Promise<void>;
  skipOnboarding(): Promise<void>;
  getGameVersionCatalog(): Promise<VersionCatalog>;
  getFabricLoaders(gameVersion: string): Promise<FabricLoaderSummary[]>;
  getQuiltLoaders(gameVersion: string): Promise<FabricLoaderSummary[]>;
  getForgeVersions(gameVersion: string): Promise<FabricLoaderSummary[]>;
  getNeoForgeVersions(gameVersion: string): Promise<FabricLoaderSummary[]>;
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
  checkContentUpdates(instanceId: string): Promise<ContentUpdateInfo[]>;
  planContentUpdate(
    instanceId: string,
    projectIds: string[],
  ): Promise<ContentInstallTask>;
  getInstanceContentAutoUpdate(instanceId: string): Promise<boolean>;
  setInstanceContentAutoUpdate(instanceId: string, enabled: boolean): Promise<void>;
  listInstanceWorlds(instanceId: string): Promise<string[]>;
  listInstanceResources(instanceId: string): Promise<InstanceResource[]>;
  /** 打开原生文件选择器挑选要导入的资源文件；用户取消时返回 null。 */
  pickResourceFile(kind: InstanceResourceKind): Promise<string | null>;
  importInstanceResource(
    instanceId: string,
    kind: InstanceResourceKind,
    sourcePath: string,
    worldName?: string,
  ): Promise<InstanceResource>;
  setInstanceResourceEnabled(resourceId: string, enabled: boolean): Promise<InstanceResource>;
  listInstanceWorldDetails(instanceId: string): Promise<InstanceWorldInfo[]>;
  /** 打开原生保存对话框选择世界导出位置；用户取消时返回 null。 */
  pickWorldExportPath(worldName: string): Promise<string | null>;
  /** 打开原生文件选择器挑选要导入的世界 ZIP；用户取消时返回 null。 */
  pickWorldZip(): Promise<string | null>;
  exportInstanceWorld(
    instanceId: string,
    worldName: string,
    destination: string,
  ): Promise<number>;
  importInstanceWorld(instanceId: string, sourcePath: string): Promise<InstanceWorldInfo>;
  rollbackWorldBackup(backupId: string): Promise<WorldBackupSummary>;
  listInstanceScreenshots(instanceId: string): Promise<InstanceScreenshot[]>;
  /** 把截图图片写入系统剪贴板。 */
  copyScreenshotToClipboard(instanceId: string, fileName: string): Promise<void>;
  openScreenshotLocation(instanceId: string, fileName: string): Promise<void>;
  deleteInstanceScreenshot(instanceId: string, fileName: string): Promise<RecycleBinItem>;
  deleteInstanceResource(resourceId: string): Promise<RecycleBinItem>;
  deleteInstanceWorld(instanceId: string, worldName: string): Promise<RecycleBinItem>;
  restoreRecycledEntry(itemId: string): Promise<RecycleBinItem>;
  getWorldBackupSettings(): Promise<WorldBackupSettings>;
  setWorldBackupIntervalMinutes(minutes: number): Promise<void>;
  setWorldBackupKeepCount(count: number): Promise<void>;
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
  getWindowCloseBehavior(): Promise<WindowCloseBehavior>;
  setWindowCloseBehavior(behavior: WindowCloseBehavior): Promise<void>;
  resolveWindowClose(resolution: WindowCloseResolution): Promise<void>;
  getExitImpact(): Promise<ExitImpact>;
  confirmExit(): Promise<void>;
  forceExit(): Promise<void>;
  getShellState(): Promise<ShellStateSnapshot | null>;
  persistShellState(state: ShellStateSnapshot): Promise<void>;
  getWindowStartupKind(): Promise<WindowStartupKind>;
  takePendingIntent(): Promise<PendingIntent | null>;
  getTasksPaused(): Promise<boolean>;
  pauseAllTasks(): Promise<void>;
  resumeAllTasks(): Promise<void>;
  pauseTask(taskId: string, kind: TaskKind): Promise<void>;
  resumeTask(taskId: string, kind: TaskKind): Promise<void>;
  setTaskPriority(taskId: string, kind: TaskKind, priority: number): Promise<void>;
  getDownloadSpeedLimit(): Promise<number>;
  setDownloadSpeedLimit(bytesPerSec: number): Promise<void>;
  getDownloadSourcePolicy(): Promise<SourcePolicy>;
  setDownloadSourcePolicy(policy: SourcePolicy): Promise<void>;
  listJavaEnvironments(): Promise<JavaEnvironment[]>;
  listDeletedJavaEnvironments(): Promise<JavaEnvironment[]>;
  deleteJavaEnvironment(environmentId: string, force: boolean): Promise<JavaDeleteOutcome>;
  verifyJavaEnvironment(environmentId: string): Promise<boolean>;
  restoreJavaEnvironment(environmentId: string): Promise<JavaEnvironment>;
  setInstanceJavaEnvironment(instanceId: string, environmentId: string): Promise<void>;
  openJavaLocation(environmentId: string): Promise<void>;
  /** 注册窗口关闭请求回调(标题栏关闭按钮与系统关闭共用),返回取消注册函数。 */
  onCloseRequested(handler: () => void): () => void;
  /** 托盘动作产生待处理意图时通知前端取走,返回取消注册函数。 */
  onPendingIntent(handler: () => void): () => void;
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
const BROWSER_CONTENT_UPDATES_KEY = "moyumax.browser.contentUpdates";
const BROWSER_CONTENT_AUTO_UPDATE_KEY = "moyumax.browser.contentAutoUpdate";
const BROWSER_INSTANCE_RESOURCES_KEY = "moyumax.browser.instanceResources";
const BROWSER_INSTANCE_WORLDS_KEY = "moyumax.browser.instanceWorlds";
const BROWSER_WORLD_DETAILS_KEY = "moyumax.browser.worldDetails";
const BROWSER_SCREENSHOTS_KEY = "moyumax.browser.screenshots";
const BROWSER_BACKUP_SETTINGS_KEY = "moyumax.browser.backupSettings";
const BROWSER_MODRINTH_OFFLINE_KEY = "moyumax.browser.modrinthOffline";
const BROWSER_CLOSE_BEHAVIOR_KEY = "moyumax.browser.windowCloseBehavior";
const BROWSER_SHELL_STATE_KEY = "moyumax.browser.shellState";
const BROWSER_STARTUP_KIND_KEY = "moyumax.browser.startupKind";
const BROWSER_PENDING_INTENT_KEY = "moyumax.browser.pendingIntent";
const BROWSER_TASKS_PAUSED_KEY = "moyumax.browser.tasksPaused";
const BROWSER_WINDOW_STATE_KEY = "moyumax.browser.windowState";
const BROWSER_SOURCE_POLICY_KEY = "moyumax.browser.sourcePolicy";
const BROWSER_JAVA_ENVIRONMENTS_KEY = "moyumax.browser.javaEnvironments";
const BROWSER_SPEED_LIMIT_KEY = "moyumax.browser.speedLimit";
const browserPreviews = new Map<string, InstallSelection>();
const browserContentPreviews = new Map<string, ContentInstallPlan>();
const browserDiagnosticPreviews = new Map<string, string>();
const browserCloseHandlers = new Set<() => void>();
const browserPendingIntentHandlers = new Set<() => void>();

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
    getQuiltLoaders: (gameVersion) =>
      invoke<FabricLoaderSummary[]>("get_quilt_loaders", { gameVersion }),
    getForgeVersions: (gameVersion) =>
      invoke<FabricLoaderSummary[]>("get_forge_versions", { gameVersion }),
    getNeoForgeVersions: (gameVersion) =>
      invoke<FabricLoaderSummary[]>("get_neoforge_versions", { gameVersion }),
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
    checkContentUpdates: (instanceId) =>
      invoke<ContentUpdateInfo[]>("check_content_updates", { instanceId }),
    planContentUpdate: (instanceId, projectIds) =>
      invoke<ContentInstallTask>("plan_content_update", { instanceId, projectIds }),
    getInstanceContentAutoUpdate: (instanceId) =>
      invoke<boolean>("get_instance_content_auto_update", { instanceId }),
    setInstanceContentAutoUpdate: (instanceId, enabled) =>
      invoke<void>("set_instance_content_auto_update", { instanceId, enabled }),
    listInstanceWorlds: (instanceId) =>
      invoke<string[]>("list_instance_worlds", { instanceId }),
    listInstanceResources: (instanceId) =>
      invoke<InstanceResource[]>("list_instance_resources", { instanceId }),
    pickResourceFile: async (kind) => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name:
              kind === "datapack" ? "数据包" : kind === "shader" ? "光影包" : "资源包",
            extensions: ["zip", "jar"],
          },
        ],
      });
      return typeof selected === "string" ? selected : null;
    },
    importInstanceResource: (instanceId, kind, sourcePath, worldName) =>
      invoke<InstanceResource>("import_instance_resource", {
        instanceId,
        kind,
        sourcePath,
        worldName: worldName ?? null,
      }),
    setInstanceResourceEnabled: (resourceId, enabled) =>
      invoke<InstanceResource>("set_instance_resource_enabled", { resourceId, enabled }),
    listInstanceWorldDetails: (instanceId) =>
      invoke<InstanceWorldInfo[]>("list_instance_world_details", { instanceId }),
    pickWorldExportPath: async (worldName) => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      return await save({
        defaultPath: `${worldName}.zip`,
        filters: [{ name: "世界存档", extensions: ["zip"] }],
      });
    },
    pickWorldZip: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "世界存档", extensions: ["zip"] }],
      });
      return typeof selected === "string" ? selected : null;
    },
    exportInstanceWorld: (instanceId, worldName, destination) =>
      invoke<number>("export_instance_world", { instanceId, worldName, destination }),
    importInstanceWorld: (instanceId, sourcePath) =>
      invoke<InstanceWorldInfo>("import_instance_world", { instanceId, sourcePath }),
    rollbackWorldBackup: (backupId) =>
      invoke<WorldBackupSummary>("rollback_world_backup", { backupId }),
    listInstanceScreenshots: (instanceId) =>
      invoke<InstanceScreenshot[]>("list_instance_screenshots", { instanceId }),
    copyScreenshotToClipboard: async (instanceId, fileName) => {
      const bytes = await invoke<number[]>("read_instance_screenshot", {
        instanceId,
        fileName,
      });
      const { Image } = await import("@tauri-apps/api/image");
      const { writeImage } = await import("@tauri-apps/plugin-clipboard-manager");
      const image = await Image.fromBytes(new Uint8Array(bytes));
      await writeImage(image);
    },
    openScreenshotLocation: (instanceId, fileName) =>
      invoke<void>("open_screenshot_location", { instanceId, fileName }),
    deleteInstanceScreenshot: (instanceId, fileName) =>
      invoke<RecycleBinItem>("delete_instance_screenshot", { instanceId, fileName }),
    deleteInstanceResource: (resourceId) =>
      invoke<RecycleBinItem>("delete_instance_resource", { resourceId }),
    deleteInstanceWorld: (instanceId, worldName) =>
      invoke<RecycleBinItem>("delete_instance_world", { instanceId, worldName }),
    restoreRecycledEntry: (itemId) =>
      invoke<RecycleBinItem>("restore_recycled_entry", { itemId }),
    getWorldBackupSettings: () =>
      invoke<WorldBackupSettings>("get_world_backup_settings"),
    setWorldBackupIntervalMinutes: (minutes) =>
      invoke<void>("set_world_backup_interval_minutes", { minutes }),
    setWorldBackupKeepCount: (count) =>
      invoke<void>("set_world_backup_keep_count", { count }),
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
    getWindowCloseBehavior: () =>
      invoke<WindowCloseBehavior>("get_window_close_behavior"),
    setWindowCloseBehavior: (behavior) =>
      invoke<void>("set_window_close_behavior", { behavior }),
    resolveWindowClose: (resolution) =>
      invoke<void>("resolve_window_close", { resolution }),
    getExitImpact: () => invoke<ExitImpact>("get_exit_impact"),
    confirmExit: () => invoke<void>("confirm_exit"),
    forceExit: () => invoke<void>("force_exit"),
    getShellState: () => invoke<ShellStateSnapshot | null>("get_shell_state"),
    persistShellState: (state) =>
      invoke<void>("persist_shell_state", { state }),
    getWindowStartupKind: () =>
      invoke<WindowStartupKind>("get_window_startup_kind"),
    takePendingIntent: () =>
      invoke<PendingIntent | null>("take_pending_intent"),
    getTasksPaused: () => invoke<boolean>("get_tasks_paused"),
    pauseAllTasks: () => invoke<void>("pause_all_tasks"),
    resumeAllTasks: () => invoke<void>("resume_all_tasks"),
    pauseTask: (taskId, kind) => invoke<void>("pause_task", { taskId, kind }),
    resumeTask: (taskId, kind) => invoke<void>("resume_task", { taskId, kind }),
    setTaskPriority: (taskId, kind, priority) =>
      invoke<void>("set_task_priority", { taskId, kind, priority }),
    getDownloadSpeedLimit: () => invoke<number>("get_download_speed_limit"),
    setDownloadSpeedLimit: (bytesPerSec) =>
      invoke<void>("set_download_speed_limit", { bytesPerSec }),
    getDownloadSourcePolicy: () =>
      invoke<SourcePolicy>("get_download_source_policy"),
    setDownloadSourcePolicy: (policy) =>
      invoke<void>("set_download_source_policy", { policy }),
    listJavaEnvironments: () =>
      invoke<JavaEnvironment[]>("list_java_environments"),
    listDeletedJavaEnvironments: () =>
      invoke<JavaEnvironment[]>("list_deleted_java_environments"),
    deleteJavaEnvironment: (environmentId, force) =>
      invoke<JavaDeleteOutcome>("delete_java_environment", { environmentId, force }),
    verifyJavaEnvironment: (environmentId) =>
      invoke<boolean>("verify_java_environment", { environmentId }),
    restoreJavaEnvironment: (environmentId) =>
      invoke<JavaEnvironment>("restore_java_environment", { environmentId }),
    setInstanceJavaEnvironment: (instanceId, environmentId) =>
      invoke<void>("set_instance_java_environment", { instanceId, environmentId }),
    openJavaLocation: (environmentId) =>
      invoke<void>("open_java_location", { environmentId }),
    onCloseRequested: (handler) => {
      let unlisten: (() => void) | undefined;
      void listen(CLOSE_REQUESTED_EVENT, handler).then((release) => {
        unlisten = release;
      });
      return () => unlisten?.();
    },
    onPendingIntent: (handler) => {
      let unlisten: (() => void) | undefined;
      void listen(PENDING_INTENT_EVENT, handler).then((release) => {
        unlisten = release;
      });
      return () => unlisten?.();
    },
  };
}

const CLOSE_REQUESTED_EVENT = "moyumax://close-requested";
const PENDING_INTENT_EVENT = "moyumax://pending-intent";

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
    async getQuiltLoaders() {
      return [
        { version: "0.30.0", stable: true, recommended: true },
        { version: "0.30.1-beta.1", stable: false, recommended: false },
      ];
    },
    async getForgeVersions() {
      return [
        { version: "58.1.19", stable: true, recommended: false },
        { version: "58.1.20", stable: true, recommended: true },
      ];
    },
    async getNeoForgeVersions() {
      return [
        { version: "21.8.53", stable: true, recommended: false },
        { version: "21.8.54", stable: true, recommended: true },
      ];
    },
    async previewInstall(selection) {
      const id = crypto.randomUUID();
      browserPreviews.set(id, selection);
      const loaderName =
        selection.loader.kind === "fabric"
          ? "Fabric"
          : selection.loader.kind === "quilt"
            ? "Quilt"
            : selection.loader.kind === "forge"
              ? "Forge"
              : selection.loader.kind === "neoforge"
                ? "NeoForge"
                : "原版";
      const loaderVersion =
        selection.loader.kind === "vanilla" ? null : selection.loader.version;
      return {
        id,
        instanceName: selection.instanceName,
        gameVersion: selection.gameVersion.id,
        loaderName,
        loaderVersion,
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
        priority: 0,
        pausedBy: null,
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
      const supportedLoaders = ["fabric", "quilt", "forge", "neoforge"];
      if (
        !instance ||
        !supportedLoaders.includes(instance.loaderKind) ||
        instance.state !== "ready"
      ) {
        throw new Error("目标实例不存在或加载器不支持 Modrinth 模组安装");
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
        isUpdate: false,
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
        priority: 0,
        pausedBy: null,
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
    async checkContentUpdates(instanceId) {
      if (window.localStorage.getItem(BROWSER_MODRINTH_OFFLINE_KEY) === "true") {
        throw new Error("无法连接 Modrinth：浏览器测试环境处于离线状态");
      }
      const installed = browserInstalledContent().filter(
        (entry) => entry.instanceId === instanceId,
      );
      const installedProjects = new Set(installed.map((entry) => entry.projectId));
      return browserContentUpdates().filter(
        (update) =>
          update.instanceId === instanceId && installedProjects.has(update.projectId),
      );
    },
    async planContentUpdate(instanceId, projectIds) {
      if (projectIds.length === 0) throw new Error("没有选择要更新的项目");
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance) throw new Error("目标实例不存在");
      const updates = browserContentUpdates().filter(
        (update) => update.instanceId === instanceId,
      );
      const entries: ContentPlanEntry[] = [];
      for (const projectId of projectIds) {
        const update = updates.find((candidate) => candidate.projectId === projectId);
        if (!update) throw new Error(`项目 ${projectId} 没有可用更新`);
        entries.push({
          projectId: update.projectId,
          versionId: update.latestVersionId,
          projectTitle: update.projectTitle,
          versionNumber: update.latestVersionNumber,
          requiredByProjectId: null,
          file: update.file,
        });
      }
      const plan: ContentInstallPlan = {
        schemaVersion: 1,
        instanceId,
        instanceName: instance.name,
        gameVersion: instance.gameVersion,
        loader: instance.loaderKind,
        rootProjectId: entries[0]!.projectId,
        entries,
        optionalDependencies: [],
        incompatibleDependencies: [],
        isUpdate: true,
      };
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
        priority: 0,
        pausedBy: null,
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
    async getInstanceContentAutoUpdate(instanceId) {
      return browserContentAutoUpdate()[instanceId] ?? false;
    },
    async setInstanceContentAutoUpdate(instanceId, enabled) {
      const flags = browserContentAutoUpdate();
      flags[instanceId] = enabled;
      window.localStorage.setItem(
        BROWSER_CONTENT_AUTO_UPDATE_KEY,
        JSON.stringify(flags),
      );
    },
    async listInstanceWorlds(instanceId) {
      return browserInstanceWorlds()[instanceId] ?? [];
    },
    async listInstanceResources(instanceId) {
      return browserInstanceResources().filter(
        (resource) => resource.instanceId === instanceId,
      );
    },
    async pickResourceFile(_kind) {
      // 浏览器测试运行时没有原生选择器，由测试预置待导入路径。
      return window.localStorage.getItem("moyumax.browser.pickedResourceFile");
    },
    async importInstanceResource(instanceId, kind, sourcePath, worldName) {
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance) throw new Error("目标实例不存在");
      const fileName = sourcePath.split(/[\\/]/).pop() ?? "";
      const lowered = fileName.toLowerCase();
      if (!lowered.endsWith(".zip") && !lowered.endsWith(".jar")) {
        throw new Error("资源文件名不安全或不是 ZIP/JAR");
      }
      if (kind === "datapack") {
        const worlds = browserInstanceWorlds()[instanceId] ?? [];
        if (!worldName) throw new Error("导入数据包必须先选择目标世界");
        if (!worlds.includes(worldName)) {
          throw new Error(`世界 ${worldName} 不存在，数据包必须装入用户选择的世界`);
        }
      }
      const resources = browserInstanceResources();
      if (
        resources.some(
          (resource) =>
            resource.instanceId === instanceId &&
            resource.kind === kind &&
            resource.fileName.toLowerCase() === fileName.toLowerCase(),
        )
      ) {
        throw new Error(`同名文件 ${fileName} 已存在，已拒绝导入且未覆盖`);
      }
      const resource: InstanceResource = {
        id: crypto.randomUUID(),
        instanceId,
        kind,
        displayName: fileName.replace(/\.(zip|jar)$/i, ""),
        fileName,
        relativePath:
          kind === "datapack"
            ? `.minecraft/saves/${worldName}/datapacks/${fileName}`
            : kind === "shader"
              ? `.minecraft/shaderpacks/${fileName}`
              : `.minecraft/resourcepacks/${fileName}`,
        size: 1024,
        sha256: "3".repeat(64),
        enabled: true,
        worldName: kind === "datapack" ? (worldName ?? null) : null,
        importedAtUnixSeconds: Math.floor(Date.now() / 1000),
      };
      resources.push(resource);
      window.localStorage.setItem(
        BROWSER_INSTANCE_RESOURCES_KEY,
        JSON.stringify(resources),
      );
      return resource;
    },
    async setInstanceResourceEnabled(resourceId, enabled) {
      const resources = browserInstanceResources();
      const resource = resources.find((candidate) => candidate.id === resourceId);
      if (!resource) throw new Error("资源项不存在");
      resource.enabled = enabled;
      window.localStorage.setItem(
        BROWSER_INSTANCE_RESOURCES_KEY,
        JSON.stringify(resources),
      );
      return resource;
    },
    async listInstanceWorldDetails(instanceId) {
      return browserWorldDetails()[instanceId] ?? [];
    },
    async pickWorldExportPath(_worldName) {
      return window.localStorage.getItem("moyumax.browser.worldExportPath");
    },
    async pickWorldZip() {
      return window.localStorage.getItem("moyumax.browser.pickedWorldZip");
    },
    async exportInstanceWorld(instanceId, worldName, _destination) {
      const worlds = browserWorldDetails()[instanceId] ?? [];
      if (!worlds.some((world) => world.name === worldName)) {
        throw new Error(`世界 ${worldName} 不存在`);
      }
      return 2048;
    },
    async importInstanceWorld(instanceId, sourcePath) {
      const fileName = sourcePath.split(/[\\/]/).pop() ?? "";
      if (!fileName.toLowerCase().endsWith(".zip")) {
        throw new Error("世界 ZIP 无法读取");
      }
      const name = fileName.replace(/\.zip$/i, "");
      const all = browserWorldDetails();
      const worlds = all[instanceId] ?? [];
      if (worlds.some((world) => world.name === name)) {
        throw new Error(`世界 ${name} 已存在，已拒绝导入且未覆盖`);
      }
      const imported: InstanceWorldInfo = {
        name,
        sizeBytes: 1024,
        lastPlayedUnixSeconds: Math.floor(Date.now() / 1000),
      };
      all[instanceId] = [...worlds, imported].sort((left, right) =>
        left.name.localeCompare(right.name),
      );
      window.localStorage.setItem(BROWSER_WORLD_DETAILS_KEY, JSON.stringify(all));
      return imported;
    },
    async rollbackWorldBackup(backupId) {
      const backups = browserWorldBackups();
      const backup = backups.find((candidate) => candidate.id === backupId);
      if (!backup) throw new Error("备份不存在");
      if (backup.state !== "ready") throw new Error("只有已完成的备份可以回滚");
      const now = Math.floor(Date.now() / 1000);
      const recovery: WorldBackupSummary = {
        id: `backup-${crypto.randomUUID()}`,
        instanceId: backup.instanceId,
        instanceName: backup.instanceName,
        launchSessionId: null,
        trigger: "manual",
        state: "ready",
        archivePath: `D:\\MoyuMax\\data\\backups\\instances\\${backup.instanceId}\\${now}-manual.zip`,
        worldCount: backup.worldCount,
        sourceBytes: backup.sourceBytes,
        archiveBytes: backup.archiveBytes,
        createdAtUnixSeconds: now,
        completedAtUnixSeconds: now,
        errorSummary: null,
        kind: "full",
        baseBackupId: null,
      };
      backups.unshift(recovery);
      window.localStorage.setItem(BROWSER_WORLD_BACKUPS_KEY, JSON.stringify(backups));
      return recovery;
    },
    async getWorldBackupSettings() {
      return browserBackupSettings();
    },
    async setWorldBackupIntervalMinutes(minutes) {
      if (minutes > 1440) throw new Error("备份间隔不能超过 1440 分钟");
      const settings = { ...browserBackupSettings(), intervalMinutes: minutes };
      window.localStorage.setItem(BROWSER_BACKUP_SETTINGS_KEY, JSON.stringify(settings));
    },
    async setWorldBackupKeepCount(count) {
      if (count === 0 || count > 100) throw new Error("备份保留数量必须在 1 到 100 之间");
      const settings = { ...browserBackupSettings(), keepCount: count };
      window.localStorage.setItem(BROWSER_BACKUP_SETTINGS_KEY, JSON.stringify(settings));
    },
    async listInstanceScreenshots(instanceId) {
      return browserScreenshots()[instanceId] ?? [];
    },
    async copyScreenshotToClipboard(instanceId, fileName) {
      const exists = (browserScreenshots()[instanceId] ?? []).some(
        (screenshot) => screenshot.fileName === fileName,
      );
      if (!exists) throw new Error(`截图 ${fileName} 不存在`);
      window.localStorage.setItem("moyumax.browser.clipboardImage", fileName);
    },
    async openScreenshotLocation(_instanceId, fileName) {
      window.localStorage.setItem("moyumax.browser.openedLocation", fileName);
    },
    async deleteInstanceScreenshot(instanceId, fileName) {
      const all = browserScreenshots();
      const screenshots = all[instanceId] ?? [];
      const index = screenshots.findIndex(
        (screenshot) => screenshot.fileName === fileName,
      );
      if (index < 0) throw new Error(`截图 ${fileName} 不存在`);
      const [removed] = screenshots.splice(index, 1);
      all[instanceId] = screenshots;
      window.localStorage.setItem(BROWSER_SCREENSHOTS_KEY, JSON.stringify(all));
      return browserPushRecycleEntry({
        kind: "screenshot",
        subjectId: instanceId,
        displayName: fileName,
        originalPath: `D:\\MoyuMax\\data\\instances\\${instanceId}\\.minecraft\\screenshots\\${fileName}`,
        sizeBytes: removed?.sizeBytes ?? 0,
        payload: null,
      });
    },
    async deleteInstanceResource(resourceId) {
      const resources = browserInstanceResources();
      const index = resources.findIndex((candidate) => candidate.id === resourceId);
      if (index < 0) throw new Error("资源项不存在");
      const [removed] = resources.splice(index, 1);
      window.localStorage.setItem(
        BROWSER_INSTANCE_RESOURCES_KEY,
        JSON.stringify(resources),
      );
      return browserPushRecycleEntry({
        kind: "resource",
        subjectId: removed!.instanceId,
        displayName: removed!.displayName,
        originalPath: `D:\\MoyuMax\\data\\instances\\${removed!.instanceId}\\${removed!.relativePath}`,
        sizeBytes: removed!.size,
        payload: JSON.stringify(removed),
      });
    },
    async deleteInstanceWorld(instanceId, worldName) {
      const all = browserWorldDetails();
      const worlds = all[instanceId] ?? [];
      const index = worlds.findIndex((world) => world.name === worldName);
      if (index < 0) throw new Error(`世界 ${worldName} 不存在`);
      const [removed] = worlds.splice(index, 1);
      all[instanceId] = worlds;
      window.localStorage.setItem(BROWSER_WORLD_DETAILS_KEY, JSON.stringify(all));
      const worldNames = browserInstanceWorlds();
      worldNames[instanceId] = (worldNames[instanceId] ?? []).filter(
        (name) => name !== worldName,
      );
      window.localStorage.setItem(BROWSER_INSTANCE_WORLDS_KEY, JSON.stringify(worldNames));
      return browserPushRecycleEntry({
        kind: "world",
        subjectId: instanceId,
        displayName: worldName,
        originalPath: `D:\\MoyuMax\\data\\instances\\${instanceId}\\.minecraft\\saves\\${worldName}`,
        sizeBytes: removed?.sizeBytes ?? 0,
        payload: null,
      });
    },
    async restoreRecycledEntry(itemId) {
      const entries = browserRecycleEntries();
      const index = entries.findIndex((candidate) => candidate.id === itemId);
      const entry = entries[index];
      if (!entry || entry.state !== "ready" || entry.kind === "instance") {
        throw new Error("该回收站项目当前不能恢复");
      }
      entries.splice(index, 1);
      window.localStorage.setItem(BROWSER_RECYCLE_BIN_KEY, JSON.stringify(entries));
      if (entry.kind === "screenshot") {
        const all = browserScreenshots();
        all[entry.subjectId] = [
          ...(all[entry.subjectId] ?? []),
          {
            fileName: entry.displayName,
            sizeBytes: entry.sizeBytes,
            takenAtUnixSeconds: entry.deletedAtUnixSeconds,
          },
        ];
        window.localStorage.setItem(BROWSER_SCREENSHOTS_KEY, JSON.stringify(all));
      } else if (entry.kind === "resource" && entry.payload) {
        const resources = browserInstanceResources();
        resources.push(JSON.parse(entry.payload) as InstanceResource);
        window.localStorage.setItem(
          BROWSER_INSTANCE_RESOURCES_KEY,
          JSON.stringify(resources),
        );
      } else if (entry.kind === "world") {
        const all = browserWorldDetails();
        all[entry.subjectId] = [
          ...(all[entry.subjectId] ?? []),
          {
            name: entry.displayName,
            sizeBytes: entry.sizeBytes,
            lastPlayedUnixSeconds: entry.deletedAtUnixSeconds,
          },
        ].sort((left, right) => left.name.localeCompare(right.name));
        window.localStorage.setItem(BROWSER_WORLD_DETAILS_KEY, JSON.stringify(all));
      }
      const { instance: _instance, ...summary } = entry;
      return summary;
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
        payload: null,
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
    async closeWindow() {
      // 浏览器环境没有真实窗口,closeWindow 模拟原生关闭请求事件。
      for (const handler of browserCloseHandlers) handler();
    },
    async getWindowCloseBehavior() {
      const stored = window.localStorage.getItem(BROWSER_CLOSE_BEHAVIOR_KEY);
      if (stored === "minimizeToTray" || stored === "exit") return stored;
      return "ask";
    },
    async setWindowCloseBehavior(behavior) {
      window.localStorage.setItem(BROWSER_CLOSE_BEHAVIOR_KEY, behavior);
    },
    async resolveWindowClose(resolution) {
      if (resolution.remember) {
        window.localStorage.setItem(
          BROWSER_CLOSE_BEHAVIOR_KEY,
          resolution.action === "minimize" ? "minimizeToTray" : "exit",
        );
      }
      if (resolution.action === "minimize") {
        window.localStorage.setItem(BROWSER_WINDOW_STATE_KEY, "hidden");
        return;
      }
      const impact = browserExitImpact();
      if (
        impact.runningSessions.length > 0 ||
        impact.activeInstallTasks > 0 ||
        impact.activeContentTasks > 0
      ) {
        throw new Error("退出前需要确认运行中游戏与活动任务的影响");
      }
      window.localStorage.setItem(BROWSER_WINDOW_STATE_KEY, "exited");
    },
    async getExitImpact() {
      return browserExitImpact();
    },
    async confirmExit() {
      // 与桌面优雅退出对齐:运行中会话安全停止,执行中任务转为可恢复暂停。
      const sessions = browserLaunchSessions();
      for (const session of sessions) {
        if (["starting", "running"].includes(session.state)) {
          session.state = "stopped";
          session.endedAtUnixSeconds = Math.floor(Date.now() / 1000);
          const instance = browserInstances().find(
            (candidate) => candidate.id === session.instanceId,
          );
          if (instance) {
            session.postExitBackup = createBrowserWorldBackup(
              instance,
              session.id,
              "postExit",
            );
          }
        }
      }
      window.localStorage.setItem(
        BROWSER_LAUNCH_SESSIONS_KEY,
        JSON.stringify(sessions),
      );
      const runningInstallTasks = browserInstallTasks();
      for (const task of runningInstallTasks) {
        if (task.state === "running") task.state = "paused";
      }
      const runningContentTasks = browserContentTasks();
      for (const task of runningContentTasks) {
        if (task.state === "running") task.state = "paused";
      }
      window.localStorage.setItem(
        BROWSER_TASKS_KEY,
        JSON.stringify(runningInstallTasks),
      );
      window.localStorage.setItem(
        BROWSER_CONTENT_TASKS_KEY,
        JSON.stringify(runningContentTasks),
      );
      window.localStorage.setItem(BROWSER_WINDOW_STATE_KEY, "exited");
    },
    async forceExit() {
      window.localStorage.setItem(BROWSER_WINDOW_STATE_KEY, "exited");
    },
    async getShellState() {
      const serialized = window.localStorage.getItem(BROWSER_SHELL_STATE_KEY);
      return serialized ? (JSON.parse(serialized) as ShellStateSnapshot) : null;
    },
    async persistShellState(state) {
      window.localStorage.setItem(BROWSER_SHELL_STATE_KEY, JSON.stringify(state));
    },
    async getWindowStartupKind() {
      const stored = window.localStorage.getItem(BROWSER_STARTUP_KIND_KEY);
      return stored === "wake" ? "wake" : "cold";
    },
    async takePendingIntent() {
      const serialized = window.localStorage.getItem(BROWSER_PENDING_INTENT_KEY);
      window.localStorage.removeItem(BROWSER_PENDING_INTENT_KEY);
      return serialized ? (JSON.parse(serialized) as PendingIntent) : null;
    },
    async getTasksPaused() {
      return window.localStorage.getItem(BROWSER_TASKS_PAUSED_KEY) === "true";
    },
    async pauseAllTasks() {
      window.localStorage.setItem(BROWSER_TASKS_PAUSED_KEY, "true");
      const installTasks = browserInstallTasks();
      for (const task of installTasks) {
        if (task.state === "running") {
          task.state = "paused";
          task.pausedBy = "global";
        }
      }
      window.localStorage.setItem(BROWSER_TASKS_KEY, JSON.stringify(installTasks));
      const contentTasks = browserContentTasks();
      for (const task of contentTasks) {
        if (task.state === "running") {
          task.state = "paused";
          task.pausedBy = "global";
        }
      }
      window.localStorage.setItem(
        BROWSER_CONTENT_TASKS_KEY,
        JSON.stringify(contentTasks),
      );
    },
    async resumeAllTasks() {
      window.localStorage.setItem(BROWSER_TASKS_PAUSED_KEY, "false");
      const installTasks = browserInstallTasks();
      for (const task of installTasks) {
        if (task.state === "paused" && task.pausedBy === "global") task.state = "queued";
        if (task.state === "paused") task.pausedBy = null;
      }
      window.localStorage.setItem(BROWSER_TASKS_KEY, JSON.stringify(installTasks));
      const contentTasks = browserContentTasks();
      for (const task of contentTasks) {
        if (task.state === "paused" && task.pausedBy === "global") task.state = "queued";
        if (task.state === "paused") task.pausedBy = null;
      }
      window.localStorage.setItem(
        BROWSER_CONTENT_TASKS_KEY,
        JSON.stringify(contentTasks),
      );
    },
    async pauseTask(taskId, kind) {
      const key = kind === "content" ? BROWSER_CONTENT_TASKS_KEY : BROWSER_TASKS_KEY;
      const tasks = kind === "content" ? browserContentTasks() : browserInstallTasks();
      const task = tasks.find((entry) => entry.id === taskId);
      if (!task || !["queued", "running"].includes(task.state)) {
        throw new Error("任务不存在或当前状态不能暂停");
      }
      task.state = "paused";
      task.pausedBy = "user";
      window.localStorage.setItem(key, JSON.stringify(tasks));
    },
    async resumeTask(taskId, kind) {
      const key = kind === "content" ? BROWSER_CONTENT_TASKS_KEY : BROWSER_TASKS_KEY;
      const tasks = kind === "content" ? browserContentTasks() : browserInstallTasks();
      const task = tasks.find((entry) => entry.id === taskId);
      if (!task || task.state !== "paused") {
        throw new Error("任务不存在或当前不是暂停状态");
      }
      task.state = "queued";
      task.pausedBy = null;
      window.localStorage.setItem(key, JSON.stringify(tasks));
    },
    async setTaskPriority(taskId, kind, priority) {
      const key = kind === "content" ? BROWSER_CONTENT_TASKS_KEY : BROWSER_TASKS_KEY;
      const tasks = kind === "content" ? browserContentTasks() : browserInstallTasks();
      const task = tasks.find((entry) => entry.id === taskId);
      if (!task || !["queued", "paused"].includes(task.state)) {
        throw new Error("任务不存在或当前状态不能调整优先级");
      }
      task.priority = priority;
      window.localStorage.setItem(key, JSON.stringify(tasks));
    },
    async getDownloadSpeedLimit() {
      const value = window.localStorage.getItem(BROWSER_SPEED_LIMIT_KEY);
      return value ? Number(value) : 0;
    },
    async setDownloadSpeedLimit(bytesPerSec) {
      window.localStorage.setItem(BROWSER_SPEED_LIMIT_KEY, String(bytesPerSec));
    },
    async getDownloadSourcePolicy() {
      const serialized = window.localStorage.getItem(BROWSER_SOURCE_POLICY_KEY);
      return serialized
        ? (JSON.parse(serialized) as SourcePolicy)
        : { kind: "mirrorFirst" };
    },
    async setDownloadSourcePolicy(policy) {
      window.localStorage.setItem(BROWSER_SOURCE_POLICY_KEY, JSON.stringify(policy));
    },
    async listJavaEnvironments() {
      return browserJavaEnvironments().filter((env) => env.status !== "deleted");
    },
    async listDeletedJavaEnvironments() {
      return browserJavaEnvironments().filter((env) => env.status === "deleted");
    },
    async deleteJavaEnvironment(environmentId, force) {
      const environments = browserJavaEnvironments();
      const environment = environments.find((env) => env.id === environmentId);
      if (!environment || environment.status === "deleted") {
        throw new Error("环境不存在或已经被删除");
      }
      if (environment.referencingInstances.length > 0 && !force) {
        return {
          kind: "requiresConfirmation",
          instances: environment.referencingInstances,
        };
      }
      environment.status = "deleted";
      environment.healthy = false;
      environment.sizeBytes = 0;
      window.localStorage.setItem(
        BROWSER_JAVA_ENVIRONMENTS_KEY,
        JSON.stringify(environments),
      );
      return { kind: "deleted", filesRemoved: true };
    },
    async verifyJavaEnvironment(environmentId) {
      const environment = browserJavaEnvironments().find(
        (env) => env.id === environmentId,
      );
      if (!environment) throw new Error("Java 环境不存在");
      return environment.healthy;
    },
    async restoreJavaEnvironment(environmentId) {
      const environments = browserJavaEnvironments();
      const environment = environments.find(
        (env) => env.id === environmentId && env.status === "deleted",
      );
      if (!environment) throw new Error("该环境未被删除或不存在");
      environment.status = "ready";
      environment.healthy = true;
      environment.sizeBytes = 190 * 1024 * 1024;
      window.localStorage.setItem(
        BROWSER_JAVA_ENVIRONMENTS_KEY,
        JSON.stringify(environments),
      );
      return environment;
    },
    async setInstanceJavaEnvironment(instanceId, environmentId) {
      const environment = browserJavaEnvironments().find(
        (env) => env.id === environmentId && env.status === "ready",
      );
      if (!environment) throw new Error("只能指派已就绪的 Java 环境");
      const instance = browserInstances().find((entry) => entry.id === instanceId);
      if (!instance) throw new Error("实例不存在");
      const instanceMajor = Number(instance.gameVersion.split(".")[0]) || 0;
      const envMajor = Number(environment.fullVersion.split(".")[0]);
      if (instanceMajor >= 21 && envMajor !== 21) {
        throw new Error("主版本不一致：实例需要更高版本的 Java 环境");
      }
      const instances = browserInstances().map((entry) =>
        entry.id === instanceId
          ? { ...entry, javaEnvironmentId: environmentId }
          : entry,
      );
      window.localStorage.setItem(BROWSER_INSTANCES_KEY, JSON.stringify(instances));
    },
    async openJavaLocation(environmentId) {
      const environment = browserJavaEnvironments().find(
        (env) => env.id === environmentId,
      );
      if (!environment) throw new Error("Java 环境不存在");
    },
    onCloseRequested(handler) {
      browserCloseHandlers.add(handler);
      return () => browserCloseHandlers.delete(handler);
    },
    onPendingIntent(handler) {
      browserPendingIntentHandlers.add(handler);
      return () => browserPendingIntentHandlers.delete(handler);
    },
  };
}

function browserExitImpact(): ExitImpact {
  const instances = browserInstances();
  const runningSessions = browserLaunchSessions()
    .filter((session) => ["starting", "running"].includes(session.state))
    .map((session) => ({
      sessionId: session.id,
      instanceId: session.instanceId,
      instanceName:
        instances.find((instance) => instance.id === session.instanceId)?.name ??
        session.instanceId,
    }));
  const installTasks = browserInstallTasks();
  const contentTasks = browserContentTasks();
  const activeStates = ["queued", "running", "committing", "awaitingRecovery"];
  const executingStates = ["running", "committing"];
  return {
    runningSessions,
    activeInstallTasks: installTasks.filter((task) => activeStates.includes(task.state))
      .length,
    activeContentTasks: contentTasks.filter((task) => activeStates.includes(task.state))
      .length,
    executingInstallTasks: installTasks.filter((task) =>
      executingStates.includes(task.state),
    ).length,
    executingContentTasks: contentTasks.filter((task) =>
      executingStates.includes(task.state),
    ).length,
    pausedTasks:
      installTasks.filter((task) => task.state === "paused").length +
      contentTasks.filter((task) => task.state === "paused").length,
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

interface BrowserContentUpdate extends ContentUpdateInfo {
  instanceId: string;
}

function browserContentUpdates(): BrowserContentUpdate[] {
  const serialized = window.localStorage.getItem(BROWSER_CONTENT_UPDATES_KEY);
  return serialized ? (JSON.parse(serialized) as BrowserContentUpdate[]) : [];
}

function browserContentAutoUpdate(): Record<string, boolean> {
  const serialized = window.localStorage.getItem(BROWSER_CONTENT_AUTO_UPDATE_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, boolean>) : {};
}

function browserInstanceResources(): InstanceResource[] {
  const serialized = window.localStorage.getItem(BROWSER_INSTANCE_RESOURCES_KEY);
  return serialized ? (JSON.parse(serialized) as InstanceResource[]) : [];
}

function browserInstanceWorlds(): Record<string, string[]> {
  const serialized = window.localStorage.getItem(BROWSER_INSTANCE_WORLDS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, string[]>) : {};
}

function browserWorldDetails(): Record<string, InstanceWorldInfo[]> {
  const serialized = window.localStorage.getItem(BROWSER_WORLD_DETAILS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, InstanceWorldInfo[]>) : {};
}

function browserScreenshots(): Record<string, InstanceScreenshot[]> {
  const serialized = window.localStorage.getItem(BROWSER_SCREENSHOTS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, InstanceScreenshot[]>) : {};
}

function browserBackupSettings(): WorldBackupSettings {
  const serialized = window.localStorage.getItem(BROWSER_BACKUP_SETTINGS_KEY);
  return serialized
    ? (JSON.parse(serialized) as WorldBackupSettings)
    : { intervalMinutes: 30, keepCount: 20 };
}

function browserPushRecycleEntry(input: {
  kind: RecycleItemKind;
  subjectId: string;
  displayName: string;
  originalPath: string;
  sizeBytes: number;
  payload: string | null;
}): RecycleBinItem {
  const now = Math.floor(Date.now() / 1000);
  const entry: BrowserRecycleEntry = {
    id: `recycle-${crypto.randomUUID()}`,
    kind: input.kind,
    subjectId: input.subjectId,
    displayName: input.displayName,
    originalPath: input.originalPath,
    recycledPath: `D:\\MoyuMax\\data\\.recycle\\entries\\${crypto.randomUUID()}`,
    originalState: "ready",
    sizeBytes: input.sizeBytes,
    deletedAtUnixSeconds: now,
    expiresAtUnixSeconds: now + 30 * 24 * 60 * 60,
    state: "ready",
    payload: input.payload,
    instance: {
      id: input.subjectId,
      name: "实例",
      gameVersion: "26.2",
      loaderKind: "fabric",
      loaderVersion: null,
      rootDirectory: `D:\\MoyuMax\\data\\instances\\${input.subjectId}`,
      state: "ready",
    },
  };
  const entries = browserRecycleEntries();
  entries.unshift(entry);
  window.localStorage.setItem(BROWSER_RECYCLE_BIN_KEY, JSON.stringify(entries));
  const { instance: _instance, ...summary } = entry;
  return summary;
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
    kind: "full",
    baseBackupId: null,
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

function browserJavaEnvironments(): JavaEnvironment[] {
  const serialized = window.localStorage.getItem(BROWSER_JAVA_ENVIRONMENTS_KEY);
  return serialized ? (JSON.parse(serialized) as JavaEnvironment[]) : [];
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
