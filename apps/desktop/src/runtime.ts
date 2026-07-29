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
  | "createRollbackPoint"
  | "modpackFiles";

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

/** HTTP 代理偏好：跟随系统（默认）/ 直连 / 自定义代理，与核心 ProxyPreference 一致。 */
export type ProxyPreference =
  | { mode: "system" }
  | { mode: "direct" }
  | { mode: "custom"; url: string };

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

/** 启动内存配置（MiB）。 */
export interface LaunchOptions {
  minimumMemoryMib: number;
  maximumMemoryMib: number;
}

/** 全局启动内存偏好：自动分配（默认）或自定义区间（MiB）。 */
export type GlobalLaunchPreference =
  | { mode: "auto" }
  | { mode: "custom"; minMib: number; maxMib: number };

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

export type ModrinthProjectType = "mod" | "modpack" | "shader" | "resourcepack";

export interface ModrinthSearchQuery {
  query: string;
  gameVersion: string;
  loader: string;
  index: ModrinthSearchIndex;
  offset: number;
  limit: number;
  /** 搜索的项目类型；缺省为模组。 */
  projectType?: ModrinthProjectType;
  /** Modrinth 内容分类（optimization/technology 等）；空为全部。 */
  category?: string;
}

export interface ModrinthProjectSummary {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  downloads: number;
  clientSide: string;
  serverSide: string;
  /** 项目图标 URL；为空时前端显示首字母占位。 */
  iconUrl: string | null;
  /** 主要作者；可能为空。 */
  author: string | null;
  /** 最近更新时间（ISO 8601）。 */
  dateModified: string | null;
  /** 支持的游戏版本。 */
  versions: string[];
}

export interface ModrinthSearchPage {
  hits: ModrinthProjectSummary[];
  offset: number;
  limit: number;
  totalHits: number;
}

export type CatalogProjectSource = "modrinth" | "curseforge";

/** 统一目录项目摘要（CurseForge 数字 ID 转字符串，与 Modrinth 摘要同形）。 */
export interface CatalogProjectSummary {
  projectId: string;
  title: string;
  slug: string;
  author: string | null;
  description: string;
  iconUrl: string | null;
  downloads: number;
  dateModified: string | null;
  gameVersions: string[];
  categories: string[];
  source: CatalogProjectSource;
}

/** 统一目录搜索分页。 */
export interface CatalogSearchPage {
  hits: CatalogProjectSummary[];
  index: number;
  pageSize: number;
  totalCount: number;
}

export type CurseforgeSortField = "featured" | "popularity" | "lastUpdated" | "name" | "totalDownloads";
export type CurseforgeSortOrder = "asc" | "desc";

/** CurseForge 目录搜索条件（gameId=432 由 core 固定）。 */
export interface CurseforgeSearchQuery {
  query: string;
  classId: number;
  gameVersion?: string | null;
  categoryId?: number | null;
  modLoader?: string | null;
  sortField: CurseforgeSortField;
  sortOrder: CurseforgeSortOrder;
  index: number;
  pageSize: number;
}

/** CurseForge 文件摘要（与 ModrinthVersionSummary 同形，另带下载与校验信息）。 */
export interface CurseforgeFileSummary {
  id: string;
  versionNumber: string;
  /** release / beta / alpha。 */
  versionType: string;
  datePublished: string;
  gameVersions: string[];
  loaders: string[];
  downloads: number;
  fileName: string;
  size: number;
  /** 来源未提供校验值时为 null，下载按大小校验。 */
  sha1: string | null;
  /** 官方返回的下载地址；为 null 时 core 按 ForgeCDN edge 规则兜底。 */
  downloadUrl: string | null;
}

/** CurseForge 内容分类（id 由 API 下发，不硬编码）。 */
export interface CurseforgeCategory {
  id: number;
  name: string;
  slug: string;
}

/** CurseForge 内容分类 classId（官方 REST 文档口径：6/12/4471/6552）。 */
export const CURSEFORGE_CLASS_IDS: Record<ModrinthProjectType, number> = {
  mod: 6,
  resourcepack: 12,
  modpack: 4471,
  shader: 6552,
};

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

export interface InstalledContent {  id: string;
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

/** 实例模组目录实测条目(mods/ 扫描与安装记录合并;未收录文件 content 为 null)。 */
export interface InstanceModEntry {
  fileName: string;
  relativePath: string;
  sizeBytes: number;
  enabled: boolean;
  content: InstalledContent | null;
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

/** 主题包元数据(v2 标准;builtin 为内置包)。 */
export interface ThemePackMeta {
  id: string;
  name: string;
  author: string;
  builtin: boolean;
}

export type InstanceResourceKind = "resourcepack" | "shader" | "datapack" | "mod";

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

export interface InstanceServerEntry {
  name: string;
  /** 服务器地址(NBT 中的 ip 字段),形如 host[:port]。 */
  address: string;
  /** 服务器图标(data:image/png;base64,...),可为空。 */
  icon: string | null;
  acceptTextures: boolean | null;
}

export interface MinecraftServerStatus {
  online: boolean;
  /** MOTD 纯文本(保留 § 格式码)。 */
  motd: string | null;
  playersOnline: number | null;
  playersMax: number | null;
  versionName: string | null;
  /** 从发起连接到收完状态响应的耗时。 */
  latencyMs: number | null;
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

export type NavigationKey = "home" | "instances" | "resources" | "netplay" | "tasks" | "data" | "accounts" | "settings";

export type AccountKind = "offline" | "authlib" | "microsoft";
export type AccountSessionState = "valid" | "expired";

export interface AccountSummary {
  id: string;
  kind: AccountKind;
  username: string;
  playerUuid: string;
  serverUrl: string | null;
  isDefault: boolean;
  sessionState: AccountSessionState;
  createdAtUnixSeconds: number;
  lastValidatedAtUnixSeconds: number | null;
}

/** 项目版本摘要（自由下载的版本选择列表与资源详情文件列表）。 */
export interface ModrinthVersionSummary {
  id: string;
  versionNumber: string;
  versionType: string;
  datePublished: string;
  gameVersions: string[];
  loaders: string[];
  /** 该版本的累计下载量（资源详情文件行展示）。 */
  downloads: number;
}

/** 联机房间的非敏感视图（不携带密码）。 */
export interface NetplayRoomView {
  networkName: string;
  virtualIp: string;
  isHost: boolean;
  /** 主机侧侦测到的 MC「对局域网开放」端口。 */
  mcLanPort?: number | null;
  /** 客机侧已建立的本机回环转发端口（游戏内连接 127.0.0.1:该端口）。 */
  forwardedLocalPort?: number | null;
}

/** 简化 NAT 检测报告。 */
export interface NatReportView {
  mappedAddress: string;
  behindNat: boolean;
  impact: string;
}

/** 联机房间成员（EasyTier peer 的非敏感视图）。 */
export interface NetplayPeerView {
  ipv4: string;
  /** 显示名（已去掉 H|/J| 角色前缀）。 */
  hostname: string;
  isHost: boolean;
  /** 往返延迟（毫秒）；对端未上报时为 null。 */
  latencyMs: number | null;
  /** 连接方式：p2p 直连 / relay 中继。 */
  connection: "p2p" | "relay" | "local";
}

/** Microsoft 设备码登录的展示信息（用户码与验证地址）。 */
export interface DeviceCodeInfo {
  userCode: string;
  verificationUri: string;
  expiresInSeconds: number;
}

/** `microsoft-device-login` 事件负载（绝不携带令牌）。 */
export interface MicrosoftLoginEvent {
  state: "completed" | "failed" | "cancelled";
  account: AccountSummary | null;
  message: string | null;
}

export interface UiPreferences {
  theme: string;
  language: string;
  motion: string;
  contrast: string;
}

export interface UpdateAsset {
  name: string;
  url: string;
  size: number;
  sha256: string | null;
}

export interface ReleaseInfo {
  tag: string;
  name: string;
  notes: string;
  pageUrl: string;
  minAppVersion: string | null;
  installer: UpdateAsset | null;
}

export interface ThemePack {
  formatVersion: number;
  name: string;
  author: string;
  colors: Record<string, string>;
}

export type UiBackground =
  | { type: "default" }
  | { type: "color"; color: string }
  | { type: "image"; file: string }
  | { type: "themePack"; pack: ThemePack };

export type ModpackProvider = "modrinth" | "curseforge";

export interface ModpackPreview {
  provider: ModpackProvider;
  name: string;
  version: string;
  gameVersion: string;
  loaderKind: string;
  loaderVersion: string;
  fileCount: number;
  totalBytes: number;
}

export interface ModpackPreviewResponse {
  id: string;
  preview: ModpackPreview;
}

export interface InstalledModpack {
  provider: ModpackProvider;
  packName: string;
  packVersion: string;
  gameVersion: string;
  loaderKind: string;
  managedFiles: { relativePath: string; sha512: string; size: number }[];
  installedAtUnixSeconds: number;
  iconUrl: string | null;
}

export interface ModpackInstallReport {
  instanceId: string;
  packName: string;
  packVersion: string;
  installedFiles: number;
}

export interface ModpackUpdateReport {
  packName: string;
  fromVersion: string;
  toVersion: string;
  addedFiles: number;
  replacedFiles: number;
  deletedFiles: number;
  keptUserModified: string[];
}

export interface ExportModpackOptions {
  name: string;
  version: string;
  includeConfig: boolean;
  includeResourcePacks: boolean;
  includeShaders: boolean;
  includeServers: boolean;
  includeScreenshots: boolean;
}

export interface ExportModpackReport {
  instanceId: string;
  packName: string;
  packVersion: string;
  outputPath: string;
  totalBytes: number;
  /** 写入 files 引用的内容数（安装时按 URL 重新下载）。 */
  referencedFiles: number;
  /** 打入 overrides 的文件本体数。 */
  bundledFiles: number;
}

/** 整合包导出文件名：过滤 Windows 非法字符与控制字符，末尾去点。 */
export function sanitizeModpackFileName(name: string, version: string): string {
  const clean = (value: string): string =>
    value
      .replace(/[\\/:*?"<>|\u0000-\u001F]/g, "-")
      .trim()
      .replace(/[. ]+$/, "");
  const base = clean(name) || "modpack";
  const suffix = clean(version) || "1.0.0";
  return `${base}-${suffix}`;
}

export interface ModpackProgressEvent {
  stage: string;
  current: number;
  total: number;
  item: string;
}

export const LITTLESKIN_YGGDRASIL_URL = "https://littleskin.cn/api/yggdrasil";

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

export interface LaunchLogChunk {
  content: string;
  nextOffset: number;
  truncated: boolean;
}

export interface LaunchLogRead {
  sessionId: string;
  state: LaunchSessionState;
  stdout: LaunchLogChunk;
  stderr: LaunchLogChunk;
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
    versionId?: string,
  ): Promise<ContentInstallPreview>;
  confirmContentPreview(previewId: string): Promise<ContentInstallTask>;
  getContentInstallTasks(): Promise<ContentInstallTask[]>;
  getInstalledContent(instanceId: string): Promise<InstalledContent[]>;
  /** 实例模组目录实测清单(mods/ 扫描与安装记录合并;未收录文件 content 为 null)。 */
  getInstanceMods(instanceId: string): Promise<InstanceModEntry[]>;
  /** 启停模组文件(jar ↔ jar.disabled 改名并同步索引)。 */
  setInstanceModEnabled(instanceId: string, relativePath: string, enabled: boolean): Promise<InstanceModEntry>;
  checkContentUpdates(instanceId: string): Promise<ContentUpdateInfo[]>;
  planContentUpdate(
    instanceId: string,
    projectIds: string[],
  ): Promise<ContentInstallTask>;
  getInstanceContentAutoUpdate(instanceId: string): Promise<boolean>;
  setInstanceContentAutoUpdate(instanceId: string, enabled: boolean): Promise<void>;
  /** 启用或停用实例已安装内容（Mod 等）；只更新索引标记。 */
  setInstalledContentEnabled(contentId: string, enabled: boolean): Promise<InstalledContent>;
  /** 实例自定义的启动内存配置；返回 null 表示跟随全局设置。 */
  getInstanceLaunchOptions(instanceId: string): Promise<LaunchOptions | null>;
  setInstanceLaunchOptions(instanceId: string, options: LaunchOptions): Promise<void>;
  /** 清除实例自定义启动内存，恢复为跟随全局设置。 */
  clearInstanceLaunchOptions(instanceId: string): Promise<void>;
  /** 全局启动内存偏好；未设置时默认为自动分配。 */
  getGlobalLaunchPreference(): Promise<GlobalLaunchPreference>;
  setGlobalLaunchPreference(preference: GlobalLaunchPreference): Promise<void>;
  /** 当前机器按自动规则计算出的启动内存（供界面展示"自动分配"取值）。 */
  getAutoLaunchOptions(): Promise<LaunchOptions>;
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
  /** 手动创建一个备份。 */
  createManualWorldBackup(instanceId: string): Promise<WorldBackupSummary>;
  /** 手动删除一个备份（记录与归档随事务删除）。 */
  deleteWorldBackup(backupId: string): Promise<void>;
  listInstanceScreenshots(instanceId: string): Promise<InstanceScreenshot[]>;
  /** 把截图图片写入系统剪贴板。 */
  copyScreenshotToClipboard(instanceId: string, fileName: string): Promise<void>;
  openScreenshotLocation(instanceId: string, fileName: string): Promise<void>;
  deleteInstanceScreenshot(instanceId: string, fileName: string): Promise<RecycleBinItem>;
  deleteInstanceResource(resourceId: string): Promise<RecycleBinItem>;
  deleteInstanceWorld(instanceId: string, worldName: string): Promise<RecycleBinItem>;
  /** 读取实例 servers.dat 中的服务器列表。 */
  listInstanceServers(instanceId: string): Promise<InstanceServerEntry[]>;
  /** 追加服务器,返回写入后的完整列表。 */
  addInstanceServer(
    instanceId: string,
    name: string,
    address: string,
  ): Promise<InstanceServerEntry[]>;
  /** 按序号删除服务器,返回写入后的完整列表。 */
  removeInstanceServer(instanceId: string, index: number): Promise<InstanceServerEntry[]>;
  /** 按序号更新服务器名称与地址,返回写入后的完整列表。 */
  updateInstanceServer(
    instanceId: string,
    index: number,
    name: string,
    address: string,
  ): Promise<InstanceServerEntry[]>;
  /** 探测服务器状态;不可达返回 online=false 而非抛错。 */
  pingMinecraftServer(address: string): Promise<MinecraftServerStatus>;
  restoreRecycledEntry(itemId: string): Promise<RecycleBinItem>;
  getWorldBackupSettings(): Promise<WorldBackupSettings>;
  setWorldBackupIntervalMinutes(minutes: number): Promise<void>;
  setWorldBackupKeepCount(count: number): Promise<void>;
  listAccounts(): Promise<AccountSummary[]>;
  addOfflineAccount(username: string): Promise<AccountSummary>;
  addAuthlibAccount(
    serverUrl: string,
    username: string,
    password: string,
  ): Promise<AccountSummary>;
  setDefaultAccount(accountId: string): Promise<void>;
  removeAccount(accountId: string): Promise<void>;
  refreshAccountSession(accountId: string): Promise<AccountSummary>;
  /** 发起 Microsoft 设备码登录；返回用户码与验证地址，结果经事件到达。 */
  startMicrosoftDeviceLogin(): Promise<DeviceCodeInfo>;
  /** 取消正在进行的 Microsoft 设备码登录。 */
  cancelMicrosoftDeviceLogin(): Promise<void>;
  /** 订阅 Microsoft 设备码登录结果事件，返回取消订阅函数。 */
  onMicrosoftDeviceLogin(handler: (event: MicrosoftLoginEvent) => void): () => void;
  /** 在系统浏览器打开 https 外部链接。 */
  openExternalUrl(url: string): Promise<void>;
  /** 创建或加入 EasyTier 联机房间（首次自动下载校验 EasyTier）。 */
  startNetplayRoom(networkName: string, networkSecret: string, isHost: boolean): Promise<NetplayRoomView>;
  /** 离开当前联机房间。 */
  stopNetplayRoom(): Promise<void>;
  /** 当前联机房间状态。 */
  getNetplayStatus(): Promise<NetplayRoomView | null>;
  /** 当前房间成员列表（不在房间时返回空列表）。 */
  listNetplayPeers(): Promise<NetplayPeerView[]>;
  /** 客机建立到主机 MC 端口的本机回环转发；返回游戏内直连的本机端口。 */
  setNetplayForward(mcPort: number): Promise<number>;
  /** 简化 NAT 检测（STUN，仅手动触发）。 */
  detectNatType(): Promise<NatReportView>;
  /** 订阅 EasyTier 首次下载进度事件，返回取消订阅函数。 */
  onNetplayDownloadProgress(handler: (event: { current: number; total: number }) => void): () => void;
  getUiPreferences(): Promise<UiPreferences>;
  setUiTheme(theme: string): Promise<void>;
  setUiLanguage(language: string): Promise<void>;
  setUiMotion(motion: string): Promise<void>;
  setUiContrast(contrast: string): Promise<void>;
  getCliEnabled(): Promise<boolean>;
  setCliEnabled(enabled: boolean): Promise<void>;
  getUpdateChecksEnabled(): Promise<boolean>;
  setUpdateChecksEnabled(enabled: boolean): Promise<void>;
  checkForUpdates(): Promise<ReleaseInfo | null>;
  downloadUpdateInstaller(release: ReleaseInfo): Promise<string>;
  openUpdateLocation(path: string): Promise<void>;
  getUiBackground(): Promise<UiBackground>;
  setUiBackground(background: UiBackground): Promise<void>;
  importBackgroundImage(sourcePath: string): Promise<UiBackground>;
  importThemePack(sourcePath: string): Promise<ThemePack>;
  readBackgroundImage(): Promise<[string, number[]] | null>;
  /** 主题包标准 v2:导入(v1 自动升级)/列表/读取/删除/当前启用。 */
  importThemePackV2(sourcePath: string): Promise<ThemePackMeta>;
  listImportedThemePacks(): Promise<ThemePackMeta[]>;
  readThemePackV2(packId: string): Promise<string>;
  removeThemePack(packId: string): Promise<void>;
  getUiThemePack(): Promise<string>;
  setUiThemePack(packId: string): Promise<void>;
  /** 打开原生文件选择器挑选背景图片；用户取消时返回 null。 */
  pickBackgroundImage(): Promise<string | null>;
  /** 打开原生文件选择器挑选主题包 JSON；用户取消时返回 null。 */
  pickThemePackFile(): Promise<string | null>;
  /** 打开原生目录选择器；用户取消时返回 null。 */
  pickDirectory(): Promise<string | null>;
  /** 项目版本列表（自由下载对话框的版本选择）。 */
  listModrinthVersions(
    projectId: string,
    gameVersion?: string,
    loader?: string,
  ): Promise<ModrinthVersionSummary[]>;
  /** 自由下载：指定版本主文件下载到目标目录并按自定义文件名保存。 */
  downloadModrinthFile(versionId: string, targetDir: string, fileName: string): Promise<string>;
  /** 打开原生文件选择器挑选整合包（.mrpack/.zip）；用户取消时返回 null。 */
  pickModpackFile(): Promise<string | null>;
  importModpackPreview(sourcePath: string): Promise<ModpackPreviewResponse>;
  installModpack(previewId: string): Promise<ModpackInstallReport>;
  updateModpack(instanceId: string, sourcePath: string): Promise<ModpackUpdateReport>;
  getInstanceModpack(instanceId: string): Promise<InstalledModpack | null>;
  /** 该实例的整合包文件是否正在安装中（安装中禁止启动）。 */
  isModpackInstalling(instanceId: string): Promise<boolean>;
  /** 读取实例整合包图标：https 原样返回，本地图标转 data URL；无图标返回 null。 */
  getModpackIconDataUrl(instanceId: string): Promise<string | null>;
  /** 登记实例整合包图标（在线项目 iconUrl 或本地绝对路径）。 */
  setModpackIconUrl(instanceId: string, iconUrl: string): Promise<void>;
  /** 打开原生保存对话框选择整合包导出位置；用户取消时返回 null。 */
  pickModpackExportPath(packName: string, version: string): Promise<string | null>;
  /** 把实例导出为 Modrinth mrpack 到指定路径。 */
  exportInstanceModpack(
    instanceId: string,
    options: ExportModpackOptions,
    destinationPath: string,
  ): Promise<ExportModpackReport>;
  /** 在线整合包预览：下载并解析 Modrinth 整合包后返回预览，确认走 installModpack；可指定版本。 */
  previewOnlineModpack(projectId: string, versionId?: string): Promise<ModpackPreviewResponse>;
  /** 在线光影/资源包/模组安装：按实例版本解析（可指定版本），下载校验后导入实例。 */
  installOnlineResource(
    instanceId: string,
    kind: InstanceResourceKind,
    projectId: string,
    versionId?: string,
  ): Promise<InstanceResource>;
  /** 本机保存的 CurseForge API Key（仅本机使用；null 表示未配置）。 */
  getCurseforgeApiKey(): Promise<string | null>;
  /** 保存 CurseForge API Key；空白输入视为清除。 */
  setCurseforgeApiKey(key: string): Promise<void>;
  /** 用当前 Key 调官方接口验证有效性；成功返回游戏名。 */
  testCurseforgeApiKey(): Promise<string>;
  /** CurseForge 目录搜索（gameId=432 固定，classId 区分内容类型）。 */
  searchCurseforgeProjects(query: CurseforgeSearchQuery): Promise<CatalogSearchPage>;
  /** 项目文件列表（资源详情与版本选择，归一化为统一摘要）。 */
  listCurseforgeFiles(
    projectId: string,
    gameVersion?: string,
    loader?: string,
  ): Promise<CurseforgeFileSummary[]>;
  /** 分类列表（id 由 API 下发，前端不硬编码）。 */
  listCurseforgeCategories(classId: number): Promise<CurseforgeCategory[]>;
  /** 自由下载：指定文件下载到目标目录并按自定义文件名保存。 */
  downloadCurseforgeFile(
    projectId: string,
    fileId: string,
    targetDir: string,
    fileName: string,
  ): Promise<string>;
  /** 在线整合包预览（CurseForge 官方源），确认走 installModpack；可指定版本。 */
  previewCurseforgeModpack(projectId: string, fileId?: string): Promise<ModpackPreviewResponse>;
  /** 在线光影/资源包/模组安装（CurseForge 官方源，按选定文件直接安装，不做依赖闭包解析）。 */
  installCurseforgeResource(
    instanceId: string,
    kind: InstanceResourceKind,
    projectId: string,
    fileId?: string,
  ): Promise<InstanceResource>;
  /** 订阅整合包安装/更新进度事件，返回取消订阅函数。 */
  onModpackProgress(handler: (event: ModpackProgressEvent) => void): () => void;
  retryContentTask(taskId: string): Promise<void>;
  resolveContentTaskRecovery(taskId: string, decision: RecoveryDecision): Promise<void>;
  listInstances(): Promise<ManagedInstance[]>;
  listRecycleBinItems(): Promise<RecycleBinItem[]>;
  /** 存储概览:实例占用实测 + 数据目录所在磁盘总量与剩余(可能为 null)。 */
  storageOverview(): Promise<{
    instancesBytes: number;
    diskTotalBytes: number | null;
    diskFreeBytes: number | null;
  }>;
  recycleInstance(instanceId: string): Promise<RecycleBinItem>;
  restoreRecycleBinItem(itemId: string): Promise<ManagedInstance>;
  purgeRecycleBinItem(itemId: string): Promise<RecyclePurgeResult>;
  listWorldBackups(instanceId?: string): Promise<WorldBackupSummary[]>;
  startInstance(instanceId: string): Promise<LaunchSession>;
  stopInstance(instanceId: string): Promise<void>;
  listLaunchSessions(): Promise<LaunchSession[]>;
  /** 尾部跟随读取一次启动会话的游戏输出;偏移量按通道分别传入,单文件单次最多返回 2 MiB 尾部。 */
  readLaunchLog(
    sessionId: string,
    stdoutOffset: number,
    stderrOffset: number,
  ): Promise<LaunchLogRead>;
  /** 在系统文件管理器中选中该会话的游戏日志文件。 */
  openLaunchLogLocation(sessionId: string): Promise<void>;
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
  /** 取消排队、暂停或运行中的任务；运行中任务在下载边界中断后保持已取消。 */
  cancelTask(taskId: string, kind: TaskKind): Promise<void>;
  /** 删除终态任务（failed/completed/cancelled）并清理其受管暂存目录。 */
  deleteTask(taskId: string, kind: TaskKind): Promise<void>;
  getDownloadSpeedLimit(): Promise<number>;
  setDownloadSpeedLimit(bytesPerSec: number): Promise<void>;
  /** 下载并发连接数（1-32，默认 24）；持久化，重启后构造执行器时生效。 */
  getDownloadConcurrency(): Promise<number>;
  setDownloadConcurrency(connections: number): Promise<void>;
  getDownloadSourcePolicy(): Promise<SourcePolicy>;
  setDownloadSourcePolicy(policy: SourcePolicy): Promise<void>;
  /** HTTP 代理偏好；保存后新构造的客户端立即生效，已初始化的组件重启后完全切换。 */
  getProxyPreference(): Promise<ProxyPreference>;
  setProxyPreference(preference: ProxyPreference): Promise<void>;
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
const BROWSER_LAUNCH_LOGS_KEY = "moyumax.browser.launchLogs";
const BROWSER_CRASH_REPORTS_KEY = "moyumax.browser.crashReports";
const BROWSER_CONTENT_TASKS_KEY = "moyumax.browser.contentTasks";
const BROWSER_INSTALLED_CONTENT_KEY = "moyumax.browser.installedContent";
const BROWSER_CONTENT_UPDATES_KEY = "moyumax.browser.contentUpdates";
const BROWSER_CONTENT_AUTO_UPDATE_KEY = "moyumax.browser.contentAutoUpdate";
const BROWSER_LAUNCH_OPTIONS_KEY = "moyumax.browser.launchOptions";
const BROWSER_GLOBAL_LAUNCH_PREFERENCE_KEY = "moyumax.browser.globalLaunchPreference";
const BROWSER_INSTANCE_RESOURCES_KEY = "moyumax.browser.instanceResources";
const BROWSER_INSTANCE_WORLDS_KEY = "moyumax.browser.instanceWorlds";
const BROWSER_WORLD_DETAILS_KEY = "moyumax.browser.worldDetails";
const BROWSER_INSTANCE_SERVERS_KEY = "moyumax.browser.instanceServers";
const BROWSER_OFFLINE_SERVERS_KEY = "moyumax.browser.offlineServers";
const BROWSER_SCREENSHOTS_KEY = "moyumax.browser.screenshots";
const BROWSER_BACKUP_SETTINGS_KEY = "moyumax.browser.backupSettings";
const BROWSER_ACCOUNTS_KEY = "moyumax.browser.accounts";
const BROWSER_MODPACKS_KEY = "moyumax.browser.modpacks";
const BROWSER_CURSEFORGE_API_KEY = "moyumax.browser.curseforgeApiKey";
const BROWSER_CURSEFORGE_CATALOG_KEY = "moyumax.browser.curseforgeCatalog";
const BROWSER_CURSEFORGE_FILES_KEY = "moyumax.browser.curseforgeFiles";
const BROWSER_CURSEFORGE_CATEGORIES_KEY = "moyumax.browser.curseforgeCategories";
const BROWSER_CURSEFORGE_MODPACK_PREVIEW_KEY = "moyumax.browser.curseforgeModpackPreview";
const browserModpackProgressHandlers = new Set<(event: ModpackProgressEvent) => void>();
const browserMicrosoftLoginHandlers = new Set<(event: MicrosoftLoginEvent) => void>();
let browserMicrosoftLoginTimer: number | undefined;
const browserNetplayProgressHandlers = new Set<(event: { current: number; total: number }) => void>();

function browserEmitNetplayProgress(event: { current: number; total: number }): void {
  for (const handler of browserNetplayProgressHandlers) {
    handler(event);
  }
}

function browserEmitMicrosoftLogin(event: MicrosoftLoginEvent): void {
  for (const handler of browserMicrosoftLoginHandlers) {
    handler(event);
  }
}

function browserEmitModpackProgress(event: ModpackProgressEvent): void {
  for (const handler of browserModpackProgressHandlers) {
    handler(event);
  }
}
const BROWSER_MODRINTH_OFFLINE_KEY = "moyumax.browser.modrinthOffline";
const BROWSER_CLOSE_BEHAVIOR_KEY = "moyumax.browser.windowCloseBehavior";
const BROWSER_SHELL_STATE_KEY = "moyumax.browser.shellState";
const BROWSER_STARTUP_KIND_KEY = "moyumax.browser.startupKind";
const BROWSER_PENDING_INTENT_KEY = "moyumax.browser.pendingIntent";
const BROWSER_TASKS_PAUSED_KEY = "moyumax.browser.tasksPaused";
const BROWSER_WINDOW_STATE_KEY = "moyumax.browser.windowState";
const BROWSER_SOURCE_POLICY_KEY = "moyumax.browser.sourcePolicy";
const BROWSER_PROXY_PREFERENCE_KEY = "moyumax.browser.proxyPreference";
const BROWSER_JAVA_ENVIRONMENTS_KEY = "moyumax.browser.javaEnvironments";
const BROWSER_SPEED_LIMIT_KEY = "moyumax.browser.speedLimit";
const BROWSER_CONCURRENCY_KEY = "moyumax.browser.downloadConcurrency";
const browserPreviews = new Map<string, InstallSelection>();
const browserContentPreviews = new Map<string, ContentInstallPlan>();
const browserDiagnosticPreviews = new Map<string, string>();
/** 在线整合包预览登记（Modrinth/CurseForge 共用，installModpack 按 id 取回）。 */
const browserOnlineModpackPreviews = new Map<string, ModpackPreview>();
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
    previewModrinthInstall: (instanceId, projectId, selectedOptionalProjects, versionId) =>
      invoke<ContentInstallPreview>("preview_modrinth_install", {
        instanceId,
        projectId,
        selectedOptionalProjects,
        versionId: versionId ?? null,
      }),
    confirmContentPreview: (previewId) =>
      invoke<ContentInstallTask>("confirm_content_preview", { previewId }),
    getContentInstallTasks: () =>
      invoke<ContentInstallTask[]>("get_content_install_tasks"),
    getInstalledContent: (instanceId) =>
      invoke<InstalledContent[]>("get_installed_content", { instanceId }),
    getInstanceMods: (instanceId) =>
      invoke<InstanceModEntry[]>("list_instance_mods", { instanceId }),
    setInstanceModEnabled: (instanceId, relativePath, enabled) =>
      invoke<InstanceModEntry>("set_instance_mod_enabled", { instanceId, relativePath, enabled }),
    checkContentUpdates: (instanceId) =>
      invoke<ContentUpdateInfo[]>("check_content_updates", { instanceId }),
    planContentUpdate: (instanceId, projectIds) =>
      invoke<ContentInstallTask>("plan_content_update", { instanceId, projectIds }),
    getInstanceContentAutoUpdate: (instanceId) =>
      invoke<boolean>("get_instance_content_auto_update", { instanceId }),
    setInstanceContentAutoUpdate: (instanceId, enabled) =>
      invoke<void>("set_instance_content_auto_update", { instanceId, enabled }),
    setInstalledContentEnabled: (contentId, enabled) =>
      invoke<InstalledContent>("set_installed_content_enabled", { contentId, enabled }),
    getInstanceLaunchOptions: (instanceId) =>
      invoke<LaunchOptions | null>("get_instance_launch_options", { instanceId }),
    setInstanceLaunchOptions: (instanceId, options) =>
      invoke<void>("set_instance_launch_options", { instanceId, options }),
    clearInstanceLaunchOptions: (instanceId) =>
      invoke<void>("clear_instance_launch_options", { instanceId }),
    getGlobalLaunchPreference: () =>
      invoke<GlobalLaunchPreference>("get_global_launch_preference"),
    setGlobalLaunchPreference: (preference) =>
      invoke<void>("set_global_launch_preference", { preference }),
    getAutoLaunchOptions: () => invoke<LaunchOptions>("get_auto_launch_options"),
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
    pickDirectory: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      return typeof selected === "string" ? selected : null;
    },
    listModrinthVersions: (projectId, gameVersion, loader) =>
      invoke<ModrinthVersionSummary[]>("list_modrinth_versions", {
        projectId,
        gameVersion: gameVersion ?? null,
        loader: loader ?? null,
      }),
    downloadModrinthFile: (versionId, targetDir, fileName) =>
      invoke<string>("download_modrinth_file", { versionId, targetDir, fileName }),
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
    createManualWorldBackup: (instanceId) =>
      invoke<WorldBackupSummary>("create_manual_world_backup", { instanceId }),
    deleteWorldBackup: (backupId) =>
      invoke<void>("delete_world_backup", { backupId }),
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
    listInstanceServers: (instanceId) =>
      invoke<InstanceServerEntry[]>("list_instance_servers", { instanceId }),
    addInstanceServer: (instanceId, name, address) =>
      invoke<InstanceServerEntry[]>("add_instance_server", { instanceId, name, address }),
    removeInstanceServer: (instanceId, index) =>
      invoke<InstanceServerEntry[]>("remove_instance_server", { instanceId, index }),
    updateInstanceServer: (instanceId, index, name, address) =>
      invoke<InstanceServerEntry[]>("update_instance_server", {
        instanceId,
        index,
        name,
        address,
      }),
    pingMinecraftServer: (address) =>
      invoke<MinecraftServerStatus>("ping_minecraft_server", { address }),
    restoreRecycledEntry: (itemId) =>
      invoke<RecycleBinItem>("restore_recycled_entry", { itemId }),
    getWorldBackupSettings: () =>
      invoke<WorldBackupSettings>("get_world_backup_settings"),
    setWorldBackupIntervalMinutes: (minutes) =>
      invoke<void>("set_world_backup_interval_minutes", { minutes }),
    setWorldBackupKeepCount: (count) =>
      invoke<void>("set_world_backup_keep_count", { count }),
    listAccounts: () => invoke<AccountSummary[]>("list_accounts"),
    addOfflineAccount: (username) =>
      invoke<AccountSummary>("add_offline_account", { username }),
    addAuthlibAccount: (serverUrl, username, password) =>
      invoke<AccountSummary>("add_authlib_account", { serverUrl, username, password }),
    setDefaultAccount: (accountId) =>
      invoke<void>("set_default_account", { accountId }),
    removeAccount: (accountId) => invoke<void>("remove_account", { accountId }),
    refreshAccountSession: (accountId) =>
      invoke<AccountSummary>("refresh_account_session", { accountId }),
    startMicrosoftDeviceLogin: () =>
      invoke<DeviceCodeInfo>("start_microsoft_device_login"),
    cancelMicrosoftDeviceLogin: () =>
      invoke<void>("cancel_microsoft_device_login"),
    onMicrosoftDeviceLogin: (handler) => {
      let unlisten: (() => void) | undefined;
      void listen<MicrosoftLoginEvent>("microsoft-device-login", (event) => {
        handler(event.payload);
      }).then((release) => {
        unlisten = release;
      });
      return () => unlisten?.();
    },
    openExternalUrl: (url) => invoke<void>("open_external_url", { url }),
    startNetplayRoom: (networkName, networkSecret, isHost) =>
      invoke<NetplayRoomView>("start_netplay_room", { networkName, networkSecret, isHost }),
    stopNetplayRoom: () => invoke<void>("stop_netplay_room"),
    getNetplayStatus: () => invoke<NetplayRoomView | null>("get_netplay_status"),
    listNetplayPeers: () => invoke<NetplayPeerView[]>("list_netplay_peers"),
    setNetplayForward: (mcPort) => invoke<number>("set_netplay_forward", { mcPort }),
    detectNatType: () => invoke<NatReportView>("detect_nat_type"),
    onNetplayDownloadProgress: (handler) => {
      let unlisten: (() => void) | undefined;
      void listen<{ current: number; total: number }>(
        "netplay-download-progress",
        (event) => handler(event.payload),
      ).then((release) => {
        unlisten = release;
      });
      return () => unlisten?.();
    },
    getUiPreferences: () => invoke<UiPreferences>("get_ui_preferences"),
    setUiTheme: (theme) => invoke<void>("set_ui_theme", { theme }),
    setUiLanguage: (language) => invoke<void>("set_ui_language", { language }),
    setUiMotion: (motion) => invoke<void>("set_ui_motion", { motion }),
    setUiContrast: (contrast) => invoke<void>("set_ui_contrast", { contrast }),
    getCliEnabled: () => invoke<boolean>("get_cli_enabled"),
    setCliEnabled: (enabled) => invoke<void>("set_cli_enabled", { enabled }),
    getUpdateChecksEnabled: () => invoke<boolean>("get_update_checks_enabled"),
    setUpdateChecksEnabled: (enabled) =>
      invoke<void>("set_update_checks_enabled", { enabled }),
    checkForUpdates: () => invoke<ReleaseInfo | null>("check_for_updates"),
    downloadUpdateInstaller: (release) =>
      invoke<string>("download_update_installer", { release }),
    openUpdateLocation: (path) => invoke<void>("open_update_location", { path }),
    getUiBackground: () => invoke<UiBackground>("get_ui_background"),
    setUiBackground: (background) =>
      invoke<void>("set_ui_background", { background }),
    importBackgroundImage: (sourcePath) =>
      invoke<UiBackground>("import_background_image", { sourcePath }),
    importThemePack: (sourcePath) =>
      invoke<ThemePack>("import_theme_pack", { sourcePath }),
    importThemePackV2: (sourcePath) =>
      invoke<ThemePackMeta>("import_theme_pack_v2", { sourcePath }),
    listImportedThemePacks: () =>
      invoke<ThemePackMeta[]>("list_imported_theme_packs"),
    readThemePackV2: (packId) => invoke<string>("read_theme_pack_v2", { packId }),
    removeThemePack: (packId) => invoke<void>("remove_theme_pack", { packId }),
    getUiThemePack: () => invoke<string>("get_ui_theme_pack"),
    setUiThemePack: (packId) => invoke<void>("set_ui_theme_pack", { packId }),
    readBackgroundImage: () =>
      invoke<[string, number[]] | null>("read_background_image"),
    pickBackgroundImage: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "背景图片", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      return typeof selected === "string" ? selected : null;
    },
    pickThemePackFile: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "主题包", extensions: ["json"] }],
      });
      return typeof selected === "string" ? selected : null;
    },
    pickModpackFile: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "整合包", extensions: ["mrpack", "zip"] }],
      });
      return typeof selected === "string" ? selected : null;
    },
    importModpackPreview: (sourcePath) =>
      invoke<ModpackPreviewResponse>("import_modpack_preview", { sourcePath }),
    installModpack: (previewId) =>
      invoke<ModpackInstallReport>("install_modpack", { previewId }),
    updateModpack: (instanceId, sourcePath) =>
      invoke<ModpackUpdateReport>("update_modpack", { instanceId, sourcePath }),
    getInstanceModpack: (instanceId) =>
      invoke<InstalledModpack | null>("get_instance_modpack", { instanceId }),
    isModpackInstalling: (instanceId) => invoke<boolean>("is_modpack_installing", { instanceId }),
    getModpackIconDataUrl: (instanceId) =>
      invoke<string | null>("get_modpack_icon_data_url", { instanceId }),
    setModpackIconUrl: (instanceId, iconUrl) =>
      invoke<void>("set_modpack_icon_url", { instanceId, iconUrl }),
    pickModpackExportPath: async (packName, version) => {
      const { save } = await import("@tauri-apps/plugin-dialog");
      return await save({
        defaultPath: `${sanitizeModpackFileName(packName, version)}.mrpack`,
        filters: [{ name: "Modrinth 整合包", extensions: ["mrpack"] }],
      });
    },
    exportInstanceModpack: (instanceId, options, destinationPath) =>
      invoke<ExportModpackReport>("export_instance_modpack", {
        instanceId,
        options,
        destinationPath,
      }),
    previewOnlineModpack: (projectId, versionId) =>
      invoke<ModpackPreviewResponse>("preview_online_modpack", { projectId, versionId: versionId ?? null }),
    installOnlineResource: (instanceId, kind, projectId, versionId) =>
      invoke<InstanceResource>("install_online_resource", { instanceId, kind, projectId, versionId }),
    getCurseforgeApiKey: () => invoke<string | null>("get_curseforge_api_key"),
    setCurseforgeApiKey: (key) => invoke<void>("set_curseforge_api_key", { key }),
    testCurseforgeApiKey: () => invoke<string>("test_curseforge_api_key"),
    searchCurseforgeProjects: (query) =>
      invoke<CatalogSearchPage>("search_curseforge_projects", { query }),
    listCurseforgeFiles: (projectId, gameVersion, loader) =>
      invoke<CurseforgeFileSummary[]>("list_curseforge_files", {
        projectId,
        gameVersion: gameVersion ?? null,
        loader: loader ?? null,
      }),
    listCurseforgeCategories: (classId) =>
      invoke<CurseforgeCategory[]>("list_curseforge_categories", { classId }),
    downloadCurseforgeFile: (projectId, fileId, targetDir, fileName) =>
      invoke<string>("download_curseforge_file", { projectId, fileId, targetDir, fileName }),
    previewCurseforgeModpack: (projectId, fileId) =>
      invoke<ModpackPreviewResponse>("preview_curseforge_modpack", {
        projectId,
        fileId: fileId ?? null,
      }),
    installCurseforgeResource: (instanceId, kind, projectId, fileId) =>
      invoke<InstanceResource>("install_curseforge_resource", {
        instanceId,
        kind,
        projectId,
        fileId: fileId ?? null,
      }),
    onModpackProgress: (handler) => {
      let unlisten: (() => void) | undefined;
      void listen<ModpackProgressEvent>("modpack-progress", (event) => {
        handler(event.payload);
      }).then((release) => {
        unlisten = release;
      });
      return () => unlisten?.();
    },
    retryContentTask: (taskId) => invoke<void>("retry_content_task", { taskId }),
    resolveContentTaskRecovery: (taskId, decision) =>
      invoke<void>("resolve_content_task_recovery", { taskId, decision }),
    listInstances: () => invoke<ManagedInstance[]>("list_instances"),
    listRecycleBinItems: () =>
      invoke<RecycleBinItem[]>("list_recycle_bin_items"),
    storageOverview: () =>
      invoke<{ instancesBytes: number; diskTotalBytes: number | null; diskFreeBytes: number | null }>("storage_overview"),
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
    readLaunchLog: (sessionId, stdoutOffset, stderrOffset) =>
      invoke<LaunchLogRead>("read_launch_log", {
        sessionId,
        stdoutOffset,
        stderrOffset,
      }),
    openLaunchLogLocation: (sessionId) =>
      invoke<void>("open_launch_log_location", { sessionId }),
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
    cancelTask: (taskId, kind) =>
      invoke<void>(kind === "content" ? "cancel_content_task" : "cancel_install_task", {
        taskId,
      }),
    deleteTask: (taskId, kind) =>
      invoke<void>(kind === "content" ? "delete_content_task" : "delete_install_task", {
        taskId,
      }),
    getDownloadSpeedLimit: () => invoke<number>("get_download_speed_limit"),
    setDownloadSpeedLimit: (bytesPerSec) =>
      invoke<void>("set_download_speed_limit", { bytesPerSec }),
    getDownloadConcurrency: () => invoke<number>("get_download_concurrency"),
    setDownloadConcurrency: (connections) =>
      invoke<void>("set_download_concurrency", { connections }),
    getDownloadSourcePolicy: () =>
      invoke<SourcePolicy>("get_download_source_policy"),
    setDownloadSourcePolicy: (policy) =>
      invoke<void>("set_download_source_policy", { policy }),
    getProxyPreference: () => invoke<ProxyPreference>("get_proxy_preference"),
    setProxyPreference: (preference) =>
      invoke<void>("set_proxy_preference", { preference }),
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
      const catalog: ModrinthProjectSummary[] = [
        {
          projectId: "ROOT0001",
          slug: "continuity",
          title: "Continuity",
          description: "为方块纹理提供连续连接效果。",
          downloads: 34_200_000,
          clientSide: "required",
          serverSide: "optional",
          iconUrl: null,
          author: "peppodev",
          dateModified: "2026-06-18T10:00:00Z",
          versions: ["26.1", "26.2"],
        },
        {
          projectId: "ROOT0002",
          slug: "lithium",
          title: "Lithium",
          description: "不改动原版行为的游戏逻辑性能优化。",
          downloads: 18_700_000,
          clientSide: "optional",
          serverSide: "optional",
          iconUrl: null,
          author: "jellysquid3",
          dateModified: "2026-05-30T10:00:00Z",
          versions: ["26.2"],
        },
      ];
      const keyword = query.query.trim().toLocaleLowerCase();
      const hits = keyword
        ? catalog.filter((hit) =>
            `${hit.title} ${hit.description}`.toLocaleLowerCase().includes(keyword),
          )
        : catalog;
      return {
        hits: hits.slice(query.offset, query.offset + query.limit),
        offset: query.offset,
        limit: query.limit,
        totalHits: hits.length,
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
    async getInstanceMods(instanceId) {
      const records = browserInstalledContent().filter((entry) => entry.instanceId === instanceId);
      const files = browserInstanceModFiles(instanceId);
      return files
        .filter((file) => {
          const base = file.fileName.endsWith(".disabled")
            ? file.fileName.slice(0, -".disabled".length)
            : file.fileName;
          return base.toLowerCase().endsWith(".jar");
        })
        .map((file) => {
          const relativePath = `mods/${file.fileName}`;
          const base = file.fileName.endsWith(".disabled")
            ? file.fileName.slice(0, -".disabled".length)
            : file.fileName;
          const content =
            records.find((record) => {
              const stored = record.relativePath.startsWith(".minecraft/")
                ? record.relativePath.slice(".minecraft/".length)
                : record.relativePath;
              return stored === relativePath || stored === `mods/${base}`;
            }) ?? null;
          return {
            fileName: file.fileName,
            relativePath,
            sizeBytes: file.sizeBytes,
            enabled: !file.fileName.endsWith(".disabled"),
            content,
          } satisfies InstanceModEntry;
        })
        .sort((left, right) =>
          (left.content?.projectTitle ?? left.fileName)
            .toLowerCase()
            .localeCompare((right.content?.projectTitle ?? right.fileName).toLowerCase()),
        );
    },
    async setInstanceModEnabled(instanceId, relativePath, enabled) {
      if (!relativePath.startsWith("mods/") || relativePath.includes("..")) {
        throw new Error("模组路径无效");
      }
      const key = `moyumax.browser.instanceMods.${instanceId}`;
      const files = browserInstanceModFiles(instanceId);
      const fileName = relativePath.slice("mods/".length);
      const index = files.findIndex((file) => file.fileName === fileName);
      if (index < 0) throw new Error("模组文件不存在");
      const currentlyEnabled = !fileName.endsWith(".disabled");
      const finalName = enabled
        ? fileName.replace(/\.disabled$/, "")
        : currentlyEnabled
          ? `${fileName}.disabled`
          : fileName;
      files[index] = { ...files[index]!, fileName: finalName };
      window.localStorage.setItem(key, JSON.stringify(files));
      const finalRelative = `mods/${finalName}`;
      const contents = browserInstalledContent();
      const record = contents.find(
        (entry) =>
          entry.instanceId === instanceId &&
          (entry.relativePath === relativePath || entry.relativePath === `.minecraft/${relativePath}`),
      );
      if (record) {
        record.enabled = enabled;
        record.relativePath = record.relativePath.startsWith(".minecraft/")
          ? `.minecraft/${finalRelative}`
          : finalRelative;
        window.localStorage.setItem(BROWSER_INSTALLED_CONTENT_KEY, JSON.stringify(contents));
      }
      return {
        fileName: finalName,
        relativePath: finalRelative,
        sizeBytes: files[index]!.sizeBytes,
        enabled,
        content: record ?? null,
      } satisfies InstanceModEntry;
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
    async setInstalledContentEnabled(contentId, enabled) {
      const contents = browserInstalledContent();
      const entry = contents.find((candidate) => candidate.id === contentId);
      if (!entry) throw new Error("内容项不存在");
      entry.enabled = enabled;
      window.localStorage.setItem(
        BROWSER_INSTALLED_CONTENT_KEY,
        JSON.stringify(contents),
      );
      return entry;
    },
    async getInstanceLaunchOptions(instanceId) {
      if (!browserInstances().some((candidate) => candidate.id === instanceId)) {
        throw new Error("实例不存在");
      }
      return browserLaunchOptions()[instanceId] ?? null;
    },
    async setInstanceLaunchOptions(instanceId, options) {
      if (!browserInstances().some((candidate) => candidate.id === instanceId)) {
        throw new Error("实例不存在");
      }
      assertValidLaunchOptions(options);
      const all = browserLaunchOptions();
      all[instanceId] = {
        minimumMemoryMib: options.minimumMemoryMib,
        maximumMemoryMib: options.maximumMemoryMib,
      };
      window.localStorage.setItem(BROWSER_LAUNCH_OPTIONS_KEY, JSON.stringify(all));
    },
    async clearInstanceLaunchOptions(instanceId) {
      if (!browserInstances().some((candidate) => candidate.id === instanceId)) {
        throw new Error("实例不存在");
      }
      const all = browserLaunchOptions();
      delete all[instanceId];
      window.localStorage.setItem(BROWSER_LAUNCH_OPTIONS_KEY, JSON.stringify(all));
    },
    async getGlobalLaunchPreference() {
      return browserGlobalLaunchPreference();
    },
    async setGlobalLaunchPreference(preference) {
      if (preference.mode === "custom") {
        assertValidLaunchOptions({
          minimumMemoryMib: preference.minMib,
          maximumMemoryMib: preference.maxMib,
        });
      }
      window.localStorage.setItem(
        BROWSER_GLOBAL_LAUNCH_PREFERENCE_KEY,
        JSON.stringify(preference),
      );
    },
    async getAutoLaunchOptions() {
      return { minimumMemoryMib: 512, maximumMemoryMib: 4096 };
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
    async createManualWorldBackup(instanceId) {
      const backups = browserWorldBackups();
      const instances = browserInstances();
      const instance = instances.find((candidate) => candidate.id === instanceId);
      if (!instance) throw new Error("目标实例不存在");
      const now = Math.floor(Date.now() / 1000);
      const backup: WorldBackupSummary = {
        id: `backup-${crypto.randomUUID()}`,
        instanceId,
        instanceName: instance.name,
        launchSessionId: null,
        trigger: "manual",
        state: "ready",
        archivePath: `D:\\MoyuMax\\data\\backups\\instances\\${instanceId}\\${now}-manual.zip`,
        worldCount: 1,
        sourceBytes: 1024,
        archiveBytes: 512,
        createdAtUnixSeconds: now,
        completedAtUnixSeconds: now,
        errorSummary: null,
        kind: "full",
        baseBackupId: null,
      };
      backups.unshift(backup);
      window.localStorage.setItem(BROWSER_WORLD_BACKUPS_KEY, JSON.stringify(backups));
      return backup;
    },
    async deleteWorldBackup(backupId) {
      const backups = browserWorldBackups();
      const index = backups.findIndex((candidate) => candidate.id === backupId);
      if (index < 0) throw new Error("备份不存在或已被删除");
      backups.splice(index, 1);
      window.localStorage.setItem(BROWSER_WORLD_BACKUPS_KEY, JSON.stringify(backups));
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
    async listAccounts() {
      return browserAccounts();
    },
    async addOfflineAccount(username) {
      if (!/^[A-Za-z0-9_]{3,16}$/.test(username)) {
        throw new Error("本地玩家名称必须是 3-16 位 ASCII 字母、数字或下划线");
      }
      const accounts = browserAccounts();
      const account: AccountSummary = {
        id: crypto.randomUUID(),
        kind: "offline",
        username,
        playerUuid: crypto.randomUUID(),
        serverUrl: null,
        isDefault: accounts.every((candidate) => !candidate.isDefault),
        sessionState: "valid",
        createdAtUnixSeconds: Math.floor(Date.now() / 1000),
        lastValidatedAtUnixSeconds: null,
      };
      accounts.push(account);
      window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
      return account;
    },
    async addAuthlibAccount(serverUrl, username, password) {
      if (password === "wrong") {
        throw new Error("账户凭据无效或会话已过期：用户名或密码错误");
      }
      if (!username.trim() || !password) {
        throw new Error("用户名和密码不能为空");
      }
      const accounts = browserAccounts();
      for (const candidate of accounts) candidate.isDefault = false;
      const account: AccountSummary = {
        id: crypto.randomUUID(),
        kind: "authlib",
        username: username.split("@")[0] ?? username,
        playerUuid: crypto.randomUUID(),
        serverUrl,
        isDefault: true,
        sessionState: "valid",
        createdAtUnixSeconds: Math.floor(Date.now() / 1000),
        lastValidatedAtUnixSeconds: Math.floor(Date.now() / 1000),
      };
      accounts.push(account);
      window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
      return account;
    },
    async setDefaultAccount(accountId) {
      const accounts = browserAccounts();
      if (!accounts.some((candidate) => candidate.id === accountId)) {
        throw new Error("账户不存在");
      }
      for (const account of accounts) {
        account.isDefault = account.id === accountId;
      }
      window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
    },
    async removeAccount(accountId) {
      const accounts = browserAccounts();
      const index = accounts.findIndex((candidate) => candidate.id === accountId);
      if (index < 0) throw new Error("账户不存在");
      const [removed] = accounts.splice(index, 1);
      if (removed?.isDefault && accounts.length > 0 && accounts[0]) {
        accounts[0].isDefault = true;
      }
      window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
    },
    async refreshAccountSession(accountId) {
      const accounts = browserAccounts();
      const account = accounts.find((candidate) => candidate.id === accountId);
      if (!account) throw new Error("账户不存在");
      if (account.kind === "authlib" && account.sessionState === "expired") {
        throw new Error("账户凭据无效或会话已过期：会话已被认证服务器吊销，请重新登录");
      }
      if (account.kind === "microsoft" && account.sessionState === "expired") {
        throw new Error("Microsoft 会话已失效，请重新登录：Microsoft 会话已被吊销，请重新登录");
      }
      account.lastValidatedAtUnixSeconds = Math.floor(Date.now() / 1000);
      window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
      return account;
    },
    async startMicrosoftDeviceLogin() {
      if (browserMicrosoftLoginTimer !== undefined) {
        throw new Error("已有 Microsoft 登录正在进行");
      }
      const scenario =
        window.localStorage.getItem("moyumax.browser.msLoginScenario") ?? "success";
      const info: DeviceCodeInfo = {
        userCode: "AB12-CD34",
        verificationUri: "https://www.microsoft.com/link",
        expiresInSeconds: 900,
      };
      if (scenario === "fail-start") {
        throw new Error("无法获取 Microsoft 设备码（HTTP 503），请稍后重试");
      }
      const delay = scenario === "pending" ? 60_000 : 1_000;
      browserMicrosoftLoginTimer = window.setTimeout(() => {
        browserMicrosoftLoginTimer = undefined;
        if (scenario === "pending") {
          return;
        }
        if (scenario === "fail") {
          browserEmitMicrosoftLogin({
            state: "failed",
            account: null,
            message: "该 Microsoft 账户未拥有 Minecraft，请使用已购买游戏的账户登录",
          });
          return;
        }
        const accounts = browserAccounts();
        for (const candidate of accounts) candidate.isDefault = false;
        const now = Math.floor(Date.now() / 1000);
        const account: AccountSummary = {
          id: crypto.randomUUID(),
          kind: "microsoft",
          username: "Steve",
          playerUuid: crypto.randomUUID(),
          serverUrl: null,
          isDefault: true,
          sessionState: "valid",
          createdAtUnixSeconds: now,
          lastValidatedAtUnixSeconds: now,
        };
        accounts.push(account);
        window.localStorage.setItem(BROWSER_ACCOUNTS_KEY, JSON.stringify(accounts));
        browserEmitMicrosoftLogin({ state: "completed", account, message: null });
      }, delay);
      return info;
    },
    async cancelMicrosoftDeviceLogin() {
      if (browserMicrosoftLoginTimer !== undefined) {
        window.clearTimeout(browserMicrosoftLoginTimer);
        browserMicrosoftLoginTimer = undefined;
        browserEmitMicrosoftLogin({
          state: "cancelled",
          account: null,
          message: "Microsoft 登录已取消",
        });
      }
    },
    onMicrosoftDeviceLogin(handler) {
      browserMicrosoftLoginHandlers.add(handler);
      return () => {
        browserMicrosoftLoginHandlers.delete(handler);
      };
    },
    async openExternalUrl(url) {
      if (!url.startsWith("https://")) {
        throw new Error("只允许打开 https 链接");
      }
    },
    async startNetplayRoom(networkName, networkSecret, isHost) {
      if (!/^[A-Za-z0-9_-]{4,32}$/.test(networkName)) {
        throw new Error("房间号必须是 4-32 位字母、数字、连字符或下划线");
      }
      if (!/^[!-~]{8,64}$/.test(networkSecret)) {
        throw new Error("房间密码必须是 8-64 位可见字符（不含空格）");
      }
      if (window.localStorage.getItem("moyumax.browser.easytierNeedsDownload") === "true") {
        for (const [current, total] of [[6_000_000, 21_000_000], [14_000_000, 21_000_000], [21_000_000, 21_000_000]] as const) {
          browserEmitNetplayProgress({ current, total });
        }
      }
      const view: NetplayRoomView = {
        networkName,
        virtualIp: isHost ? "10.144.144.1" : "自动分配中…",
        isHost,
        mcLanPort: isHost ? 25565 : null,
        forwardedLocalPort: null,
      };
      window.localStorage.setItem("moyumax.browser.netplayRoom", JSON.stringify(view));
      return view;
    },
    async stopNetplayRoom() {
      window.localStorage.removeItem("moyumax.browser.netplayRoom");
    },
    async getNetplayStatus() {
      const serialized = window.localStorage.getItem("moyumax.browser.netplayRoom");
      return serialized ? (JSON.parse(serialized) as NetplayRoomView) : null;
    },
    async listNetplayPeers() {
      const serialized = window.localStorage.getItem("moyumax.browser.netplayRoom");
      if (!serialized) {
        return [];
      }
      const seeded = window.localStorage.getItem("moyumax.browser.netplayPeers");
      if (seeded) {
        return JSON.parse(seeded) as NetplayPeerView[];
      }
      const room = JSON.parse(serialized) as NetplayRoomView;
      // 默认种子：主机房看到一名直连成员，客机看到主机（经中继）。
      return room.isHost
        ? [{ ipv4: "10.144.144.2", hostname: "MoyuMax", isHost: false, latencyMs: 23, connection: "p2p" as const }]
        : [{ ipv4: "10.144.144.1", hostname: "MoyuMax", isHost: true, latencyMs: 18, connection: "relay" as const }];
    },
    async setNetplayForward(mcPort) {
      if (!Number.isInteger(mcPort) || mcPort < 1 || mcPort > 65535) {
        throw new Error("端口号必须是 1-65535 的整数");
      }
      const serialized = window.localStorage.getItem("moyumax.browser.netplayRoom");
      if (!serialized) {
        throw new Error("当前不在联机房间中");
      }
      const room = JSON.parse(serialized) as NetplayRoomView;
      if (room.isHost) {
        throw new Error("主机无需端口转发，直接告诉队友你的局域网端口即可");
      }
      room.virtualIp = room.virtualIp === "自动分配中…" ? "10.144.144.2" : room.virtualIp;
      room.forwardedLocalPort = 16565;
      window.localStorage.setItem("moyumax.browser.netplayRoom", JSON.stringify(room));
      return 16565;
    },
    async detectNatType() {
      if (window.localStorage.getItem("moyumax.browser.natOffline") === "true") {
        throw new Error("NAT 检测失败：无法连接 STUN 服务器");
      }
      return {
        mappedAddress: "203.0.113.55:54321",
        behindNat: true,
        impact: "你在 NAT 之后，直连入站通常不可达；建议使用联机房间组网",
      };
    },
    onNetplayDownloadProgress(handler) {
      browserNetplayProgressHandlers.add(handler);
      return () => {
        browserNetplayProgressHandlers.delete(handler);
      };
    },
    async getUiPreferences() {
      const serialized = window.localStorage.getItem("moyumax.browser.uiPreferences");
      return serialized
        ? (JSON.parse(serialized) as UiPreferences)
        : { theme: "system", language: "zh-CN", motion: "system", contrast: "standard" };
    },
    async setUiTheme(theme) {
      const preferences = await this.getUiPreferences();
      preferences.theme = theme;
      window.localStorage.setItem(
        "moyumax.browser.uiPreferences",
        JSON.stringify(preferences),
      );
    },
    async setUiLanguage(language) {
      const preferences = await this.getUiPreferences();
      preferences.language = language;
      window.localStorage.setItem(
        "moyumax.browser.uiPreferences",
        JSON.stringify(preferences),
      );
    },
    async setUiMotion(motion) {
      const preferences = await this.getUiPreferences();
      preferences.motion = motion;
      window.localStorage.setItem(
        "moyumax.browser.uiPreferences",
        JSON.stringify(preferences),
      );
    },
    async setUiContrast(contrast) {
      const preferences = await this.getUiPreferences();
      preferences.contrast = contrast;
      window.localStorage.setItem(
        "moyumax.browser.uiPreferences",
        JSON.stringify(preferences),
      );
    },
    async getCliEnabled() {
      return window.localStorage.getItem("moyumax.browser.cliEnabled") === "true";
    },
    async setCliEnabled(enabled) {
      window.localStorage.setItem("moyumax.browser.cliEnabled", String(enabled));
    },
    async getUpdateChecksEnabled() {
      return window.localStorage.getItem("moyumax.browser.updateChecks") !== "false";
    },
    async setUpdateChecksEnabled(enabled) {
      window.localStorage.setItem("moyumax.browser.updateChecks", String(enabled));
    },
    async checkForUpdates() {
      if (window.localStorage.getItem("moyumax.browser.updateChecks") === "false") {
        throw new Error("更新提示已关闭；可在设置中重新开启");
      }
      const serialized = window.localStorage.getItem("moyumax.browser.latestRelease");
      return serialized ? (JSON.parse(serialized) as ReleaseInfo) : null;
    },
    async downloadUpdateInstaller(release) {
      if (window.localStorage.getItem("moyumax.browser.updateDownloadFails") === "true") {
        throw new Error("安装包 SHA-256 校验失败");
      }
      if (!release.installer) throw new Error("该发布没有 Windows 安装包资产");
      return `D:\\MoyuMax\\data\\updates\\${release.tag}\\${release.installer.name}`;
    },
    async openUpdateLocation(path) {
      window.localStorage.setItem("moyumax.browser.openedLocation", path);
    },
    async getUiBackground() {
      const serialized = window.localStorage.getItem("moyumax.browser.uiBackground");
      return serialized
        ? (JSON.parse(serialized) as UiBackground)
        : { type: "default" };
    },
    async setUiBackground(background) {
      window.localStorage.setItem(
        "moyumax.browser.uiBackground",
        JSON.stringify(background),
      );
    },
    async importBackgroundImage(_sourcePath) {
      const background: UiBackground = { type: "image", file: "background-mock.png" };
      window.localStorage.setItem(
        "moyumax.browser.uiBackground",
        JSON.stringify(background),
      );
      return background;
    },
    async importThemePack(sourcePath) {
      const raw = window.localStorage.getItem("moyumax.browser.themePackJson") ?? "";
      void sourcePath;
      let pack: ThemePack;
      try {
        pack = JSON.parse(raw) as ThemePack;
      } catch {
        throw new Error("主题包不是有效的 JSON");
      }
      if (pack.formatVersion !== 1) throw new Error("不支持的主题包格式版本");
      const entries = Object.entries(pack.colors ?? {});
      if (entries.length === 0) throw new Error("主题包没有颜色定义");
      for (const [token, value] of entries) {
        if (!/^#[0-9a-fA-F]{6}$/.test(value)) {
          throw new Error(`颜色必须是 #rrggbb 形式：${token}`);
        }
      }
      const background: UiBackground = { type: "themePack", pack };
      window.localStorage.setItem(
        "moyumax.browser.uiBackground",
        JSON.stringify(background),
      );
      return pack;
    },
    async importThemePackV2(_sourcePath) {
      const raw = window.localStorage.getItem("moyumax.browser.themePackJson") ?? "";
      let pack: { id?: string; name?: string; author?: string; formatVersion?: number };
      try {
        pack = JSON.parse(raw) as typeof pack;
      } catch {
        throw new Error("主题包不是有效的 JSON");
      }
      if (pack.formatVersion !== 2) throw new Error("不支持的主题包格式版本");
      if (!pack.id) throw new Error("主题包缺少 id");
      const packs = browserThemePacks();
      packs[pack.id] = raw;
      window.localStorage.setItem("moyumax.browser.themePacks", JSON.stringify(packs));
      return { id: pack.id, name: pack.name ?? pack.id, author: pack.author ?? "", builtin: false };
    },
    async listImportedThemePacks() {
      return Object.entries(browserThemePacks()).map(([id, raw]) => {
        const pack = JSON.parse(raw) as { name?: string; author?: string };
        return { id, name: pack.name ?? id, author: pack.author ?? "", builtin: false };
      });
    },
    async readThemePackV2(packId) {
      const source = browserThemePacks()[packId];
      if (!source) throw new Error("主题包不存在或已被删除");
      return source;
    },
    async removeThemePack(packId) {
      const packs = browserThemePacks();
      delete packs[packId];
      window.localStorage.setItem("moyumax.browser.themePacks", JSON.stringify(packs));
    },
    async getUiThemePack() {
      return window.localStorage.getItem("moyumax.browser.uiThemePack") ?? "default";
    },
    async setUiThemePack(packId) {
      window.localStorage.setItem("moyumax.browser.uiThemePack", packId);
    },
    async readBackgroundImage() {      const serialized = window.localStorage.getItem("moyumax.browser.uiBackground");
      const background = serialized ? (JSON.parse(serialized) as UiBackground) : null;
      if (background?.type !== "image") return null;
      // 1x1 深色 PNG，供浏览器模拟图片渲染。
      const pngBase64 =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
      return ["image/png", Array.from(Uint8Array.from(atob(pngBase64), (c) => c.charCodeAt(0)))];
    },
    async pickBackgroundImage() {
      return window.localStorage.getItem("moyumax.browser.pickedBackgroundImage");
    },
    async pickThemePackFile() {
      return window.localStorage.getItem("moyumax.browser.pickedThemePack");
    },
    async pickModpackFile() {
      return window.localStorage.getItem("moyumax.browser.pickedModpackFile");
    },
    async importModpackPreview(_sourcePath) {
      const serialized = window.localStorage.getItem("moyumax.browser.modpackPreview");
      if (!serialized) throw new Error("整合包缺少 modrinth.index.json 或 manifest.json");
      const preview = JSON.parse(serialized) as ModpackPreview;
      return { id: crypto.randomUUID(), preview };
    },
    async previewOnlineModpack(projectId) {
      if (!projectId) throw new Error("Modrinth 项目 ID 格式无效");
      const serialized = window.localStorage.getItem("moyumax.browser.modpackPreview");
      if (!serialized) throw new Error("整合包缺少 modrinth.index.json 或 manifest.json");
      const preview = JSON.parse(serialized) as ModpackPreview;
      const id = crypto.randomUUID();
      browserOnlineModpackPreviews.set(id, preview);
      return { id, preview };
    },
    async pickDirectory() {
      return window.localStorage.getItem("moyumax.browser.pickedDirectory");
    },
    async listModrinthVersions(projectId) {
      if (!projectId) throw new Error("Modrinth 项目 ID 格式无效");
      const serialized = window.localStorage.getItem("moyumax.browser.modVersions");
      if (serialized) return JSON.parse(serialized) as ModrinthVersionSummary[];
      return [
        {
          id: "VER00002",
          versionNumber: "3.0.2+26.2",
          versionType: "release",
          datePublished: "2026-06-18T10:00:00Z",
          gameVersions: ["26.2"],
          loaders: ["fabric"],
          downloads: 342_000,
        },
        {
          id: "VER00001",
          versionNumber: "3.0.1+26.2",
          versionType: "release",
          datePublished: "2026-05-30T10:00:00Z",
          gameVersions: ["26.1", "26.2"],
          loaders: ["fabric"],
          downloads: 128_500,
        },
      ];
    },
    async downloadModrinthFile(versionId, targetDir, fileName) {
      if (!versionId) throw new Error("Modrinth 版本 ID 格式无效");
      const trimmed = fileName.trim();
      if (!trimmed || trimmed.includes("/") || trimmed.includes("\\")) {
        throw new Error("保存文件名无效");
      }
      const downloaded = JSON.parse(
        window.localStorage.getItem("moyumax.browser.downloadedFiles") ?? "[]",
      ) as { path: string; versionId: string }[];
      if (downloaded.some((entry) => entry.path === `${targetDir}/${trimmed}`)) {
        throw new Error(`同名文件 ${trimmed} 已存在，已拒绝下载且未覆盖`);
      }
      const path = `${targetDir}/${trimmed}`;
      downloaded.push({ path, versionId });
      window.localStorage.setItem("moyumax.browser.downloadedFiles", JSON.stringify(downloaded));
      return path;
    },
    async installOnlineResource(instanceId, kind, projectId, _versionId) {
      if (!projectId) throw new Error("Modrinth 项目 ID 格式无效");
      if (kind !== "resourcepack" && kind !== "shader" && kind !== "mod") {
        throw new Error("在线安装仅支持资源包、光影与模组");
      }
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance) throw new Error("目标实例不存在");
      const fileName = kind === "mod" ? `${projectId}.jar` : `${projectId}.zip`;
      const resources = browserInstanceResources();
      const resource: InstanceResource = {
        id: crypto.randomUUID(),
        instanceId,
        kind,
        displayName: projectId,
        fileName,
        relativePath:
          kind === "shader"
            ? `.minecraft/shaderpacks/${fileName}`
            : kind === "mod"
              ? `.minecraft/mods/${fileName}`
              : `.minecraft/resourcepacks/${fileName}`,
        size: 1024,
        sha256: "3".repeat(64),
        enabled: true,
        worldName: null,
        importedAtUnixSeconds: Math.floor(Date.now() / 1000),
      };
      resources.push(resource);
      window.localStorage.setItem(
        BROWSER_INSTANCE_RESOURCES_KEY,
        JSON.stringify(resources),
      );
      return resource;
    },
    async getCurseforgeApiKey() {
      return window.localStorage.getItem(BROWSER_CURSEFORGE_API_KEY);
    },
    async setCurseforgeApiKey(key) {
      const trimmed = key.trim();
      if (!trimmed) {
        window.localStorage.removeItem(BROWSER_CURSEFORGE_API_KEY);
      } else {
        window.localStorage.setItem(BROWSER_CURSEFORGE_API_KEY, trimmed);
      }
    },
    async testCurseforgeApiKey() {
      const key = window.localStorage.getItem(BROWSER_CURSEFORGE_API_KEY);
      if (!key) {
        throw new Error(
          "未配置 CurseForge API Key：请在设置 → 来源 中配置；未配置时 CurseForge 内容经 MCI Mirror 内置镜像提供",
        );
      }
      if (key === "invalid") {
        throw new Error("CurseForge API Key 无效或已过期，请在设置 → 来源 中重新配置");
      }
      return "Minecraft";
    },
    async searchCurseforgeProjects(query) {
      requireBrowserCurseforgeKey();
      const catalog = browserCurseforgeCatalog();
      const keyword = query.query.trim().toLocaleLowerCase();
      const hits = keyword
        ? catalog.filter((hit) =>
            `${hit.title} ${hit.description}`.toLocaleLowerCase().includes(keyword),
          )
        : catalog;
      return {
        hits: hits.slice(query.index, query.index + query.pageSize),
        index: query.index,
        pageSize: query.pageSize,
        totalCount: hits.length,
      };
    },
    async listCurseforgeFiles(projectId) {
      requireBrowserCurseforgeKey();
      if (!projectId) throw new Error("CurseForge 项目 ID 必须是数字");
      return browserCurseforgeFiles();
    },
    async listCurseforgeCategories() {
      requireBrowserCurseforgeKey();
      return browserCurseforgeCategories();
    },
    async downloadCurseforgeFile(projectId, fileId, targetDir, fileName) {
      requireBrowserCurseforgeKey();
      if (!projectId || !fileId) throw new Error("CurseForge 项目或文件 ID 必须是数字");
      const trimmed = fileName.trim();
      if (!trimmed || trimmed.includes("/") || trimmed.includes("\\")) {
        throw new Error("保存文件名无效");
      }
      const downloaded = JSON.parse(
        window.localStorage.getItem("moyumax.browser.downloadedFiles") ?? "[]",
      ) as { path: string; versionId: string }[];
      if (downloaded.some((entry) => entry.path === `${targetDir}/${trimmed}`)) {
        throw new Error(`同名文件 ${trimmed} 已存在，已拒绝下载且未覆盖`);
      }
      const path = `${targetDir}/${trimmed}`;
      downloaded.push({ path, versionId: fileId });
      window.localStorage.setItem("moyumax.browser.downloadedFiles", JSON.stringify(downloaded));
      return path;
    },
    async previewCurseforgeModpack(projectId) {
      requireBrowserCurseforgeKey();
      if (!projectId) throw new Error("CurseForge 项目 ID 必须是数字");
      const serialized = window.localStorage.getItem(BROWSER_CURSEFORGE_MODPACK_PREVIEW_KEY);
      const preview: ModpackPreview = serialized
        ? (JSON.parse(serialized) as ModpackPreview)
        : {
            provider: "curseforge",
            name: "All the Mods 10",
            version: "3.2.1",
            gameVersion: "26.2",
            loaderKind: "neoforge",
            loaderVersion: "21.8.54",
            fileCount: 412,
            totalBytes: 512 * 1024 * 1024,
          };
      const id = crypto.randomUUID();
      browserOnlineModpackPreviews.set(id, preview);
      return { id, preview };
    },
    async installCurseforgeResource(instanceId, kind, projectId, fileId) {
      requireBrowserCurseforgeKey();
      if (!projectId) throw new Error("CurseForge 项目 ID 必须是数字");
      if (kind !== "resourcepack" && kind !== "shader" && kind !== "mod") {
        throw new Error("在线安装仅支持资源包、光影与模组");
      }
      const instance = browserInstances().find((candidate) => candidate.id === instanceId);
      if (!instance) throw new Error("目标实例不存在");
      const file = browserCurseforgeFiles().find((candidate) => candidate.id === fileId);
      const fileName = file?.fileName ?? (kind === "mod" ? `${projectId}.jar` : `${projectId}.zip`);
      const resources = browserInstanceResources();
      const resource: InstanceResource = {
        id: crypto.randomUUID(),
        instanceId,
        kind,
        displayName: projectId,
        fileName,
        relativePath:
          kind === "shader"
            ? `.minecraft/shaderpacks/${fileName}`
            : kind === "mod"
              ? `.minecraft/mods/${fileName}`
              : `.minecraft/resourcepacks/${fileName}`,
        size: file?.size ?? 1024,
        sha256: "3".repeat(64),
        enabled: true,
        worldName: null,
        importedAtUnixSeconds: Math.floor(Date.now() / 1000),
      };
      resources.push(resource);
      window.localStorage.setItem(
        BROWSER_INSTANCE_RESOURCES_KEY,
        JSON.stringify(resources),
      );
      return resource;
    },
    async installModpack(previewId) {
      if (!previewId) throw new Error("整合包预览已失效，请重新选择文件");
      // 在线预览（Modrinth/CurseForge）按登记 id 取回；本地导入回退到种子键。
      const registered = browserOnlineModpackPreviews.get(previewId);
      browserOnlineModpackPreviews.delete(previewId);
      const serialized = window.localStorage.getItem("moyumax.browser.modpackPreview");
      if (!registered && !serialized) throw new Error("整合包预览已失效，请重新选择文件");
      const preview = registered ?? (JSON.parse(serialized ?? "null") as ModpackPreview);
      browserEmitModpackProgress({ stage: "game", current: 1, total: 1, item: "正在安装游戏" });
      browserEmitModpackProgress({
        stage: "files",
        current: preview.fileCount,
        total: preview.fileCount,
        item: "整合包文件已校验",
      });
      const instances = browserInstances();
      const instance = instances[0];
      const modpacks = browserModpacks();
      if (instance) {
        modpacks[instance.id] = {
          provider: preview.provider,
          packName: preview.name,
          packVersion: preview.version,
          gameVersion: preview.gameVersion,
          loaderKind: preview.loaderKind,
          managedFiles: [],
          installedAtUnixSeconds: Math.floor(Date.now() / 1000),
          iconUrl: null,
        };
        window.localStorage.setItem(BROWSER_MODPACKS_KEY, JSON.stringify(modpacks));
      }
      return {
        instanceId: instance?.id ?? "instance-id",
        packName: preview.name,
        packVersion: preview.version,
        installedFiles: preview.fileCount,
      };
    },
    async updateModpack(instanceId, _sourcePath) {
      const serialized = window.localStorage.getItem("moyumax.browser.modpackUpdateReport");
      const modpacks = browserModpacks();
      const existing = modpacks[instanceId];
      if (!existing) throw new Error("该实例不是由整合包安装的");
      if (serialized) {
        const report = JSON.parse(serialized) as ModpackUpdateReport;
        existing.packVersion = report.toVersion;
        window.localStorage.setItem(BROWSER_MODPACKS_KEY, JSON.stringify(modpacks));
        return report;
      }
      const previewRaw = window.localStorage.getItem("moyumax.browser.modpackPreview");
      const preview = previewRaw ? (JSON.parse(previewRaw) as ModpackPreview) : null;
      return {
        packName: existing.packName,
        fromVersion: existing.packVersion,
        toVersion: preview?.version ?? existing.packVersion,
        addedFiles: 0,
        replacedFiles: 0,
        deletedFiles: 0,
        keptUserModified: [],
      };
    },
    async getInstanceModpack(instanceId) {
      return browserModpacks()[instanceId] ?? null;
    },
    async isModpackInstalling(instanceId) {
      const raw = window.localStorage.getItem("moyumax.browser.modpackInstalling") ?? "[]";
      const list = JSON.parse(raw) as string[];
      return list.includes(instanceId);
    },
    async getModpackIconDataUrl(instanceId) {
      return browserModpacks()[instanceId]?.iconUrl ?? null;
    },
    async setModpackIconUrl(instanceId, iconUrl) {
      const packs = browserModpacks();
      const pack = packs[instanceId];
      if (pack) {
        pack.iconUrl = iconUrl;
        window.localStorage.setItem(BROWSER_MODPACKS_KEY, JSON.stringify(packs));
      }
    },
    async pickModpackExportPath(packName, version) {
      return (
        window.localStorage.getItem("moyumax.browser.pickedModpackExportPath") ??
        `/mock/exports/${sanitizeModpackFileName(packName, version)}.mrpack`
      );
    },
    async exportInstanceModpack(instanceId, options, destinationPath) {
      if (!destinationPath.toLowerCase().endsWith(".mrpack")) {
        throw new Error("导出目标必须是 .mrpack 文件");
      }
      const report: ExportModpackReport = {
        instanceId,
        packName: options.name,
        packVersion: options.version,
        outputPath: destinationPath,
        totalBytes: 4096,
        referencedFiles: 2,
        bundledFiles: 3,
      };
      window.localStorage.setItem(
        "moyumax.browser.lastModpackExport",
        JSON.stringify({ options, destinationPath, report }),
      );
      return report;
    },
    onModpackProgress(handler) {
      browserModpackProgressHandlers.add(handler);
      return () => {
        browserModpackProgressHandlers.delete(handler);
      };
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
    async listInstanceServers(instanceId) {
      return browserInstanceServers()[instanceId] ?? [];
    },
    async addInstanceServer(instanceId, name, address) {
      const entry = browserValidateServer(name, address);
      const all = browserInstanceServers();
      const servers = [...(all[instanceId] ?? []), entry];
      all[instanceId] = servers;
      window.localStorage.setItem(BROWSER_INSTANCE_SERVERS_KEY, JSON.stringify(all));
      return servers;
    },
    async removeInstanceServer(instanceId, index) {
      const all = browserInstanceServers();
      const servers = [...(all[instanceId] ?? [])];
      if (index < 0 || index >= servers.length) throw new Error("服务器序号超出列表范围");
      servers.splice(index, 1);
      all[instanceId] = servers;
      window.localStorage.setItem(BROWSER_INSTANCE_SERVERS_KEY, JSON.stringify(all));
      return servers;
    },
    async updateInstanceServer(instanceId, index, name, address) {
      const entry = browserValidateServer(name, address);
      const all = browserInstanceServers();
      const servers = [...(all[instanceId] ?? [])];
      if (index < 0 || index >= servers.length) throw new Error("服务器序号超出列表范围");
      servers[index] = { ...servers[index], ...entry };
      all[instanceId] = servers;
      window.localStorage.setItem(BROWSER_INSTANCE_SERVERS_KEY, JSON.stringify(all));
      return servers;
    },
    async pingMinecraftServer(address) {
      // 与核心一致先校验地址;离线名单里的地址模拟不可达。
      browserParseServerAddress(address);
      const offline = JSON.parse(
        window.localStorage.getItem(BROWSER_OFFLINE_SERVERS_KEY) ?? "[]",
      ) as string[];
      if (offline.includes(address)) {
        return {
          online: false,
          motd: null,
          playersOnline: null,
          playersMax: null,
          versionName: null,
          latencyMs: null,
        };
      }
      return {
        online: true,
        motd: "§aMoyuMax §7测试服务器",
        playersOnline: 3,
        playersMax: 20,
        versionName: "26.2",
        latencyMs: 42,
      };
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
    async storageOverview() {
      const serialized = window.localStorage.getItem("moyumax.browser.storageOverview");
      if (serialized) {
        return JSON.parse(serialized) as {
          instancesBytes: number;
          diskTotalBytes: number | null;
          diskFreeBytes: number | null;
        };
      }
      return {
        instancesBytes: browserInstances().length * 64 * 1024 * 1024,
        diskTotalBytes: 96 * 1024 * 1024 * 1024,
        diskFreeBytes: 58 * 1024 * 1024 * 1024,
      };
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
      // 模拟游戏进程的初始输出,日志副页立即可见内容。
      const logs = browserLaunchLogs();
      logs[sessionId] = {
        stdout: `[MoyuMax] 浏览器模拟的游戏进程已启动,实例 ${instance.name}\n[Init] 正在加载游戏 ${instance.gameVersion}\n`,
        stderr: "",
      };
      window.localStorage.setItem(BROWSER_LAUNCH_LOGS_KEY, JSON.stringify(logs));
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
    async readLaunchLog(sessionId, stdoutOffset, stderrOffset) {
      const session = browserLaunchSessions().find(
        (candidate) => candidate.id === sessionId,
      );
      if (!session) throw new Error("启动会话不存在");
      const entry = browserLaunchLogs()[sessionId] ?? { stdout: "", stderr: "" };
      return {
        sessionId,
        state: session.state,
        stdout: sliceLaunchLogChunk(entry.stdout, stdoutOffset),
        stderr: sliceLaunchLogChunk(entry.stderr, stderrOffset),
      };
    },
    async openLaunchLogLocation(sessionId) {
      if (!browserLaunchSessions().some((candidate) => candidate.id === sessionId)) {
        throw new Error("启动会话不存在");
      }
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
    async cancelTask(taskId, kind) {
      const key = kind === "content" ? BROWSER_CONTENT_TASKS_KEY : BROWSER_TASKS_KEY;
      const tasks = kind === "content" ? browserContentTasks() : browserInstallTasks();
      const task = tasks.find((entry) => entry.id === taskId);
      if (!task || !["queued", "paused", "running"].includes(task.state)) {
        throw new Error("任务不存在或当前状态不能取消");
      }
      task.state = "cancelled";
      task.pausedBy = null;
      window.localStorage.setItem(key, JSON.stringify(tasks));
    },
    async deleteTask(taskId, kind) {
      const key = kind === "content" ? BROWSER_CONTENT_TASKS_KEY : BROWSER_TASKS_KEY;
      const tasks = kind === "content" ? browserContentTasks() : browserInstallTasks();
      const task = tasks.find((entry) => entry.id === taskId);
      if (!task || !["failed", "completed", "cancelled"].includes(task.state)) {
        throw new Error("任务不存在或当前状态不能删除");
      }
      window.localStorage.setItem(
        key,
        JSON.stringify(tasks.filter((entry) => entry.id !== taskId)),
      );
    },
    async getDownloadSpeedLimit() {
      const value = window.localStorage.getItem(BROWSER_SPEED_LIMIT_KEY);
      return value ? Number(value) : 0;
    },
    async setDownloadSpeedLimit(bytesPerSec) {
      window.localStorage.setItem(BROWSER_SPEED_LIMIT_KEY, String(bytesPerSec));
    },
    async getDownloadConcurrency() {
      const value = window.localStorage.getItem(BROWSER_CONCURRENCY_KEY);
      return value ? Number(value) : 24;
    },
    async setDownloadConcurrency(connections) {
      if (!Number.isInteger(connections) || connections < 1 || connections > 32) {
        throw new Error("下载并发连接数必须在 1 到 32 之间");
      }
      window.localStorage.setItem(BROWSER_CONCURRENCY_KEY, String(connections));
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
    async getProxyPreference() {
      const serialized = window.localStorage.getItem(BROWSER_PROXY_PREFERENCE_KEY);
      return serialized
        ? (JSON.parse(serialized) as ProxyPreference)
        : { mode: "system" };
    },
    async setProxyPreference(preference) {
      if (preference.mode === "custom") {
        const url = preference.url.trim();
        const supported =
          url.startsWith("http://") ||
          url.startsWith("https://") ||
          url.startsWith("socks5h://");
        if (!supported) {
          throw new Error("代理地址必须以 http://、https:// 或 socks5h:// 开头");
        }
        let hostname = "";
        try {
          hostname = new URL(url).hostname;
        } catch {
          hostname = "";
        }
        if (!hostname) {
          throw new Error("代理地址无效");
        }
      }
      window.localStorage.setItem(
        BROWSER_PROXY_PREFERENCE_KEY,
        JSON.stringify(preference),
      );
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

function browserThemePacks(): Record<string, string> {
  const serialized = window.localStorage.getItem("moyumax.browser.themePacks");
  return serialized ? (JSON.parse(serialized) as Record<string, string>) : {};
}

interface BrowserInstanceModFile {
  fileName: string;
  sizeBytes: number;
}

/** 浏览器 mock 的实例 mods 目录:优先读显式种子,缺省时从安装记录推导(与真实扫描语义一致)。 */
function browserInstanceModFiles(instanceId: string): BrowserInstanceModFile[] {
  const key = `moyumax.browser.instanceMods.${instanceId}`;
  const serialized = window.localStorage.getItem(key);
  if (serialized) return JSON.parse(serialized) as BrowserInstanceModFile[];
  return browserInstalledContent()
    .filter((entry) => {
      if (entry.instanceId !== instanceId) return false;
      const stored = entry.relativePath.startsWith(".minecraft/")
        ? entry.relativePath.slice(".minecraft/".length)
        : entry.relativePath;
      return stored.startsWith("mods/");
    })
    .map((entry) => {
      const stored = entry.relativePath.startsWith(".minecraft/")
        ? entry.relativePath.slice(".minecraft/".length)
        : entry.relativePath;
      return {
        fileName: entry.enabled
          ? stored.slice("mods/".length)
          : `${stored.slice("mods/".length)}.disabled`,
        sizeBytes: entry.size,
      };
    });
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

function browserLaunchOptions(): Record<string, LaunchOptions> {
  const serialized = window.localStorage.getItem(BROWSER_LAUNCH_OPTIONS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, LaunchOptions>) : {};
}

function browserGlobalLaunchPreference(): GlobalLaunchPreference {
  const serialized = window.localStorage.getItem(BROWSER_GLOBAL_LAUNCH_PREFERENCE_KEY);
  return serialized ? (JSON.parse(serialized) as GlobalLaunchPreference) : { mode: "auto" };
}

function assertValidLaunchOptions(options: LaunchOptions): void {
  if (
    !Number.isInteger(options.minimumMemoryMib) ||
    !Number.isInteger(options.maximumMemoryMib) ||
    options.minimumMemoryMib < 256 ||
    options.maximumMemoryMib < options.minimumMemoryMib ||
    options.maximumMemoryMib > 65536
  ) {
    throw new Error("内存设置必须满足 256 MiB <= 最小值 <= 最大值 <= 65536 MiB");
  }
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

function browserInstanceServers(): Record<string, InstanceServerEntry[]> {
  const serialized = window.localStorage.getItem(BROWSER_INSTANCE_SERVERS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, InstanceServerEntry[]>) : {};
}

/** 与核心一致的最小校验:名称非空,地址为 host[:port],端口 1-65535。 */
function browserValidateServer(name: string, address: string): InstanceServerEntry {
  const trimmedName = name.trim();
  if (!trimmedName) throw new Error("服务器名称不能为空");
  const trimmedAddress = browserParseServerAddress(address);
  return { name: trimmedName, address: trimmedAddress, icon: null, acceptTextures: null };
}

function browserParseServerAddress(address: string): string {
  const trimmed = address.trim();
  if (!trimmed || /[\s]/.test(trimmed)) throw new Error("服务器地址不能为空或包含空白");
  let host = trimmed;
  let portText: string | null = null;
  if (trimmed.startsWith("[")) {
    const end = trimmed.indexOf("]");
    if (end < 0) throw new Error("IPv6 地址缺少右方括号");
    host = trimmed.slice(1, end);
    const tail = trimmed.slice(end + 1);
    if (tail && !tail.startsWith(":")) throw new Error("IPv6 地址后只能跟 :端口");
    portText = tail ? tail.slice(1) : null;
  } else {
    const colons = trimmed.split(":").length - 1;
    if (colons > 1) throw new Error("IPv6 地址需用方括号包裹,如 [::1]:25565");
    if (colons === 1) {
      [host, portText] = trimmed.split(":") as [string, string];
    }
  }
  if (!host) throw new Error("服务器地址缺少主机名");
  if (portText !== null) {
    if (!/^\d+$/.test(portText)) throw new Error("端口必须是 1-65535 的数字");
    const port = Number(portText);
    if (port < 1 || port > 65535) throw new Error("端口必须在 1-65535 之间");
  }
  return trimmed;
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

function browserAccounts(): AccountSummary[] {
  const serialized = window.localStorage.getItem(BROWSER_ACCOUNTS_KEY);
  return serialized ? (JSON.parse(serialized) as AccountSummary[]) : [];
}

function browserModpacks(): Record<string, InstalledModpack> {
  const serialized = window.localStorage.getItem(BROWSER_MODPACKS_KEY);
  return serialized ? (JSON.parse(serialized) as Record<string, InstalledModpack>) : {};
}

/** CurseForge 调用前置检查：未配置 Key 时与 core 同一报错文案。 */
function requireBrowserCurseforgeKey(): void {
  if (!window.localStorage.getItem(BROWSER_CURSEFORGE_API_KEY)) {
    throw new Error(
      "未配置 CurseForge API Key：请在设置 → 来源 中配置；未配置时 CurseForge 内容经 MCI Mirror 内置镜像提供",
    );
  }
}

/** CurseForge mock 目录（独立种子键，默认返回与 Modrinth mock 同形数据）。 */
function browserCurseforgeCatalog(): CatalogProjectSummary[] {
  const serialized = window.localStorage.getItem(BROWSER_CURSEFORGE_CATALOG_KEY);
  if (serialized) return JSON.parse(serialized) as CatalogProjectSummary[];
  return [
    {
      projectId: "360438",
      slug: "sodium",
      title: "Sodium",
      author: "jellysquid3",
      description: "现代化渲染引擎，显著提升帧率。",
      iconUrl: null,
      downloads: 40_100_000,
      dateModified: "2026-06-18T10:00:00Z",
      gameVersions: ["26.1", "26.2"],
      categories: ["Optimization"],
      source: "curseforge",
    },
    {
      projectId: "310111",
      slug: "jei",
      title: "Just Enough Items (JEI)",
      author: "mezz",
      description: "查看物品与配方。",
      iconUrl: null,
      downloads: 33_500_000,
      dateModified: "2026-05-30T10:00:00Z",
      gameVersions: ["26.2"],
      categories: ["Map and Information"],
      source: "curseforge",
    },
  ];
}

/** CurseForge mock 文件列表（独立种子键；第二条故意无 sha1 供大小校验提示测试）。 */
function browserCurseforgeFiles(): CurseforgeFileSummary[] {
  const serialized = window.localStorage.getItem(BROWSER_CURSEFORGE_FILES_KEY);
  if (serialized) return JSON.parse(serialized) as CurseforgeFileSummary[];
  return [
    {
      id: "5500002",
      versionNumber: "0.6.2+26.2",
      versionType: "release",
      datePublished: "2026-06-18T10:00:00Z",
      gameVersions: ["26.2"],
      loaders: ["fabric"],
      downloads: 402_000,
      fileName: "sodium-fabric-0.6.2.jar",
      size: 1_234_567,
      sha1: "a".repeat(40),
      downloadUrl: "https://edge.forgecdn.net/files/5500/2/sodium-fabric-0.6.2.jar",
    },
    {
      id: "5500001",
      versionNumber: "0.6.1+26.1",
      versionType: "beta",
      datePublished: "2026-05-30T10:00:00Z",
      gameVersions: ["26.1", "26.2"],
      loaders: ["fabric"],
      downloads: 128_500,
      fileName: "sodium-fabric-0.6.1.jar",
      size: 1_200_000,
      sha1: null,
      downloadUrl: null,
    },
  ];
}

function browserCurseforgeCategories(): CurseforgeCategory[] {
  const serialized = window.localStorage.getItem(BROWSER_CURSEFORGE_CATEGORIES_KEY);
  if (serialized) return JSON.parse(serialized) as CurseforgeCategory[];
  return [
    { id: 420, name: "Storage", slug: "storage" },
    { id: 424, name: "API and Library", slug: "library" },
    { id: 425, name: "Adventure and RPG", slug: "adventure-rpg" },
  ];
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

/** 浏览器 mock 的会话日志内容存储:会话 id → 两通道文本。 */
function browserLaunchLogs(): Record<string, { stdout: string; stderr: string }> {
  const serialized = window.localStorage.getItem(BROWSER_LAUNCH_LOGS_KEY);
  return serialized
    ? (JSON.parse(serialized) as Record<string, { stdout: string; stderr: string }>)
    : {};
}

const BROWSER_LAUNCH_LOG_LIMIT_BYTES = 2 * 1024 * 1024;

/** 与核心 read_log_chunk 对齐的字节偏移切片:尾部 2 MiB 上限 + UTF-8 字符边界保护。 */
function sliceLaunchLogChunk(content: string, fromOffset: number): LaunchLogChunk {
  const bytes = new TextEncoder().encode(content);
  let start = Math.min(Math.max(0, fromOffset), bytes.length);
  const truncated = bytes.length - start > BROWSER_LAUNCH_LOG_LIMIT_BYTES;
  if (truncated) {
    start = bytes.length - BROWSER_LAUNCH_LOG_LIMIT_BYTES;
  }
  if (start > 0) {
    while (start < bytes.length && ((bytes[start] ?? 0) & 0b1100_0000) === 0b1000_0000) {
      start += 1;
    }
  }
  return {
    content: new TextDecoder().decode(bytes.subarray(start)),
    nextOffset: bytes.length,
    truncated,
  };
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
      {
        ...release,
        id: "25w30a",
        releaseType: "snapshot",
        releaseTime: "2026-07-20T12:00:00+00:00",
        recommended: false,
      },
      {
        ...release,
        id: "b1.7.3",
        releaseType: "oldBeta",
        releaseTime: "2011-07-08T12:00:00+00:00",
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
