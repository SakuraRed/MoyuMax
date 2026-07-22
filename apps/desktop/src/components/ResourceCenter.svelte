<script lang="ts">
  import { onMount } from "svelte";

  import type {
    ContentInstallPreview,
    InstalledContent,
    ManagedInstance,
    ModrinthProjectSummary,
    ModrinthSearchPage,
    MoyuRuntime,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    instances: ManagedInstance[];
    onBack: () => void;
    onOpenTasks: () => void;
    onTasksChanged: () => Promise<void>;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    instances,
    onBack,
    onOpenTasks,
    onTasksChanged,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  const eligibleInstances = $derived(
    instances.filter(
      (instance) => instance.state === "ready" && instance.loaderKind === "fabric",
    ),
  );
  let selectedInstanceId = $state("");
  let installed = $state<InstalledContent[]>([]);
  let localLoading = $state(false);
  let localError = $state("");
  let query = $state("");
  let searching = $state(false);
  let searchError = $state("");
  let searchPage = $state<ModrinthSearchPage | null>(null);
  let previewingProject = $state("");
  let preview = $state<ContentInstallPreview | null>(null);
  let selectedOptionalProjects = $state<string[]>([]);
  let optionalSelectionDirty = $state(false);
  let submitting = $state(false);
  let queued = $state(false);

  onMount(() => {
    selectedInstanceId = eligibleInstances[0]?.id ?? "";
    if (selectedInstanceId) void loadInstalled();
  });

  async function selectInstance(event: Event): Promise<void> {
    selectedInstanceId = (event.currentTarget as HTMLSelectElement).value;
    preview = null;
    searchPage = null;
    queued = false;
    await loadInstalled();
  }

  async function loadInstalled(): Promise<void> {
    if (!selectedInstanceId) return;
    localLoading = true;
    localError = "";
    try {
      installed = await runtime.getInstalledContent(selectedInstanceId);
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error);
    } finally {
      localLoading = false;
    }
  }

  async function search(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const instance = eligibleInstances.find(
      (candidate) => candidate.id === selectedInstanceId,
    );
    if (!instance || !query.trim()) return;
    searching = true;
    searchError = "";
    searchPage = null;
    preview = null;
    queued = false;
    try {
      searchPage = await runtime.searchModrinthMods({
        query: query.trim(),
        gameVersion: instance.gameVersion,
        loader: instance.loaderKind,
        index: "relevance",
        offset: 0,
        limit: 20,
      });
    } catch (error) {
      searchError = error instanceof Error ? error.message : String(error);
    } finally {
      searching = false;
    }
  }

  async function createPreview(project: ModrinthProjectSummary): Promise<void> {
    previewingProject = project.projectId;
    searchError = "";
    queued = false;
    selectedOptionalProjects = [];
    try {
      preview = await runtime.previewModrinthInstall(
        selectedInstanceId,
        project.projectId,
        [],
      );
      optionalSelectionDirty = false;
    } catch (error) {
      searchError = error instanceof Error ? error.message : String(error);
    } finally {
      previewingProject = "";
    }
  }

  function toggleOptional(projectId: string, checked: boolean): void {
    selectedOptionalProjects = checked
      ? [...selectedOptionalProjects, projectId]
      : selectedOptionalProjects.filter((candidate) => candidate !== projectId);
    optionalSelectionDirty = true;
  }

  async function applyOptionalSelection(): Promise<void> {
    if (!preview) return;
    previewingProject = preview.plan.rootProjectId;
    searchError = "";
    try {
      preview = await runtime.previewModrinthInstall(
        selectedInstanceId,
        preview.plan.rootProjectId,
        selectedOptionalProjects,
      );
      optionalSelectionDirty = false;
    } catch (error) {
      searchError = error instanceof Error ? error.message : String(error);
    } finally {
      previewingProject = "";
    }
  }

  async function confirm(): Promise<void> {
    if (!preview || optionalSelectionDirty) return;
    submitting = true;
    searchError = "";
    try {
      await runtime.confirmContentPreview(preview.id);
      await onTasksChanged();
      queued = true;
    } catch (error) {
      searchError = error instanceof Error ? error.message : String(error);
    } finally {
      submitting = false;
    }
  }

  function bytes(value: number): string {
    if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MiB`;
    if (value >= 1024) return `${(value / 1024).toFixed(1)} KiB`;
    return `${value} B`;
  }
</script>

<AppShell
  pageTitle="资源"
  dataDirectory={settings.dataDirectory}
  activeNavigation="resources"
  navigationTargets={["home", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus={searchError ? "远程内容服务不可用 · 本地索引可用" : "Modrinth 按需联网 · 本地内容离线可用"}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content resource-content">
    <header class="resource-heading">
      <div>
        <h1>实例内容</h1>
        <p>本地列表始终离线可用。只有主动搜索或解析安装计划时才访问 Modrinth。</p>
      </div>
      <button class="button ghost compact" onclick={onBack}>返回首页</button>
    </header>

    {#if eligibleInstances.length === 0}
      <section class="resource-empty">
        <Icon name="compass" size={28} />
        <h2>还没有可管理的 Fabric 实例</h2>
        <p>先安装一个 Fabric 游戏实例，再从 Modrinth 添加兼容模组。</p>
      </section>
    {:else}
      <label class="resource-instance-field">
        <span>目标实例</span>
        <select value={selectedInstanceId} onchange={(event) => void selectInstance(event)}>
          {#each eligibleInstances as instance}
            <option value={instance.id}>{instance.name} · Minecraft {instance.gameVersion} · Fabric {instance.loaderVersion ?? ""}</option>
          {/each}
        </select>
      </label>

      <section class="local-content-section" aria-labelledby="local-content-title">
        <header>
          <div><h2 id="local-content-title">本地已安装内容</h2><p>自动更新默认关闭，不会在后台改动实例。</p></div>
          <button class="button ghost compact" disabled={localLoading} onclick={() => void loadInstalled()}>刷新本地列表</button>
        </header>
        {#if localError}
          <div class="error-block" role="alert"><strong>无法读取本地内容索引</strong><span>{localError}</span></div>
        {:else if localLoading}
          <div class="content-loading" aria-live="polite"><span>正在读取本地索引</span></div>
        {:else if installed.length === 0}
          <div class="local-content-empty">这个实例还没有由 MoyuMax 索引的模组。</div>
        {:else}
          <div class="installed-content-list">
            {#each installed as entry}
              <article class="installed-content-row">
                <div><strong>{entry.projectTitle}</strong><small>{entry.versionNumber} · {entry.fileName}</small></div>
                <span>{entry.autoUpdateEnabled ? "自动更新开启" : "自动更新关闭"}</span>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <section class="remote-content-section" aria-labelledby="remote-content-title">
        <header><h2 id="remote-content-title">从 Modrinth 添加</h2><p>结果已限定当前实例的 Minecraft 版本、Fabric 和客户端兼容性。</p></header>
        <form class="content-search" onsubmit={(event) => void search(event)}>
          <label>
            <span class="sr-live">搜索 Modrinth 模组</span>
            <Icon name="search" size={15} />
            <input bind:value={query} type="search" aria-label="搜索 Modrinth 模组" placeholder="输入模组名称" />
          </label>
          <button class="button primary" disabled={searching || !query.trim()}>{searching ? "正在搜索" : "搜索兼容模组"}</button>
        </form>

        {#if searchError}
          <div class="error-block content-search-error" role="alert">
            <strong>远程搜索不可用，本地内容不受影响</strong>
            <span>{searchError}</span>
          </div>
        {/if}

        {#if searchPage && searchPage.hits.length === 0}
          <div class="content-search-empty">没有找到与当前实例兼容的客户端模组。</div>
        {:else if searchPage}
          <div class="content-result-list" aria-label="Modrinth 搜索结果">
            {#each searchPage.hits as project}
              <article class="content-result-card">
                <div>
                  <strong>{project.title}</strong>
                  <p>{project.description}</p>
                  <small>Modrinth 项目 {project.projectId} · {project.downloads.toLocaleString()} 次下载</small>
                </div>
                <button class="button" disabled={Boolean(previewingProject)} onclick={() => void createPreview(project)}>
                  {previewingProject === project.projectId ? "正在解析依赖" : "查看安装计划"}
                </button>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      {#if preview}
        <section class="content-preview" aria-labelledby="content-preview-title">
          <header><h2 id="content-preview-title">确认依赖与文件</h2><p>只有目标模组、必需依赖以及你明确选择的可选依赖会进入事务。</p></header>
          <div class="content-plan-list">
            {#each preview.plan.entries as entry}
              <article class="content-plan-row">
                <div><strong>{entry.projectTitle}</strong><small>{entry.versionNumber} · {entry.file.filename} · {bytes(entry.file.size)}</small></div>
                <span>{entry.projectId === preview.plan.rootProjectId ? "目标模组" : selectedOptionalProjects.includes(entry.projectId) ? "已选可选依赖" : "必需依赖"}</span>
              </article>
            {/each}
          </div>
          {#if preview.plan.optionalDependencies.length > 0}
            <fieldset class="optional-content-list">
              <legend>可选依赖，默认不安装</legend>
              {#each preview.plan.optionalDependencies as dependency}
                {#if dependency.projectId}
                  <label>
                    <input
                      type="checkbox"
                      checked={selectedOptionalProjects.includes(dependency.projectId)}
                      onchange={(event) => toggleOptional(dependency.projectId!, (event.currentTarget as HTMLInputElement).checked)}
                    />
                    <span><strong>{dependency.title}</strong><small>由 {dependency.requiredByProjectId} 声明，可不安装</small></span>
                  </label>
                {/if}
              {/each}
            </fieldset>
          {/if}
          {#if preview.plan.incompatibleDependencies.length > 0}
            <div class="warning-panel"><strong>发现不兼容声明</strong><span>提交时会与本地索引和本次计划再次核对；存在实际冲突时不会安装。</span></div>
          {/if}
          <div class="content-preview-actions">
            {#if optionalSelectionDirty}
              <button class="button" disabled={Boolean(previewingProject)} onclick={() => void applyOptionalSelection()}>应用可选依赖选择</button>
            {/if}
            <button class="button primary" disabled={submitting || optionalSelectionDirty} onclick={() => void confirm()}>{submitting ? "正在提交" : "确认并加入任务"}</button>
          </div>
        </section>
      {/if}

      {#if queued}
        <div class="content-queued" role="status">
          <div><strong>内容安装任务已进入统一队列</strong><span>下载、校验、文件发布和索引写入将在同一持久化任务中完成。</span></div>
          <button class="button primary" onclick={onOpenTasks}>查看任务中心</button>
        </div>
      {/if}
    {/if}
  </main>
</AppShell>
