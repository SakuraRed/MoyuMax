<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import { shellAccount } from "../accounts.svelte";
  import type {
    InstanceResource,
    InstanceResourceKind,
    InstanceScreenshot,
    InstanceServerEntry,
    InstanceWorldInfo,
    InstalledContent,
    InstalledModpack,
    JavaEnvironment,
    LaunchSession,
    LaunchSessionState,
    ManagedInstance,
    MinecraftServerStatus,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    /** 当前实例；被回收等情况下为 null，组件自行退回首页。 */
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

  type DetailTab = "overview" | "setup" | "mods" | "saves" | "screenshots" | "resourcepacks" | "shaders" | "servers" | "logs";
  type ContentFilter = "all" | "enabled" | "disabled";

  const NAV_GROUPS: { groupKey: string; items: { key: DetailTab; labelKey: string }[] }[] = [
    {
      groupKey: "instanceDetail.nav.groupGame",
      items: [
        { key: "overview", labelKey: "instanceDetail.nav.overview" },
        { key: "setup", labelKey: "instanceDetail.nav.setup" },
        { key: "logs", labelKey: "instanceDetail.nav.logs" },
      ],
    },
    {
      groupKey: "instanceDetail.nav.groupResource",
      items: [
        { key: "mods", labelKey: "instanceDetail.nav.mods" },
        { key: "saves", labelKey: "instanceDetail.nav.saves" },
        { key: "screenshots", labelKey: "instanceDetail.nav.screenshots" },
        { key: "resourcepacks", labelKey: "instanceDetail.nav.resourcepacks" },
        { key: "shaders", labelKey: "instanceDetail.nav.shaders" },
        { key: "servers", labelKey: "instanceDetail.nav.servers" },
      ],
    },
  ];

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

  let tab = $state<DetailTab>("overview");
  let loading = $state(true);
  let modpack = $state<InstalledModpack | null>(null);
  let javaEnvironments = $state<JavaEnvironment[]>([]);
  let memoryMin = $state("");
  let memoryMax = $state("");
  let autoUpdate = $state(false);
  let mods = $state<InstalledContent[]>([]);
  let resources = $state<InstanceResource[]>([]);
  let worlds = $state<InstanceWorldInfo[]>([]);
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
  let modFilter = $state<ContentFilter>("all");
  let resourceFilter = $state<ContentFilter>("all");
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
  let message = $state("");
  let errorMessage = $state("");

  // 游戏日志副页:双通道偏移尾部跟随,运行中会话每 2 秒轮询一次。
  const LOG_POLL_INTERVAL_MS = 2_000;
  const LOG_LINE_LIMIT = 5_000;
  let logSessionId = $state("");
  let logLines = $state<string[]>([]);
  let logStdoutOffset = $state(0);
  let logStderrOffset = $state(0);
  let logTruncated = $state(false);
  let logState = $state<LaunchSessionState | null>(null);
  let logAutoScroll = $state(true);
  let logCopied = $state(false);
  let logStopping = $state(false);
  let logViewport = $state<HTMLElement | null>(null);
  let logTimer: ReturnType<typeof setInterval> | undefined;
  let logCopyTimer: ReturnType<typeof setTimeout> | undefined;

  const instanceId = $derived(instance?.id ?? "");
  const instanceSessions = $derived(
    launchSessions.filter((session) => session.instanceId === instanceId),
  );
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
  const assignedJavaId = $derived(
    javaEnvironments.find((environment) =>
      environment.referencingInstances.some((entry) => entry.id === instanceId),
    )?.id ?? "",
  );
  const filteredMods = $derived(
    mods.filter((entry) =>
      modFilter === "all" ? true : modFilter === "enabled" ? entry.enabled : !entry.enabled,
    ),
  );

  onMount(() => {
    if (
      initialTab &&
      NAV_GROUPS.some((group) => group.items.some((item) => item.key === initialTab))
    ) {
      selectTab(initialTab as DetailTab);
    }
    void loadDetail();
    return () => {
      stopLogPolling();
      if (logCopyTimer !== undefined) clearTimeout(logCopyTimer);
    };
  });

  // 实例被回收（或外部删除）后列表快照不再包含它，优雅退回首页。
  $effect(() => {
    if (!instance) onExit();
  });

  async function loadDetail(): Promise<void> {
    const current = instance;
    if (!current) return;
    loading = true;
    errorMessage = "";
    try {
      const [pack, environments, options, auto, content, resourceList, worldList, shotList, serverList] =
        await Promise.all([
          runtime.getInstanceModpack(current.id),
          runtime.listJavaEnvironments(),
          runtime.getInstanceLaunchOptions(current.id),
          runtime.getInstanceContentAutoUpdate(current.id),
          runtime.getInstalledContent(current.id),
          runtime.listInstanceResources(current.id),
          runtime.listInstanceWorldDetails(current.id),
          runtime.listInstanceScreenshots(current.id),
          runtime.listInstanceServers(current.id),
        ]);
      modpack = pack;
      javaEnvironments = environments;
      memoryMin = String(options.minimumMemoryMib);
      memoryMax = String(options.maximumMemoryMib);
      autoUpdate = auto;
      mods = content;
      resources = resourceList;
      worlds = worldList;
      screenshots = shotList;
      servers = serverList;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
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

  /** 进入日志副页或切换会话时从零偏移重读;仅运行中会话需要周期跟随。 */
  function restartLogStream(): void {
    stopLogPolling();
    logLines = [];
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
        ...splitLogContent(read.stdout.content),
        ...splitLogContent(read.stderr.content),
      ];
      if (appended.length > 0) {
        logLines = [...logLines, ...appended].slice(-LOG_LINE_LIMIT);
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
      await navigator.clipboard.writeText(logLines.join("\n"));
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

  function clearMessages(): void {
    message = "";
    errorMessage = "";
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
      // 状态刷新后 instance 变为 null,$effect 会退回首页。
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

  async function assignJava(environmentId: string): Promise<void> {
    const current = instance;
    if (!current || !environmentId || environmentId === assignedJavaId) return;
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

  async function toggleMod(entry: InstalledContent, enabled: boolean): Promise<void> {
    clearMessages();
    try {
      const updated = await runtime.setInstalledContentEnabled(entry.id, enabled);
      mods = mods.map((candidate) => (candidate.id === updated.id ? updated : candidate));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      mods = instance ? await runtime.getInstalledContent(instance.id) : mods;
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
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
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
  pageTitle={instance ? instance.name : t("instanceDetail.pageTitle")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="instances"
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content settings-content">
    <div class="settings-layout">
      <nav class="settings-nav" aria-label={t("instanceDetail.nav.aria")}>
        <button class="sn-item sn-back" onclick={onExit}>{t("instanceDetail.back")}</button>
        {#each NAV_GROUPS as group}
          <div class="sn-group">{t(group.groupKey)}</div>
          {#each group.items as item}
            <button
              class="sn-item"
              class:active={tab === item.key}
              aria-current={tab === item.key ? "page" : undefined}
              onclick={() => selectTab(item.key)}
            >{t(item.labelKey)}</button>
          {/each}
        {/each}
      </nav>

      <div class="settings-main" data-scroll-region="main">
        {#if loading}
          <section class="data-loading" aria-live="polite">
            <div class="loading-line wide"></div>
            <div class="loading-line"></div>
            <span>{t("instanceDetail.loading")}</span>
          </section>
        {:else if instance}
          {#if tab === "overview"}
            <section class="backup-settings" aria-labelledby="instance-overview-title">
              <header>
                <div>
                  <h2 id="instance-overview-title">{t("instanceDetail.overview.title")}</h2>
                  <p>{t("instanceDetail.overview.description")}</p>
                </div>
              </header>
              <div class="instance-overview-grid">
                <article class="instance-info-card">
                  <h3>{t("instanceDetail.overview.versionCard")}</h3>
                  <dl class="instance-meta">
                    <div><dt>{t("instanceDetail.overview.gameVersion")}</dt><dd>{instance.gameVersion}</dd></div>
                    <div><dt>{t("instanceDetail.overview.loader")}</dt><dd>{loaderLabel(instance)}</dd></div>
                    <div><dt>{t("instanceDetail.overview.directory")}</dt><dd><code>{instance.rootDirectory}</code></dd></div>
                    <div>
                      <dt>{t("instanceDetail.overview.state")}</dt>
                      <dd>{activeSession ? sessionStateLabel(activeSession.state) : instance.state === "ready" ? t("instanceDetail.overview.stateReady") : instance.state}</dd>
                    </div>
                  </dl>
                </article>
                {#if modpack}
                  <article class="instance-info-card">
                    <h3>{t("instanceDetail.overview.modpackCard")}</h3>
                    <p><strong>{modpack.packName}</strong> {modpack.packVersion}</p>
                    <p class="instance-card-note">{modpack.provider === "modrinth" ? "Modrinth" : "CurseForge"} · {modpack.gameVersion} · {loaderName(modpack.loaderKind)}</p>
                    <div class="task-buttons">
                      <button class="button ghost compact" disabled={updatingPack} onclick={() => void updatePack()}>{updatingPack ? t("modpack.updating") : t("modpack.update")}</button>
                    </div>
                  </article>
                {/if}
                <article class="instance-info-card">
                  <h3>{t("instanceDetail.overview.manageCard")}</h3>
                  <div class="task-buttons">
                    {#if activeSession}
                      <button class="button" disabled={changingInstance} onclick={() => void stop()}>{changingInstance ? t("home.launch.stopping") : t("home.launch.stop")}</button>
                    {:else}
                      <button class="button primary" disabled={changingInstance || instance.state !== "ready"} onclick={() => void start()}>
                        <Icon name="play" size={14} />{changingInstance ? t("home.launch.starting") : t("home.launch.start")}
                      </button>
                    {/if}
                    <button class="button danger-subtle" disabled={changingInstance || Boolean(activeSession)} onclick={() => void askRecycle()}>{t("home.launch.recycle")}</button>
                  </div>
                </article>
              </div>
            </section>
          {:else if tab === "setup"}
            <section class="backup-settings" aria-labelledby="instance-setup-java">
              <header>
                <div>
                  <h2 id="instance-setup-java">{t("instanceDetail.setup.javaCard")}</h2>
                  <p>{t("instanceDetail.setup.javaDescription")}</p>
                </div>
              </header>
              {#if readyEnvironments.length === 0}
                <div class="instance-empty">
                  <p>{t("instanceDetail.setup.javaEmpty")}</p>
                  <button class="button primary compact" onclick={() => onNavigate("settings")}>{t("instanceDetail.setup.javaGoSettings")}</button>
                </div>
              {:else}
                <label class="instance-field">
                  <span>{t("instanceDetail.setup.javaLabel")}</span>
                  <select
                    aria-label={t("instanceDetail.setup.javaAria")}
                    disabled={assigningJava}
                    value={assignedJavaId}
                    onchange={(event) => void assignJava((event.currentTarget as HTMLSelectElement).value)}
                  >
                    {#if !assignedJavaId}
                      <option value="" disabled>{t("instanceDetail.setup.javaUnset")}</option>
                    {/if}
                    {#each readyEnvironments as environment}
                      <option value={environment.id}>{environment.fullVersion} · {environment.distribution === "azulZulu" ? "Azul Zulu" : environment.distribution}</option>
                    {/each}
                  </select>
                </label>
              {/if}
            </section>

            <section class="backup-settings" aria-labelledby="instance-setup-memory">
              <header>
                <div>
                  <h2 id="instance-setup-memory">{t("instanceDetail.setup.memoryCard")}</h2>
                  <p>{t("instanceDetail.setup.memoryDescription")}</p>
                </div>
              </header>
              <div class="instance-memory-inputs">
                <label>
                  <span>{t("instanceDetail.setup.memoryMin")}</span>
                  <input bind:value={memoryMin} type="text" inputmode="numeric" aria-label={t("instanceDetail.setup.memoryMinAria")} />
                </label>
                <label>
                  <span>{t("instanceDetail.setup.memoryMax")}</span>
                  <input bind:value={memoryMax} type="text" inputmode="numeric" aria-label={t("instanceDetail.setup.memoryMaxAria")} />
                </label>
                <button class="button primary compact" disabled={savingMemory} onclick={() => void saveMemory()}>{savingMemory ? t("instanceDetail.setup.memorySaving") : t("instanceDetail.setup.memorySave")}</button>
              </div>
            </section>

            <section class="backup-settings" aria-labelledby="instance-setup-autoupdate">
              <header>
                <div>
                  <h2 id="instance-setup-autoupdate">{t("instanceDetail.setup.autoUpdateCard")}</h2>
                  <p>{t("instanceDetail.setup.autoUpdateDescription")}</p>
                </div>
              </header>
              <label class="resource-enable-toggle">
                <input
                  type="checkbox"
                  checked={autoUpdate}
                  disabled={savingAutoUpdate}
                  aria-label={t("instanceDetail.setup.autoUpdateAria")}
                  onchange={(event) => void toggleAutoUpdate((event.currentTarget as HTMLInputElement).checked)}
                />
                <span>{autoUpdate ? t("resources.files.enabled") : t("resources.files.disabled")}</span>
              </label>
            </section>
          {:else if tab === "mods"}
            <section class="backup-settings" aria-labelledby="instance-mods-title">
              <header>
                <div>
                  <h2 id="instance-mods-title">{t("instanceDetail.mods.title")}</h2>
                  <p>{t("instanceDetail.mods.description")}</p>
                </div>
                <div class="screenshot-filters" role="group" aria-label={t("instanceDetail.filter.aria")}>
                  <button class="filter-chip" class:active={modFilter === "all"} onclick={() => { modFilter = "all"; }}>{t("instanceDetail.filter.all").replace("{count}", String(mods.length))}</button>
                  <button class="filter-chip" class:active={modFilter === "enabled"} onclick={() => { modFilter = "enabled"; }}>{t("instanceDetail.filter.enabled")}</button>
                  <button class="filter-chip" class:active={modFilter === "disabled"} onclick={() => { modFilter = "disabled"; }}>{t("instanceDetail.filter.disabled")}</button>
                </div>
              </header>
              {#if mods.length === 0}
                <div class="instance-empty">
                  <p>{t("instanceDetail.mods.empty")}</p>
                  <button class="button primary compact" onclick={() => onNavigate("resources")}>{t("instanceDetail.mods.emptyAction")}</button>
                </div>
              {:else if filteredMods.length === 0}
                <div class="backup-empty-row">{t("instanceDetail.filter.empty")}</div>
              {:else}
                <div class="installed-content-list">
                  {#each filteredMods as entry}
                    <article class="installed-content-row">
                      <div>
                        <strong>{entry.projectTitle}</strong>
                        <small>{entry.versionNumber} · {entry.fileName} · {formatBytes(entry.size)}</small>
                      </div>
                      <div class="resource-row-actions">
                        <label class="resource-enable-toggle">
                          <input
                            type="checkbox"
                            checked={entry.enabled}
                            aria-label={t("resources.files.toggleAria").replace("{name}", entry.projectTitle)}
                            onchange={(event) => void toggleMod(entry, (event.currentTarget as HTMLInputElement).checked)}
                          />
                          <span>{entry.enabled ? t("resources.files.enabled") : t("resources.files.disabled")}</span>
                        </label>
                      </div>
                    </article>
                  {/each}
                </div>
              {/if}
            </section>
          {:else if tab === "saves"}
            <section class="backup-settings" aria-labelledby="instance-saves-title">
              <header>
                <div>
                  <h2 id="instance-saves-title">{t("instanceDetail.saves.title")}</h2>
                  <p>{t("instanceDetail.saves.description")}</p>
                </div>
                <div class="local-content-actions">
                  <button class="button ghost compact" disabled={importing} onclick={() => void importWorld()}>{importing ? t("data.worlds.busy") : t("data.worlds.import")}</button>
                </div>
              </header>
              {#if worlds.length === 0}
                <div class="instance-empty">
                  <p>{t("data.worlds.empty")}</p>
                  <button class="button ghost compact" onclick={() => void importWorld()}>{t("data.worlds.import")}</button>
                </div>
              {:else}
                <div class="backup-list">
                  {#each worlds as world}
                    <article class="backup-row">
                      <div>
                        <div class="backup-title-line"><h3>{world.name}</h3><span>{t("data.worlds.badge")}</span></div>
                        <p>{formatBytes(world.sizeBytes)}{world.lastPlayedUnixSeconds ? t("data.worlds.lastPlayed").replace("{time}", timestampLabel(world.lastPlayedUnixSeconds)) : ""}</p>
                      </div>
                      <div class="backup-side">
                        <button class="button ghost compact" disabled={busy} onclick={() => void exportWorld(world)}>{t("data.worlds.export")}</button>
                        {#if pendingDelete === `world-${world.name}`}
                          <button class="button danger-subtle compact" disabled={busy} onclick={() => void deleteWorld(world)}>{t("common.confirmDelete")}</button>
                          <button class="button ghost compact" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                        {:else}
                          <button class="button danger-subtle compact" disabled={busy} onclick={() => { pendingDelete = `world-${world.name}`; }}>{t("common.delete")}</button>
                        {/if}
                      </div>
                    </article>
                  {/each}
                </div>
              {/if}
            </section>
          {:else if tab === "screenshots"}
            <section class="backup-settings" aria-labelledby="instance-shots-title">
              <header>
                <div>
                  <h2 id="instance-shots-title">{t("instanceDetail.screenshots.title")}</h2>
                  <p>{t("instanceDetail.screenshots.description")}</p>
                </div>
              </header>
              {#if screenshots.length === 0}
                <div class="instance-empty">
                  <p>{t("data.screenshots.empty")}</p>
                </div>
              {:else}
                <div class="screenshot-grid">
                  {#each screenshots as screenshot}
                    <button
                      class="screenshot-card"
                      class:selected={selectedScreenshot === screenshot.fileName}
                      aria-pressed={selectedScreenshot === screenshot.fileName}
                      aria-label={t("data.screenshots.cardAria").replace("{name}", screenshot.fileName)}
                      onclick={() => {
                        selectedScreenshot = selectedScreenshot === screenshot.fileName ? null : screenshot.fileName;
                        pendingDelete = null;
                      }}
                    >
                      <Icon name="disk" size={20} />
                      <span class="screenshot-name">{screenshot.fileName}</span>
                      <small>{formatBytes(screenshot.sizeBytes)} · {timestampLabel(screenshot.takenAtUnixSeconds)}</small>
                    </button>
                  {/each}
                </div>
                {#if selectedScreenshot}
                  <div class="screenshot-actions">
                    <span>{t("data.screenshots.selected").replace("{name}", selectedScreenshot)}</span>
                    <div class="local-content-actions">
                      <button class="button ghost compact" onclick={() => void copyScreenshot(selectedScreenshot!)}>{t("data.screenshots.copy")}</button>
                      <button class="button ghost compact" onclick={() => void openScreenshot(selectedScreenshot!)}>{t("data.screenshots.openLocation")}</button>
                      {#if pendingDelete === "screenshot"}
                        <button class="button danger-subtle compact" disabled={busy} onclick={() => void deleteScreenshot(selectedScreenshot!)}>{t("common.confirmDelete")}</button>
                        <button class="button ghost compact" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                      {:else}
                        <button class="button danger-subtle compact" disabled={busy} onclick={() => { pendingDelete = "screenshot"; }}>{t("common.delete")}</button>
                      {/if}
                    </div>
                  </div>
                {/if}
              {/if}
            </section>
          {:else if tab === "resourcepacks"}
            {@render resourceSection("resourcepack", "instanceDetail.resourcepacks.title")}
          {:else if tab === "shaders"}
            {@render resourceSection("shader", "instanceDetail.shaders.title")}
          {:else if tab === "servers"}
            <section class="backup-settings" aria-labelledby="instance-servers-title">
              <header>
                <div>
                  <h2 id="instance-servers-title">{t("instanceDetail.servers.title")}</h2>
                  <p>{t("instanceDetail.servers.description")}</p>
                </div>
                <div class="local-content-actions">
                  <button
                    class="button ghost compact"
                    disabled={refreshingServers || servers.length === 0}
                    onclick={() => void refreshAllServerStatus()}
                  >{refreshingServers ? t("instanceDetail.servers.refreshing") : t("instanceDetail.servers.refreshAll")}</button>
                </div>
              </header>
              <form
                class="server-add-form"
                onsubmit={(event) => {
                  event.preventDefault();
                  void addServer();
                }}
              >
                <input
                  bind:value={serverFormName}
                  type="text"
                  maxlength="64"
                  placeholder={t("instanceDetail.servers.name")}
                  aria-label={t("instanceDetail.servers.nameAria")}
                />
                <input
                  bind:value={serverFormAddress}
                  type="text"
                  placeholder={t("instanceDetail.servers.addressHint")}
                  aria-label={t("instanceDetail.servers.addressAria")}
                />
                <button class="button primary compact" type="submit" disabled={addingServer}>
                  {addingServer ? t("instanceDetail.servers.adding") : t("instanceDetail.servers.addAction")}
                </button>
              </form>
              {#if servers.length === 0}
                <div class="instance-empty">
                  <p>{t("instanceDetail.servers.empty")}</p>
                  <small>{t("instanceDetail.servers.emptyHint")}</small>
                </div>
              {:else}
                <div class="backup-list">
                  {#each servers as server, index}
                    {@const status = serverStatus[index]}
                    <article class="backup-row server-row">
                      {#if editingServer === index}
                        <div class="server-edit-form">
                          <input
                            bind:value={editName}
                            type="text"
                            maxlength="64"
                            aria-label={t("instanceDetail.servers.nameAria")}
                          />
                          <input
                            bind:value={editAddress}
                            type="text"
                            aria-label={t("instanceDetail.servers.addressAria")}
                          />
                        </div>
                        <div class="backup-side">
                          <button class="button primary compact" disabled={savingServer} onclick={() => void saveEditServer(index)}>{t("instanceDetail.servers.saveEdit")}</button>
                          <button class="button ghost compact" disabled={savingServer} onclick={() => { editingServer = null; }}>{t("instanceDetail.servers.cancelEdit")}</button>
                        </div>
                      {:else}
                        <div class="server-info">
                          {#if server.icon}
                            <img class="server-icon" src={server.icon} alt="" />
                          {:else}
                            <span class="server-icon server-icon-fallback"><Icon name="wifi" size={16} /></span>
                          {/if}
                          <div class="server-text">
                            <div class="backup-title-line"><h3>{server.name}</h3><span>{server.address}</span></div>
                            {#if status === "loading"}
                              <small>{t("instanceDetail.servers.pinging")}</small>
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
                              <small>
                                {t("instanceDetail.servers.players").replace("{online}", String(status.playersOnline ?? 0)).replace("{max}", String(status.playersMax ?? 0))}
                                · {t("instanceDetail.servers.latency").replace("{ms}", String(status.latencyMs ?? 0))}
                                {status.versionName ? ` · ${status.versionName}` : ""}
                              </small>
                            {:else if status}
                              <small class="server-offline">{t("instanceDetail.servers.offline")}</small>
                            {:else}
                              <small>{t("instanceDetail.servers.unpinged")}</small>
                            {/if}
                          </div>
                        </div>
                        <div class="backup-side">
                          <button
                            class="button ghost compact"
                            disabled={status === "loading"}
                            aria-label={t("instanceDetail.servers.refreshAria").replace("{name}", server.name)}
                            onclick={() => void refreshServerStatus(index)}
                          >{t("instanceDetail.servers.refresh")}</button>
                          <button
                            class="button ghost compact"
                            aria-label={t("instanceDetail.servers.editAria").replace("{name}", server.name)}
                            onclick={() => startEditServer(index)}
                          >{t("instanceDetail.servers.edit")}</button>
                          {#if pendingDelete === `server-${index}`}
                            <button class="button danger-subtle compact" disabled={busy} onclick={() => void deleteServer(index)}>{t("common.confirmDelete")}</button>
                            <button class="button ghost compact" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                          {:else}
                            <button
                              class="button danger-subtle compact"
                              disabled={busy}
                              aria-label={t("instanceDetail.servers.deleteAria").replace("{name}", server.name)}
                              onclick={() => { pendingDelete = `server-${index}`; }}
                            >{t("common.delete")}</button>
                          {/if}
                        </div>
                      {/if}
                    </article>
                  {/each}
                </div>
              {/if}
            </section>
          {:else if tab === "logs"}
            <section class="backup-settings" aria-labelledby="instance-logs-title">
              <header>
                <div>
                  <h2 id="instance-logs-title">{t("instanceDetail.logs.title")}</h2>
                  <p>{t("instanceDetail.logs.description")}</p>
                </div>
              </header>
              {#if instanceSessions.length === 0}
                <div class="resource-empty">
                  <p>{t("instanceDetail.logs.empty")}</p>
                </div>
              {:else}
                <div class="log-toolbar">
                  <label class="log-session-picker">
                    <span>{t("instanceDetail.logs.session")}</span>
                    <select
                      aria-label={t("instanceDetail.logs.sessionAria")}
                      bind:value={logSessionId}
                      onchange={() => restartLogStream()}
                    >
                      {#each instanceSessions as session}
                        <option value={session.id}>{logSessionLabel(session)}</option>
                      {/each}
                    </select>
                  </label>
                  <label class="log-autoscroll">
                    <input type="checkbox" bind:checked={logAutoScroll} />
                    <span>{t("instanceDetail.logs.autoScroll")}</span>
                  </label>
                  <div class="log-actions">
                    <button
                      class="button ghost compact"
                      disabled={logLines.length === 0}
                      onclick={() => void copyLaunchLog()}
                    >{logCopied ? t("instanceDetail.logs.copied") : t("instanceDetail.logs.copy")}</button>
                    <button
                      class="button ghost compact"
                      onclick={() => void openLogLocation()}
                    >{t("instanceDetail.logs.openLocation")}</button>
                    <button
                      class="button ghost compact"
                      disabled={logLines.length === 0}
                      onclick={() => { logLines = []; }}
                    >{t("instanceDetail.logs.clear")}</button>
                    {#if logSessionRunning}
                      <button
                        class="button danger-subtle compact"
                        disabled={logStopping}
                        onclick={() => void stopLogSession()}
                      >{logStopping ? t("instanceDetail.logs.stopping") : t("instanceDetail.logs.stop")}</button>
                    {/if}
                  </div>
                </div>
                {#if logTruncated}
                  <p class="log-truncated">{t("instanceDetail.logs.truncated")}</p>
                {/if}
                <div
                  class="log-viewport"
                  bind:this={logViewport}
                  aria-label={t("instanceDetail.logs.viewportAria")}
                >
                  {#if logLines.length === 0}
                    <p class="log-empty">{t("instanceDetail.logs.noOutput")}</p>
                  {:else}
                    <pre class="log-output">{logLines.join("\n")}</pre>
                  {/if}
                </div>
              {/if}
            </section>
          {/if}
        {/if}
      </div>
    </div>
  </main>

  {#if errorMessage}
    <div class="toast danger-toast" role="alert"><Icon name="info" size={16} /><span>{errorMessage}</span></div>
  {:else if message}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{message}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{message || errorMessage}</div>

  {#if recycleConfirm && instance}
    <div class="modal-backdrop">
      <div
        class="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="instance-recycle-title"
        tabindex="-1"
        bind:this={recycleDialog}
        onkeydown={handleRecycleDialogKeydown}
      >
        <header>
          <h2 id="instance-recycle-title">{t("home.recycle.title").replace("{name}", instance.name)}</h2>
          <p>{t("home.recycle.description")}</p>
        </header>
        <div class="confirmation-impact">
          <strong>{t("home.recycle.impactTitle")}</strong>
          <span>{t("home.recycle.impactBody")}</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingInstance} onclick={cancelRecycle}>{t("common.cancel")}</button>
          <button class="button danger" disabled={changingInstance} onclick={() => void recycleNow()}>
            {changingInstance ? t("home.recycle.moving") : t("home.launch.recycle")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

{#snippet resourceSection(kind: "resourcepack" | "shader", titleKey: string)}
  {@const kindResources = resources.filter((resource) => resource.kind === kind)}
  {@const shown = kindResources.filter((resource) =>
    resourceFilter === "all" ? true : resourceFilter === "enabled" ? resource.enabled : !resource.enabled,
  )}
  <section class="backup-settings" aria-labelledby="instance-resource-{kind}-title">
    <header>
      <div>
        <h2 id="instance-resource-{kind}-title">{t(titleKey)}</h2>
        <p>{t("instanceDetail.resources.description")}</p>
      </div>
      <div class="local-content-actions">
        <div class="screenshot-filters" role="group" aria-label={t("instanceDetail.filter.aria")}>
          <button class="filter-chip" class:active={resourceFilter === "all"} onclick={() => { resourceFilter = "all"; }}>{t("instanceDetail.filter.all").replace("{count}", String(kindResources.length))}</button>
          <button class="filter-chip" class:active={resourceFilter === "enabled"} onclick={() => { resourceFilter = "enabled"; }}>{t("instanceDetail.filter.enabled")}</button>
          <button class="filter-chip" class:active={resourceFilter === "disabled"} onclick={() => { resourceFilter = "disabled"; }}>{t("instanceDetail.filter.disabled")}</button>
        </div>
        <button class="button ghost compact" disabled={importing} onclick={() => void importResource(kind)}>{importing ? t("data.worlds.busy") : t("instanceDetail.resources.import")}</button>
      </div>
    </header>
    {#if kindResources.length === 0}
      <div class="instance-empty">
        <p>{t("instanceDetail.resources.empty")}</p>
        <button class="button ghost compact" onclick={() => onNavigate("resources")}>{t("instanceDetail.resources.emptyAction")}</button>
      </div>
    {:else if shown.length === 0}
      <div class="backup-empty-row">{t("instanceDetail.filter.empty")}</div>
    {:else}
      <div class="installed-content-list">
        {#each shown as resource}
          <article class="installed-content-row">
            <div>
              <strong>{resource.displayName}</strong>
              <small>{kindLabel(resource.kind)} · {resource.fileName} · {formatBytes(resource.size)}</small>
            </div>
            <div class="resource-row-actions">
              <label class="resource-enable-toggle">
                <input
                  type="checkbox"
                  checked={resource.enabled}
                  aria-label={t("resources.files.toggleAria").replace("{name}", resource.displayName)}
                  onchange={(event) => void toggleResource(resource, (event.currentTarget as HTMLInputElement).checked)}
                />
                <span>{resource.enabled ? t("resources.files.enabled") : t("resources.files.disabled")}</span>
              </label>
              {#if pendingDelete === resource.id}
                <button class="button danger-subtle compact" disabled={busy} onclick={() => void deleteResource(resource)}>{t("common.confirmDelete")}</button>
                <button class="button ghost compact" disabled={busy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
              {:else}
                <button class="button danger-subtle compact" disabled={busy} aria-label={t("resources.files.deleteAria").replace("{name}", resource.displayName)} onclick={() => { pendingDelete = resource.id; }}>{t("common.delete")}</button>
              {/if}
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </section>
{/snippet}
