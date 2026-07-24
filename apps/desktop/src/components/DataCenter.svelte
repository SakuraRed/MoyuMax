<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    BackupState,
    BackupTrigger,
    InstanceScreenshot,
    InstanceWorldInfo,
    ManagedInstance,
    MoyuRuntime,
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
    onBack: () => void;
    onOpenResources: () => void;
    onOpenTasks: () => void;
    onInstancesChanged: () => Promise<void>;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    onBack,
    onOpenResources,
    onOpenTasks,
    onInstancesChanged,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let items = $state<RecycleBinItem[]>([]);
  let backups = $state<WorldBackupSummary[]>([]);
  let loading = $state(true);
  let changingItem = $state<string | null>(null);
  let purgeCandidate = $state<RecycleBinItem | null>(null);
  let purgeDialog = $state<HTMLElement | null>(null);
  let worldInstances = $state<ManagedInstance[]>([]);
  let worldInstanceId = $state("");
  let worlds = $state<InstanceWorldInfo[]>([]);
  let worldBusy = $state(false);
  let screenshots = $state<InstanceScreenshot[]>([]);
  let screenshotFilter = $state<"all" | "week">("all");
  let selectedScreenshot = $state<string | null>(null);
  let pendingDelete = $state<string | null>(null);
  let rollbackCandidate = $state<WorldBackupSummary | null>(null);
  let rollbackDialog = $state<HTMLElement | null>(null);
  let message = $state("");
  let errorMessage = $state("");
  const totalBytes = $derived(items.reduce((sum, item) => sum + item.sizeBytes, 0));
  const backupBytes = $derived(backups.reduce((sum, backup) => sum + backup.archiveBytes, 0));
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
      [items, backups, worldInstances] = await Promise.all([
        runtime.listRecycleBinItems(),
        runtime.listWorldBackups(),
        runtime.listInstances(),
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

  async function askRollback(backup: WorldBackupSummary): Promise<void> {
    message = "";
    errorMessage = "";
    rollbackCandidate = backup;
    await tick();
    rollbackDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function cancelRollback(): void {
    if (changingItem === rollbackCandidate?.id) return;
    rollbackCandidate = null;
  }

  async function confirmRollback(): Promise<void> {
    const backup = rollbackCandidate;
    if (!backup) return;
    changingItem = backup.id;
    message = "";
    errorMessage = "";
    try {
      await runtime.rollbackWorldBackup(backup.id);
      rollbackCandidate = null;
      backups = await runtime.listWorldBackups();
      message = t("data.msg.rollbackDone");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingItem = null;
    }
  }

  function handleRollbackDialogKeydown(event: KeyboardEvent): void {
    handleDialogKeydown(event, rollbackDialog, cancelRollback);
  }

  async function restore(item: RecycleBinItem): Promise<void> {
    changingItem = item.id;
    message = "";
    errorMessage = "";
    try {
      if (item.kind === "instance") {
        await runtime.restoreRecycleBinItem(item.id);
      } else {
        await runtime.restoreRecycledEntry(item.id);
      }
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

  async function askPurge(item: RecycleBinItem): Promise<void> {
    message = "";
    errorMessage = "";
    purgeCandidate = item;
    await tick();
    purgeDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function cancelPurge(): void {
    if (changingItem === purgeCandidate?.id) return;
    purgeCandidate = null;
  }

  async function purge(): Promise<void> {
    const item = purgeCandidate;
    if (!item) return;
    changingItem = item.id;
    message = "";
    errorMessage = "";
    try {
      const result = await runtime.purgeRecycleBinItem(item.id);
      items = items.filter((candidate) => candidate.id !== item.id);
      purgeCandidate = null;
      message = t("data.msg.purged").replace("{count}", String(result.removedSubjects)).replace("{size}", formatBytes(result.releasedBytes));
      await onInstancesChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingItem = null;
    }
  }

  function handlePurgeDialogKeydown(event: KeyboardEvent): void {
    handleDialogKeydown(event, purgeDialog, cancelPurge);
  }

  function handleDialogKeydown(
    event: KeyboardEvent,
    dialog: HTMLElement | null,
    cancel: () => void,
  ): void {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
      return;
    }
    if (event.key !== "Tab" || !dialog) return;
    const controls = [...dialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
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

  function expiryLabel(item: RecycleBinItem): string {
    const remainingSeconds = item.expiresAtUnixSeconds - Math.floor(Date.now() / 1000);
    const days = Math.max(0, Math.ceil(remainingSeconds / (24 * 60 * 60)));
    return days === 0 ? t("data.recycle.expired") : t("data.recycle.expiryDays").replace("{days}", String(days));
  }

  function deletedLabel(item: RecycleBinItem): string {
    return timestampLabel(item.deletedAtUnixSeconds);
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
  dataDirectory={settings.dataDirectory}
  activeNavigation="data"
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus={t("data.connectionStatus")}
  taskStatus={changingItem ? t("data.taskStatus.updating") : t("data.taskStatus.summary").replace("{backups}", String(backups.length)).replace("{items}", String(items.length))}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content data-content">
    <div class="data-scroll" data-scroll-region="main">
      <header class="data-heading">
        <div>
          <h1>{t("data.heading.title")}</h1>
          <p>{t("data.heading.description")}</p>
        </div>
        <button class="button" disabled={loading || changingItem !== null} onclick={() => void loadItems()}>{t("data.refresh")}</button>
      </header>

      <section class="data-overview" aria-label={t("data.overview.aria")}>
        <div><span>{t("data.overview.backupCount")}</span><strong>{backups.length}</strong></div>
        <div><span>{t("data.overview.backupSize")}</span><strong>{formatBytes(backupBytes)}</strong></div>
        <div><span>{t("data.overview.recycleCount")}</span><strong>{items.length}</strong></div>
        <div><span>{t("data.overview.recycleSize")}</span><strong>{formatBytes(totalBytes)}</strong></div>
      </section>

      {#if loading}
        <section class="data-loading" aria-live="polite">
          <div class="loading-line wide"></div>
          <div class="loading-line"></div>
          <span>{t("data.loading")}</span>
        </section>
      {:else}
        <section class="backup-section" aria-labelledby="worlds-title">
          <header>
            <div>
              <h2 id="worlds-title">{t("data.worlds.title")}</h2>
              <p>{t("data.worlds.description")}</p>
            </div>
            <div class="world-toolbar">
              <label class="sr-live" for="world-instance-select">{t("data.worlds.selectInstance")}</label>
              <select id="world-instance-select" value={worldInstanceId} onchange={(event) => void selectWorldInstance(event)}>
                {#each worldInstances.filter((instance) => instance.state === "ready") as instance}
                  <option value={instance.id}>{instance.name}</option>
                {/each}
              </select>
              <button class="button ghost compact" disabled={worldBusy || !worldInstanceId} onclick={() => void importWorld()}>{worldBusy ? t("data.worlds.busy") : t("data.worlds.import")}</button>
            </div>
          </header>
          {#if !worldInstanceId}
            <div class="backup-empty-row">{t("data.worlds.noInstance")}</div>
          {:else if worlds.length === 0}
            <div class="backup-empty-row">{t("data.worlds.empty")}</div>
          {:else}
            <div class="backup-list">
              {#each worlds as world}
                <article class="backup-row">
                  <div>
                    <div class="backup-title-line"><h3>{world.name}</h3><span>{t("data.worlds.badge")}</span></div>
                    <p>{formatBytes(world.sizeBytes)}{world.lastPlayedUnixSeconds ? t("data.worlds.lastPlayed").replace("{time}", timestampLabel(world.lastPlayedUnixSeconds)) : ""}</p>
                  </div>
                  <div class="backup-side">
                    <button class="button ghost compact" disabled={worldBusy} onclick={() => void exportWorld(world)}>{t("data.worlds.export")}</button>
                    {#if pendingDelete === `world-${world.name}`}
                      <button class="button danger-subtle compact" disabled={worldBusy} onclick={() => void deleteWorld(world)}>{t("common.confirmDelete")}</button>
                      <button class="button ghost compact" disabled={worldBusy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                    {:else}
                      <button class="button danger-subtle compact" disabled={worldBusy} onclick={() => { pendingDelete = `world-${world.name}`; }}>{t("common.delete")}</button>
                    {/if}
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </section>

        <section class="backup-section" aria-labelledby="screenshots-title">
          <header>
            <div>
              <h2 id="screenshots-title">{t("data.screenshots.title")}</h2>
              <p>{t("data.screenshots.description")}</p>
            </div>
            <div class="screenshot-filters" role="group" aria-label={t("data.screenshots.filterAria")}>
              <button class="filter-chip" class:active={screenshotFilter === "all"} onclick={() => { screenshotFilter = "all"; }}>{t("data.screenshots.filterAll").replace("{count}", String(screenshots.length))}</button>
              <button class="filter-chip" class:active={screenshotFilter === "week"} onclick={() => { screenshotFilter = "week"; }}>{t("data.screenshots.filterWeek")}</button>
            </div>
          </header>
          {#if !worldInstanceId}
            <div class="backup-empty-row">{t("data.screenshots.noInstance")}</div>
          {:else if filteredScreenshots.length === 0}
            <div class="backup-empty-row">{screenshotFilter === "week" ? t("data.screenshots.emptyWeek") : t("data.screenshots.empty")}</div>
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
                    <button class="button danger-subtle compact" disabled={worldBusy} onclick={() => void deleteScreenshot(selectedScreenshot!)}>{t("common.confirmDelete")}</button>
                    <button class="button ghost compact" disabled={worldBusy} onclick={() => { pendingDelete = null; }}>{t("common.cancel")}</button>
                  {:else}
                    <button class="button danger-subtle compact" disabled={worldBusy} onclick={() => { pendingDelete = "screenshot"; }}>{t("common.delete")}</button>
                  {/if}
                </div>
              </div>
            {/if}
          {/if}
        </section>

        <section class="backup-section" aria-labelledby="world-backups-title">
          <header>
            <div>
              <h2 id="world-backups-title">{t("data.backups.title")}</h2>
              <p>{t("data.backups.description")}</p>
            </div>
          </header>
          {#if backups.length === 0}
            <div class="backup-empty-row">{t("data.backups.empty")}</div>
          {:else}
            <div class="backup-list">
              {#each backups as backup}
                <article class:failed={backup.state === "failed"} class="backup-row">
                  <div>
                    <div class="backup-title-line">
                      <h3>{backup.instanceName}</h3>
                      <span>{backupTriggerLabel(backup.trigger)}</span>
                      <span>{backup.kind === "incremental" ? t("data.backups.kind.incremental") : t("data.backups.kind.full")}</span>
                    </div>
                    <p>{t("data.backups.line").replace("{time}", timestampLabel(backup.createdAtUnixSeconds)).replace("{count}", String(backup.worldCount)).replace("{size}", formatBytes(backup.archiveBytes || backup.sourceBytes))}</p>
                    {#if backup.errorSummary}<small>{backup.errorSummary}</small>{/if}
                  </div>
                  <div class="backup-side">
                    <strong>{backupStateLabel(backup.state)}</strong>
                    {#if backup.state === "ready"}
                      <button
                        class="button ghost compact"
                        aria-label={t("data.backups.rollbackAria").replace("{name}", backup.instanceName)}
                        disabled={changingItem !== null}
                        onclick={() => void askRollback(backup)}
                      >{t("data.backups.rollback")}</button>
                    {/if}
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </section>

        <section class="recycle-section" aria-labelledby="recycle-bin-title">
          <header>
            <div>
              <h2 id="recycle-bin-title">{t("data.recycle.title")}</h2>
              <p>{t("data.recycle.description")}</p>
            </div>
          </header>
          {#if items.length === 0}
            <div class="data-empty compact-empty">
              <Icon name="database" size={26} />
              <h2>{t("data.recycle.emptyTitle")}</h2>
              <p>{t("data.recycle.emptyDescription")}</p>
            </div>
          {:else}
            <div class="recycle-list" aria-label={t("data.recycle.listAria")}>
              {#each items as item}
                <article class:failed={item.state === "failed"} class="recycle-card">
                  <div class="recycle-copy">
                    <div class="recycle-title-line">
                      <h2>{item.displayName}</h2>
                      <span>{recycleKindLabel(item.kind)}</span>
                    </div>
                    <p>{formatBytes(item.sizeBytes)} · {expiryLabel(item)}</p>
                    <dl class="recycle-meta">
                      <div><dt>{t("data.recycle.deletedAt")}</dt><dd>{deletedLabel(item)}</dd></div>
                      <div><dt>{t("data.recycle.originalPath")}</dt><dd><code>{item.originalPath}</code></dd></div>
                    </dl>
                    {#if item.state === "failed"}
                      <small class="recycle-error">{t("data.recycle.convergeError")}</small>
                    {/if}
                  </div>
                  <div class="recycle-actions">
                    <button
                      class="button primary"
                      aria-label={t("data.recycle.restoreAria").replace("{name}", item.displayName)}
                      disabled={item.state !== "ready" || changingItem !== null}
                      onclick={() => void restore(item)}
                    >{changingItem === item.id ? t("data.recycle.restoring") : t("data.recycle.restore")}</button>
                    <button
                      class="button danger-subtle"
                      aria-label={t("data.recycle.purgeAria").replace("{name}", item.displayName)}
                      disabled={item.state !== "ready" || changingItem !== null}
                      onclick={() => void askPurge(item)}
                    >{t("data.recycle.purge")}</button>
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </section>
      {/if}
    </div>
  </main>

  {#if errorMessage}
    <div class="toast danger-toast" role="alert"><Icon name="info" size={16} /><span>{errorMessage}</span></div>
  {:else if message}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{message}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{message || errorMessage}</div>

  {#if rollbackCandidate}
    <div class="modal-backdrop">
      <div
        class="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rollback-confirm-title"
        tabindex="-1"
        bind:this={rollbackDialog}
        onkeydown={handleRollbackDialogKeydown}
      >
        <header>
          <h2 id="rollback-confirm-title">{t("data.rollback.title").replace("{name}", rollbackCandidate.instanceName)}</h2>
          <p>{t("data.rollback.description").replace("{time}", timestampLabel(rollbackCandidate.createdAtUnixSeconds)).replace("{count}", String(rollbackCandidate.worldCount))}</p>
        </header>
        <div class="confirmation-impact">
          <strong>{t("data.rollback.impactTitle")}</strong>
          <span>{t("data.rollback.impactBody")}</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingItem === rollbackCandidate.id} onclick={cancelRollback}>{t("common.cancel")}</button>
          <button class="button primary" disabled={changingItem === rollbackCandidate.id} onclick={() => void confirmRollback()}>
            {changingItem === rollbackCandidate.id ? t("data.rollback.rolling") : t("data.rollback.confirm")}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if purgeCandidate}
    <div class="modal-backdrop">
      <div
        class="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="purge-confirm-title"
        tabindex="-1"
        bind:this={purgeDialog}
        onkeydown={handlePurgeDialogKeydown}
      >
        <header>
          <h2 id="purge-confirm-title">{t("data.purge.title").replace("{name}", purgeCandidate.displayName)}</h2>
          <p>{t("data.purge.description").replace("{count}", "1").replace("{size}", formatBytes(purgeCandidate.sizeBytes))}</p>
        </header>
        <div class="confirmation-impact danger-impact">
          <strong>{t("data.purge.impactTitle")}</strong>
          <span>{t("data.purge.impactBody")}</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingItem === purgeCandidate.id} onclick={cancelPurge}>{t("common.cancel")}</button>
          <button class="button danger" disabled={changingItem === purgeCandidate.id} onclick={() => void purge()}>
            {changingItem === purgeCandidate.id ? t("data.purge.purging") : t("data.recycle.purge")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
