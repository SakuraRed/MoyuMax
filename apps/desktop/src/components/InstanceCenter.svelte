<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import { markAvatarFailed, shellAccount, skinAvatarUrl } from "../accounts.svelte";
  import type {
    ContentUpdateInfo,
    GlobalLaunchPreference,
    InstanceResource,
    InstanceResourceKind,
    InstanceScreenshot,
    InstanceServerEntry,
    InstanceWorldInfo,
    InstanceModEntry,
    InstalledModpack,
    ExportModpackReport,
    JavaEnvironment,
    LaunchOptions,
    LaunchSession,
    LaunchSessionState,
    ManagedInstance,
    MinecraftServerStatus,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    WorldBackupSummary,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    /** 当前实例；被回收等情况下为 null，组件自行退回实例列表。 */
    instance: ManagedInstance | null;
    launchSessions: LaunchSession[];
    /** 进入详情页时定位的子页（如首页运行卡片直达 "logs"）。 */
    initialTab?: string | null;
    onExit: () => void;
    onStateChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    instance,
    launchSessions,
    initialTab = null,
    onExit,
    onStateChanged,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  type DetailTab = "overview" | "content" | "worlds" | "screenshots" | "logs" | "settings";

  const TABS: { key: DetailTab; labelKey: string }[] = [
    { key: "overview", labelKey: "instanceDetail.tabs.overview" },
    { key: "content", labelKey: "instanceDetail.tabs.content" },
    { key: "worlds", labelKey: "instanceDetail.tabs.worlds" },
    { key: "screenshots", labelKey: "instanceDetail.tabs.screenshots" },
    { key: "logs", labelKey: "instanceDetail.tabs.logs" },
    { key: "settings", labelKey: "instanceDetail.tabs.settings" },
  ];

  /** 旧版子页键映射到六页签,保持 initialTab 调用方(首页/列表)不用跟随改名。 */
  const LEGACY_TAB: Record<string, DetailTab> = {
    overview: "overview",
    setup: "settings",
    mods: "content",
    saves: "worlds",
    screenshots: "screenshots",
    resourcepacks: "content",
    shaders: "content",
    servers: "worlds",
    logs: "logs",
    export: "settings",
  };

  const LOADER_DISPLAY: Record<string, string> = {
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };

  /** MOTD § 颜色码 → 样式类(0-9a-f 十六色)。 */
  const MOTD_COLOR_CLASSES: Record<string, string> = {
    "0": "motd-c0",
    "1": "motd-c1",
    "2": "motd-c2",
    "3": "motd-c3",
    "4": "motd-c4",
    "5": "motd-c5",
    "6": "motd-c6",
    "7": "motd-c7",
    "8": "motd-c8",
    "9": "motd-c9",
    a: "motd-ca",
    b: "motd-cb",
    c: "motd-cc",
    d: "motd-cd",
    e: "motd-ce",
    f: "motd-cf",
  };
  const SERVER_PING_CONCURRENCY = 4;

  interface MotdSegment {
    text: string;
    colorClass: string | null;
    bold: boolean;
  }

  type ServerPingState = MinecraftServerStatus | "loading";

  type LogLevel = "info" | "warn" | "error";

  interface LogLine {
    raw: string;
    time: string;
    level: LogLevel;
    msg: string;
  }

  let tab = $state<DetailTab>("overview");
  let loading = $state(true);
  let modpack = $state<InstalledModpack | null>(null);
  let heroIcon = $state("");
  let javaEnvironments = $state<JavaEnvironment[]>([]);
  let memoryMin = $state("");
  let memoryMax = $state("");
  let memoryMode = $state<"global" | "custom">("global");
  let globalPreference = $state<GlobalLaunchPreference>({ mode: "auto" });
  let autoOptions = $state<LaunchOptions>({ minimumMemoryMib: 512, maximumMemoryMib: 4096 });
  let autoUpdate = $state(false);
  let mods = $state<InstanceModEntry[]>([]);
  let contentUpdates = $state<ContentUpdateInfo[]>([]);
  let checkingUpdates = $state(false);
  let planningUpdateId = $state("");
  let resources = $state<InstanceResource[]>([]);
  let worlds = $state<InstanceWorldInfo[]>([]);
  let selectedWorld = $state<string | null>(null);
  let backups = $state<WorldBackupSummary[]>([]);
  let rollingBackup = $state("");
  let screenshots = $state<InstanceScreenshot[]>([]);
  let servers = $state<InstanceServerEntry[]>([]);
  let serverStatus = $state<Record<number, ServerPingState>>({});
  let serverFormName = $state("");
  let serverFormAddress = $state("");
  let addingServer = $state(false);
  let refreshingServers = $state(false);
  let editingServer = $state<number | null>(null);
  let editName = $state("");
  let editAddress = $state("");
  let savingServer = $state(false);
  let selectedScreenshot = $state<string | null>(null);
  let pendingDelete = $state<string | null>(null);
  let recycleConfirm = $state(false);
  let recycleDialog = $state<HTMLElement | null>(null);
  let changingInstance = $state(false);
  let updatingPack = $state(false);
  let importing = $state(false);
  let busy = $state(false);
  let savingMemory = $state(false);
  let savingAutoUpdate = $state(false);
  let assigningJava = $state(false);
  let defaultAccountName = $state("");
  // 导出整合包(设置页签内,Modrinth mrpack)。
  let exportName = $state("");
  let exportVersion = $state("1.0.0");
  let exportIncludeConfig = $state(true);
  let exportIncludeResourcePacks = $state(true);
  let exportIncludeShaders = $state(true);
  let exportIncludeServers = $state(false);
  let exportIncludeScreenshots = $state(false);
  let exporting = $state(false);
  let exportReport = $state<ExportModpackReport | null>(null);
  let message = $state("");
  let errorMessage = $state("");

  // 运行时长实时计时:仅在存在运行中会话时页面展示该面板。
  let nowTick = $state(Date.now());

  // 游戏日志页签:双通道偏移尾部跟随,运行中会话每 2 秒轮询一次。
  const LOG_POLL_INTERVAL_MS = 2_000;
  const LOG_LINE_LIMIT = 5_000;
  let logSessionId = $state("");
  let logEntries = $state<LogLine[]>([]);
  let logStdoutOffset = $state(0);
  let logStderrOffset = $state(0);
  let logTruncated = $state(false);
  let logState = $state<LaunchSessionState | null>(null);
  let logAutoScroll = $state(true);
  let logLevel = $state<"all" | LogLevel>("all");
  let logQuery = $state("");
  let logCopied = $state(false);
  let logStopping = $state(false);
  let logViewport = $state<HTMLElement | null>(null);
  let logTimer: ReturnType<typeof setInterval> | undefined;
  let logCopyTimer: ReturnType<typeof setTimeout> | undefined;

  const instanceId = $derived(instance?.id ?? "");
  const instanceSessions = $derived(
    launchSessions.filter((session) => session.instanceId === instanceId),
  );
  const latestSession = $derived(instanceSessions[0] ?? null);
  const selectedLogSession = $derived(
    instanceSessions.find((session) => session.id === logSessionId) ?? null,
  );
  const logSessionRunning = $derived(
    logState !== null && ["starting", "running"].includes(logState),
  );
  const activeSession = $derived(
    launchSessions.find(
      (session) =>
        session.instanceId === instanceId &&
        ["starting", "running"].includes(session.state),
    ),
  );
  const readyEnvironments = $derived(
    javaEnvironments.filter((environment) => environment.status === "ready"),
  );
  const assignedJava = $derived(
    javaEnvironments.find((environment) =>
      environment.referencingInstances.some((entry) => entry.id === instanceId),
    ) ?? null,
  );
  const updatesByProject = $derived(
    new Map(contentUpdates.map((update) => [update.projectId, update])),
  );
  const selectedWorldInfo = $derived(
    worlds.find((world) => world.name === selectedWorld) ?? null,
  );
  const displayedLogs = $derived(
    logEntries.filter((line) => {
      if (logLevel !== "all" && line.level !== logLevel) return false;
      const needle = logQuery.trim().toLowerCase();
      if (needle && !line.raw.toLowerCase().includes(needle)) return false;
      return true;
    }),
  );

  const LOG_LEVELS: { key: "all" | LogLevel; labelKey: string }[] = [
    { key: "all", labelKey: "instanceDetail.logs.levelAll" },
    { key: "info", labelKey: "" },
    { key: "warn", labelKey: "" },
    { key: "error", labelKey: "" },
  ];

  onMount(() => {
    const mapped = initialTab ? LEGACY_TAB[initialTab] : undefined;
    if (mapped) selectTab(mapped);
    void loadDetail();
    void loadDefaultAccount();
    const tickTimer = setInterval(() => {
      nowTick = Date.now();
    }, 1000);
    return () => {
      stopLogPolling();
      clearInterval(tickTimer);
      if (logCopyTimer !== undefined) clearTimeout(logCopyTimer);
    };
  });

  // 实例被回收（或外部删除）后列表快照不再包含它，优雅退回实例列表。
  $effect(() => {
    if (!instance) onExit();
  });

  async function loadDefaultAccount(): Promise<void> {
    try {
      const accounts = await runtime.listAccounts();
      defaultAccountName = accounts.find((account) => account.isDefault)?.username ?? "";
    } catch {
      defaultAccountName = "";
    }
  }

  async function loadDetail(): Promise<void> {
    const current = instance;
    if (!current) return;
    loading = true;
    errorMessage = "";
    try {
      const [pack, environments, options, auto, content, resourceList, worldList, shotList, serverList, preference, autoMemory, backupList] =
        await Promise.all([
          runtime.getInstanceModpack(current.id),
          runtime.listJavaEnvironments(),
          runtime.getInstanceLaunchOptions(current.id),
          runtime.getInstanceContentAutoUpdate(current.id),
          runtime.getInstanceMods(current.id),
          runtime.listInstanceResources(current.id),
          runtime.listInstanceWorldDetails(current.id),
          runtime.listInstanceScreenshots(current.id),
          runtime.listInstanceServers(current.id),
          runtime.getGlobalLaunchPreference(),
          runtime.getAutoLaunchOptions(),
          runtime.listWorldBackups(current.id),
        ]);
      modpack = pack;
      heroIcon = pack ? ((await runtime.getModpackIconDataUrl(current.id).catch(() => null)) ?? "") : "";
      javaEnvironments = environments;
      globalPreference = preference;
      autoOptions = autoMemory;
      if (options) {
        memoryMode = "custom";
        memoryMin = String(options.minimumMemoryMib);
        memoryMax = String(options.maximumMemoryMib);
      } else {
        memoryMode = "global";
        memoryMin = "";
        memoryMax = "";
      }
      autoUpdate = auto;
      mods = content;
      resources = resourceList;
      worlds = worldList;
      if (!worldList.some((world) => world.name === selectedWorld)) {
        selectedWorld = worldList[0]?.name ?? null;
      }
      screenshots = shotList;
      servers = serverList;
      backups = readyBackups(backupList);
      if (!exportName) exportName = current.name;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  /** 时间线只展示已完成的备份,按创建时间倒序,最多 8 条。 */
  function readyBackups(list: WorldBackupSummary[]): WorldBackupSummary[] {
    return list
      .filter((backup) => backup.state === "ready")
      .sort((a, b) => b.createdAtUnixSeconds - a.createdAtUnixSeconds)
      .slice(0, 8);
  }

  function selectTab(next: DetailTab): void {
    tab = next;
    pendingDelete = null;
    selectedScreenshot = null;
    editingServer = null;
    message = "";
    errorMessage = "";
    if (next === "logs") {
      // 默认选中该实例最近一次启动会话(list 已按开始时间倒序)。
      logSessionId = instanceSessions[0]?.id ?? "";
      restartLogStream();
    } else {
      stopLogPolling();
    }
  }

  function stopLogPolling(): void {
    if (logTimer !== undefined) {
      clearInterval(logTimer);
      logTimer = undefined;
    }
  }

  /** 进入日志页签或切换会话时从零偏移重读;仅运行中会话需要周期跟随。 */
  function restartLogStream(): void {
    stopLogPolling();
    logEntries = [];
    logStdoutOffset = 0;
    logStderrOffset = 0;
    logTruncated = false;
    logCopied = false;
    const session = selectedLogSession;
    logState = session?.state ?? null;
    if (!session) return;
    void pullLaunchLog();
    if (["starting", "running"].includes(session.state)) {
      logTimer = setInterval(() => void pullLaunchLog(), LOG_POLL_INTERVAL_MS);
    }
  }

  async function pullLaunchLog(): Promise<void> {
    const sessionId = logSessionId;
    if (!sessionId) return;
    try {
      const read = await runtime.readLaunchLog(
        sessionId,
        logStdoutOffset,
        logStderrOffset,
      );
      // 读取期间用户已切换会话,丢弃过期结果避免串台。
      if (sessionId !== logSessionId) return;
      logStdoutOffset = read.stdout.nextOffset;
      logStderrOffset = read.stderr.nextOffset;
      logTruncated = read.stdout.truncated || read.stderr.truncated;
      const appended = [
        ...splitLogContent(read.stdout.content).map((raw) => parseLogLine(raw, "info")),
        ...splitLogContent(read.stderr.content).map((raw) => parseLogLine(raw, "error")),
      ];
      if (appended.length > 0) {
        logEntries = [...logEntries, ...appended].slice(-LOG_LINE_LIMIT);
        if (logAutoScroll) {
          await tick();
          if (logViewport) logViewport.scrollTop = logViewport.scrollHeight;
        }
      }
      const wasRunning = logSessionRunning;
      logState = read.state;
      if (wasRunning && !logSessionRunning) {
        stopLogPolling();
        // 会话刚结束,同步父级快照让首页与详情状态一致。
        await onStateChanged();
      }
    } catch (error) {
      stopLogPolling();
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  /** 增量内容按行拆分;去掉末尾换行产生的空段。 */
  function splitLogContent(content: string): string[] {
    if (!content) return [];
    const lines = content.split("\n");
    if (lines[lines.length - 1] === "") lines.pop();
    return lines;
  }

  /**
   * 从行内容解析时间与级别:[INFO]/[WARN]/[ERROR]/[FATAL] 标记优先,
   * 无标记时按通道兜底(stdout → INFO,stderr → ERROR)。
   */
  function parseLogLine(raw: string, fallback: LogLevel): LogLine {
    const head = raw.slice(0, 48);
    const timeMatch = head.match(/(\d{2}:\d{2}:\d{2})/);
    const levelMatch = head.match(/\[?(INFO|WARN(?:ING)?|ERROR|FATAL)\]?/i);
    let level = fallback;
    if (levelMatch && levelMatch[1]) {
      const token = levelMatch[1].toUpperCase();
      level = token === "INFO" ? "info" : token.startsWith("WARN") ? "warn" : "error";
    }
    let msg = raw;
    if (levelMatch) msg = msg.replace(levelMatch[0], " ");
    if (timeMatch) msg = msg.replace(timeMatch[0], " ");
    msg = msg.replace(/\[\s*\]/g, " ").replace(/\s+/g, " ").trim();
    return { raw, time: timeMatch?.[1] ?? "", level, msg: msg || raw };
  }

  function logSessionLabel(session: LaunchSession): string {
    return `${formatSessionTime(session.startedAtUnixSeconds)} · ${sessionStateLabel(session.state)}`;
  }

  function formatSessionTime(unixSeconds: number): string {
    return new Intl.DateTimeFormat(uiLanguage(), {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(unixSeconds * 1000));
  }

  async function copyLaunchLog(): Promise<void> {
    try {
      await navigator.clipboard.writeText(displayedLogs.map((line) => line.raw).join("\n"));
    } catch {
      errorMessage = t("instanceDetail.logs.copyFailed");
      return;
    }
    logCopied = true;
    if (logCopyTimer !== undefined) clearTimeout(logCopyTimer);
    logCopyTimer = setTimeout(() => {
      logCopied = false;
    }, 1600);
  }

  async function stopLogSession(): Promise<void> {
    if (!instance) return;
    logStopping = true;
    errorMessage = "";
    try {
      await runtime.stopInstance(instance.id);
      await onStateChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      logStopping = false;
    }
  }

  async function openLogLocation(): Promise<void> {
    if (!logSessionId) return;
    try {
      await runtime.openLaunchLogLocation(logSessionId);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function loaderLabel(entry: ManagedInstance): string {
    const name = LOADER_DISPLAY[entry.loaderKind];
    if (name) {
      return `${name}${entry.loaderVersion ? ` ${entry.loaderVersion}` : ""}`;
    }
    return entry.loaderKind === "vanilla" ? t("home.loader.vanilla") : entry.loaderKind;
  }

  function loaderName(kind: string): string {
    return LOADER_DISPLAY[kind] ?? kind;
  }

  function sessionStateLabel(state: LaunchSession["state"]): string {
    switch (state) {
      case "starting":
        return t("home.state.starting");
      case "running":
        return t("home.state.running");
      case "completed":
        return t("home.state.completed");
      case "failed":
        return t("home.state.failed");
      case "stopped":
        return t("home.state.stopped");
      case "interrupted":
        return t("home.state.interrupted");
    }
  }

  function kindLabel(kind: InstanceResourceKind): string {
    return kind === "resourcepack"
      ? t("resources.kind.resourcepack")
      : kind === "shader"
        ? t("resources.kind.shader")
        : kind === "mod"
          ? t("resources.kind.mod")
          : t("resources.kind.datapack");
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    const units = ["KiB", "MiB", "GiB", "TiB"];
    let value = bytes / 1024;
    let unit = units[0];
    for (let index = 1; index < units.length && value >= 1024; index += 1) {
      value /= 1024;
      unit = units[index];
    }
    return `${value.toFixed(1)} ${unit}`;
  }

  function timestampLabel(unixSeconds: number): string {
    return new Intl.DateTimeFormat(uiLanguage(), {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(unixSeconds * 1000));
  }

  function identityLabel(): string {
    const account = shellAccount();
    if (account.kind === "microsoft") {
      return t("home.action.identityMicrosoft").replace("{name}", account.name);
    }
    if (account.kind === "authlib") {
      return t("home.action.identityAuthlib").replace("{name}", account.name);
    }
    return t("home.action.identityOffline");
  }

  function openAccounts(): void {
    onNavigate("accounts");
  }

  function clearMessages(): void {
    message = "";
    errorMessage = "";
  }

  function lastRunLabel(): string {
    if (activeSession) return t("home.state.running");
    if (latestSession) return formatSessionTime(latestSession.startedAtUnixSeconds);
    return t("instanceDetail.overview.neverRun");
  }

  function sessionDuration(session: LaunchSession): string {
    if (!session.endedAtUnixSeconds) return "";
    const seconds = Math.max(0, session.endedAtUnixSeconds - session.startedAtUnixSeconds);
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (hours > 0) return t("home.session.durationHm").replace("{h}", String(hours)).replace("{m}", String(minutes));
    return t("home.session.durationM").replace("{m}", String(Math.max(1, minutes)));
  }

  function backupStateLabel(backup: WorldBackupSummary | null | undefined): string {
    if (!backup) return t("home.backup.none");
    switch (backup.state) {
      case "ready":
        return t("home.backup.ready");
      case "skipped":
        return t("home.backup.skipped");
      case "failed":
        return t("home.backup.failed");
      case "staging":
        return t("home.backup.staging");
    }
  }

  function runningDuration(): string {
    if (!activeSession) return "00:00:00";
    const seconds = Math.max(0, Math.floor(nowTick / 1000 - activeSession.startedAtUnixSeconds));
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    const rest = seconds % 60;
    const pad = (value: number): string => String(value).padStart(2, "0");
    return `${pad(hours)}:${pad(minutes)}:${pad(rest)}`;
  }

  function worldSub(world: InstanceWorldInfo): string {
    const played = world.lastPlayedUnixSeconds
      ? formatSessionTime(world.lastPlayedUnixSeconds)
      : t("instanceDetail.worlds.neverPlayed");
    return `${played} · ${formatBytes(world.sizeBytes)}`;
  }

  function backupName(backup: WorldBackupSummary): string {
    return t(`instanceDetail.backups.name.${backup.trigger}`);
  }

  async function start(): Promise<void> {
    const current = instance;
    if (!current) return;
    changingInstance = true;
    clearMessages();
    try {
      await runtime.startInstance(current.id);
      message = t("home.action.starting")
        .replace("{name}", current.name)
        .replace("{identity}", identityLabel());
      await onStateChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = false;
    }
  }

  async function stop(): Promise<void> {
    const current = instance;
    if (!current) return;
    changingInstance = true;
    clearMessages();
    try {
      await runtime.stopInstance(current.id);
      message = t("home.action.stopRequested").replace("{name}", current.name);
      await onStateChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = false;
    }
  }

  async function askRecycle(): Promise<void> {
    clearMessages();
    recycleConfirm = true;
    await tick();
    recycleDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function cancelRecycle(): void {
    if (changingInstance) return;
    recycleConfirm = false;
  }

  async function recycleNow(): Promise<void> {
    const current = instance;
    if (!current) return;
    changingInstance = true;
    clearMessages();
    try {
      await runtime.recycleInstance(current.id);
      recycleConfirm = false;
      message = t("home.action.recycled").replace("{name}", current.name);
      await onStateChanged();
      // 状态刷新后 instance 变为 null,$effect 会退回实例列表。
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = false;
    }
  }

  function handleRecycleDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelRecycle();
      return;
    }
    if (event.key !== "Tab" || !recycleDialog) return;
    const controls = [...recycleDialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const first = controls.at(0);
    const last = controls.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  async function updatePack(): Promise<void> {
    const current = instance;
    if (!current) return;
    updatingPack = true;
    clearMessages();
    try {
      const path = await runtime.pickModpackFile();
      if (!path) return;
      const report = await runtime.updateModpack(current.id, path);
      message = t("modpack.updateDone")
        .replace("{name}", report.packName)
        .replace("{from}", report.fromVersion)
        .replace("{to}", report.toVersion);
      if (report.keptUserModified.length > 0) {
        message += t("modpack.keptNote").replace("{files}", report.keptUserModified.join("、"));
      }
      modpack = await runtime.getInstanceModpack(current.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      updatingPack = false;
    }
  }

  async function startExport(): Promise<void> {
    const current = instance;
    if (!current || exporting) return;
    clearMessages();
    exportReport = null;
    if (!exportName.trim() || !exportVersion.trim()) {
      errorMessage = t("instanceDetail.export.invalidInput");
      return;
    }
    const destination = await runtime.pickModpackExportPath(exportName, exportVersion);
    if (!destination) return;
    exporting = true;
    try {
      exportReport = await runtime.exportInstanceModpack(
        current.id,
        {
          name: exportName,
          version: exportVersion,
          includeConfig: exportIncludeConfig,
          includeResourcePacks: exportIncludeResourcePacks,
          includeShaders: exportIncludeShaders,
          includeServers: exportIncludeServers,
          includeScreenshots: exportIncludeScreenshots,
        },
        destination,
      );
      message = t("instanceDetail.export.success");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      exporting = false;
    }
  }

  async function assignJava(environmentId: string): Promise<void> {
    const current = instance;
    if (!current || !environmentId || environmentId === assignedJava?.id) return;
    assigningJava = true;
    clearMessages();
    try {
      await runtime.setInstanceJavaEnvironment(current.id, environmentId);
      javaEnvironments = await runtime.listJavaEnvironments();
      message = t("instanceDetail.setup.javaAssigned");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      assigningJava = false;
    }
  }

  /** 当前全局设置下实例跟随全局时的生效摘要（自动分配或全局自定义值）。 */
  function globalMemorySummary(): string {
    if (globalPreference.mode === "custom") {
      return t("instanceDetail.setup.memoryGlobalCustom")
        .replace("{min}", String(globalPreference.minMib))
        .replace("{max}", String(globalPreference.maxMib));
    }
    return t("instanceDetail.setup.memoryGlobalAuto")
      .replace("{min}", String(autoOptions.minimumMemoryMib))
      .replace("{max}", String(autoOptions.maximumMemoryMib));
  }

  async function selectMemoryMode(mode: "global" | "custom"): Promise<void> {
    const current = instance;
    if (!current || mode === memoryMode) return;
    clearMessages();
    if (mode === "global") {
      savingMemory = true;
      try {
        await runtime.clearInstanceLaunchOptions(current.id);
        memoryMode = "global";
        memoryMin = "";
        memoryMax = "";
        message = t("instanceDetail.setup.memoryFollowSaved");
      } catch (error) {
        errorMessage = error instanceof Error ? error.message : String(error);
      } finally {
        savingMemory = false;
      }
      return;
    }
    // 切到自定义:用当前跟随全局的生效值预填,保存后才写入实例覆盖。
    memoryMode = "custom";
    const source =
      globalPreference.mode === "custom"
        ? { minimumMemoryMib: globalPreference.minMib, maximumMemoryMib: globalPreference.maxMib }
        : autoOptions;
    memoryMin = String(source.minimumMemoryMib);
    memoryMax = String(source.maximumMemoryMib);
  }

  async function saveMemory(): Promise<void> {
    const current = instance;
    if (!current) return;
    const minimum = Number(memoryMin.trim());
    const maximum = Number(memoryMax.trim());
    clearMessages();
    if (
      !Number.isInteger(minimum) ||
      !Number.isInteger(maximum) ||
      minimum < 256 ||
      maximum < minimum ||
      maximum > 65536
    ) {
      errorMessage = t("instanceDetail.setup.memoryInvalid");
      return;
    }
    savingMemory = true;
    try {
      await runtime.setInstanceLaunchOptions(current.id, {
        minimumMemoryMib: minimum,
        maximumMemoryMib: maximum,
      });
      message = t("instanceDetail.setup.memorySaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      savingMemory = false;
    }
  }

  async function toggleAutoUpdate(enabled: boolean): Promise<void> {
    const current = instance;
    if (!current) return;
    savingAutoUpdate = true;
    clearMessages();
    try {
      await runtime.setInstanceContentAutoUpdate(current.id, enabled);
      autoUpdate = enabled;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      savingAutoUpdate = false;
    }
  }

  /** 检查更新是显式动作:只在用户点击后联网,结果以「有更新」标签呈现。 */
  async function checkUpdates(): Promise<void> {
    const current = instance;
    if (!current) return;
    checkingUpdates = true;
    clearMessages();
    try {
      contentUpdates = await runtime.checkContentUpdates(current.id);
      message =
        contentUpdates.length > 0
          ? t("resources.updates.count").replace("{count}", String(contentUpdates.length))
          : t("resources.updates.none");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      checkingUpdates = false;
    }
  }

  async function updateOne(entry: InstanceModEntry): Promise<void> {
    const current = instance;
    const projectId = entry.content?.projectId;
    if (!current || !projectId || planningUpdateId) return;
    planningUpdateId = projectId;
    clearMessages();
    try {
      await runtime.planContentUpdate(current.id, [projectId]);
      contentUpdates = contentUpdates.filter((update) => update.projectId !== projectId);
      message = t("resources.updates.queuedTitle");
      await onStateChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      planningUpdateId = "";
    }
  }

  async function toggleMod(entry: InstanceModEntry, enabled: boolean): Promise<void> {
    clearMessages();
    if (!instance) return;
    try {
      const updated = await runtime.setInstanceModEnabled(instance.id, entry.relativePath, enabled);
      mods = mods.map((candidate) =>
        candidate.relativePath === entry.relativePath ? updated : candidate,
      );
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      mods = await runtime.getInstanceMods(instance.id).catch(() => mods);
    }
  }

  async function toggleResource(resource: InstanceResource, enabled: boolean): Promise<void> {
    clearMessages();
    try {
      const updated = await runtime.setInstanceResourceEnabled(resource.id, enabled);
      resources = resources.map((candidate) =>
        candidate.id === updated.id ? updated : candidate,
      );
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      resources = instance ? await runtime.listInstanceResources(instance.id) : resources;
    }
  }

  async function deleteResource(resource: InstanceResource): Promise<void> {
    const current = instance;
    if (!current) return;
    busy = true;
    clearMessages();
    try {
      await runtime.deleteInstanceResource(resource.id);
      pendingDelete = null;
      message = t("instanceDetail.resources.deleted").replace("{name}", resource.displayName);
      resources = await runtime.listInstanceResources(current.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function importResource(kind: InstanceResourceKind): Promise<void> {
    const current = instance;
    if (!current) return;
    importing = true;
    clearMessages();
    try {
      const path = await runtime.pickResourceFile(kind);
      if (!path) return;
      const imported = await runtime.importInstanceResource(current.id, kind, path);
      message = t("instanceDetail.resources.imported").replace("{name}", imported.displayName);
      resources = await runtime.listInstanceResources(current.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      importing = false;
    }
  }

  async function importWorld(): Promise<void> {
    const current = instance;
    if (!current) return;
    importing = true;
    clearMessages();
    try {
      const source = await runtime.pickWorldZip();
      if (!source) return;
      const imported = await runtime.importInstanceWorld(current.id, source);
      message = t("data.msg.worldImported").replace("{name}", imported.name);
      worlds = await runtime.listInstanceWorldDetails(current.id);
      selectedWorld = imported.name;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      importing = false;
    }
  }

  async function exportWorld(world: InstanceWorldInfo): Promise<void> {
    const current = instance;
    if (!current) return;
    busy = true;
    clearMessages();
    try {
      const destination = await runtime.pickWorldExportPath(world.name);
      if (!destination) return;
      const bytes = await runtime.exportInstanceWorld(current.id, world.name, destination);
      message = t("data.msg.worldExported")
        .replace("{name}", world.name)
        .replace("{size}", formatBytes(bytes));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function deleteWorld(world: InstanceWorldInfo): Promise<void> {
    const current = instance;
    if (!current) return;
    busy = true;
    clearMessages();
    try {
      await runtime.deleteInstanceWorld(current.id, world.name);
      pendingDelete = null;
      message = t("data.msg.worldDeleted").replace("{name}", world.name);
      worlds = await runtime.listInstanceWorldDetails(current.id);
      if (!worlds.some((candidate) => candidate.name === selectedWorld)) {
        selectedWorld = worlds[0]?.name ?? null;
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  /** 回滚由核心先创建当前状态恢复点,本身可撤销;完成后刷新时间线与世界清单。 */
  async function rollback(backup: WorldBackupSummary): Promise<void> {
    const current = instance;
    if (!current || rollingBackup) return;
    rollingBackup = backup.id;
    clearMessages();
    try {
      await runtime.rollbackWorldBackup(backup.id);
      message = t("data.msg.rollbackDone");
      backups = readyBackups(await runtime.listWorldBackups(current.id));
      worlds = await runtime.listInstanceWorldDetails(current.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      rollingBackup = "";
    }
  }

  async function copyScreenshot(fileName: string): Promise<void> {
    const current = instance;
    if (!current) return;
    clearMessages();
    try {
      await runtime.copyScreenshotToClipboard(current.id, fileName);
      message = t("data.msg.copied").replace("{name}", fileName);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function openScreenshot(fileName: string): Promise<void> {
    const current = instance;
    if (!current) return;
    clearMessages();
    try {
      await runtime.openScreenshotLocation(current.id, fileName);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function deleteScreenshot(fileName: string): Promise<void> {
    const current = instance;
    if (!current) return;
    busy = true;
    clearMessages();
    try {
      await runtime.deleteInstanceScreenshot(current.id, fileName);
      selectedScreenshot = null;
      pendingDelete = null;
      message = t("data.msg.screenshotDeleted").replace("{name}", fileName);
      screenshots = await runtime.listInstanceScreenshots(current.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  /** 解析 MOTD 的 § 格式码:颜色码切分段落并着色,§l 粗体,§r 重置,其余格式码去码不渲染。 */
  function motdSegments(motd: string): MotdSegment[] {
    const segments: MotdSegment[] = [];
    let colorClass: string | null = null;
    let bold = false;
    let buffer = "";
    const flush = (): void => {
      if (buffer) {
        segments.push({ text: buffer, colorClass, bold });
        buffer = "";
      }
    };
    for (let index = 0; index < motd.length; index += 1) {
      const character = motd.charAt(index);
      if (character === "§" && index + 1 < motd.length) {
        const code = motd.charAt(index + 1).toLowerCase();
        if (MOTD_COLOR_CLASSES[code] || code === "l" || code === "r" || "kmno".includes(code)) {
          flush();
          if (MOTD_COLOR_CLASSES[code]) {
            // 颜色码会重置格式(与游戏行为一致)。
            colorClass = MOTD_COLOR_CLASSES[code];
            bold = false;
          } else if (code === "l") {
            bold = true;
          } else if (code === "r") {
            colorClass = null;
            bold = false;
          }
          index += 1;
          continue;
        }
      }
      buffer += character;
    }
    flush();
    return segments;
  }

  async function addServer(): Promise<void> {
    const current = instance;
    if (!current) return;
    addingServer = true;
    clearMessages();
    try {
      servers = await runtime.addInstanceServer(current.id, serverFormName, serverFormAddress);
      message = t("instanceDetail.servers.added").replace("{name}", serverFormName.trim());
      serverFormName = "";
      serverFormAddress = "";
      // 不主动联网:状态由用户点「全部刷新」或单项刷新触发。
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      addingServer = false;
    }
  }

  function startEditServer(index: number): void {
    const server = servers[index];
    if (!server) return;
    editingServer = index;
    editName = server.name;
    editAddress = server.address;
    pendingDelete = null;
    clearMessages();
  }

  async function saveEditServer(index: number): Promise<void> {
    const current = instance;
    if (!current) return;
    savingServer = true;
    clearMessages();
    try {
      servers = await runtime.updateInstanceServer(current.id, index, editName, editAddress);
      message = t("instanceDetail.servers.updated").replace("{name}", editName.trim());
      editingServer = null;
      // 地址可能变化,旧状态作废,等用户手动刷新。
      const { [index]: _dropped, ...rest } = serverStatus;
      serverStatus = rest;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      savingServer = false;
    }
  }

  async function deleteServer(index: number): Promise<void> {
    const current = instance;
    if (!current) return;
    const name = servers[index]?.name ?? "";
    busy = true;
    clearMessages();
    try {
      servers = await runtime.removeInstanceServer(current.id, index);
      pendingDelete = null;
      message = t("instanceDetail.servers.deleted").replace("{name}", name);
      // 序号整体上移,重建状态表。
      const shifted: Record<number, ServerPingState> = {};
      for (const [key, value] of Object.entries(serverStatus)) {
        const position = Number(key);
        if (position < index) shifted[position] = value;
        else if (position > index) shifted[position - 1] = value;
      }
      serverStatus = shifted;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }

  async function refreshServerStatus(index: number): Promise<void> {
    const server = servers[index];
    if (!server) return;
    const address = server.address;
    serverStatus = { ...serverStatus, [index]: "loading" };
    let status: MinecraftServerStatus;
    try {
      status = await runtime.pingMinecraftServer(address);
    } catch {
      status = {
        online: false,
        motd: null,
        playersOnline: null,
        playersMax: null,
        versionName: null,
        latencyMs: null,
      };
    }
    // 写入前确认该行未被编辑/删除错位。
    if (servers[index] && servers[index].address === address) {
      serverStatus = { ...serverStatus, [index]: status };
    }
  }

  /** 并发上限 4;失败项显示离线,不阻塞其余。 */
  async function refreshAllServerStatus(): Promise<void> {
    if (refreshingServers) return;
    refreshingServers = true;
    try {
      let cursor = 0;
      const total = servers.length;
      const workers = Array.from(
        { length: Math.min(SERVER_PING_CONCURRENCY, total) },
        async () => {
          while (cursor < total) {
            const index = cursor;
            cursor += 1;
            await refreshServerStatus(index);
          }
        },
      );
      await Promise.all(workers);
    } finally {
      refreshingServers = false;
    }
  }
</script>

<AppShell
  pageTitle={t("instanceDetail.pageTitle")}
  titleSuffix={instance?.name ?? ""}
  dataDirectory={settings.dataDirectory}
  activeNavigation="instances"
  pageKey="instanceDetail"
  onBack={onExit}
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content">
    <div class="tabs" aria-label={t("instanceDetail.nav.aria")}>
      {#each TABS as item}
        <button
          class:on={tab === item.key}
          aria-current={tab === item.key ? "page" : undefined}
          onclick={() => selectTab(item.key)}
        >{t(item.labelKey)}</button>
      {/each}
    </div>

    {#if loading}
      <div class="col" style="gap:16px" aria-live="polite">
        <div class="skel" style="height:120px"></div>
        <div class="skel" style="height:200px"></div>
        <span class="dim">{t("instanceDetail.loading")}</span>
      </div>
    {:else if instance}
      {#if tab === "overview"}
        <div class="inst-grid2">
          <div class="col" style="gap:16px">
            <section class="panel hero-card">
              <div class="cube large" aria-hidden="true">
                {#if heroIcon}<img src={heroIcon} alt="" />{:else}{instance.name.slice(0, 1)}{/if}
              </div>
              <div class="hero-meta" style="flex:1">
                <h1>{instance.name}</h1>
                <div class="ver">
                  Minecraft {instance.gameVersion} · {loaderLabel(instance)} · {t("instanceDetail.hero.modsCount").replace("{count}", String(mods.length))}
                </div>
                <div class="launch-row">
                  {#if activeSession}
                    <button class="btn danger-soft large" disabled={changingInstance} onclick={() => void stop()}>
                      {changingInstance ? t("home.launch.stopping") : t("home.hero.stop")}
                    </button>
                  {:else}
                    <button
                      class="btn primary large"
                      disabled={changingInstance || instance.state !== "ready"}
                      onclick={() => void start()}
                    >{changingInstance ? t("home.launch.starting") : t("home.hero.launch")}</button>
                  {/if}
                  <button class="acct-chip" title={t("home.hero.accountChipTitle")} onclick={openAccounts}>
                    {#if shellAccount().loaded && shellAccount().kind !== null}
                      {@const account = shellAccount()}
                      {@const avatarUrl = account.avatarFailed ? "" : skinAvatarUrl(account.playerUuid, account.kind)}
                      <span class="avatar">
                        {#if avatarUrl}<img src={avatarUrl} alt="" onerror={() => markAvatarFailed()} />{:else}{account.name.slice(0, 1) || "?"}{/if}
                      </span>
                      <div>
                        <div style="font-size:12.5px;font-weight:600">{account.name}</div>
                        <div style="font-size:11px;color:var(--text-3)">{t("home.hero.accountChipHint")}</div>
                      </div>
                    {:else}
                      <span class="avatar">?</span>
                      <div>
                        <div style="font-size:12.5px;font-weight:600">{defaultAccountName || t("home.instance.localAccount")}</div>
                        <div style="font-size:11px;color:var(--text-3)">{t("home.hero.accountChipHint")}</div>
                      </div>
                    {/if}
                  </button>
                </div>
                <div class="dim" style="margin-top:8px">{t("instanceDetail.hero.accountNote")}</div>
              </div>
              {#if activeSession}
                <span class="tag accent"><span class="cdot"></span>{t("home.state.running")}</span>
              {:else if instance.state === "ready"}
                <span class="tag ok"><span class="cdot"></span>{t("home.instance.ready")}</span>
              {:else}
                <span class="tag neutral"><span class="cdot"></span>{instance.state}</span>
              {/if}
            </section>

            <section class="panel pad">
              <div class="panel-title">{t("instanceDetail.overview.infoTitle")}</div>
              <div style="margin-top:6px">
                <div class="kv-row"><span class="muted">{t("instanceDetail.overview.gameVersion")}</span><span>Minecraft {instance.gameVersion}</span></div>
                <div class="kv-row"><span class="muted">{t("instanceDetail.overview.loader")}</span><span>{loaderLabel(instance)}</span></div>
                <div class="kv-row"><span class="muted">{t("instanceDetail.overview.lastRun")}</span><span>{lastRunLabel()}</span></div>
                <div class="kv-row">
                  <span class="muted">{t("instanceDetail.overview.health")}</span>
                  {#if activeSession}
                    <span class="tag accent"><span class="cdot"></span>{t("home.state.running")}</span>
                  {:else if latestSession && ["failed", "interrupted"].includes(latestSession.state)}
                    <span class="tag danger"><span class="cdot"></span>{t("home.session.exitedAbnormal")}</span>
                  {:else}
                    <span class="tag ok"><span class="cdot"></span>{t("instanceDetail.overview.healthGood")}</span>
                  {/if}
                </div>
              </div>
            </section>

            <section class="panel pad">
              <div class="row spread">
                <div class="panel-title">{t("home.session.title")}</div>
                {#if latestSession}
                  {#if latestSession.state === "completed"}
                    <span class="tag ok">{t("home.session.exitedClean")}</span>
                  {:else if latestSession.state === "stopped"}
                    <span class="tag neutral">{t("home.state.stopped")}</span>
                  {:else if ["failed", "interrupted"].includes(latestSession.state)}
                    <span class="tag danger">{t("home.session.exitedAbnormal")}</span>
                  {:else}
                    <span class="tag accent">{t("home.state.running")}</span>
                  {/if}
                {/if}
              </div>
              {#if latestSession}
                <div class="col" style="gap:8px;margin-top:8px">
                  {#if sessionDuration(latestSession)}
                    <div class="row spread"><span class="muted">{t("home.session.playTime")}</span><span>{sessionDuration(latestSession)}</span></div>
                  {/if}
                  <div class="row spread">
                    <span class="muted">{t("home.session.backups")}</span>
                    <span class="muted">{t("home.instance.latestBackups").replace("{pre}", backupStateLabel(latestSession.preLaunchBackup)).replace("{post}", backupStateLabel(latestSession.postExitBackup))}</span>
                  </div>
                </div>
              {:else}
                <div class="dim" style="margin-top:8px">{t("instanceDetail.session.empty")}</div>
              {/if}
            </section>
          </div>

          <div class="col" style="gap:16px">
            {#if activeSession}
              <section class="panel pad">
                <div class="row spread">
                  <div class="panel-title">{t("instanceDetail.running.title")}</div>
                  <span class="tag accent"><span class="cdot"></span>{t("instanceDetail.running.live")}</span>
                </div>
                <div class="col" style="gap:10px;margin-top:12px">
                  <div class="row spread"><span class="muted">{t("instanceDetail.running.duration")}</span><span class="mono">{runningDuration()}</span></div>
                  <div class="dim">{t("instanceDetail.running.backupNote")}</div>
                  <button class="btn danger-soft" style="margin-top:4px" disabled={changingInstance} onclick={() => void stop()}>
                    {changingInstance ? t("home.launch.stopping") : t("home.hero.stop")}
                  </button>
                </div>
              </section>
            {/if}
            <section class="panel pad">
              <div class="panel-title">{t("instanceDetail.quick.title")}</div>
              <div class="col" style="gap:6px;margin-top:8px">
                <button class="btn secondary" onclick={() => selectTab("logs")}>{t("instanceDetail.quick.logs")}</button>
              </div>
            </section>
          </div>
        </div>
      {:else if tab === "content"}
        <div class="banner info" style="margin-bottom:16px">
          <span>{autoUpdate ? t("instanceDetail.content.bannerOn") : t("instanceDetail.content.bannerOff")}</span>
          <div class="b-act">
            {#if autoUpdate}
              <button class="btn small ghost" disabled={savingAutoUpdate} onclick={() => void toggleAutoUpdate(false)}>{t("instanceDetail.content.disable")}</button>
            {:else}
              <button class="btn small secondary" disabled={savingAutoUpdate} onclick={() => void toggleAutoUpdate(true)}>{t("instanceDetail.content.enable")}</button>
            {/if}
          </div>
        </div>

        <section class="panel" style="margin-bottom:16px">
          <div class="panel-head">
            <div class="panel-title">{t("instanceDetail.content.modsTitle")}</div>
            <button class="btn small ghost" disabled={checkingUpdates || mods.length === 0} onclick={() => void checkUpdates()}>
              {checkingUpdates ? t("instanceDetail.content.checking") : t("instanceDetail.content.checkUpdates")}
            </button>
          </div>
          {#if mods.length === 0}
            <div class="empty-line">
              <span class="dim">{t("instanceDetail.mods.empty")}</span>
              <button class="btn small secondary" onclick={() => onNavigate("resources")}>{t("instanceDetail.mods.emptyAction")}</button>
            </div>
          {:else}
            {#each mods as entry}
              {@const title = entry.content?.projectTitle ?? entry.fileName}
              {@const update = entry.content ? updatesByProject.get(entry.content.projectId) : undefined}
              <div class="list-row">
                <div class="lr-main">
                  <div class="lr-name">{title} {#if entry.content}<span class="dim" style="font-weight:400">{entry.content.versionNumber}</span>{/if}</div>
                  <div class="lr-sub">
                    {#if entry.content}
                      {t("instanceDetail.content.sourceLine").replace("{provider}", "Modrinth")} · {entry.fileName} · {formatBytes(entry.sizeBytes)}
                    {:else}
                      {t("instanceDetail.content.localLine")} · {entry.fileName} · {formatBytes(entry.sizeBytes)}
                    {/if}
                  </div>
                </div>
                {#if update}
                  <span class="tag warn">{t("instanceDetail.content.updateAvailable").replace("{version}", update.latestVersionNumber)}</span>
                {/if}
                <div class="mod-acts">
                  {#if update}
                    <button class="btn small secondary" disabled={Boolean(planningUpdateId)} onclick={() => void updateOne(entry)}>{t("resources.updates.updateOne")}</button>
                  {/if}
                </div>
                <button
                  type="button"
                  class="switch"
                  class:on={entry.enabled}
                  role="switch"
                  aria-checked={entry.enabled}
                  aria-label={t("resources.files.toggleAria").replace("{name}", title)}
                  onclick={() => void toggleMod(entry, !entry.enabled)}
                ></button>
              </div>
            {/each}
          {/if}
        </section>

        {@render resourcePanel("resourcepack", "instanceDetail.resourcepacks.title", "resources.files.importResourcepack")}
        {@render resourcePanel("shader", "instanceDetail.shaders.title", "resources.files.importShader")}
      {:else if tab === "worlds"}
        <div class="world-grid">
          <section class="panel">
            {#if worlds.length === 0}
              <div class="dim" style="padding:14px">{t("data.worlds.empty")}</div>
            {:else}
              {#each worlds as world}
                <button
                  type="button"
                  class="list-row world-row world-hit"
                  class:sel={selectedWorld === world.name}
                  onclick={() => {
                    selectedWorld = world.name;
                    pendingDelete = null;
                  }}
                >
                  <div class="lr-main">
                    <div class="lr-name">{world.name}</div>
                    <div class="lr-sub">{worldSub(world)}</div>
                  </div>
                </button>
              {/each}
            {/if}
            <div class="world-actions">
              <button class="btn small secondary" disabled={importing} onclick={() => void importWorld()}>{importing ? t("data.worlds.busy") : t("data.worlds.import")}</button>
              <button class="btn small secondary" disabled={busy || !selectedWorldInfo} onclick={() => selectedWorldInfo && void exportWorld(selectedWorldInfo)}>{t("data.worlds.export")}</button>
              {#if selectedWorldInfo}
                {#if pendingDelete === "world"}
                  <button class="btn small danger-soft" disabled={busy} onclick={() => selectedWorldInfo && void deleteWorld(selectedWorldInfo)}>{t("common.confirmDelete")}</button>
                  <button class="btn small ghost" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                {:else}
                  <button class="btn small ghost" disabled={busy} onclick={() => { pendingDelete = "world"; }}>{t("common.delete")}</button>
                {/if}
              {/if}
            </div>
          </section>

          <section class="panel pad">
            <div class="row spread">
              <div>
                <div class="panel-title">{t("instanceDetail.backups.title")}</div>
                <div class="panel-desc">{t("instanceDetail.backups.desc")}</div>
              </div>
              {#if backups.length > 0}
                <span class="tag ok"><span class="cdot"></span>{t("instanceDetail.backups.tagOk")}</span>
              {/if}
            </div>
            <div style="margin-top:10px">
              {#if backups.length === 0}
                <div class="dim">{t("instanceDetail.backups.empty")}</div>
              {:else}
                {#each backups as backup}
                  <div class="tl-item">
                    <span class="tl-dot"></span>
                    <div class="lr-main">
                      <div class="lr-name">{backupName(backup)}</div>
                      <div class="lr-sub">{timestampLabel(backup.createdAtUnixSeconds)} · {formatBytes(backup.archiveBytes)}</div>
                    </div>
                    <button class="btn small ghost" disabled={Boolean(rollingBackup)} onclick={() => void rollback(backup)}>
                      {rollingBackup === backup.id ? t("instanceDetail.backups.rolling") : t("data.backups.rollback")}
                    </button>
                  </div>
                {/each}
              {/if}
            </div>
            {#if backups.length > 0}
              <div class="dim" style="margin-top:12px">{t("instanceDetail.backups.rollbackNote")}</div>
            {/if}
          </section>
        </div>

        <section class="panel pad" style="margin-top:16px">
          <div class="row spread">
            <div>
              <div class="panel-title">{t("instanceDetail.servers.title")}</div>
              <div class="panel-desc">{t("instanceDetail.servers.description")}</div>
            </div>
            <button
              class="btn small ghost"
              disabled={refreshingServers || servers.length === 0}
              onclick={() => void refreshAllServerStatus()}
            >{refreshingServers ? t("instanceDetail.servers.refreshing") : t("instanceDetail.servers.refreshAll")}</button>
          </div>
          <form
            class="server-add"
            onsubmit={(event) => {
              event.preventDefault();
              void addServer();
            }}
          >
            <input
              class="input"
              bind:value={serverFormName}
              type="text"
              maxlength="64"
              placeholder={t("instanceDetail.servers.name")}
              aria-label={t("instanceDetail.servers.nameAria")}
            />
            <input
              class="input server-address"
              bind:value={serverFormAddress}
              type="text"
              placeholder={t("instanceDetail.servers.addressHint")}
              aria-label={t("instanceDetail.servers.addressAria")}
            />
            <button class="btn small primary" type="submit" disabled={addingServer}>
              {addingServer ? t("instanceDetail.servers.adding") : t("instanceDetail.servers.addAction")}
            </button>
          </form>
          {#if servers.length === 0}
            <div class="dim" style="padding:6px 0 2px">{t("instanceDetail.servers.empty")}</div>
          {:else}
            <div style="margin:0 -20px -18px">
              {#each servers as server, index}
                {@const status = serverStatus[index]}
                <div class="list-row server-row">
                  {#if editingServer === index}
                    <div class="server-edit">
                      <input
                        class="input"
                        bind:value={editName}
                        type="text"
                        maxlength="64"
                        aria-label={t("instanceDetail.servers.nameAria")}
                      />
                      <input
                        class="input server-address"
                        bind:value={editAddress}
                        type="text"
                        aria-label={t("instanceDetail.servers.addressAria")}
                      />
                    </div>
                    <div class="mod-acts">
                      <button class="btn small primary" disabled={savingServer} onclick={() => void saveEditServer(index)}>{t("instanceDetail.servers.saveEdit")}</button>
                      <button class="btn small ghost" disabled={savingServer} onclick={() => { editingServer = null; }}>{t("instanceDetail.servers.cancelEdit")}</button>
                    </div>
                  {:else}
                    {#if server.icon}
                      <img class="server-icon" src={server.icon} alt="" />
                    {:else}
                      <span class="server-icon server-icon-fallback"><Icon name="wifi" size={16} /></span>
                    {/if}
                    <div class="lr-main">
                      <div class="lr-name">{server.name} <span class="dim" style="font-weight:400">{server.address}</span></div>
                      <div class="lr-sub">
                        {#if status === "loading"}
                          {t("instanceDetail.servers.pinging")}
                        {:else if status && status.online}
                          <span class="server-motd">
                            {#if status.motd}
                              {#each motdSegments(status.motd) as segment}
                                <span class={segment.colorClass ?? ""} class:motd-bold={segment.bold}>{segment.text}</span>
                              {/each}
                            {:else}
                              {t("instanceDetail.servers.noMotd")}
                            {/if}
                          </span>
                          {" · "}
                          {t("instanceDetail.servers.players").replace("{online}", String(status.playersOnline ?? 0)).replace("{max}", String(status.playersMax ?? 0))}
                          {" · "}
                          {t("instanceDetail.servers.latency").replace("{ms}", String(status.latencyMs ?? 0))}
                          {status.versionName ? ` · ${status.versionName}` : ""}
                        {:else if status}
                          {t("instanceDetail.servers.offline")}
                        {:else}
                          {t("instanceDetail.servers.unpinged")}
                        {/if}
                      </div>
                    </div>
                    <div class="mod-acts">
                      <button
                        class="btn small ghost"
                        disabled={status === "loading"}
                        aria-label={t("instanceDetail.servers.refreshAria").replace("{name}", server.name)}
                        onclick={() => void refreshServerStatus(index)}
                      >{t("instanceDetail.servers.refresh")}</button>
                      <button
                        class="btn small ghost"
                        aria-label={t("instanceDetail.servers.editAria").replace("{name}", server.name)}
                        onclick={() => startEditServer(index)}
                      >{t("instanceDetail.servers.edit")}</button>
                      {#if pendingDelete === `server-${index}`}
                        <button class="btn small danger-soft" disabled={busy} onclick={() => void deleteServer(index)}>{t("common.confirmDelete")}</button>
                        <button class="btn small ghost" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                      {:else}
                        <button
                          class="btn small ghost"
                          disabled={busy}
                          aria-label={t("instanceDetail.servers.deleteAria").replace("{name}", server.name)}
                          onclick={() => { pendingDelete = `server-${index}`; }}
                        >{t("common.delete")}</button>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {:else if tab === "screenshots"}
        {#if screenshots.length === 0}
          <section class="panel pad"><div class="dim">{t("data.screenshots.empty")}</div></section>
        {:else}
          <div class="shot-grid">
            {#each screenshots as shot}
              <button
                type="button"
                class="shot-card"
                class:sel={selectedScreenshot === shot.fileName}
                aria-pressed={selectedScreenshot === shot.fileName}
                aria-label={t("data.screenshots.cardAria").replace("{name}", shot.fileName)}
                onclick={() => {
                  selectedScreenshot = selectedScreenshot === shot.fileName ? null : shot.fileName;
                  pendingDelete = null;
                }}
              >
                <Icon name="disk" size={20} />
                <span class="shot-name">{shot.fileName}</span>
                <small>{formatBytes(shot.sizeBytes)} · {timestampLabel(shot.takenAtUnixSeconds)}</small>
              </button>
            {/each}
          </div>
          {#if selectedScreenshot}
            <div class="banner info" style="margin-top:14px">
              <span>{t("data.screenshots.selected").replace("{name}", selectedScreenshot)}</span>
              <div class="b-act">
                <button class="btn small ghost" onclick={() => void copyScreenshot(selectedScreenshot!)}>{t("data.screenshots.copy")}</button>
                <button class="btn small ghost" onclick={() => void openScreenshot(selectedScreenshot!)}>{t("data.screenshots.openLocation")}</button>
                {#if pendingDelete === "screenshot"}
                  <button class="btn small danger-soft" disabled={busy} onclick={() => void deleteScreenshot(selectedScreenshot!)}>{t("common.confirmDelete")}</button>
                  <button class="btn small ghost" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                {:else}
                  <button class="btn small danger-soft" disabled={busy} onclick={() => { pendingDelete = "screenshot"; }}>{t("common.delete")}</button>
                {/if}
              </div>
            </div>
          {/if}
        {/if}
      {:else if tab === "logs"}
        {#if instanceSessions.length === 0}
          <section class="panel pad"><div class="dim">{t("instanceDetail.logs.empty")}</div></section>
        {:else}
          <div class="log-toolbar">
            <select
              class="input"
              aria-label={t("instanceDetail.logs.sessionAria")}
              bind:value={logSessionId}
              onchange={() => restartLogStream()}
            >
              {#each instanceSessions as session}
                <option value={session.id}>{logSessionLabel(session)}</option>
              {/each}
            </select>
            <button
              class="btn small secondary"
              aria-pressed={!logAutoScroll}
              onclick={() => { logAutoScroll = !logAutoScroll; }}
            >{logAutoScroll ? t("instanceDetail.logs.pauseScroll") : t("instanceDetail.logs.resumeScroll")}</button>
            <div class="seg" role="group" aria-label={t("instanceDetail.logs.levelAria")}>
              {#each LOG_LEVELS as level}
                <button
                  class:on={logLevel === level.key}
                  onclick={() => { logLevel = level.key; }}
                >{level.key === "all" ? t(level.labelKey) : level.key.toUpperCase()}</button>
              {/each}
            </div>
            <input
              class="input log-search"
              placeholder={t("instanceDetail.logs.searchPlaceholder")}
              aria-label={t("instanceDetail.logs.searchAria")}
              bind:value={logQuery}
            />
            <span style="flex:1"></span>
            <button
              class="btn small ghost"
              disabled={displayedLogs.length === 0}
              onclick={() => void copyLaunchLog()}
            >{logCopied ? t("instanceDetail.logs.copied") : t("instanceDetail.logs.copy")}</button>
            <button
              class="btn small ghost"
              onclick={() => void openLogLocation()}
            >{t("instanceDetail.logs.openLocation")}</button>
            <button
              class="btn small ghost"
              disabled={logEntries.length === 0}
              onclick={() => { logEntries = []; }}
            >{t("instanceDetail.logs.clear")}</button>
            {#if logSessionRunning}
              <button
                class="btn small danger-soft"
                disabled={logStopping}
                onclick={() => void stopLogSession()}
              >{logStopping ? t("instanceDetail.logs.stopping") : t("instanceDetail.logs.stop")}</button>
            {/if}
          </div>
          {#if logTruncated}
            <p class="dim" style="margin:0 0 8px">{t("instanceDetail.logs.truncated")}</p>
          {/if}
          <div
            class="console"
            bind:this={logViewport}
            aria-label={t("instanceDetail.logs.viewportAria")}
          >
            {#if displayedLogs.length === 0}
              <div class="dim">{t("instanceDetail.logs.noOutput")}</div>
            {:else}
              {#each displayedLogs as line}
                <div><span class="t">{line.time}</span><span class="lv-{line.level}">[{line.level.toUpperCase()}] </span><span class="msg">{line.msg}</span></div>
              {/each}
            {/if}
          </div>
        {/if}
      {:else if tab === "settings"}
        <section class="panel pad" style="margin-bottom:16px">
          <div class="set-row">
            <div class="sr-main">
              <div class="sr-name">{t("instanceDetail.setup.javaCard")}</div>
              <div class="sr-desc">
                {#if assignedJava}
                  {assignedJava.fullVersion} · {assignedJava.distribution === "azulZulu" ? "Azul Zulu" : assignedJava.distribution} · {t("instanceDetail.settings.javaNote")}
                {:else if readyEnvironments.length === 0}
                  {t("instanceDetail.setup.javaEmpty")}
                {:else}
                  {t("instanceDetail.setup.javaUnset")}
                {/if}
              </div>
            </div>
            {#if readyEnvironments.length > 0}
              <select
                class="input"
                style="width:260px"
                aria-label={t("instanceDetail.setup.javaAria")}
                disabled={assigningJava}
                value={assignedJava?.id ?? ""}
                onchange={(event) => void assignJava((event.currentTarget as HTMLSelectElement).value)}
              >
                {#if !assignedJava}
                  <option value="" disabled>{t("instanceDetail.setup.javaUnset")}</option>
                {/if}
                {#each readyEnvironments as environment}
                  <option value={environment.id}>{environment.fullVersion} · {environment.distribution === "azulZulu" ? "Azul Zulu" : environment.distribution}</option>
                {/each}
              </select>
            {:else}
              <button class="btn small secondary" onclick={() => onNavigate("settings")}>{t("instanceDetail.setup.javaGoSettings")}</button>
            {/if}
          </div>
          <div class="set-row">
            <div class="sr-main">
              <div class="sr-name">{t("instanceDetail.settings.memoryTitle")}</div>
              <div class="sr-desc">{memoryMode === "global" ? globalMemorySummary() : t("instanceDetail.settings.memoryDesc")}</div>
            </div>
            {#if memoryMode === "global"}
              <button class="btn small secondary" disabled={savingMemory} onclick={() => void selectMemoryMode("custom")}>{t("instanceDetail.setup.memoryCustom")}</button>
            {:else}
              <span class="mono">{memoryMax || "0"} MB</span>
              <input
                type="range"
                class="mem-slider"
                min="256"
                max="16384"
                step="256"
                bind:value={memoryMax}
                aria-label={t("instanceDetail.settings.memoryRangeAria")}
              />
            {/if}
          </div>
          {#if memoryMode === "custom"}
            <div class="set-row">
              <div class="sr-main">
                <div class="sr-desc">{t("instanceDetail.setup.memoryCustomHint")}</div>
              </div>
              <label class="mem-field">
                <span class="dim">{t("instanceDetail.setup.memoryMin")}</span>
                <input class="input mem-num" bind:value={memoryMin} type="text" inputmode="numeric" aria-label={t("instanceDetail.setup.memoryMinAria")} />
              </label>
              <label class="mem-field">
                <span class="dim">{t("instanceDetail.setup.memoryMax")}</span>
                <input class="input mem-num" bind:value={memoryMax} type="text" inputmode="numeric" aria-label={t("instanceDetail.setup.memoryMaxAria")} />
              </label>
              <button class="btn small primary" disabled={savingMemory} onclick={() => void saveMemory()}>{savingMemory ? t("instanceDetail.setup.memorySaving") : t("instanceDetail.setup.memorySave")}</button>
              <button class="btn small ghost" disabled={savingMemory} onclick={() => void selectMemoryMode("global")}>{t("instanceDetail.setup.memoryFollowGlobal")}</button>
            </div>
          {/if}
        </section>

        <details class="adv" style="margin-bottom:16px">
          <summary>{t("instanceDetail.settings.advTitle")}</summary>
          <div class="adv-body col" style="gap:14px">
            <div class="field">
              <label for="instance-detail-directory">{t("instanceDetail.overview.directory")}</label>
              <input id="instance-detail-directory" class="input mono" value={instance.rootDirectory} readonly aria-label={t("instanceDetail.overview.directory")} />
            </div>
          </div>
        </details>

        {#if modpack}
          <section class="panel pad" style="margin-bottom:16px">
            <div class="row spread">
              <div>
                <div class="panel-title">{t("instanceDetail.overview.modpackCard")}</div>
                <div class="panel-desc">
                  {modpack.packName} {modpack.packVersion} · {modpack.provider === "modrinth" ? "Modrinth" : "CurseForge"} · {modpack.gameVersion} · {loaderName(modpack.loaderKind)}
                </div>
              </div>
              <button class="btn small secondary" disabled={updatingPack} onclick={() => void updatePack()}>{updatingPack ? t("modpack.updating") : t("modpack.update")}</button>
            </div>
          </section>
        {/if}

        <section class="panel pad" style="margin-bottom:16px">
          <div class="panel-title">{t("instanceDetail.export.title")}</div>
          <div class="panel-desc">{t("instanceDetail.export.description")}</div>
          <div class="export-grid" style="margin-top:12px">
            <div class="field">
              <label for="instance-export-name">{t("instanceDetail.export.nameLabel")}</label>
              <input id="instance-export-name" class="input" bind:value={exportName} type="text" aria-label={t("instanceDetail.export.nameAria")} />
            </div>
            <div class="field">
              <label for="instance-export-version">{t("instanceDetail.export.versionLabel")}</label>
              <input id="instance-export-version" class="input" bind:value={exportVersion} type="text" aria-label={t("instanceDetail.export.versionAria")} />
            </div>
          </div>
          <div class="export-options" role="group" aria-label={t("instanceDetail.export.optionsAria")}>
            <label class="exp-opt"><input type="checkbox" bind:checked={exportIncludeConfig} aria-label={t("instanceDetail.export.optionConfigAria")} /><span>{t("instanceDetail.export.optionConfig")}</span></label>
            <label class="exp-opt"><input type="checkbox" bind:checked={exportIncludeResourcePacks} aria-label={t("instanceDetail.export.optionResourcePacksAria")} /><span>{t("instanceDetail.export.optionResourcePacks")}</span></label>
            <label class="exp-opt"><input type="checkbox" bind:checked={exportIncludeShaders} aria-label={t("instanceDetail.export.optionShadersAria")} /><span>{t("instanceDetail.export.optionShaders")}</span></label>
            <label class="exp-opt"><input type="checkbox" bind:checked={exportIncludeServers} aria-label={t("instanceDetail.export.optionServersAria")} /><span>{t("instanceDetail.export.optionServers")}</span></label>
            <label class="exp-opt"><input type="checkbox" bind:checked={exportIncludeScreenshots} aria-label={t("instanceDetail.export.optionScreenshotsAria")} /><span>{t("instanceDetail.export.optionScreenshots")}</span></label>
          </div>
          <div class="dim" style="margin-top:8px">{t("instanceDetail.export.hint")}</div>
          <div style="margin-top:12px">
            <button class="btn primary" disabled={exporting} onclick={() => void startExport()}>
              {exporting ? t("instanceDetail.export.running") : t("instanceDetail.export.start")}
            </button>
          </div>
          {#if exportReport}
            <div style="margin-top:12px" aria-label={t("instanceDetail.export.reportAria")}>
              <div class="kv-row"><span class="muted">{t("instanceDetail.export.reportPath")}</span><span class="mono">{exportReport.outputPath}</span></div>
              <div class="kv-row"><span class="muted">{t("instanceDetail.export.reportSize")}</span><span>{formatBytes(exportReport.totalBytes)}</span></div>
              <div class="kv-row"><span class="muted">{t("instanceDetail.export.reportReferenced")}</span><span>{exportReport.referencedFiles}</span></div>
              <div class="kv-row"><span class="muted">{t("instanceDetail.export.reportBundled")}</span><span>{exportReport.bundledFiles}</span></div>
            </div>
          {/if}
        </section>

        <section class="panel pad">
          <div class="set-row">
            <div class="sr-main">
              <div class="sr-name">{t("instanceDetail.settings.recycleTitle")}</div>
              <div class="sr-desc">{t("instanceDetail.settings.recycleDesc")}</div>
            </div>
            <button class="btn danger-soft" disabled={changingInstance || Boolean(activeSession)} onclick={() => void askRecycle()}>{t("home.launch.recycle")}</button>
          </div>
        </section>
      {/if}
    {/if}
  </main>

  {#if errorMessage}
    <div class="toast" role="alert" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{errorMessage}</span></div>
  {:else if message}
    <div class="toast" role="status" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{message}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{message || errorMessage}</div>

  {#if recycleConfirm && instance}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="instance-recycle-title"
        tabindex="-1"
        bind:this={recycleDialog}
        onkeydown={handleRecycleDialogKeydown}
      >
        <h3 id="instance-recycle-title">{t("home.recycle.title").replace("{name}", instance.name)}</h3>
        <div class="m-body">
          <p>{t("home.recycle.description")}</p>
          <p style="margin-top:8px"><strong>{t("home.recycle.impactTitle")}</strong> {t("home.recycle.impactBody")}</p>
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={changingInstance} onclick={cancelRecycle}>{t("common.cancel")}</button>
          <button class="btn danger" disabled={changingInstance} onclick={() => void recycleNow()}>
            {changingInstance ? t("home.recycle.moving") : t("home.launch.recycle")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

{#snippet resourcePanel(kind: "resourcepack" | "shader", titleKey: string, importKey: string)}
  {@const kindResources = resources.filter((resource) => resource.kind === kind)}
  <section class="panel" style="margin-bottom:16px">
    <div class="panel-head">
      <div class="panel-title">{t(titleKey)}</div>
      <button class="btn small secondary" disabled={importing} onclick={() => void importResource(kind)}>{importing ? t("data.worlds.busy") : t(importKey)}</button>
    </div>
    {#if kindResources.length === 0}
      <div class="empty-line">
        <span class="dim">{t("instanceDetail.resources.empty")}</span>
        <button class="btn small ghost" onclick={() => onNavigate("resources")}>{t("instanceDetail.resources.emptyAction")}</button>
      </div>
    {:else}
      {#each kindResources as resource}
        <div class="list-row">
          <div class="lr-main">
            <div class="lr-name">{resource.displayName}</div>
            <div class="lr-sub">{kindLabel(resource.kind)}{resource.worldName ? t("resources.files.worldSuffix").replace("{world}", resource.worldName) : ""} · {resource.fileName} · {formatBytes(resource.size)}</div>
          </div>
          <div class="mod-acts">
            {#if pendingDelete === resource.id}
              <button class="btn small danger-soft" disabled={busy} onclick={() => void deleteResource(resource)}>{t("common.confirmDelete")}</button>
              <button class="btn small ghost" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
            {:else}
              <button class="btn small ghost" disabled={busy} aria-label={t("resources.files.deleteAria").replace("{name}", resource.displayName)} onclick={() => { pendingDelete = resource.id; }}>{t("common.delete")}</button>
            {/if}
          </div>
          <button
            type="button"
            class="switch"
            class:on={resource.enabled}
            role="switch"
            aria-checked={resource.enabled}
            aria-label={t("resources.files.toggleAria").replace("{name}", resource.displayName)}
            onclick={() => void toggleResource(resource, !resource.enabled)}
          ></button>
        </div>
      {/each}
    {/if}
  </section>
{/snippet}

<style>
  .inst-grid2 {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 16px;
  }
  @media (max-width: 1100px) {
    .inst-grid2 {
      grid-template-columns: 1fr;
    }
  }
  .hero-card {
    padding: 24px 26px;
    display: flex;
    gap: 20px;
    align-items: center;
  }
  .hero-meta h1 {
    font-size: 21px;
  }
  .hero-meta .ver {
    color: var(--text-2);
    font-size: 13px;
    margin-top: 2px;
  }
  .launch-row {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 16px;
    flex-wrap: wrap;
  }
  .acct-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    padding: 6px 12px 6px 6px;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.18);
    color: var(--text-1);
    font-family: var(--font);
    text-align: left;
  }
  .acct-chip:hover {
    background: var(--glass-strong);
  }
  .acct-chip .avatar {
    width: 26px;
    height: 26px;
    border-radius: 4px;
    background: linear-gradient(135deg, #3fd8c2, #2e82b4);
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    color: var(--accent-ink);
    overflow: hidden;
    flex: none;
  }
  .acct-chip .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .kv-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 9px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .kv-row:first-child {
    border-top: none;
  }
  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 14px 14px 6px;
  }
  .empty-line {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 14px 14px;
  }
  .mod-acts {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .world-grid {
    display: grid;
    grid-template-columns: 340px 1fr;
    gap: 16px;
  }
  @media (max-width: 1100px) {
    .world-grid {
      grid-template-columns: 1fr;
    }
  }
  .world-hit {
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .world-row.sel {
    background: var(--accent-soft);
  }
  .world-actions {
    padding: 12px 14px;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .tl-item {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .tl-item:first-of-type {
    border-top: none;
  }
  .tl-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    flex: none;
  }
  .server-add {
    display: flex;
    gap: 8px;
    margin: 12px 0 10px;
    flex-wrap: wrap;
  }
  .server-add .input {
    height: 34px;
  }
  .server-address {
    flex: 1;
    min-width: 220px;
  }
  .server-edit {
    flex: 1;
    display: flex;
    gap: 8px;
    min-width: 0;
    flex-wrap: wrap;
  }
  .server-icon {
    width: 28px;
    height: 28px;
    border-radius: var(--r);
    flex: none;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .server-icon-fallback {
    display: grid;
    place-items: center;
    background: var(--glass-strong);
    color: var(--text-2);
  }
  .server-motd {
    font-weight: 600;
  }
  .shot-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 12px;
  }
  .shot-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 14px;
    border-radius: var(--r);
    border: 1px solid var(--glass-border);
    background: var(--glass);
    color: var(--text-1);
    font-family: var(--font);
    text-align: left;
    cursor: pointer;
  }
  .shot-card:hover {
    background: var(--glass-strong);
  }
  .shot-card.sel {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .shot-card .shot-name {
    font-size: 12.5px;
    font-weight: 600;
    word-break: break-all;
  }
  .shot-card small {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .log-toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .log-toolbar select.input {
    max-width: 230px;
  }
  .log-toolbar .input {
    height: 34px;
  }
  .log-search {
    width: 180px;
  }
  .console {
    background: rgba(3, 10, 14, 0.75);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    padding: 14px 16px;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 1.9;
    height: 520px;
    overflow-y: auto;
  }
  .console .t {
    color: var(--text-3);
    margin-right: 10px;
  }
  .console .lv-info {
    color: var(--info);
  }
  .console .lv-warn {
    color: var(--warn);
  }
  .console .lv-error {
    color: var(--danger);
  }
  .console .msg {
    color: var(--text-2);
    word-break: break-all;
    white-space: pre-wrap;
  }
  .mem-slider {
    width: 240px;
    accent-color: var(--accent);
  }
  .mem-field {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .mem-num {
    width: 92px;
    height: 32px;
  }
  .export-grid {
    display: grid;
    grid-template-columns: 1fr 200px;
    gap: 12px;
  }
  .export-options {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 16px;
    margin-top: 12px;
  }
  .exp-opt {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
</style>
