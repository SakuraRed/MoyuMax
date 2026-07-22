<script lang="ts">
  import { onMount, tick } from "svelte";

  import type { MoyuRuntime, OnboardingSelection, RecycleBinItem } from "../runtime";
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
  let loading = $state(true);
  let changingItem = $state<string | null>(null);
  let purgeCandidate = $state<RecycleBinItem | null>(null);
  let purgeDialog = $state<HTMLElement | null>(null);
  let message = $state("");
  let errorMessage = $state("");
  const totalBytes = $derived(items.reduce((sum, item) => sum + item.sizeBytes, 0));

  onMount(() => {
    void loadItems();
  });

  async function loadItems(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      items = await runtime.listRecycleBinItems();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
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
    return new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(item.deletedAtUnixSeconds * 1000));
  }
</script>

<AppShell
  pageTitle="数据"
  dataDirectory={settings.dataDirectory}
  activeNavigation="data"
  navigationTargets={["home", "resources", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus="完全本地管理 · 不自动清理"
  taskStatus={changingItem ? "正在更新回收站" : `${items.length} 个回收站项目`}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content data-content">
    <div class="data-scroll">
      <header class="data-heading">
        <div>
          <h1>数据与回收站</h1>
          <p>误删实例可在本地恢复。MoyuMax 不会未经同意自动清理回收站。</p>
        </div>
        <button class="button" disabled={loading || changingItem !== null} onclick={() => void loadItems()}>刷新</button>
      </header>

      <section class="data-overview" aria-label="回收站摘要">
        <div><span>项目</span><strong>{items.length}</strong></div>
        <div><span>占用空间</span><strong>{formatBytes(totalBytes)}</strong></div>
        <div><span>默认保留</span><strong>30 天</strong></div>
      </section>

      {#if loading}
        <section class="data-loading" aria-live="polite">
          <div class="loading-line wide"></div>
          <div class="loading-line"></div>
          <span>正在读取本地回收站…</span>
        </section>
      {:else if items.length === 0}
        <section class="data-empty">
          <Icon name="database" size={30} />
          <h2>回收站为空</h2>
          <p>从实例页面删除的内容会先保留在这里，不会同时删除托管 Java。</p>
        </section>
      {:else}
        <section class="recycle-list" aria-label="回收站项目">
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
