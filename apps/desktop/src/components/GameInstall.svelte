<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";

  import {
    defaultInstanceName,
    formatBytes,
    installStageLabel,
    recommendedFabricLoader,
    recommendedVersion,
    taskProgressAriaLabel,
  } from "../installation";
  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    FabricLoaderSummary,
    GameVersionSummary,
    InstallPreview,
    InstallTask,
    LoaderChoice,
    ModpackInstallReport,
    ModpackProgressEvent,
    ModpackPreviewResponse,
    ModrinthVersionSummary,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    VersionCatalog,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Fish from "./Fish.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onBack: () => void;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  type InstallView = "loading" | "configure" | "previewing" | "confirm" | "queueing" | "queued";
  type FoldLoaderKind = "forge" | "neoforge" | "fabric" | "quilt";

  /** Modrinth 上 Fabric API 的项目 ID（PCL 同款附带安装对象）。 */
  const FABRIC_API_PROJECT_ID = "P7dR8mSH";

  const LOADER_DISPLAY: Record<FoldLoaderKind, string> = {
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };

  let {
    runtime,
    settings,
    onBack,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let view = $state<InstallView>("loading");
  let catalog = $state<VersionCatalog | null>(null);
  let selectedVersion = $state<GameVersionSummary | null>(null);
  let fabricLoaders = $state<FabricLoaderSummary[]>([]);
  let quiltLoaders = $state<FabricLoaderSummary[]>([]);
  let forgeVersions = $state<FabricLoaderSummary[]>([]);
  let neoforgeVersions = $state<FabricLoaderSummary[]>([]);
  let loader = $state<LoaderChoice>({ kind: "vanilla" });
  let instanceName = $state("");
  let nameEdited = $state(false);
  let preview = $state<InstallPreview | null>(null);
  let task = $state<InstallTask | null>(null);
  let errorMessage = $state("");
  let loaderMessage = $state("");
  let loaderMessageWarn = $state(false);
  let pageRoot: HTMLElement | undefined = $state();
  let loaderRequestSequence = 0;
  let taskPoll: ReturnType<typeof setInterval> | undefined;
  let taskPollRunning = false;
  let packPreview = $state<ModpackPreviewResponse | null>(null);
  let packInstalling = $state(false);
  let packProgress = $state<ModpackProgressEvent | null>(null);
  let packDone = $state<ModpackInstallReport | null>(null);
  let packError = $state("");
  let expandedMajors = $state<Set<string>>(new Set());
  let showSnapshots = $state(false);
  let showOldVersions = $state(false);
  let expandedLoader = $state<FoldLoaderKind | null>(null);
  let fabricApiVersions = $state<ModrinthVersionSummary[]>([]);
  let fabricApiEnabled = $state(false);
  let fabricApiVersionId = $state("");
  let fabricApiLoading = $state(false);
  let fabricApiRequestSequence = 0;
  let fabricApiInstall = $state<"idle" | "installing" | "done" | "failed">("idle");
  let fabricApiInstallError = $state("");
  let fabricApiInstallTriggered = false;

  /** 大版本号：1.21.4 → 1.21；26.2 → 26.2（两段及以下保持原样）。 */
  function majorOf(id: string): string {
    const parts = id.split(".");
    return parts.length >= 3 ? `${parts[0]}.${parts[1]}` : id;
  }

  /** 最新快捷卡：最新正式版 +（快照开关打开且快照更新时的）最新快照。 */
  const latestReleaseVersion = $derived(
    (catalog?.versions ?? []).find((version) => version.id === catalog?.latestRelease) ??
      (catalog?.versions ?? []).find((version) => version.releaseType === "release") ??
      null,
  );
  const latestSnapshotVersion = $derived.by(() => {
    const candidate =
      (catalog?.versions ?? []).find((version) => version.id === catalog?.latestSnapshot) ??
      (catalog?.versions ?? []).find((version) => version.releaseType === "snapshot") ??
      null;
    if (!candidate || !latestReleaseVersion) return null;
    const snapshotTime = new Date(candidate.releaseTime).valueOf();
    const releaseTime = new Date(latestReleaseVersion.releaseTime).valueOf();
    return snapshotTime > releaseTime ? candidate : null;
  });
  const showLatestSnapshot = $derived(showSnapshots && latestSnapshotVersion !== null);
  const releaseGroups = $derived.by(() => {
    const groups = new Map<string, GameVersionSummary[]>();
    for (const version of (catalog?.versions ?? []).filter(
      (candidate) =>
        candidate.releaseType === "release" && candidate.id !== latestReleaseVersion?.id,
    )) {
      const major = majorOf(version.id);
      if (!groups.has(major)) groups.set(major, []);
      groups.get(major)?.push(version);
    }
    return [...groups.entries()].map(([major, versions]) => ({ major, versions }));
  });
  const snapshotVersions = $derived(
    (catalog?.versions ?? []).filter(
      (version) =>
        version.releaseType === "snapshot" &&
        !(showLatestSnapshot && version.id === latestSnapshotVersion?.id),
    ),
  );
  const oldVersions = $derived(
    (catalog?.versions ?? []).filter(
      (version) =>
        version.releaseType === "oldBeta" ||
        version.releaseType === "oldAlpha" ||
        version.releaseType === "unknown",
    ),
  );

  /** 当前展开的加载器版本清单：卡片网格下方整宽呈现。 */
  const expandedLoaderMeta = $derived.by(() => {
    if (!expandedLoader) return null;
    const meta: Record<FoldLoaderKind, { versions: FabricLoaderSummary[]; label: string }> = {
      forge: { versions: forgeVersions, label: t("install.loader.forgeField") },
      neoforge: { versions: neoforgeVersions, label: t("install.loader.neoforgeField") },
      fabric: { versions: fabricLoaders, label: t("install.loader.fabricField") },
      quilt: { versions: quiltLoaders, label: t("install.loader.quiltField") },
    };
    return { kind: expandedLoader, ...meta[expandedLoader] };
  });

  function toggleMajor(major: string): void {
    const next = new Set(expandedMajors);
    if (next.has(major)) {
      next.delete(major);
    } else {
      next.add(major);
    }
    expandedMajors = next;
  }

  $effect(() => {
    view;
    void tick().then(() => {
      pageRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });
  });

  onMount(() => {
    void loadCatalog();
    const unlisten = runtime.onModpackProgress((event) => {
      packProgress = event;
      // 切页重挂载:进度事件仍在流动即视为安装进行中,恢复进度展示。
      if (!packDone && !packError) packInstalling = true;
    });
    return () => {
      unlisten();
    };
  });

  onDestroy(() => {
    if (taskPoll) clearInterval(taskPoll);
  });

  async function loadCatalog(): Promise<void> {
    view = "loading";
    errorMessage = "";
    try {
      // 页面重挂载恢复:有进行中的安装任务时直接回到排队视图,
      // 任务本体在服务端持续运行,不因切换页面"消失"。
      const active = (await runtime.getInstallTasks())
        .filter((candidate) => ["queued", "running", "committing"].includes(candidate.state))
        .sort((a, b) => b.updatedAtUnixSeconds - a.updatedAtUnixSeconds)[0];
      if (active) {
        task = active;
        view = "queued";
        startTaskPolling();
        return;
      }
      catalog = await runtime.getGameVersionCatalog();
      const recommended = recommendedVersion(catalog.versions);
      if (!recommended) throw new Error(t("install.error.noVersions"));
      selectedVersion = recommended;
      expandedMajors = new Set([majorOf(recommended.id)]);
      await loadLoaders(recommended, true);
      if (loader.kind === "fabric" || loader.kind === "quilt") {
        expandedLoader = loader.kind;
        void loadFabricApi(loader.kind);
      }
      view = "configure";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function importPack(): Promise<void> {
    packError = "";
    packDone = null;
    try {
      const path = await runtime.pickModpackFile();
      if (!path) return;
      packPreview = await runtime.importModpackPreview(path);
    } catch (error) {
      packError = error instanceof Error ? error.message : String(error);
    }
  }

  async function confirmPackInstall(): Promise<void> {
    if (!packPreview || packInstalling) return;
    packInstalling = true;
    packError = "";
    packProgress = null;
    try {
      packDone = await runtime.installModpack(packPreview.id);
    } catch (error) {
      packError = error instanceof Error ? error.message : String(error);
    } finally {
      packInstalling = false;
    }
  }

  async function selectVersion(version: GameVersionSummary): Promise<void> {
    selectedVersion = version;
    loader = { kind: "vanilla" };
    expandedLoader = null;
    resetFabricApi();
    fabricLoaders = [];
    quiltLoaders = [];
    forgeVersions = [];
    neoforgeVersions = [];
    updateGeneratedName();
    await loadLoaders(version, false);
  }

  async function loadLoaders(version: GameVersionSummary, selectRecommended: boolean): Promise<void> {
    await Promise.all([
      loadFabric(version, selectRecommended),
      loadQuilt(version),
      loadForge(version),
      loadNeoForge(version),
    ]);
  }

  async function loadFabric(version: GameVersionSummary, selectRecommended: boolean): Promise<void> {
    const requestSequence = ++loaderRequestSequence;
    loaderMessage = t("install.loader.querying");
    loaderMessageWarn = false;
    try {
      const compatibleLoaders = await runtime.getFabricLoaders(version.id);
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      fabricLoaders = compatibleLoaders;
      const recommended = recommendedFabricLoader(fabricLoaders);
      if (recommended && selectRecommended) {
        loader = { kind: "fabric", version: recommended.version };
      }
      loaderMessage = fabricLoaders.length === 0 ? t("install.loader.noneAvailable") : "";
      loaderMessageWarn = fabricLoaders.length === 0;
      updateGeneratedName();
    } catch (error) {
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      fabricLoaders = [];
      loaderMessage = t("install.loader.metadataUnavailable").replace("{error}", error instanceof Error ? error.message : String(error));
      loaderMessageWarn = true;
      updateGeneratedName();
    }
  }

  async function loadQuilt(version: GameVersionSummary): Promise<void> {
    try {
      const compatibleLoaders = await runtime.getQuiltLoaders(version.id);
      if (selectedVersion?.id !== version.id) return;
      quiltLoaders = compatibleLoaders;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      quiltLoaders = [];
    }
  }

  async function loadForge(version: GameVersionSummary): Promise<void> {
    try {
      const versions = await runtime.getForgeVersions(version.id);
      if (selectedVersion?.id !== version.id) return;
      forgeVersions = versions;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      forgeVersions = [];
    }
  }

  async function loadNeoForge(version: GameVersionSummary): Promise<void> {
    try {
      const versions = await runtime.getNeoForgeVersions(version.id);
      if (selectedVersion?.id !== version.id) return;
      neoforgeVersions = versions;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      neoforgeVersions = [];
    }
  }

  function selectVanilla(): void {
    loader = { kind: "vanilla" };
    expandedLoader = null;
    resetFabricApi();
    updateGeneratedName();
  }

  function selectFabric(): void {
    const recommended = recommendedFabricLoader(fabricLoaders);
    if (!recommended) return;
    loader = { kind: "fabric", version: recommended.version };
    updateGeneratedName();
    void loadFabricApi("fabric");
  }

  function selectQuilt(): void {
    const recommended = recommendedFabricLoader(quiltLoaders);
    if (!recommended) return;
    loader = { kind: "quilt", version: recommended.version };
    updateGeneratedName();
    void loadFabricApi("quilt");
  }

  function selectForge(): void {
    const recommended = recommendedFabricLoader(forgeVersions);
    if (!recommended) return;
    loader = { kind: "forge", version: recommended.version };
    updateGeneratedName();
  }

  function selectNeoForge(): void {
    const recommended = recommendedFabricLoader(neoforgeVersions);
    if (!recommended) return;
    loader = { kind: "neoforge", version: recommended.version };
    updateGeneratedName();
  }

  function selectFabricVersion(version: string): void {
    loader = { kind: "fabric", version };
    updateGeneratedName();
  }

  function selectQuiltVersion(version: string): void {
    loader = { kind: "quilt", version };
    updateGeneratedName();
  }

  function selectForgeVersion(version: string): void {
    loader = { kind: "forge", version };
    updateGeneratedName();
  }

  function selectNeoForgeVersion(version: string): void {
    loader = { kind: "neoforge", version };
    updateGeneratedName();
  }

  function currentLoaderVersion(): string | null {
    return loader.kind === "vanilla" ? null : loader.version;
  }

  /** 卡片主按钮：选中该加载器（未选时）并展开其版本列表、收起其他卡。 */
  function selectLoaderKind(kind: FoldLoaderKind): void {
    if (loader.kind !== kind) {
      if (kind === "forge") selectForge();
      else if (kind === "neoforge") selectNeoForge();
      else if (kind === "fabric") selectFabric();
      else selectQuilt();
    }
    expandedLoader = kind;
  }

  function toggleLoaderCard(kind: FoldLoaderKind): void {
    expandedLoader = expandedLoader === kind ? null : kind;
  }

  function selectLoaderVersion(kind: FoldLoaderKind, version: string): void {
    if (kind === "forge") selectForgeVersion(version);
    else if (kind === "neoforge") selectNeoForgeVersion(version);
    else if (kind === "fabric") selectFabricVersion(version);
    else selectQuiltVersion(version);
  }

  /** 卡片角落清除：回到不安装加载器的状态。 */
  function clearLoaderSelection(): void {
    loader = { kind: "vanilla" };
    resetFabricApi();
    updateGeneratedName();
  }

  /** 卡片副标题：已选版本 / 推荐版本与兼容说明 / 不可用。 */
  function loaderCardSub(kind: FoldLoaderKind, versions: FabricLoaderSummary[]): string {
    if (versions.length === 0) return t("install.loader.unavailable");
    if (loader.kind === kind) {
      return `${currentLoaderVersion() ?? ""} · ${t("install.loader.selectedNote")}`;
    }
    const recommended = recommendedFabricLoader(versions);
    return `${recommended?.version ?? ""} · ${t("install.loader.compatible").replace("{version}", selectedVersion?.id ?? "")}`;
  }

  function loaderAutoNote(): string {
    if (loader.kind === "vanilla") return "";
    return t("install.loader.autoNote")
      .replace("{loader}", LOADER_DISPLAY[loader.kind])
      .replace("{version}", loader.version);
  }

  async function loadFabricApi(kind: "fabric" | "quilt"): Promise<void> {
    const version = selectedVersion;
    if (!version) return;
    const requestSequence = ++fabricApiRequestSequence;
    fabricApiLoading = true;
    fabricApiVersions = [];
    fabricApiVersionId = "";
    fabricApiEnabled = false;
    try {
      const versions = await runtime.listModrinthVersions(FABRIC_API_PROJECT_ID, version.id, kind);
      if (requestSequence !== fabricApiRequestSequence || loader.kind !== kind || selectedVersion?.id !== version.id) return;
      fabricApiVersions = versions;
      fabricApiVersionId = versions[0]?.id ?? "";
      fabricApiEnabled = kind === "fabric" && versions.length > 0;
    } catch {
      if (requestSequence !== fabricApiRequestSequence) return;
      fabricApiVersions = [];
    } finally {
      if (requestSequence === fabricApiRequestSequence) fabricApiLoading = false;
    }
  }

  function resetFabricApi(): void {
    fabricApiRequestSequence += 1;
    fabricApiLoading = false;
    fabricApiVersions = [];
    fabricApiVersionId = "";
    fabricApiEnabled = false;
    fabricApiInstall = "idle";
    fabricApiInstallError = "";
    fabricApiInstallTriggered = false;
  }

  function fabricApiSummary(): string {
    if (fabricApiLoading) return t("install.fabricApi.loading");
    const selected = fabricApiVersions.find((candidate) => candidate.id === fabricApiVersionId);
    return selected?.versionNumber ?? t("install.fabricApi.none");
  }

  function updateGeneratedName(): void {
    if (!nameEdited && selectedVersion) {
      instanceName = defaultInstanceName(selectedVersion.id, loader);
    }
  }

  async function createPreview(): Promise<void> {
    if (!selectedVersion || instanceName.trim() === "") return;
    view = "previewing";
    errorMessage = "";
    try {
      preview = await runtime.previewInstall({
        instanceName: instanceName.trim(),
        gameVersion: selectedVersion,
        loader,
        isolation: "full",
      });
      view = "confirm";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function confirmInstall(): Promise<void> {
    if (!preview) return;
    view = "queueing";
    errorMessage = "";
    fabricApiInstall = "idle";
    fabricApiInstallError = "";
    fabricApiInstallTriggered = false;
    try {
      task = await runtime.confirmInstallPreview(preview.id);
      view = "queued";
      startTaskPolling();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "confirm";
    }
  }

  function startTaskPolling(): void {
    if (taskPoll) clearInterval(taskPoll);
    taskPoll = setInterval(() => void refreshCurrentTask(), 400);
  }

  async function refreshCurrentTask(): Promise<void> {
    if (!task || taskPollRunning) return;
    taskPollRunning = true;
    try {
      const refreshed = (await runtime.getInstallTasks()).find((candidate) => candidate.id === task?.id);
      if (refreshed) task = refreshed;
      if (refreshed && ["completed", "failed", "cancelled"].includes(refreshed.state)) {
        if (taskPoll) clearInterval(taskPoll);
        taskPoll = undefined;
        if (refreshed.state === "completed") void installFabricApiAddon(refreshed);
      }
    } finally {
      taskPollRunning = false;
    }
  }

  /** 安装任务完成后附带安装 Fabric API；失败不阻塞实例，仅给出可重试提示。 */
  async function installFabricApiAddon(completedTask: InstallTask): Promise<void> {
    if (fabricApiInstallTriggered || !fabricApiEnabled) return;
    if (loader.kind !== "fabric" && loader.kind !== "quilt") return;
    fabricApiInstallTriggered = true;
    fabricApiInstall = "installing";
    fabricApiInstallError = "";
    try {
      await runtime.installOnlineResource(
        completedTask.plan.instanceId,
        "mod",
        FABRIC_API_PROJECT_ID,
        fabricApiVersionId || undefined,
      );
      fabricApiInstall = "done";
    } catch (error) {
      fabricApiInstall = "failed";
      fabricApiInstallError = error instanceof Error ? error.message : String(error);
    }
  }

  function taskStateLabel(current: InstallTask): string {
    if (current.state === "completed") return t("install.taskState.completed");
    if (current.state === "failed") return t("install.taskState.failed");
    if (current.state === "committing") return t("install.taskState.committing");
    if (current.state === "running") return t("install.taskState.running");
    return t("install.taskState.waiting");
  }

  function taskProgressPercent(current: InstallTask): number | null {
    const total = current.progress.totalBytes;
    if (!total || total <= 0) return null;
    return Math.min(100, (current.progress.completedBytes / total) * 100);
  }

  function returnToConfiguration(): void {
    preview = null;
    view = "configure";
  }

  function releaseDescription(version: GameVersionSummary): string {
    const date = new Date(version.releaseTime);
    const releaseDate = Number.isNaN(date.valueOf())
      ? t("install.version.unknownDate")
      : new Intl.DateTimeFormat(uiLanguage(), { dateStyle: "medium" }).format(date);
    return t("install.version.releaseLine").replace("{date}", releaseDate) + (version.recommended ? t("install.version.stableSuffix") : "");
  }
</script>

<AppShell
  pageTitle={view === "confirm" || view === "queueing" ? t("install.pageTitle.confirm") : view === "queued" ? t("install.pageTitle.queued") : t("install.pageTitle.configure")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="instances"
  connectionStatus={catalog?.source === "cache" ? t("install.connection.cache") : t("install.connection.online")}
  taskStatus={task ? t("install.taskStatus.active").replace("{state}", taskStateLabel(task)) : t("shell.status.noTasks")}
  taskCount={task && !["completed", "failed", "cancelled"].includes(task.state) ? 1 : 0}
  {onBack}
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content install-content" bind:this={pageRoot}>
    {#if view === "loading"}
      <div class="install-center" aria-live="polite">
        <Fish variant="tank" message={t("install.loading.title")} />
      </div>
    {:else if view === "configure"}
      <div class="install-col">
        <h1 class="install-page-title">{t("home.empty.installFirst")}</h1>

        <section class="panel pad" aria-labelledby="modpack-import-heading">
          <div class="pack-head">
            <div>
              <h2 class="panel-title" id="modpack-import-heading">{t("modpack.heading")}</h2>
              <div class="panel-desc">{t("modpack.description")}</div>
            </div>
            {#if !packPreview && !packDone}
              <button class="btn secondary small" onclick={() => void importPack()}>{t("modpack.import")}</button>
            {/if}
          </div>
          {#if packError}
            <div class="banner danger" role="alert"><strong>{t("modpack.errorTitle")}</strong><span>{packError}</span></div>
          {/if}
          {#if packInstalling && packProgress}
            {@const packPercent = packProgress.total > 0 ? Math.min(100, (packProgress.current / packProgress.total) * 100) : null}
            <div class="pack-progress" role="status">
              <span class="dim">{packProgress.stage === "game" ? t("modpack.stageGame") : t("modpack.stageFiles")} {packProgress.current}/{packProgress.total} · {packProgress.item}</span>
              <div class="progress" class:indet={packPercent === null}>{#if packPercent !== null}<i style="width:{packPercent}%"></i>{:else}<i></i>{/if}</div>
            </div>
          {/if}
          {#if packDone}
            <div class="banner info pack-done" role="status">
              <strong>{t("modpack.doneTitle")}</strong>
              <span>{t("modpack.doneBody").replace("{name}", packDone.packName).replace("{version}", packDone.packVersion).replace("{count}", String(packDone.installedFiles))}</span>
            </div>
          {:else if packPreview}
            <div class="modpack-preview">
              <div class="row">
                <h3>{packPreview.preview.name}</h3>
                <span class="tag neutral">{packPreview.preview.provider === "modrinth" ? "Modrinth" : "CurseForge"}</span>
              </div>
              <p class="muted">{t("modpack.previewLine")
                .replace("{version}", packPreview.preview.version)
                .replace("{game}", packPreview.preview.gameVersion)
                .replace("{loader}", packPreview.preview.loaderKind)
                .replace("{loaderVersion}", packPreview.preview.loaderVersion)
                .replace("{count}", String(packPreview.preview.fileCount))
                .replace("{size}", formatBytes(packPreview.preview.totalBytes))}</p>
              <div class="row">
                <button class="btn primary small" disabled={packInstalling} onclick={() => void confirmPackInstall()}>
                  {packInstalling ? t("modpack.installing") : t("modpack.confirmInstall")}
                </button>
                <button class="btn ghost small" disabled={packInstalling} onclick={() => { packPreview = null; }}>{t("common.cancel")}</button>
              </div>
            </div>
          {/if}
        </section>

        {#if catalog?.source === "cache"}
          <div class="banner info" role="status">
            <Icon name="info" size={16} />
            <span>{t("install.cacheBanner")}</span>
          </div>
        {/if}
        {#if errorMessage}
          <div class="banner danger" role="alert">
            <strong>{t("install.error.title")}</strong>
            <span>{t("install.error.body")} {errorMessage}</span>
            <button class="btn ghost small b-act" onclick={() => void loadCatalog()}>{t("install.error.retry")}</button>
          </div>
        {/if}

        {#if selectedVersion}
          <section class="panel pad" aria-labelledby="gi-version-heading">
            <div class="step">
              <span class="step-no" aria-hidden="true">1</span>
              <div class="step-body">
                <div>
                  <h2 class="panel-title" id="gi-version-heading">{t("install.version.stepTitle")}</h2>
                  <div class="panel-desc">{t("install.version.stepDesc")}</div>
                </div>
                {#if selectedVersion.releaseType === "snapshot"}
                  <div class="banner warn" role="status">
                    <Icon name="info" size={14} />
                    <span>{t("install.hint.snapshot")}</span>
                  </div>
                {:else if selectedVersion.releaseType === "oldBeta" || selectedVersion.releaseType === "oldAlpha"}
                  <div class="banner warn" role="status">
                    <Icon name="info" size={14} />
                    <span>{t("install.hint.oldVersion")}</span>
                  </div>
                {/if}
                {#if latestReleaseVersion}
                  <div class="ver-latest" role="group" aria-label={t("install.version.latestHeading")}>
                    <button
                      class="install-choice-row ver-row"
                      class:sel={selectedVersion.id === latestReleaseVersion.id}
                      role="radio"
                      aria-checked={selectedVersion.id === latestReleaseVersion.id}
                      onclick={() => void selectVersion(latestReleaseVersion)}
                    >
                      <span class="pick-dot" aria-hidden="true"></span>
                      <span class="choice-copy">
                        <strong>Minecraft {latestReleaseVersion.id}<em>{t("install.version.latestReleaseTag")}</em>{#if latestReleaseVersion.recommended}<em>{t("install.version.recommended")}</em>{/if}</strong>
                        <small>{releaseDescription(latestReleaseVersion)}</small>
                      </span>
                    </button>
                    {#if showLatestSnapshot && latestSnapshotVersion}
                      <button
                        class="install-choice-row ver-row"
                        class:sel={selectedVersion.id === latestSnapshotVersion.id}
                        role="radio"
                        aria-checked={selectedVersion.id === latestSnapshotVersion.id}
                        onclick={() => void selectVersion(latestSnapshotVersion)}
                      >
                        <span class="pick-dot" aria-hidden="true"></span>
                        <span class="choice-copy">
                          <strong>Minecraft {latestSnapshotVersion.id}<em>{t("install.version.latestSnapshotTag")}</em></strong>
                          <small>{releaseDescription(latestSnapshotVersion)}</small>
                        </span>
                      </button>
                    {/if}
                  </div>
                {/if}
                <div class="ver-groups" role="radiogroup" aria-label={t("install.version.heading")}>
                  {#each releaseGroups as group}
                    <div class="ver-group">
                      <button
                        class="ver-group-head"
                        aria-expanded={expandedMajors.has(group.major)}
                        onclick={() => toggleMajor(group.major)}
                      >
                        <span class="chev" class:open={expandedMajors.has(group.major)} aria-hidden="true"></span>
                        <strong>Minecraft {group.major}</strong>
                        <small>{t("install.version.groupCount").replace("{count}", String(group.versions.length))}</small>
                      </button>
                      {#if expandedMajors.has(group.major)}
                        <div class="ver-rows">
                          {#each group.versions as version}
                            <button
                              class="install-choice-row ver-row"
                              class:sel={selectedVersion.id === version.id}
                              role="radio"
                              aria-checked={selectedVersion.id === version.id}
                              onclick={() => void selectVersion(version)}
                            >
                              <span class="pick-dot" aria-hidden="true"></span>
                              <span class="choice-copy">
                                <strong>Minecraft {version.id}{#if version.recommended}<em>{t("install.version.recommended")}</em>{/if}</strong>
                                <small>{releaseDescription(version)}</small>
                              </span>
                            </button>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/each}
                  {#if snapshotVersions.length > 0}
                    <div class="ver-group">
                      <button
                        class="ver-group-head"
                        aria-expanded={showSnapshots}
                        onclick={() => { showSnapshots = !showSnapshots; }}
                      >
                        <span class="chev" class:open={showSnapshots} aria-hidden="true"></span>
                        <strong>{t("install.version.groupSnapshots")}</strong>
                        <small>{t("install.version.groupCount").replace("{count}", String(snapshotVersions.length))}</small>
                      </button>
                      {#if showSnapshots}
                        <div class="ver-rows">
                          {#each snapshotVersions as version}
                            <button
                              class="install-choice-row ver-row"
                              class:sel={selectedVersion.id === version.id}
                              role="radio"
                              aria-checked={selectedVersion.id === version.id}
                              onclick={() => void selectVersion(version)}
                            >
                              <span class="pick-dot" aria-hidden="true"></span>
                              <span class="choice-copy">
                                <strong>Minecraft {version.id}</strong>
                                <small>{releaseDescription(version)}</small>
                              </span>
                            </button>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/if}
                  {#if oldVersions.length > 0}
                    <div class="ver-group">
                      <button
                        class="ver-group-head"
                        aria-expanded={showOldVersions}
                        onclick={() => { showOldVersions = !showOldVersions; }}
                      >
                        <span class="chev" class:open={showOldVersions} aria-hidden="true"></span>
                        <strong>{t("install.version.groupOld")}</strong>
                        <small>{t("install.version.groupCount").replace("{count}", String(oldVersions.length))}</small>
                      </button>
                      {#if showOldVersions}
                        <div class="ver-rows">
                          {#each oldVersions as version}
                            <button
                              class="install-choice-row ver-row"
                              class:sel={selectedVersion.id === version.id}
                              role="radio"
                              aria-checked={selectedVersion.id === version.id}
                              onclick={() => void selectVersion(version)}
                            >
                              <span class="pick-dot" aria-hidden="true"></span>
                              <span class="choice-copy">
                                <strong>Minecraft {version.id}</strong>
                                <small>{releaseDescription(version)}</small>
                              </span>
                            </button>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/if}
                </div>
              </div>
            </div>
          </section>

          <section class="panel pad" aria-labelledby="gi-loader-heading">
            <div class="step">
              <span class="step-no" aria-hidden="true">2</span>
              <div class="step-body">
                <div>
                  <h2 class="panel-title" id="gi-loader-heading">{t("install.loader.heading")}</h2>
                  <div class="panel-desc">{t("install.loader.stepDesc").replace("{version}", selectedVersion.id)}</div>
                </div>
                <div class="loader-grid" role="radiogroup" aria-label={t("install.loader.groupAria")}>
                  <div class="loader-card" class:sel={loader.kind === "vanilla"}>
                    <button
                      class="lc-main"
                      role="radio"
                      aria-checked={loader.kind === "vanilla"}
                      onclick={selectVanilla}
                    >
                      <span class="lc-name">{t("install.loader.none")}</span>
                      <span class="lc-sub">{t("install.loader.vanillaSub")}</span>
                    </button>
                  </div>
                  {@render loaderCard("fabric", "Fabric", fabricLoaders, t("install.loader.fabricField"))}
                  {@render loaderCard("forge", "Forge", forgeVersions, t("install.loader.forgeField"))}
                  {@render loaderCard("neoforge", "NeoForge", neoforgeVersions, t("install.loader.neoforgeField"))}
                  {@render loaderCard("quilt", "Quilt", quiltLoaders, t("install.loader.quiltField"))}
                </div>
                {#if expandedLoaderMeta && expandedLoaderMeta.versions.length > 0}
                  <div class="ver-rows loader-versions" role="radiogroup" aria-label={expandedLoaderMeta.label}>
                    {#each expandedLoaderMeta.versions as candidate}
                      <button
                        class="install-choice-row ver-row"
                        class:sel={loader.kind === expandedLoaderMeta.kind && currentLoaderVersion() === candidate.version}
                        role="radio"
                        aria-checked={loader.kind === expandedLoaderMeta.kind && currentLoaderVersion() === candidate.version}
                        onclick={() => selectLoaderVersion(expandedLoaderMeta!.kind, candidate.version)}
                      >
                        <span class="pick-dot" aria-hidden="true"></span>
                        <span class="choice-copy">
                          <strong>{candidate.version}{#if candidate.recommended}<em>{t("install.loader.recommendedTag")}</em>{/if}</strong>
                        </span>
                      </button>
                    {/each}
                  </div>
                {/if}
                {#if loader.kind === "fabric" || loader.kind === "quilt"}
                  <div class="fapi">
                    {#if !fabricApiEnabled}
                      <div class="banner" class:warn={loader.kind === "fabric"} class:info={loader.kind === "quilt"} role="alert">
                        <Icon name="info" size={14} />
                        <span>{loader.kind === "fabric" ? t("install.fabricApi.warning") : t("install.fabricApi.quiltHint")}</span>
                      </div>
                    {/if}
                    <div class="fapi-card">
                      <div class="fapi-head">
                        <strong>Fabric API</strong>
                        <small class="dim">{fabricApiSummary()}</small>
                        <label class="fapi-toggle">
                          <input
                            type="checkbox"
                            checked={fabricApiEnabled}
                            disabled={fabricApiLoading || fabricApiVersions.length === 0}
                            onchange={(event) => { fabricApiEnabled = event.currentTarget.checked; }}
                          />
                          {t("install.fabricApi.enable")}
                        </label>
                      </div>
                      {#if fabricApiEnabled && fabricApiVersions.length > 0}
                        <div class="ver-rows fapi-versions" role="radiogroup" aria-label="Fabric API">
                          {#each fabricApiVersions as candidate}
                            <button
                              class="install-choice-row ver-row"
                              class:sel={fabricApiVersionId === candidate.id}
                              role="radio"
                              aria-checked={fabricApiVersionId === candidate.id}
                              onclick={() => { fabricApiVersionId = candidate.id; }}
                            >
                              <span class="pick-dot" aria-hidden="true"></span>
                              <span class="choice-copy">
                                <strong>{candidate.versionNumber}</strong>
                                <small>{candidate.versionType}</small>
                              </span>
                            </button>
                          {/each}
                        </div>
                      {:else if !fabricApiLoading && fabricApiVersions.length === 0}
                        <p class="dim fabric-api-note">{t("install.fabricApi.none")}</p>
                      {/if}
                    </div>
                  </div>
                {/if}
                {#if loaderMessage}
                  {#if loaderMessageWarn}
                    <div class="banner warn" role="alert"><Icon name="info" size={14} /><span>{loaderMessage}</span></div>
                  {:else}
                    <p class="dim" role="status">{loaderMessage}</p>
                  {/if}
                {/if}
                {#if loaderAutoNote()}
                  <p class="dim">{loaderAutoNote()}</p>
                {/if}
              </div>
            </div>
          </section>

          {#snippet loaderCard(kind: FoldLoaderKind, name: string, versions: FabricLoaderSummary[], fieldLabel: string)}
            {@const selected = loader.kind === kind}
            {@const available = versions.length > 0}
            <div class="loader-card" class:sel={selected} class:off={!available}>
              <button
                class="lc-main"
                role="radio"
                aria-checked={selected}
                disabled={!available}
                onclick={() => selectLoaderKind(kind)}
              >
                <span class="lc-name">{name}</span>
                <span class="lc-sub">{loaderCardSub(kind, versions)}</span>
              </button>
              <span class="lc-acts">
                {#if selected}
                  <button class="lc-clear" aria-label={t("install.loader.clear")} title={t("install.loader.clear")} onclick={clearLoaderSelection}>×</button>
                {/if}
                <button
                  class="lc-expand"
                  aria-label={fieldLabel}
                  aria-expanded={expandedLoader === kind}
                  disabled={!available}
                  onclick={() => toggleLoaderCard(kind)}
                >
                  <span class="chev" class:open={expandedLoader === kind} aria-hidden="true"></span>
                </button>
              </span>
            </div>
          {/snippet}

          <section class="panel pad" aria-labelledby="gi-name-heading">
            <div class="step">
              <span class="step-no" aria-hidden="true">3</span>
              <div class="step-body">
                <div>
                  <h2 class="panel-title" id="gi-name-heading">{t("install.name.heading")}</h2>
                  <div class="panel-desc">{t("install.name.stepDesc")}</div>
                </div>
                <div class="install-form-card">
                  <div class="field">
                    <label for="gi-instance-name">{t("install.name.label")}</label>
                    <input
                      id="gi-instance-name"
                      class="input"
                      value={instanceName}
                      maxlength="120"
                      oninput={(event) => {
                        nameEdited = true;
                        instanceName = event.currentTarget.value;
                      }}
                    />
                    <span class="help">{t("install.name.hint")}</span>
                  </div>
                  <div class="field">
                    <label for="gi-instance-location">{t("install.name.locationLabel")}</label>
                    <input
                      id="gi-instance-location"
                      class="input mono"
                      readonly
                      value={`${settings.dataDirectory}\\instances\\<${t("install.name.instanceId")}>`}
                    />
                    <span class="help">{t("install.name.locationHint")}</span>
                  </div>
                </div>
              </div>
            </div>
          </section>

          <details class="adv">
            <summary>{t("install.adv.summary")}</summary>
            <div class="adv-body col" style="gap:14px">
              <div class="field">
                <label for="gi-data-dir">{t("install.adv.dataDirLabel")}</label>
                <input id="gi-data-dir" class="input mono" readonly value={settings.dataDirectory} />
                <span class="help">{t("install.adv.dataDirHelp")}</span>
              </div>
              <div class="set-row" style="padding:10px 4px">
                <div class="sr-main">
                  <div class="sr-name">{t("install.adv.isolationName")}</div>
                  <div class="sr-desc">{t("install.adv.isolationDesc")}</div>
                </div>
                <span class="tag accent">{t("install.confirm.isolationValue")}</span>
              </div>
            </div>
          </details>

          <div class="footer-bar">
            <button class="btn primary large" disabled={instanceName.trim() === ""} onclick={() => void createPreview()}>
              {t("install.action.preview")}
            </button>
            <button class="btn ghost" onclick={onBack}>{t("common.cancel")}</button>
          </div>
          <p class="dim">{t("install.action.previewHint")}</p>
        {/if}
      </div>
    {:else if view === "previewing"}
      <div class="install-center" aria-live="polite">
        <Fish variant="tank" message={t("install.previewing.title")} />
        <span class="dim">{t("install.previewing.description")}</span>
      </div>
    {:else if (view === "confirm" || view === "queueing") && preview}
      <div class="install-col">
        <div class="confirm-head">
          <button class="btn ghost small" disabled={view === "queueing"} onclick={returnToConfiguration}>{t("install.confirm.backEdit")}</button>
          <h1 class="install-page-title">{t("install.confirm.heading")}</h1>
        </div>
        {#if errorMessage}
          <div class="banner danger" role="alert"><strong>{t("install.confirm.errorTitle")}</strong><span>{errorMessage}</span></div>
        {/if}
        <dl class="install-summary panel">
          <div><dt>{t("install.confirm.versionLabel")}</dt><dd>{preview.gameVersion}</dd><span class="dim">{t("install.confirm.versionNote")}</span></div>
          <div><dt>{t("install.confirm.loaderLabel")}</dt><dd>{preview.loaderName}{preview.loaderVersion ? ` ${preview.loaderVersion}` : ""}</dd><span class="dim">{t("install.confirm.loaderNote")}</span></div>
          <div><dt>Java</dt><dd>Azul Zulu {preview.javaVersion} · {preview.javaArchitecture}</dd><span class="dim">{t("install.confirm.javaNote")}</span></div>
          <div><dt>{t("install.confirm.isolationLabel")}</dt><dd>{t("install.confirm.isolationValue")}</dd><span class="dim">{t("install.confirm.isolationNote")}</span></div>
          <div><dt>{t("install.confirm.downloadLabel")}</dt><dd>{formatBytes(preview.estimatedDownloadBytes)}</dd><span class="dim">{t("install.confirm.downloadNote")}</span></div>
          {#if fabricApiEnabled && (loader.kind === "fabric" || loader.kind === "quilt")}
            <div><dt>Fabric API</dt><dd>{fabricApiSummary()}</dd><span class="dim">{t("install.confirm.fabricApiNote")}</span></div>
          {/if}
        </dl>
        <div class="stage-preview panel pad">
          <h2 class="panel-title">{t("install.confirm.stagesTitle")}</h2>
          <ol>
            {#each ["prepare", "downloadGameFiles", "verifyFiles", "installGameEnvironment", "applyLoader", "commitChanges", "createRollbackPoint"] as stage}
              <li>{installStageLabel(stage as import("../runtime").InstallStage)}</li>
            {/each}
          </ol>
        </div>
        <div class="footer-bar">
          <button class="btn primary large" data-autofocus="true" disabled={view === "queueing"} onclick={() => void confirmInstall()}>
            {view === "queueing" ? t("install.confirm.creating") : t("install.confirm.start")}
          </button>
        </div>
        <p class="dim">{t("install.confirm.stagingHint")}</p>
      </div>
    {:else if view === "queued" && task}
      <section class="queued-result" aria-live="polite">
        <span class="done-mark"><Icon name={task.state === "completed" ? "check" : "task"} size={18} /></span>
        <h1>{task.state === "completed" ? t("install.queued.titleCompleted") : task.state === "failed" ? t("install.queued.titleFailed") : task.state === "queued" ? t("install.queued.titleQueued") : t("install.queued.titleRunning")}</h1>
        <p class="queued-lead">{task.state === "completed" ? t("install.queued.bodyCompleted") : task.state === "failed" ? t("install.queued.bodyFailed") : t("install.queued.bodyDefault")}</p>
        <div class="queued-task-card">
          <div class="q-card-head">
            <strong>{task.plan.instanceName}</strong>
            <span class="tag info">{taskStateLabel(task)}</span>
          </div>
          <ol>
            {#each task.plan.stages as stage, index}
              <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{installStageLabel(stage)}</b></li>
            {/each}
          </ol>
          {#if task.state === "running" || task.state === "committing"}
            {@const percent = taskProgressPercent(task)}
            <div class="q-progress" aria-label={taskProgressAriaLabel(task.progress)}>
              <div class="progress" class:indet={percent === null}>{#if percent !== null}<i style="width:{percent}%"></i>{:else}<i></i>{/if}</div>
              <small class="dim">{task.progress.currentItem ?? t("tasks.progress.processing")}</small>
            </div>
          {:else if task.state === "failed"}
            <div class="banner danger task-error" role="alert"><strong>{t("install.queued.failedTitle")}</strong><span>{task.progress.errorSummary ?? t("install.queued.failedHint")}</span></div>
          {/if}
          <small class="dim q-staging">{t("install.queued.staging")}<code>{task.stagingDirectory}</code></small>
        </div>
        {#if fabricApiInstall !== "idle"}
          <div class="fabric-api-result">
            {#if fabricApiInstall === "installing"}
              <p class="fabric-api-status" role="status">{t("install.fabricApi.installing")}</p>
            {:else if fabricApiInstall === "done"}
              <p class="fabric-api-status done" role="status">{t("install.fabricApi.done")}</p>
            {:else}
              <div class="banner warn" role="alert">
                <Icon name="info" size={14} />
                <span>{t("install.fabricApi.failed").replace("{error}", fabricApiInstallError)}</span>
              </div>
            {/if}
          </div>
        {/if}
        <div class="row">
          <button class="btn primary" data-autofocus="true" onclick={onBack}>{t("settings.back")}</button>
          <button class="btn ghost" onclick={() => onNavigate("tasks")}>{t("install.queued.openTasks")}</button>
        </div>
      </section>
    {/if}
  </main>
</AppShell>

<style>
  .install-content {
    padding: 24px 28px 40px;
    overflow: hidden auto;
  }
  .install-center {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }
  .install-col {
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .install-page-title {
    margin: 0;
    font-size: 20px;
    letter-spacing: -0.01em;
  }
  .confirm-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  /* 步骤卡（mockup 04：序号圆点 + 面板） */
  .step {
    display: flex;
    gap: 16px;
  }
  .step-no {
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    display: grid;
    place-items: center;
    font-size: 12.5px;
    font-weight: 700;
  }
  .step-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h2.panel-title {
    margin: 0;
  }

  /* 版本选择：最新卡 + 分组折叠 */
  .ver-latest {
    border: 1px solid var(--accent);
    border-radius: var(--r);
    background: var(--accent-soft);
    overflow: hidden;
  }
  .ver-groups {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ver-group-head {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 14px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ver-group-head:hover {
    background: var(--glass);
  }
  .ver-group-head small {
    margin-left: auto;
    color: var(--text-3);
    font-size: 12px;
  }
  .chev {
    width: 8px;
    height: 8px;
    flex: none;
    border-right: 2px solid var(--text-3);
    border-bottom: 2px solid var(--text-3);
    transform: rotate(-45deg);
    transition: transform 120ms ease;
  }
  .chev.open {
    transform: rotate(45deg);
  }
  .ver-rows {
    margin-top: 6px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    overflow: hidden;
  }
  .ver-rows .install-choice-row {
    border-radius: 0;
  }
  .install-choice-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .install-choice-row:hover {
    background: var(--glass);
  }
  .install-choice-row.sel {
    background: var(--accent-soft);
  }
  .ver-latest .install-choice-row + .install-choice-row,
  .ver-rows .install-choice-row + .install-choice-row {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .pick-dot {
    width: 16px;
    height: 16px;
    flex: none;
    border-radius: 50%;
    border: 1.5px solid var(--glass-highlight);
    position: relative;
  }
  .pick-dot::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 50%;
  }
  .install-choice-row.sel .pick-dot {
    border-color: var(--accent);
  }
  .install-choice-row.sel .pick-dot::after {
    background: var(--accent);
  }
  .choice-copy {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .choice-copy strong {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 13.5px;
    font-weight: 600;
  }
  .choice-copy strong em {
    font-style: normal;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
    border-radius: 999px;
    padding: 2px 8px;
  }
  .choice-copy small {
    color: var(--text-2);
    font-size: 12px;
  }

  /* 加载器卡片网格（mockup 04） */
  .loader-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(118px, 1fr));
    gap: 10px;
  }
  .loader-card {
    position: relative;
    display: block;
    padding: 0;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    overflow: hidden;
    color: var(--text-1);
    font: inherit;
    text-align: left;
  }
  .loader-card:hover {
    background: var(--glass);
  }
  .loader-card.sel {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .loader-card.off {
    opacity: 0.55;
  }
  .lc-main {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 34px 12px 14px;
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .lc-main:disabled {
    cursor: not-allowed;
  }
  .lc-name {
    font-size: 13.5px;
    font-weight: 600;
  }
  .lc-sub {
    font-size: 11.5px;
    color: var(--text-2);
    overflow-wrap: break-word;
  }
  .loader-card.sel .lc-sub {
    color: var(--accent);
  }
  .lc-acts {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    gap: 4px;
  }
  .lc-clear,
  .lc-expand {
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--r);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    padding: 0;
    font-size: 13px;
  }
  .lc-clear:hover,
  .lc-expand:hover {
    background: var(--glass-strong);
    color: var(--text-1);
  }
  .lc-expand:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .loader-versions {
    max-height: 224px;
    overflow-y: auto;
  }

  /* Fabric API 附带安装 */
  .fapi {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .fapi-card {
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    overflow: hidden;
  }
  .fapi-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    padding: 12px 14px;
  }
  .fapi-head strong {
    font-size: 13.5px;
  }
  .fapi-toggle {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-size: 12.5px;
    cursor: pointer;
  }
  .fapi-versions {
    margin-top: 0;
    border: 0;
    border-top: 1px solid var(--glass-border);
    border-radius: 0;
    max-height: 224px;
    overflow-y: auto;
  }
  .fabric-api-note {
    margin: 0;
    padding: 0 14px 12px;
  }

  /* 名称与位置内卡（padding 为 e2e 可读内边距基线） */
  .install-form-card {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px 24px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
  }
  /* 抵消旧全局 .install-form-card input 规则,回到 moyu 输入框样式 */
  .install-form-card .input {
    width: 100%;
    height: 36px;
    padding: 0 12px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.22);
    color: var(--text-1);
  }

  .footer-bar {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  /* 整合包导入 */
  .pack-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .pack-head .panel-title {
    margin-bottom: 4px;
  }
  .pack-progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 10px;
  }
  .pack-done {
    margin-top: 10px;
  }
  .modpack-preview {
    margin-top: 10px;
    padding: 16px 20px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .modpack-preview h3 {
    margin: 0;
    font-size: 14px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .modpack-preview .row {
    flex-wrap: wrap;
  }
  .modpack-preview p {
    margin: 0;
    font-size: 12.5px;
  }

  /* 确认页摘要与阶段（padding 为 e2e 可读内边距基线） */
  .install-summary {
    margin: 0;
    overflow: hidden;
  }
  .install-summary > div {
    display: grid;
    grid-template-columns: minmax(96px, auto) 1fr auto;
    gap: 4px 14px;
    align-items: baseline;
    padding: 16px 20px;
    border: 0;
  }
  .install-summary > div + div {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .install-summary dt {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .install-summary dd {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .stage-preview {
    margin-top: 0;
  }
  .stage-preview ol {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .stage-preview li {
    padding: 14px 16px;
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    color: var(--text-2);
    font-size: 13px;
  }

  /* 入队结果（padding 为 e2e 可读内边距基线） */
  .queued-result {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    overflow-y: auto;
    text-align: center;
  }
  .queued-result h1 {
    margin: 0;
    font-size: 22px;
    letter-spacing: -0.02em;
  }
  .queued-lead {
    margin: 0;
    color: var(--text-2);
    max-width: 52ch;
  }
  .done-mark {
    width: 34px;
    height: 34px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .queued-task-card {
    width: min(760px, 100%);
    margin: 10px 0;
    padding: 20px 24px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: var(--glass);
    text-align: left;
  }
  .q-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 0;
  }
  .queued-task-card ol {
    list-style: none;
    margin: 14px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  /* 显式覆盖旧全局 .queued-task-card li 的纵向居中布局 */
  .queued-task-card ol li {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    border-radius: var(--r);
    color: var(--text-2);
    font-size: 13px;
    text-align: left;
  }
  .queued-task-card ol li.current {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .queued-task-card ol li span {
    width: 20px;
    height: 20px;
    flex: none;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: var(--glass-strong);
    color: inherit;
    font-size: 11px;
  }
  .queued-task-card ol li b {
    font-weight: 600;
  }
  .q-progress {
    margin-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .task-error {
    margin-top: 14px;
  }
  .q-staging {
    display: block;
    margin-top: 14px;
    overflow-wrap: anywhere;
  }
  .q-staging code {
    font-family: var(--mono);
    font-size: 11.5px;
  }
  .fabric-api-result {
    max-width: 52ch;
  }
  .fabric-api-status {
    margin: 0;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .fabric-api-status.done {
    color: var(--accent);
  }
</style>
