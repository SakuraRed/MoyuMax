<script lang="ts">
  import { onMount, tick } from "svelte";

  import { formatBytes } from "../installation";
  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    BackupState,
    BackupTrigger,
    InstanceScreenshot,
    InstanceWorldInfo,
    JavaEnvironment,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    RecycleBinItem,
    RecycleItemKind,
    WorldBackupSummary,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onInstancesChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onOpenBackups: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    onInstancesChanged,
    onNavigate,
    onOpenBackups,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let items = $state<RecycleBinItem[]>([]);
  let backups = $state<WorldBackupSummary[]>([]);
  let javaEnvironments = $state<JavaEnvironment[]>([]);
  let loading = $state(true);
  let changingItem = $state<string | null>(null);
  let bulkBusy = $state(false);
  let purgeTargets = $state<RecycleBinItem[]>([]);
  let purgeDialog = $state<HTMLElement | null>(null);
  let worldInstances = $state<ManagedInstance[]>([]);
  let worldInstanceId = $state("");
  let worlds = $state<InstanceWorldInfo[]>([]);
  let worldBusy = $state(false);
  let screenshots = $state<InstanceScreenshot[]>([]);
  let screenshotFilter = $state<"all" | "week">("all");
  let selectedScreenshot = $state<string | null>(null);
  let pendingDelete = $state<string | null>(null);
  let message = $state("");
  let errorMessage = $state("");

  // 存储空间:只统计 runtime 有真实数据来源的维度(备份/Java/回收站)。
  // 实例磁盘占用与磁盘总量/剩余没有统计接口,不展示、不估算。
  const backupBytes = $derived(backups.reduce((sum, backup) => sum + backup.archiveBytes, 0));
  const javaBytes = $derived(
    javaEnvironments.reduce((sum, environment) => sum + environment.sizeBytes, 0),
  );
  const recycleBytes = $derived(items.reduce((sum, item) => sum + item.sizeBytes, 0));
  const knownUsedBytes = $derived(backupBytes + javaBytes + recycleBytes);
  const storageSegments = $derived(
    [
      { key: "backups", labelKey: "data.storage.legend.backups", bytes: backupBytes, color: "rgba(63,216,194,0.55)" },
      { key: "java", labelKey: "data.storage.legend.java", bytes: javaBytes, color: "rgba(63,216,194,0.30)" },
      { key: "recycle", labelKey: "data.storage.legend.recycle", bytes: recycleBytes, color: "rgba(255,255,255,0.22)" },
    ].filter((segment) => segment.bytes > 0),
  );
  const purgeBytes = $derived(purgeTargets.reduce((sum, item) => sum + item.sizeBytes, 0));
  const readyItems = $derived(items.filter((item) => item.state === "ready"));
  const filteredScreenshots = $derived(
    screenshotFilter === "week"
      ? screenshots.filter(
          (screenshot) =>
            screenshot.takenAtUnixSeconds >= Math.floor(Date.now() / 1000) - 7 * 24 * 60 * 60,
        )
      : screenshots,
  );

  onMount(() => {
    void loadItems();
  });

  async function loadItems(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      [items, backups, worldInstances, javaEnvironments] = await Promise.all([
        runtime.listRecycleBinItems(),
        runtime.listWorldBackups(),
        runtime.listInstances(),
        runtime.listJavaEnvironments(),
      ]);
      if (!worldInstances.some((instance) => instance.id === worldInstanceId)) {
        worldInstanceId =
          worldInstances.find((instance) => instance.state === "ready")?.id ?? "";
      }
      worlds = worldInstanceId
        ? await runtime.listInstanceWorldDetails(worldInstanceId)
        : [];
      screenshots = worldInstanceId
        ? await runtime.listInstanceScreenshots(worldInstanceId)
        : [];
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  async function selectWorldInstance(event: Event): Promise<void> {
    worldInstanceId = (event.currentTarget as HTMLSelectElement).value;
    message = "";
    errorMessage = "";
    selectedScreenshot = null;
    pendingDelete = null;
    try {
      worlds = worldInstanceId
        ? await runtime.listInstanceWorldDetails(worldInstanceId)
        : [];
      screenshots = worldInstanceId
        ? await runtime.listInstanceScreenshots(worldInstanceId)
        : [];
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function copyScreenshot(fileName: string): Promise<void> {
    message = "";
    errorMessage = "";
    try {
      await runtime.copyScreenshotToClipboard(worldInstanceId, fileName);
      message = t("data.msg.copied").replace("{name}", fileName);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function openScreenshot(fileName: string): Promise<void> {
    message = "";
    errorMessage = "";
    try {
      await runtime.openScreenshotLocation(worldInstanceId, fileName);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function deleteScreenshot(fileName: string): Promise<void> {
    worldBusy = true;
    message = "";
    errorMessage = "";
    try {
      await runtime.deleteInstanceScreenshot(worldInstanceId, fileName);
      selectedScreenshot = null;
      pendingDelete = null;
      message = t("data.msg.screenshotDeleted").replace("{name}", fileName);
      screenshots = await runtime.listInstanceScreenshots(worldInstanceId);
      items = await runtime.listRecycleBinItems();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      worldBusy = false;
    }
  }

  async function deleteWorld(world: InstanceWorldInfo): Promise<void> {
    worldBusy = true;
    message = "";
    errorMessage = "";
    try {
      await runtime.deleteInstanceWorld(worldInstanceId, world.name);
      pendingDelete = null;
      message = t("data.msg.worldDeleted").replace("{name}", world.name);
      worlds = await runtime.listInstanceWorldDetails(worldInstanceId);
      items = await runtime.listRecycleBinItems();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      worldBusy = false;
    }
  }

  async function importWorld(): Promise<void> {
    worldBusy = true;
    message = "";
    errorMessage = "";
    try {
      const source = await runtime.pickWorldZip();
      if (!source) return;
      const imported = await runtime.importInstanceWorld(worldInstanceId, source);
      message = t("data.msg.worldImported").replace("{name}", imported.name);
      worlds = await runtime.listInstanceWorldDetails(worldInstanceId);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      worldBusy = false;
    }
  }

  async function exportWorld(world: InstanceWorldInfo): Promise<void> {
    worldBusy = true;
    message = "";
    errorMessage = "";
    try {
      const destination = await runtime.pickWorldExportPath(world.name);
      if (!destination) return;
      const bytes = await runtime.exportInstanceWorld(worldInstanceId, world.name, destination);
      message = t("data.msg.worldExported").replace("{name}", world.name).replace("{size}", formatBytes(bytes));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      worldBusy = false;
    }
  }

  async function restoreOne(item: RecycleBinItem): Promise<void> {
    if (item.kind === "instance") {
      await runtime.restoreRecycleBinItem(item.id);
    } else {
      await runtime.restoreRecycledEntry(item.id);
    }
  }

  async function restore(item: RecycleBinItem): Promise<void> {
    changingItem = item.id;
    message = "";
    errorMessage = "";
    try {
      await restoreOne(item);
      items = items.filter((candidate) => candidate.id !== item.id);
      message = t("data.msg.restored").replace("{name}", item.displayName);
      await onInstancesChanged();
      if (worldInstanceId) {
        worlds = await runtime.listInstanceWorldDetails(worldInstanceId);
        screenshots = await runtime.listInstanceScreenshots(worldInstanceId);
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingItem = null;
    }
  }

  async function restoreAll(): Promise<void> {
    bulkBusy = true;
    message = "";
    errorMessage = "";
    try {
      let restored = 0;
      for (const item of readyItems) {
        try {
          await restoreOne(item);
          restored += 1;
        } catch {
          // 单项失败不中断批量恢复,结束后统一重新拉取列表。
        }
      }
      items = await runtime.listRecycleBinItems();
      if (restored > 0) {
        message = t("data.msg.restoredAll").replace("{count}", String(restored));
      }
      await onInstancesChanged();
      if (worldInstanceId) {
        worlds = await runtime.listInstanceWorldDetails(worldInstanceId);
        screenshots = await runtime.listInstanceScreenshots(worldInstanceId);
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      bulkBusy = false;
    }
  }

  async function askPurge(targets: RecycleBinItem[]): Promise<void> {
    if (targets.length === 0) return;
    message = "";
    errorMessage = "";
    purgeTargets = targets;
    await tick();
    purgeDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function cancelPurge(): void {
    if (bulkBusy) return;
    purgeTargets = [];
  }

  async function purge(): Promise<void> {
    if (purgeTargets.length === 0) return;
    bulkBusy = true;
    message = "";
    errorMessage = "";
    try {
      let removedSubjects = 0;
      let releasedBytes = 0;
      for (const item of purgeTargets) {
        const result = await runtime.purgeRecycleBinItem(item.id);
        removedSubjects += result.removedSubjects;
        releasedBytes += result.releasedBytes;
      }
      const removedIds = new Set(purgeTargets.map((item) => item.id));
      items = items.filter((candidate) => !removedIds.has(candidate.id));
      purgeTargets = [];
      message = t("data.msg.purgedItems")
        .replace("{count}", String(removedSubjects))
        .replace("{size}", formatBytes(releasedBytes));
      await onInstancesChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      bulkBusy = false;
    }
  }

  function handlePurgeDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelPurge();
      return;
    }
    if (event.key !== "Tab" || !purgeDialog) return;
    const controls = [...purgeDialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
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

  function expiryLabel(item: RecycleBinItem): string {
    const remainingSeconds = item.expiresAtUnixSeconds - Math.floor(Date.now() / 1000);
    const days = Math.max(0, Math.ceil(remainingSeconds / (24 * 60 * 60)));
    return days === 0
      ? t("data.recycle.expired")
      : t("data.recycle.remainingDays").replace("{days}", String(days));
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

  function backupTriggerLabel(trigger: BackupTrigger): string {
    switch (trigger) {
      case "preLaunch":
        return t("data.backups.trigger.preLaunch");
      case "postExit":
        return t("data.backups.trigger.postExit");
      case "manual":
        return t("data.backups.trigger.manual");
      case "scheduled":
        return t("data.backups.trigger.scheduled");
    }
  }

  function backupStateLabel(state: BackupState): string {
    switch (state) {
      case "ready":
        return t("data.backups.state.ready");
      case "skipped":
        return t("data.backups.state.skipped");
      case "failed":
        return t("data.backups.state.failed");
      case "staging":
        return t("data.backups.state.staging");
    }
  }

  function recycleKindLabel(kind: RecycleItemKind): string {
    switch (kind) {
      case "instance":
        return t("data.recycle.kind.instance");
      case "screenshot":
        return t("data.recycle.kind.screenshot");
      case "resource":
        return t("data.recycle.kind.resource");
      case "world":
        return t("data.recycle.kind.world");
    }
  }
</script>

<AppShell
  pageTitle={t("nav.data")}
  activeNavigation="data"
  instanceCount={worldInstances.length}
  connectionStatus={t("data.connectionStatus")}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content data-content">
    <div class="row spread page-head">
      <div style="min-width:0">
        <h1 class="page-title">{t("data.heading.title")}</h1>
        <div class="panel-desc">{t("data.heading.description")}</div>
      </div>
      <button class="btn small ghost" style="flex:none" disabled={loading || changingItem !== null || bulkBusy} onclick={() => void loadItems()}>{t("data.refresh")}</button>
    </div>

    {#if loading}
      <section class="panel pad" aria-live="polite">
        <div class="skel" style="height:14px;width:40%;margin-bottom:12px"></div>
        <div class="skel" style="height:14px;width:70%;margin-bottom:6px"></div>
        <span class="dim">{t("data.loading")}</span>
      </section>
    {:else}
      <section class="panel pad page-section" aria-label={t("data.storage.title")}>
        <div class="row spread">
          <div class="panel-title">{t("data.storage.title")}</div>
          <span class="dim" style="min-width:0;overflow-wrap:anywhere">{t("data.locations.primary")} · {settings.dataDirectory}</span>
        </div>
        <div class="row" style="align-items:baseline;gap:10px;margin:8px 0 12px">
          <span style="font-size:26px;font-weight:700">{formatBytes(knownUsedBytes)}</span>
          <span class="muted">{t("data.storage.used")}</span>
        </div>
        {#if storageSegments.length > 0}
          <div class="spacebar" aria-hidden="true">
            {#each storageSegments as segment}
              <i style="width:{(segment.bytes / knownUsedBytes) * 100}%;background:{segment.color}"></i>
            {/each}
          </div>
          <div class="legend">
            {#each storageSegments as segment}
              <span class="lg"><span class="lgdot" style="background:{segment.color}"></span>{t(segment.labelKey)} <b>{formatBytes(segment.bytes)}</b></span>
            {/each}
          </div>
        {/if}
        <div class="dim" style="margin-top:10px">{t("data.storage.gapNote")}</div>
      </section>

      <section class="panel pad page-section" aria-label={t("data.locations.title")}>
        <div class="panel-title" style="margin-bottom:6px">{t("data.locations.title")}</div>
        <div class="list-row" style="padding-left:0;padding-right:0">
          <span class="loc-ico">
            <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><rect x="1.5" y="3" width="15" height="12" rx="2" stroke="currentColor" stroke-width="1.4"/><rect x="4" y="6" width="6" height="1.6" rx="0.8" fill="currentColor"/><rect x="4" y="9.4" width="10" height="1.6" rx="0.8" fill="currentColor" opacity="0.6"/></svg>
          </span>
          <div class="lr-main">
            <div class="lr-name">{t("data.locations.primary")}</div>
            <div class="lr-sub mono">{settings.dataDirectory}</div>
          </div>
          <span class="tag ok"><span class="cdot"></span>{t("data.locations.statusOk")}</span>
        </div>
        <div class="dim" style="margin-top:4px">{t("data.locations.singleNote")}</div>
      </section>

      <section class="panel page-section" aria-labelledby="recycle-bin-title">
        <div class="row spread recycle-head">
          <div style="min-width:0">
            <h2 class="panel-title" id="recycle-bin-title">{t("data.recycle.title")}</h2>
            <div class="panel-desc" style="margin-top:2px">{t("data.recycle.retention")}</div>
          </div>
          <div class="row" style="flex:none">
            <button class="btn small ghost" disabled={readyItems.length === 0 || changingItem !== null || bulkBusy} onclick={() => void restoreAll()}>{t("data.recycle.restoreAll")}</button>
            <button class="btn small danger-soft" disabled={readyItems.length === 0 || changingItem !== null || bulkBusy} onclick={() => void askPurge(readyItems)}>{t("data.recycle.emptyBin")}</button>
          </div>
        </div>
        {#if items.length === 0}
          <div class="recycle-empty">
            <Icon name="database" size={26} />
            <h2>{t("data.recycle.emptyTitle")}</h2>
            <p class="dim">{t("data.recycle.emptyDescription")}</p>
          </div>
        {:else}
          <div class="rc-head" aria-hidden="true">
            <span>{t("data.recycle.colObject")}</span><span>{t("data.recycle.colKind")}</span><span>{t("data.recycle.colDeletedAt")}</span><span>{t("data.recycle.colRemaining")}</span><span>{t("data.recycle.colSize")}</span><span></span>
          </div>
          <div aria-label={t("data.recycle.listAria")}>
            {#each items as item (item.id)}
              <div class="rc-row recycle-card" class:failed={item.state === "failed"}>
                <div class="rc-object">
                  <div class="rc-name">{item.displayName}</div>
                  <div class="rc-sub">{t("data.recycle.origin").replace("{path}", item.originalPath)}</div>
                  {#if item.state === "failed"}
                    <div class="rc-sub" style="color:var(--danger)">{t("data.recycle.convergeError")}</div>
                  {/if}
                </div>
                <span class="rc-cell">{recycleKindLabel(item.kind)}</span>
                <span class="rc-cell">{timestampLabel(item.deletedAtUnixSeconds)}</span>
                <span class="rc-cell">{expiryLabel(item)}</span>
                <span class="rc-cell">{formatBytes(item.sizeBytes)}</span>
                <div class="row rc-acts">
                  <button
                    class="btn small secondary"
                    aria-label={t("data.recycle.restoreAria").replace("{name}", item.displayName)}
                    disabled={item.state !== "ready" || changingItem !== null || bulkBusy}
                    onclick={() => void restore(item)}
                  >{changingItem === item.id ? t("data.recycle.restoring") : t("data.recycle.restore")}</button>
                  <button
                    class="btn small ghost"
                    aria-label={t("data.recycle.purgeAria").replace("{name}", item.displayName)}
                    disabled={item.state !== "ready" || changingItem !== null || bulkBusy}
                    onclick={() => void askPurge([item])}
                  >{t("data.recycle.purge")}</button>
                </div>
              </div>
            {/each}
          </div>
          <div class="row spread recycle-foot">
            <span class="dim">{t("data.recycle.summary").replace("{count}", String(items.length)).replace("{size}", formatBytes(recycleBytes))}</span>
          </div>
        {/if}
      </section>

      <section class="panel pad page-section" aria-labelledby="worlds-title">
        <div class="row spread section-head">
          <h2 class="panel-title" id="worlds-title">{t("data.worlds.title")}</h2>
          <div class="row" style="flex:none">
            <label class="sr-live" for="world-instance-select">{t("data.worlds.selectInstance")}</label>
            <select class="input" id="world-instance-select" value={worldInstanceId} onchange={(event) => void selectWorldInstance(event)}>
              {#each worldInstances.filter((instance) => instance.state === "ready") as instance}
                <option value={instance.id}>{instance.name}</option>
              {/each}
            </select>
            <button class="btn small secondary" disabled={worldBusy || !worldInstanceId} onclick={() => void importWorld()}>{worldBusy ? t("data.worlds.busy") : t("data.worlds.import")}</button>
          </div>
        </div>
        {#if !worldInstanceId}
          <div class="dim section-empty">{t("data.worlds.noInstance")}</div>
        {:else if worlds.length === 0}
          <div class="dim section-empty">{t("data.worlds.empty")}</div>
        {:else}
          <div>
            {#each worlds as world}
              <article class="backup-row">
                <div class="lr-main">
                  <div class="row" style="gap:8px">
                    <h3 class="row-title">{world.name}</h3>
                    <span class="tag neutral">{t("data.worlds.badge")}</span>
                  </div>
                  <div class="lr-sub">{formatBytes(world.sizeBytes)}{world.lastPlayedUnixSeconds ? t("data.worlds.lastPlayed").replace("{time}", timestampLabel(world.lastPlayedUnixSeconds)) : ""}</div>
                </div>
                <div class="row" style="flex:none">
                  <button class="btn small ghost" disabled={worldBusy} onclick={() => void exportWorld(world)}>{t("data.worlds.export")}</button>
                  {#if pendingDelete === `world-${world.name}`}
                    <button class="btn small danger" disabled={worldBusy} onclick={() => void deleteWorld(world)}>{t("common.confirmDelete")}</button>
                    <button class="btn small ghost" disabled={worldBusy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                  {:else}
                    <button class="btn small danger-soft" disabled={worldBusy} onclick={() => { pendingDelete = `world-${world.name}`; }}>{t("common.delete")}</button>
                  {/if}
                </div>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <section class="panel pad page-section" aria-labelledby="screenshots-title">
        <div class="row spread section-head">
          <h2 class="panel-title" id="screenshots-title">{t("data.screenshots.title")}</h2>
          <div class="seg" role="group" aria-label={t("data.screenshots.filterAria")}>
            <button class:on={screenshotFilter === "all"} onclick={() => { screenshotFilter = "all"; }}>{t("data.screenshots.filterAll").replace("{count}", String(screenshots.length))}</button>
            <button class:on={screenshotFilter === "week"} onclick={() => { screenshotFilter = "week"; }}>{t("data.screenshots.filterWeek")}</button>
          </div>
        </div>
        {#if !worldInstanceId}
          <div class="dim section-empty">{t("data.screenshots.noInstance")}</div>
        {:else if filteredScreenshots.length === 0}
          <div class="dim section-empty">{screenshotFilter === "week" ? t("data.screenshots.emptyWeek") : t("data.screenshots.empty")}</div>
        {:else}
          <div class="screenshot-grid">
            {#each filteredScreenshots as screenshot}
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
                <small class="dim">{formatBytes(screenshot.sizeBytes)} · {timestampLabel(screenshot.takenAtUnixSeconds)}</small>
              </button>
            {/each}
          </div>
          {#if selectedScreenshot}
            <div class="screenshot-actions">
              <span class="muted" style="min-width:0;overflow:hidden;text-overflow:ellipsis">{t("data.screenshots.selected").replace("{name}", selectedScreenshot)}</span>
              <div class="row" style="flex:none">
                <button class="btn small ghost" onclick={() => void copyScreenshot(selectedScreenshot!)}>{t("data.screenshots.copy")}</button>
                <button class="btn small ghost" onclick={() => void openScreenshot(selectedScreenshot!)}>{t("data.screenshots.openLocation")}</button>
                {#if pendingDelete === "screenshot"}
                  <button class="btn small danger" disabled={worldBusy} onclick={() => void deleteScreenshot(selectedScreenshot!)}>{t("common.confirmDelete")}</button>
                  <button class="btn small ghost" disabled={worldBusy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                {:else}
                  <button class="btn small danger-soft" disabled={worldBusy} onclick={() => { pendingDelete = "screenshot"; }}>{t("common.delete")}</button>
                {/if}
              </div>
            </div>
          {/if}
        {/if}
      </section>

      <section class="panel pad page-section" aria-labelledby="world-backups-title">
        <div class="row spread section-head">
          <h2 class="panel-title" id="world-backups-title">{t("data.backups.title")}</h2>
          <button class="btn small ghost" style="flex:none" onclick={onOpenBackups}>{t("data.backups.manage")}</button>
        </div>
        {#if backups.length === 0}
          <div class="dim section-empty">{t("data.backups.empty")}</div>
        {:else}
          <div>
            {#each backups.slice(0, 3) as backup}
              <article class:failed={backup.state === "failed"} class="backup-row">
                <div class="lr-main">
                  <div class="row" style="gap:8px">
                    <h3 class="row-title">{backup.instanceName}</h3>
                    <span class="tag neutral">{backupTriggerLabel(backup.trigger)}</span>
                    <span class="tag neutral">{backup.kind === "incremental" ? t("data.backups.kind.incremental") : t("data.backups.kind.full")}</span>
                  </div>
                  <div class="lr-sub">{t("data.backups.line").replace("{time}", timestampLabel(backup.createdAtUnixSeconds)).replace("{count}", String(backup.worldCount)).replace("{size}", formatBytes(backup.archiveBytes || backup.sourceBytes))}</div>
                  {#if backup.errorSummary}<div class="lr-sub" style="color:var(--danger)">{backup.errorSummary}</div>{/if}
                </div>
                <strong class="backup-state" class:failed={backup.state === "failed"}>{backupStateLabel(backup.state)}</strong>
              </article>
            {/each}
          </div>
          {#if backups.length > 3}
            <button class="inline-link" onclick={onOpenBackups}>{t("data.backups.viewAll").replace("{count}", String(backups.length))}</button>
          {/if}
        {/if}
      </section>
    {/if}
  </main>

  {#if errorMessage}
    <div class="toast" role="alert" style="position:absolute;right:20px;bottom:20px;z-index:35"><Icon name="info" size={16} /><span>{errorMessage}</span></div>
  {:else if message}
    <div class="toast" role="status" style="position:absolute;right:20px;bottom:20px;z-index:35"><Icon name="info" size={16} /><span>{message}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{message || errorMessage}</div>

  {#if purgeTargets.length > 0}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="purge-confirm-title"
        tabindex="-1"
        bind:this={purgeDialog}
        onkeydown={handlePurgeDialogKeydown}
      >
        <h3 id="purge-confirm-title">{t("data.purge.binTitle")}</h3>
        <div class="m-body">
          <p>
            {t("data.purge.binBody").replace("{count}", String(purgeTargets.length)).replace("{size}", formatBytes(purgeBytes))}
          </p>
          <div class="purge-list">
            {#each purgeTargets as item (item.id)}
              <span>· {t("data.purge.itemLine").replace("{name}", item.displayName).replace("{kind}", recycleKindLabel(item.kind)).replace("{size}", formatBytes(item.sizeBytes))}</span>
            {/each}
          </div>
          <p class="purge-danger">{t("data.purge.irreversible")}</p>
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={bulkBusy} onclick={cancelPurge}>{t("common.cancel")}</button>
          <button class="btn danger" disabled={bulkBusy} onclick={() => void purge()}>
            {bulkBusy ? t("data.purge.purging") : t("data.purge.confirmCount").replace("{count}", String(purgeTargets.length))}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

<style>
  .page-head {
    margin-bottom: 18px;
  }
  .page-title {
    font-size: 17px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .page-section {
    margin-bottom: 16px;
  }
  .section-head {
    margin-bottom: 10px;
    flex-wrap: wrap;
    row-gap: 8px;
  }
  .section-head select {
    min-width: 0;
    max-width: 100%;
  }
  .section-empty {
    padding: 8px 0;
  }
  .row-title {
    font-size: 13.5px;
    font-weight: 600;
  }

  /* 空间分段条:同一强调色的不同透明度 + 中性色,不引入新色相 */
  .spacebar {
    display: flex;
    height: 14px;
    border-radius: var(--r);
    overflow: hidden;
    gap: 2px;
  }
  .spacebar i {
    display: block;
    height: 100%;
  }
  .legend {
    display: flex;
    gap: 18px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  .legend .lg {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .legend .lgdot {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    flex: none;
  }
  .legend b {
    color: var(--text-1);
    font-weight: 600;
  }

  .loc-ico {
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: var(--r);
    background: var(--glass-strong);
    display: grid;
    place-items: center;
    color: var(--text-2);
  }

  /* 回收站表格 */
  .recycle-head {
    padding: 16px 18px 12px;
    flex-wrap: wrap;
    row-gap: 8px;
  }
  .rc-head,
  .rc-row {
    display: grid;
    grid-template-columns: 1.9fr 0.6fr 1.1fr 0.8fr 0.7fr auto;
    gap: 12px;
    align-items: center;
    padding: 11px 14px;
  }
  /* 中和旧全局 .recycle-card 的卡片样式(类名须保留给既有 e2e 挂钩) */
  .rc-row {
    background: transparent;
    border: none;
    border-radius: 0;
  }
  .rc-head {
    font-size: 11.5px;
    color: var(--text-3);
    letter-spacing: 0.04em;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
  }
  .rc-row + .rc-row {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .rc-row:hover {
    background: var(--glass);
  }
  .rc-row.failed {
    border-left: 2px solid var(--danger);
  }
  .rc-head > *,
  .rc-row > * {
    min-width: 0;
  }
  .rc-name {
    font-size: 13.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rc-sub {
    font-size: 11.5px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rc-cell {
    font-size: 12.5px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rc-acts {
    flex-wrap: nowrap;
    gap: 6px;
  }
  .recycle-foot {
    padding: 12px 18px 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.07);
  }
  .recycle-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 28px 18px 30px;
    color: var(--text-3);
  }
  .recycle-empty h2 {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-1);
  }

  /* 世界/备份行 */
  .backup-row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 20px;
    border-radius: var(--r);
    min-width: 0;
    /* 中和旧全局 .backup-row 的网格与底边框(类名须保留给既有 e2e 挂钩) */
    border-bottom: none;
  }
  .backup-row:hover {
    background: var(--glass);
  }
  .backup-row + .backup-row {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .backup-row .lr-main {
    flex: 1;
    min-width: 0;
  }
  .backup-row .lr-sub {
    margin-top: 2px;
  }
  .backup-row.failed .row-title {
    color: var(--danger);
  }
  .backup-state {
    flex: none;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .backup-state.failed {
    color: var(--danger);
  }

  /* 截图 */
  .screenshot-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 12px;
  }
  .screenshot-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 16px 20px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    color: var(--text-2);
    font-family: var(--font);
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .screenshot-card:hover {
    background: var(--glass);
    color: var(--text-1);
  }
  .screenshot-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  .screenshot-name {
    font-size: 12.5px;
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .screenshot-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
    padding: 10px 12px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    flex-wrap: wrap;
  }

  /* 永久删除确认 */
  .purge-list {
    margin: 10px 0 2px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow: hidden auto;
  }
  .purge-danger {
    margin-top: 10px;
    color: var(--danger);
    font-weight: 600;
  }

  /* 窄窗口/高倍放大:单元格与行内文本由省略号降级为换行,避免横向裁剪 */
  @media (max-width: 900px) {
    .rc-name,
    .rc-sub,
    .rc-cell {
      white-space: normal;
      overflow-wrap: anywhere;
    }
    .page-section .lr-name,
    .page-section .lr-sub,
    .screenshot-name {
      white-space: normal;
      overflow-wrap: anywhere;
    }
  }
</style>
