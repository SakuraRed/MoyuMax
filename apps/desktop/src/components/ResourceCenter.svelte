<script lang="ts">
  import { onMount } from "svelte";

  import type {
    ContentInstallPreview,
    ContentUpdateInfo,
    InstalledContent,
    InstanceResource,
    InstanceResourceKind,
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

  const LOADER_NAMES: Record<string, string> = {
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };
  const eligibleInstances = $derived(
    instances.filter(
      (instance) => instance.state === "ready" && instance.loaderKind in LOADER_NAMES,
    ),
  );
  let selectedInstanceId = $state("");
  let installed = $state<InstalledContent[]>([]);
  let localLoading = $state(false);
  let localError = $state("");
  let updates = $state<ContentUpdateInfo[] | null>(null);
  let checkingUpdates = $state(false);
  let updateError = $state("");
  let autoUpdate = $state(false);
  let updateSubmitting = $state(false);
  let updateQueued = $state(false);
  let resources = $state<InstanceResource[]>([]);
  let worlds = $state<string[]>([]);
  let resourceError = $state("");
  let importing = $state(false);
  let datapackImportOpen = $state(false);
  let selectedWorld = $state("");
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
    updates = null;
    updateError = "";
    updateQueued = false;
    datapackImportOpen = false;
    resourceError = "";
    await loadInstalled();
  }

  async function loadInstalled(): Promise<void> {
    if (!selectedInstanceId) return;
    localLoading = true;
    localError = "";
    try {
      const [content, autoUpdateEnabled, resourceList, worldList] = await Promise.all([
        runtime.getInstalledContent(selectedInstanceId),
        runtime.getInstanceContentAutoUpdate(selectedInstanceId),
        runtime.listInstanceResources(selectedInstanceId),
        runtime.listInstanceWorlds(selectedInstanceId),
      ]);
      installed = content;
      autoUpdate = autoUpdateEnabled;
      resources = resourceList;
      worlds = worldList;
    } catch (error) {
      localError = error instanceof Error ? error.message : String(error);
    } finally {
      localLoading = false;
    }
  }

  async function checkUpdates(): Promise<void> {
    if (!selectedInstanceId) return;
    checkingUpdates = true;
    updateError = "";
    updateQueued = false;
    try {
      updates = await runtime.checkContentUpdates(selectedInstanceId);
    } catch (error) {
      updates = null;
      updateError = error instanceof Error ? error.message : String(error);
    } finally {
      checkingUpdates = false;
    }
  }

  async function planUpdates(projectIds: string[]): Promise<void> {
    updateSubmitting = true;
    updateError = "";
    updateQueued = false;
    try {
      await runtime.planContentUpdate(selectedInstanceId, projectIds);
      updates = (updates ?? []).filter(
        (update) => !projectIds.includes(update.projectId),
      );
      await onTasksChanged();
      updateQueued = true;
    } catch (error) {
      updateError = error instanceof Error ? error.message : String(error);
    } finally {
      updateSubmitting = false;
    }
  }

  async function toggleAutoUpdate(checked: boolean): Promise<void> {
    const previous = autoUpdate;
    autoUpdate = checked;
    updateError = "";
    try {
      await runtime.setInstanceContentAutoUpdate(selectedInstanceId, checked);
    } catch (error) {
      autoUpdate = previous;
      updateError = error instanceof Error ? error.message : String(error);
    }
  }

  function kindLabel(kind: InstanceResourceKind): string {
    return kind === "resourcepack" ? "资源包" : kind === "shader" ? "光影包" : "数据包";
  }

  async function importResource(kind: InstanceResourceKind, worldName?: string): Promise<void> {
    importing = true;
    resourceError = "";
    try {
      const path = await runtime.pickResourceFile(kind);
      if (!path) return;
      await runtime.importInstanceResource(selectedInstanceId, kind, path, worldName);
      resources = await runtime.listInstanceResources(selectedInstanceId);
      datapackImportOpen = false;
      selectedWorld = "";
    } catch (error) {
      resourceError = error instanceof Error ? error.message : String(error);
    } finally {
      importing = false;
    }
  }

  function openDatapackImport(): void {
    if (worlds.length === 0) {
      resourceError = "这个实例还没有世界，数据包需要先进入一个世界存档";
      return;
    }
    resourceError = "";
    selectedWorld = worlds[0] ?? "";
    datapackImportOpen = true;
  }

  async function toggleResource(resource: InstanceResource, enabled: boolean): Promise<void> {
    resourceError = "";
    try {
      const updated = await runtime.setInstanceResourceEnabled(resource.id, enabled);
      resources = resources.map((candidate) =>
        candidate.id === updated.id ? updated : candidate,
      );
    } catch (error) {
      resourceError = error instanceof Error ? error.message : String(error);
      resources = await runtime.listInstanceResources(selectedInstanceId);
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

  function loaderName(kind: string): string {
    return LOADER_NAMES[kind] ?? kind;
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
        <h2>还没有可管理内容的实例</h2>
        <p>先安装一个 Fabric、Quilt、Forge 或 NeoForge 游戏实例，再从 Modrinth 添加兼容模组。</p>
      </section>
    {:else}
      <label class="resource-instance-field">
        <span>目标实例</span>
        <select value={selectedInstanceId} onchange={(event) => void selectInstance(event)}>
          {#each eligibleInstances as instance}
            <option value={instance.id}>{instance.name} · Minecraft {instance.gameVersion} · {loaderName(instance.loaderKind)} {instance.loaderVersion ?? ""}</option>
          {/each}
        </select>
      </label>

      <section class="local-content-section" aria-labelledby="local-content-title">
        <header>
          <div><h2 id="local-content-title">本地已安装内容</h2><p>自动更新默认关闭，不会在后台改动实例。</p></div>
          <div class="local-content-actions">
            <button class="button ghost compact" disabled={localLoading || checkingUpdates} onclick={() => void checkUpdates()}>{checkingUpdates ? "正在检查" : "检查更新"}</button>
            <button class="button ghost compact" disabled={localLoading} onclick={() => void loadInstalled()}>刷新本地列表</button>
          </div>
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
        <label class="auto-update-toggle">
          <input
            type="checkbox"
            checked={autoUpdate}
            aria-label="按实例自动更新策略"
            onchange={(event) => void toggleAutoUpdate((event.currentTarget as HTMLInputElement).checked)}
          />
          <span><strong>按实例自动更新策略</strong><small>默认关闭。开启后提供“全部更新”入口；更新仍需你明确触发，不会在后台修改实例。</small></span>
        </label>
        {#if updateError}
          <div class="error-block" role="alert"><strong>更新检查或提交失败</strong><span>{updateError}</span></div>
        {/if}
        {#if updates !== null}
          <div class="content-update-panel" aria-label="可用更新清单">
            {#if updates.length === 0}
              <div class="local-content-empty">已安装内容均为最新兼容版本。</div>
            {:else}
              <div class="content-update-heading">
                <span>{updates.length} 项可用更新，更新前会自动创建恢复点</span>
                {#if autoUpdate && updates.length > 1}
                  <button class="button primary compact" disabled={updateSubmitting} onclick={() => void planUpdates((updates ?? []).map((update) => update.projectId))}>{updateSubmitting ? "正在提交" : "全部更新"}</button>
                {/if}
              </div>
              <div class="installed-content-list">
                {#each updates as update}
                  <article class="installed-content-row">
                    <div><strong>{update.projectTitle}</strong><small>{update.currentVersionNumber} → {update.latestVersionNumber}</small></div>
                    <button class="button compact" disabled={updateSubmitting} onclick={() => void planUpdates([update.projectId])}>更新</button>
                  </article>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
        {#if updateQueued}
          <div class="content-queued" role="status">
            <div><strong>内容更新任务已进入统一队列</strong><span>替换旧文件前会先创建恢复点，任何失败都会回滚到更新前状态。</span></div>
            <button class="button primary" onclick={onOpenTasks}>查看任务中心</button>
          </div>
        {/if}
      </section>

      <section class="local-content-section" aria-labelledby="instance-resource-title">
        <header>
          <div><h2 id="instance-resource-title">资源内容</h2><p>资源包、光影与数据包和实例隔离存放；游戏内仍需在选项中确认选用。</p></div>
          <div class="local-content-actions">
            <button class="button ghost compact" disabled={importing} onclick={() => void importResource("resourcepack")}>导入资源包</button>
            <button class="button ghost compact" disabled={importing} onclick={() => void importResource("shader")}>导入光影</button>
            <button class="button ghost compact" disabled={importing} onclick={openDatapackImport}>导入数据包</button>
          </div>
        </header>
        {#if resourceError}
          <div class="error-block" role="alert"><strong>资源操作失败</strong><span>{resourceError}</span></div>
        {/if}
        {#if datapackImportOpen}
          <div class="datapack-import-form" role="group" aria-label="选择数据包目标世界">
            <label>
              <span>目标世界</span>
              <select value={selectedWorld} onchange={(event) => { selectedWorld = (event.currentTarget as HTMLSelectElement).value; }}>
                {#each worlds as world}
                  <option value={world}>{world}</option>
                {/each}
              </select>
            </label>
            <div class="local-content-actions">
              <button class="button primary compact" disabled={importing || !selectedWorld} onclick={() => void importResource("datapack", selectedWorld)}>{importing ? "正在导入" : "选择文件并导入"}</button>
              <button class="button ghost compact" disabled={importing} onclick={() => { datapackImportOpen = false; }}>取消</button>
            </div>
          </div>
        {/if}
        {#if resources.length === 0}
          <div class="local-content-empty">还没有导入资源包、光影或数据包。删除与回收站能力随后续里程碑提供。</div>
        {:else}
          <div class="installed-content-list">
            {#each resources as resource}
              <article class="installed-content-row">
                <div>
                  <strong>{resource.displayName}</strong>
                  <small>{kindLabel(resource.kind)}{resource.worldName ? ` · 世界 ${resource.worldName}` : ""} · {resource.fileName}</small>
                </div>
                <label class="resource-enable-toggle">
                  <input
                    type="checkbox"
                    checked={resource.enabled}
                    aria-label={`${resource.displayName} 启用开关`}
                    onchange={(event) => void toggleResource(resource, (event.currentTarget as HTMLInputElement).checked)}
                  />
                  <span>{resource.enabled ? "已启用" : "已停用"}</span>
                </label>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      <section class="remote-content-section" aria-labelledby="remote-content-title">
        <header><h2 id="remote-content-title">从 Modrinth 添加</h2><p>结果已限定当前实例的 Minecraft 版本、加载器和客户端兼容性。</p></header>
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
