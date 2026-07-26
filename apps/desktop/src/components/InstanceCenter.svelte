<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import { shellAccount } from "../accounts.svelte";
  import type {
    InstanceResource,
    InstanceResourceKind,
    InstanceScreenshot,
    InstanceWorldInfo,
    InstalledContent,
    InstalledModpack,
    JavaEnvironment,
    LaunchSession,
    ManagedInstance,
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
    onExit,
    onStateChanged,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  type DetailTab = "overview" | "setup" | "mods" | "saves" | "screenshots" | "resourcepacks" | "shaders";
  type ContentFilter = "all" | "enabled" | "disabled";

  const NAV_GROUPS: { groupKey: string; items: { key: DetailTab; labelKey: string }[] }[] = [
    {
      groupKey: "instanceDetail.nav.groupGame",
      items: [
        { key: "overview", labelKey: "instanceDetail.nav.overview" },
        { key: "setup", labelKey: "instanceDetail.nav.setup" },
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
      ],
    },
  ];

  const LOADER_DISPLAY: Record<string, string> = {
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };

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

  const instanceId = $derived(instance?.id ?? "");
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
    void loadDetail();
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
      const [pack, environments, options, auto, content, resourceList, worldList, shotList] =
        await Promise.all([
          runtime.getInstanceModpack(current.id),
          runtime.listJavaEnvironments(),
          runtime.getInstanceLaunchOptions(current.id),
          runtime.getInstanceContentAutoUpdate(current.id),
          runtime.getInstalledContent(current.id),
          runtime.listInstanceResources(current.id),
          runtime.listInstanceWorldDetails(current.id),
          runtime.listInstanceScreenshots(current.id),
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
    message = "";
    errorMessage = "";
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
