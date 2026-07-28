<script lang="ts">
  import { onMount, tick } from "svelte";

  import { shellAccount } from "../accounts.svelte";
  import { t } from "../i18n.svelte";
  import type {
    ContentInstallTask,
    CrashReport,
    InstallTask,
    InstalledModpack,
    LaunchSession,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Fish from "./Fish.svelte";
  import { pushToast } from "../toast.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    tasks: InstallTask[];
    contentTasks: ContentInstallTask[];
    instances: ManagedInstance[];
    launchSessions: LaunchSession[];
    crashReports: CrashReport[];
    notice: string;
    onInstall: () => void;
    onOpenTasks: () => void;
    onOpenCrash: (report: CrashReport) => void;
    onManageInstance: (instance: ManagedInstance, tab?: string) => void;
    onStateChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    tasks,
    contentTasks,
    instances,
    launchSessions,
    crashReports,
    notice,
    onInstall,
    onOpenTasks,
    onOpenCrash,
    onManageInstance,
    onStateChanged,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingInstance = $state<string | null>(null);
  let actionMessage = $state("");
  let actionError = $state("");
  let modpacks = $state<Record<string, InstalledModpack>>({});
  let packIcons = $state<Record<string, string>>({});
  let installingPacks = $state<Record<string, boolean>>({});

  // ---- 工具行与筛选 ----
  let query = $state("");
  let sortKey = $state<"recent" | "name">("recent");
  let loaderFilter = $state("");
  let healthFilter = $state("");

  // ---- 批量管理 ----
  let selectMode = $state(false);
  let selected = $state<string[]>([]);
  let batchBusy = $state(false);
  let deleteConfirmOpen = $state(false);
  let deleteDialog = $state<HTMLElement | null>(null);

  onMount(() => {
    void loadModpacks(instances);
  });

  $effect(() => {
    void loadModpacks(instances);
  });

  $effect(() => {
    if (actionError) pushToast({ tone: "danger", title: actionError });
  });
  $effect(() => {
    if (actionMessage) pushToast({ tone: "ok", title: actionMessage });
  });
  $effect(() => {
    if (notice) pushToast({ tone: "info", title: notice });
  });

  async function loadModpacks(list: ManagedInstance[]): Promise<void> {
    const next: Record<string, InstalledModpack> = {};
    const iconsNext: Record<string, string> = {};
    const installingNext: Record<string, boolean> = {};
    for (const instance of list) {
      const [pack, installing] = await Promise.all([
        runtime.getInstanceModpack(instance.id).catch(() => null),
        runtime.isModpackInstalling(instance.id).catch(() => false),
      ]);
      if (pack) {
        next[instance.id] = pack;
        const icon = await runtime.getModpackIconDataUrl(instance.id).catch(() => null);
        if (icon) iconsNext[instance.id] = icon;
      }
      if (installing) installingNext[instance.id] = true;
    }
    modpacks = next;
    packIcons = iconsNext;
    installingPacks = installingNext;
  }

  const activeTasks = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );
  const activeContentTasks = $derived(
    contentTasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );

  function activeSession(instanceId: string): LaunchSession | undefined {
    return launchSessions.find(
      (session) =>
        session.instanceId === instanceId &&
        ["starting", "running"].includes(session.state),
    );
  }

  function latestSession(instanceId: string): LaunchSession | undefined {
    return launchSessions.find((session) => session.instanceId === instanceId);
  }

  function crashReportFor(instanceId: string): CrashReport | undefined {
    const latest = latestSession(instanceId);
    if (!latest || !["failed", "interrupted"].includes(latest.state)) return undefined;
    return crashReports.find((report) => report.launchSessionId === latest.id);
  }

  function isMaintaining(instanceId: string): boolean {
    if (installingPacks[instanceId]) return true;
    if (
      activeTasks.some(
        (task) => task.plan.instanceId === instanceId && task.state !== "awaitingRecovery",
      )
    ) {
      return true;
    }
    return activeContentTasks.some(
      (task) => task.plan.instanceId === instanceId && task.state !== "awaitingRecovery",
    );
  }

  type Health = "ready" | "running" | "maintaining" | "attention";

  function healthOf(instance: ManagedInstance): Health {
    if (activeSession(instance.id)) return "running";
    if (isMaintaining(instance.id)) return "maintaining";
    if (crashReportFor(instance.id)) return "attention";
    return "ready";
  }

  const HEALTH_LABEL: Record<Health, { tag: string; label: string }> = {
    ready: { tag: "ok", label: "gallery.state.ready" },
    running: { tag: "accent", label: "gallery.state.running" },
    maintaining: { tag: "info", label: "gallery.state.maintaining" },
    attention: { tag: "warn", label: "gallery.state.attention" },
  };

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    let list = instances.filter((instance) => {
      if (needle && !instance.name.toLowerCase().includes(needle)) return false;
      if (loaderFilter && instance.loaderKind !== loaderFilter) return false;
      if (healthFilter && healthOf(instance) !== healthFilter) return false;
      return true;
    });
    if (sortKey === "name") {
      list = [...list].sort((a, b) => a.name.localeCompare(b.name, "zh-Hans-CN"));
    } else {
      list = [...list].sort((a, b) => {
        const at = latestSession(a.id)?.startedAtUnixSeconds ?? 0;
        const bt = latestSession(b.id)?.startedAtUnixSeconds ?? 0;
        return bt - at;
      });
    }
    return list;
  });

  function subInfo(instance: ManagedInstance): string {
    const active = activeSession(instance.id);
    if (active) {
      const minutes = Math.max(
        1,
        Math.floor((Date.now() / 1000 - active.startedAtUnixSeconds) / 60),
      );
      return t("gallery.runningFor").replace("{minutes}", String(minutes));
    }
    if (isMaintaining(instance.id)) {
      const task = activeTasks.find((candidate) => candidate.plan.instanceId === instance.id);
      const total = task?.progress.totalBytes;
      if (task && total && total > 0) {
        const pct = Math.min(
          100,
          Math.round((task.progress.completedBytes / total) * 100),
        );
        return t("gallery.installingPct").replace("{pct}", String(pct));
      }
      return t("gallery.installing");
    }
    const crash = crashReportFor(instance.id);
    if (crash) return t("gallery.crashedHint");
    const latest = latestSession(instance.id);
    const pack = modpacks[instance.id];
    const parts: string[] = [];
    if (latest) parts.push(relativeTime(latest.startedAtUnixSeconds));
    if (pack) parts.push(`${pack.packName} ${pack.packVersion}`);
    return parts.join(" · ") || `${instance.gameVersion}`;
  }

  function relativeTime(unixSeconds: string | number): string {
    const value = typeof unixSeconds === "string" ? Number(unixSeconds) : unixSeconds;
    const delta = Math.max(0, Date.now() / 1000 - value);
    if (delta < 3600) return t("gallery.time.minutesAgo").replace("{n}", String(Math.max(1, Math.floor(delta / 60))));
    if (delta < 86400) return t("gallery.time.hoursAgo").replace("{n}", String(Math.floor(delta / 3600)));
    return t("gallery.time.daysAgo").replace("{n}", String(Math.floor(delta / 86400)));
  }

  const LOADER_DISPLAY: Record<string, string> = {
    vanilla: "Vanilla",
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };

  function loaderName(kind: string): string {
    return LOADER_DISPLAY[kind] ?? kind;
  }

  async function start(instance: ManagedInstance): Promise<void> {
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.startInstance(instance.id);
      actionMessage = t("home.action.starting")
        .replace("{name}", instance.name)
        .replace("{identity}", identityLabel());
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = null;
    }
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

  async function stop(instance: ManagedInstance): Promise<void> {
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.stopInstance(instance.id);
      actionMessage = t("home.action.stopRequested").replace("{name}", instance.name);
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = null;
    }
  }

  // ---- 批量操作 ----
  function toggleSelectMode(): void {
    selectMode = !selectMode;
    selected = [];
  }

  function togglePick(instanceId: string): void {
    selected = selected.includes(instanceId)
      ? selected.filter((candidate) => candidate !== instanceId)
      : [...selected, instanceId];
  }

  const selectedInstances = $derived(
    instances.filter((instance) => selected.includes(instance.id)),
  );

  /** 批量更新:逐实例检查内容更新并直接入队更新任务,汇总结果。 */
  async function batchUpdate(): Promise<void> {
    batchBusy = true;
    actionMessage = "";
    actionError = "";
    let queuedCount = 0;
    let updatesCount = 0;
    const failures: string[] = [];
    for (const instance of selectedInstances) {
      if (instance.loaderKind === "vanilla" || healthOf(instance) !== "ready") continue;
      try {
        const updates = await runtime.checkContentUpdates(instance.id);
        if (updates.length === 0) continue;
        await runtime.planContentUpdate(instance.id, updates.map((update) => update.projectId));
        queuedCount += 1;
        updatesCount += updates.length;
      } catch {
        failures.push(instance.name);
      }
    }
    if (queuedCount > 0) {
      actionMessage = t("gallery.batch.updateQueued")
        .replace("{instances}", String(queuedCount))
        .replace("{updates}", String(updatesCount));
    } else if (failures.length === 0) {
      actionMessage = t("gallery.batch.allLatest");
    }
    if (failures.length > 0) {
      actionError = t("gallery.batch.updateFailed").replace("{names}", failures.join("、"));
    }
    selectMode = false;
    selected = [];
    batchBusy = false;
    await onStateChanged();
    if (queuedCount > 0) onOpenTasks();
  }

  async function openDeleteConfirm(): Promise<void> {
    deleteConfirmOpen = true;
    await tick();
    deleteDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  async function batchDelete(): Promise<void> {
    batchBusy = true;
    actionMessage = "";
    actionError = "";
    const failures: string[] = [];
    let recycled = 0;
    for (const instance of selectedInstances) {
      try {
        await runtime.recycleInstance(instance.id);
        recycled += 1;
      } catch {
        failures.push(instance.name);
      }
    }
    deleteConfirmOpen = false;
    selectMode = false;
    selected = [];
    batchBusy = false;
    if (recycled > 0) {
      actionMessage = t("gallery.batch.recycled").replace("{count}", String(recycled));
    }
    if (failures.length > 0) {
      actionError = t("gallery.batch.recycleFailed").replace("{names}", failures.join("、"));
    }
    await onStateChanged();
  }

  function handleDeleteDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !batchBusy) {
      event.preventDefault();
      deleteConfirmOpen = false;
    }
  }
</script>

<AppShell
  pageTitle={t("nav.instances")}
  activeNavigation="instances"
  taskCount={activeTasks.length + activeContentTasks.length}
  instanceCount={instances.length}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  {#if instances.length === 0}
    <main class="content" style="display:flex">
      <div class="empty-stage">
        <Fish variant="tank" />
        <h1 style="font-size:20px">{t("gallery.empty.title")}</h1>
        <p class="muted" style="max-width:44ch;text-align:center">{t("gallery.empty.description")}</p>
        <div class="row">
          <button class="btn primary large" data-autofocus="true" onclick={onInstall}>{t("gallery.empty.new")}</button>
          <button class="btn secondary large" disabled title={t("gallery.empty.importRoadmap")}>{t("gallery.empty.import")}</button>
        </div>
        <span class="dim">{t("gallery.empty.importRoadmap")}</span>
      </div>
    </main>
  {:else}
    <main class="content">
      <div class="toolbar">
        <input
          class="input"
          style="width:220px"
          placeholder={t("gallery.searchPlaceholder")}
          aria-label={t("gallery.searchAria")}
          bind:value={query}
        />
        <select class="input" bind:value={sortKey} aria-label={t("gallery.sortAria")}>
          <option value="recent">{t("gallery.sort.recent")}</option>
          <option value="name">{t("gallery.sort.name")}</option>
        </select>
        <select class="input" bind:value={loaderFilter} aria-label={t("gallery.loaderAria")}>
          <option value="">{t("gallery.loaderAll")}</option>
          <option value="vanilla">Vanilla</option>
          <option value="fabric">Fabric</option>
          <option value="quilt">Quilt</option>
          <option value="forge">Forge</option>
          <option value="neoforge">NeoForge</option>
        </select>
        <select class="input" bind:value={healthFilter} aria-label={t("gallery.healthAria")}>
          <option value="">{t("gallery.healthAll")}</option>
          <option value="ready">{t("gallery.state.ready")}</option>
          <option value="running">{t("gallery.state.running")}</option>
          <option value="maintaining">{t("gallery.state.maintaining")}</option>
          <option value="attention">{t("gallery.state.attention")}</option>
        </select>
        <span style="flex:1"></span>
        <button class="btn ghost" onclick={toggleSelectMode} aria-pressed={selectMode}>
          {selectMode ? t("gallery.batch.exit") : t("gallery.batch.enter")}
        </button>
        <button class="btn primary" onclick={onInstall}>{t("gallery.newInstance")}</button>
      </div>

      {#if filtered.length === 0}
        <p class="muted" role="status">{t("gallery.noMatch")}</p>
      {:else}
        <div class="inst-grid">
          {#each filtered as instance}
            {@const health = healthOf(instance)}
            {@const stateMeta = HEALTH_LABEL[health]}
            {@const crash = crashReportFor(instance.id)}
            <section
              class="panel inst-card"
              class:checked={selected.includes(instance.id)}
            >
              <button
                class="card-hit"
                aria-label={t("gallery.cardAria").replace("{name}", instance.name)}
                onclick={() => onManageInstance(instance)}
              ></button>
              <div class="row">
                {#if selectMode}
                  <button
                    class="pick"
                    class:on={selected.includes(instance.id)}
                    aria-label={t("gallery.pickAria").replace("{name}", instance.name)}
                    aria-pressed={selected.includes(instance.id)}
                    onclick={(event) => { event.stopPropagation(); togglePick(instance.id); }}
                  >{selected.includes(instance.id) ? "✓" : ""}</button>
                {/if}
                <div class="cube" aria-hidden="true">
                  {#if packIcons[instance.id]}<img src={packIcons[instance.id]} alt="" />{:else}{instance.name.slice(0, 1)}{/if}
                </div>
                <div class="lr-main">
                  <div class="lr-name">{instance.name}</div>
                  <div class="lr-sub">{instance.gameVersion} · {loaderName(instance.loaderKind)}</div>
                </div>
                <span class="tag {stateMeta.tag}"><span class="cdot"></span>{t(stateMeta.label)}</span>
              </div>
              <div class="row spread">
                <span class="dim">{subInfo(instance)}</span>
                {#if health === "running"}
                  <button
                    class="btn small danger-soft"
                    disabled={changingInstance === instance.id}
                    onclick={(event) => { event.stopPropagation(); void stop(instance); }}
                  >{changingInstance === instance.id ? t("home.launch.stopping") : t("gallery.action.stop")}</button>
                {:else if health === "maintaining"}
                  <button
                    class="btn small secondary"
                    onclick={(event) => { event.stopPropagation(); onOpenTasks(); }}
                  >{t("gallery.action.viewTasks")}</button>
                {:else if health === "attention" && crash}
                  <button
                    class="btn small secondary"
                    onclick={(event) => { event.stopPropagation(); onOpenCrash(crash); }}
                  >{t("gallery.action.handle")}</button>
                {:else}
                  <button
                    class="btn small primary"
                    disabled={changingInstance === instance.id || instance.state !== "ready"}
                    onclick={(event) => { event.stopPropagation(); void start(instance); }}
                  >{changingInstance === instance.id ? t("home.launch.starting") : t("gallery.action.launch")}</button>
                {/if}
              </div>
            </section>
          {/each}
        </div>
      {/if}
    </main>
  {/if}

  {#if selectMode && selected.length > 0}
    <div class="batch-bar" role="toolbar" aria-label={t("gallery.batch.barAria")}>
      <span style="font-size:13px;font-weight:600">{t("gallery.batch.selected").replace("{count}", String(selected.length))}</span>
      <button class="btn small secondary" disabled={batchBusy} onclick={() => void batchUpdate()}>{t("gallery.batch.update")}</button>
      <button class="btn small danger" disabled={batchBusy} onclick={() => void openDeleteConfirm()}>{t("gallery.batch.delete")}</button>
      <button class="btn small ghost" disabled={batchBusy} onclick={() => { selected = []; }}>{t("gallery.batch.clear")}</button>
    </div>
  {/if}

  {#if deleteConfirmOpen}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="batch-delete-title"
        tabindex="-1"
        bind:this={deleteDialog}
        onkeydown={handleDeleteDialogKeydown}
      >
        <h3 id="batch-delete-title">{t("gallery.delete.title").replace("{count}", String(selectedInstances.length))}</h3>
        <div class="m-body">
          <p>{t("gallery.delete.body")}</p>
          <div style="margin-top:10px">
            {#each selectedInstances as instance}
              <div class="del-row">
                <div class="lr-main">
                  <div class="lr-name">{instance.name}</div>
                  <div class="lr-sub">{instance.gameVersion} · {loaderName(instance.loaderKind)}</div>
                </div>
              </div>
            {/each}
          </div>
          <p style="margin-top:12px">{t("gallery.delete.retention")}</p>
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={batchBusy} onclick={() => { deleteConfirmOpen = false; }}>{t("common.cancel")}</button>
          <button class="btn danger" disabled={batchBusy} onclick={() => void batchDelete()}>
            {batchBusy ? t("gallery.delete.moving") : t("gallery.delete.confirm").replace("{count}", String(selectedInstances.length))}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

<style>
  .empty-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .toolbar .input {
    height: 34px;
  }
  .inst-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(300px, 100%), 1fr));
    gap: 14px;
  }
  .inst-card {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    position: relative;
  }
  .inst-card .card-hit {
    position: absolute;
    inset: 0;
    border: none;
    background: transparent;
    border-radius: var(--r);
    cursor: pointer;
    padding: 0;
  }
  .inst-card .card-hit:hover {
    background: var(--glass);
  }
  .inst-card > .row {
    pointer-events: none;
  }
  .inst-card > .row button {
    pointer-events: auto;
  }
  .inst-card.checked {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent), inset 0 1px 0 var(--glass-highlight), var(--shadow-1);
  }
  .pick {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: var(--r);
    border: 1.5px solid var(--glass-highlight);
    background: rgba(0, 0, 0, 0.22);
    display: grid;
    place-items: center;
    font-size: 11px;
    color: var(--accent-ink);
    cursor: pointer;
    padding: 0;
  }
  .pick.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .batch-bar {
    position: absolute;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(16, 34, 42, 0.95);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    padding: 10px 14px;
    box-shadow: var(--shadow-2);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    z-index: 30;
    white-space: nowrap;
  }
  .del-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .del-row:first-of-type {
    border-top: none;
  }
</style>
