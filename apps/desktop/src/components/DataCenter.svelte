<script lang="ts">
  import { onMount, tick } from "svelte";

  import type {
    BackupState,
    BackupTrigger,
    InstanceWorldInfo,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
    RecycleBinItem,
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
  let rollbackCandidate = $state<WorldBackupSummary | null>(null);
  let rollbackDialog = $state<HTMLElement | null>(null);
  let message = $state("");
  let errorMessage = $state("");
  const totalBytes = $derived(items.reduce((sum, item) => sum + item.sizeBytes, 0));
  const backupBytes = $derived(backups.reduce((sum, backup) => sum + backup.archiveBytes, 0));

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
    try {
      worlds = worldInstanceId
        ? await runtime.listInstanceWorldDetails(worldInstanceId)
        : [];
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
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
      message = `已导入世界「${imported.name}」`;
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
      message = `已导出世界「${world.name}」（${formatBytes(bytes)}）`;
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
      message = "已回滚到所选备份，回滚前的进度保存在恢复点备份中";
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
      await runtime.restoreRecycleBinItem(item.id);
      items = items.filter((candidate) => candidate.id !== item.id);
      message = `已将「${item.displayName}」恢复到原位置`;
      await onInstancesChanged();
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
      message = `已永久删除 ${result.removedSubjects} 个实例，释放 ${formatBytes(result.releasedBytes)}`;
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
    return days === 0 ? "保留期已到" : `${days} 天后到期`;
  }

  function deletedLabel(item: RecycleBinItem): string {
    return timestampLabel(item.deletedAtUnixSeconds);
  }

  function timestampLabel(unixSeconds: number): string {
    return new Intl.DateTimeFormat("zh-CN", {
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
        return "启动前";
      case "postExit":
        return "退出后";
      case "manual":
        return "手动";
    }
  }

  function backupStateLabel(state: BackupState): string {
    switch (state) {
      case "ready":
        return "已完成";
      case "skipped":
        return "无世界，已跳过";
      case "failed":
        return "失败";
      case "staging":
        return "写入中";
    }
  }
</script>

<AppShell
  pageTitle="数据"
  dataDirectory={settings.dataDirectory}
  activeNavigation="data"
  navigationTargets={["home", "resources", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus="完全本地管理 · 不自动清理"
  taskStatus={changingItem ? "正在更新回收站" : `${backups.length} 个备份 · ${items.length} 个回收站项目`}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content data-content">
    <div class="data-scroll" data-scroll-region="main">
      <header class="data-heading">
        <div>
          <h1>数据与回收站</h1>
          <p>误删实例可在本地恢复。MoyuMax 不会未经同意自动清理回收站。</p>
        </div>
        <button class="button" disabled={loading || changingItem !== null} onclick={() => void loadItems()}>刷新</button>
      </header>

      <section class="data-overview" aria-label="本地数据摘要">
        <div><span>备份快照</span><strong>{backups.length}</strong></div>
        <div><span>备份占用</span><strong>{formatBytes(backupBytes)}</strong></div>
        <div><span>回收项目</span><strong>{items.length}</strong></div>
        <div><span>回收占用</span><strong>{formatBytes(totalBytes)}</strong></div>
      </section>

      {#if loading}
        <section class="data-loading" aria-live="polite">
          <div class="loading-line wide"></div>
          <div class="loading-line"></div>
          <span>正在读取本地回收站…</span>
        </section>
      {:else}
        <section class="backup-section" aria-labelledby="worlds-title">
          <header>
            <div>
              <h2 id="worlds-title">世界存档</h2>
              <p>按实例浏览世界并导入导出；回滚到备份前会先创建恢复点备份。</p>
            </div>
            <div class="world-toolbar">
              <label class="sr-live" for="world-instance-select">选择实例</label>
              <select id="world-instance-select" value={worldInstanceId} onchange={(event) => void selectWorldInstance(event)}>
                {#each worldInstances.filter((instance) => instance.state === "ready") as instance}
                  <option value={instance.id}>{instance.name}</option>
                {/each}
              </select>
              <button class="button ghost compact" disabled={worldBusy || !worldInstanceId} onclick={() => void importWorld()}>{worldBusy ? "处理中" : "导入世界"}</button>
            </div>
          </header>
          {#if !worldInstanceId}
            <div class="backup-empty-row">还没有可管理世界的实例。</div>
          {:else if worlds.length === 0}
            <div class="backup-empty-row">这个实例还没有世界存档。</div>
          {:else}
            <div class="backup-list">
              {#each worlds as world}
                <article class="backup-row">
                  <div>
                    <div class="backup-title-line"><h3>{world.name}</h3><span>世界</span></div>
                    <p>{formatBytes(world.sizeBytes)}{world.lastPlayedUnixSeconds ? ` · 最近游玩 ${timestampLabel(world.lastPlayedUnixSeconds)}` : ""}</p>
                  </div>
                  <button class="button ghost compact" disabled={worldBusy} onclick={() => void exportWorld(world)}>导出</button>
                </article>
              {/each}
            </div>
          {/if}
        </section>

        <section class="backup-section" aria-labelledby="world-backups-title">
          <header>
            <div>
              <h2 id="world-backups-title">世界备份</h2>
              <p>游戏启动前和退出后自动备份，默认保留每个实例最近 20 个成功快照。</p>
            </div>
          </header>
          {#if backups.length === 0}
            <div class="backup-empty-row">还没有备份记录。包含世界的实例启动后会在这里出现。</div>
          {:else}
            <div class="backup-list">
              {#each backups as backup}
                <article class:failed={backup.state === "failed"} class="backup-row">
                  <div>
                    <div class="backup-title-line">
                      <h3>{backup.instanceName}</h3>
                      <span>{backupTriggerLabel(backup.trigger)}</span>
                    </div>
                    <p>{timestampLabel(backup.createdAtUnixSeconds)} · {backup.worldCount} 个世界 · {formatBytes(backup.archiveBytes || backup.sourceBytes)}</p>
                    {#if backup.errorSummary}<small>{backup.errorSummary}</small>{/if}
                  </div>
                  <div class="backup-side">
                    <strong>{backupStateLabel(backup.state)}</strong>
                    {#if backup.state === "ready"}
                      <button
                        class="button ghost compact"
                        aria-label={`回滚实例“${backup.instanceName}”到此备份`}
                        disabled={changingItem !== null}
                        onclick={() => void askRollback(backup)}
                      >回滚</button>
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
              <h2 id="recycle-bin-title">回收站</h2>
              <p>实例默认保留 30 天，MoyuMax 不会未经同意自动清理。</p>
            </div>
          </header>
          {#if items.length === 0}
            <div class="data-empty compact-empty">
              <Icon name="database" size={26} />
              <h2>回收站为空</h2>
              <p>删除实例不会同时删除托管 Java。</p>
            </div>
          {:else}
            <div class="recycle-list" aria-label="回收站项目">
              {#each items as item}
                <article class:failed={item.state === "failed"} class="recycle-card">
                  <div class="recycle-copy">
                    <div class="recycle-title-line">
                      <h2>{item.displayName}</h2>
                      <span>{item.kind === "instance" ? "实例" : item.kind}</span>
                    </div>
                    <p>{formatBytes(item.sizeBytes)} · {expiryLabel(item)}</p>
                    <dl class="recycle-meta">
                      <div><dt>删除时间</dt><dd>{deletedLabel(item)}</dd></div>
                      <div><dt>原位置</dt><dd><code>{item.originalPath}</code></dd></div>
                    </dl>
                    {#if item.state === "failed"}
                      <small class="recycle-error">上次文件操作未能自动收敛，请保留两侧内容并查看诊断。</small>
                    {/if}
                  </div>
                  <div class="recycle-actions">
                    <button
                      class="button primary"
                      aria-label={`恢复“${item.displayName}”`}
                      disabled={item.state !== "ready" || changingItem !== null}
                      onclick={() => void restore(item)}
                    >{changingItem === item.id ? "正在恢复" : "恢复"}</button>
                    <button
                      class="button danger-subtle"
                      aria-label={`永久删除“${item.displayName}”`}
                      disabled={item.state !== "ready" || changingItem !== null}
                      onclick={() => void askPurge(item)}
                    >永久删除</button>
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
          <h2 id="rollback-confirm-title">回滚“{rollbackCandidate.instanceName}”的世界？</h2>
          <p>全部世界将恢复到 {timestampLabel(rollbackCandidate.createdAtUnixSeconds)} 的备份状态，共 {rollbackCandidate.worldCount} 个世界。</p>
        </header>
        <div class="confirmation-impact">
          <strong>当前进度会先保存为恢复点备份</strong>
          <span>回滚完成后如需撤销，可以在备份时间线中回滚到该恢复点。</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingItem === rollbackCandidate.id} onclick={cancelRollback}>取消</button>
          <button class="button primary" disabled={changingItem === rollbackCandidate.id} onclick={() => void confirmRollback()}>
            {changingItem === rollbackCandidate.id ? "正在回滚" : "创建恢复点并回滚"}
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
          <h2 id="purge-confirm-title">永久删除“{purgeCandidate.displayName}”？</h2>
          <p>将永久删除 1 个实例，共 {formatBytes(purgeCandidate.sizeBytes)}。</p>
        </header>
        <div class="confirmation-impact danger-impact">
          <strong>此操作无法恢复</strong>
          <span>实例目录、存档和实例级索引会被删除。托管 Java 与共享基础文件仍会保留。</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingItem === purgeCandidate.id} onclick={cancelPurge}>取消</button>
          <button class="button danger" disabled={changingItem === purgeCandidate.id} onclick={() => void purge()}>
            {changingItem === purgeCandidate.id ? "正在删除" : "永久删除"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
