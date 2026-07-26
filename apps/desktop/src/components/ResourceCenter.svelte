<script lang="ts">
  import { onMount } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import { mcmodEntryFor, mcmodSearchUrl } from "../mcmod-zh";
  import type {
    ContentInstallPreview,
    ContentUpdateInfo,
    InstalledContent,
    InstanceResource,
    InstanceResourceKind,
    ManagedInstance,
    ModpackPreviewResponse,
    ModrinthProjectSummary,
    ModrinthProjectType,
    ModrinthSearchPage,
    ModrinthVersionSummary,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    instances: ManagedInstance[];
    onOpenTasks: () => void;
    onTasksChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    instances,
    onOpenTasks,
    onTasksChanged,
    onNavigate,
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
  const CATALOG_TYPES: { key: ModrinthProjectType; labelKey: string }[] = [
    { key: "mod", labelKey: "resources.catalog.type.mod" },
    { key: "modpack", labelKey: "resources.catalog.type.modpack" },
    { key: "shader", labelKey: "resources.catalog.type.shader" },
    { key: "resourcepack", labelKey: "resources.catalog.type.resourcepack" },
  ];
  const CATALOG_CATEGORIES: { value: string; labelKey: string }[] = [
    { value: "optimization", labelKey: "resources.catalog.category.optimization" },
    { value: "technology", labelKey: "resources.catalog.category.technology" },
    { value: "magic", labelKey: "resources.catalog.category.magic" },
    { value: "adventure", labelKey: "resources.catalog.category.adventure" },
    { value: "decoration", labelKey: "resources.catalog.category.decoration" },
    { value: "utility", labelKey: "resources.catalog.category.utility" },
    { value: "worldgen", labelKey: "resources.catalog.category.worldgen" },
    { value: "food", labelKey: "resources.catalog.category.food" },
    { value: "storage", labelKey: "resources.catalog.category.storage" },
    { value: "equipment", labelKey: "resources.catalog.category.equipment" },
    { value: "library", labelKey: "resources.catalog.category.library" },
    { value: "mobs", labelKey: "resources.catalog.category.mobs" },
  ];
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
  let pendingResourceDelete = $state<string | null>(null);
  let previewingProject = $state("");
  let preview = $state<ContentInstallPreview | null>(null);
  let selectedOptionalProjects = $state<string[]>([]);
  let optionalSelectionDirty = $state(false);
  let submitting = $state(false);
  let queued = $state(false);

  let tab = $state<"catalog" | "instances">("catalog");
  let catalogType = $state<ModrinthProjectType>("mod");
  let catalogQuery = $state("");
  let catalogSearching = $state(false);
  let catalogError = $state("");
  let catalogPage = $state<ModrinthSearchPage | null>(null);
  let catalogHits = $state<ModrinthProjectSummary[]>([]);
  let loadingMore = $state(false);
  let sortIndex = $state<"relevance" | "downloads" | "updated">("downloads");
  let filterVersion = $state("");
  let filterLoader = $state("");
  let filterCategory = $state("");
  let packPreview = $state<ModpackPreviewResponse | null>(null);
  let packPreviewing = $state("");
  let packInstalling = $state(false);
  let packDone = $state("");
  let resourceInstalling = $state("");
  let resourceInstallDone = $state("");
  let downloadTarget = $state<ModrinthProjectSummary | null>(null);
  let downloadVersions = $state<ModrinthVersionSummary[]>([]);
  let downloadVersionId = $state("");
  let downloadFileName = $state("");
  let downloadDest = $state<"instance" | "custom">("instance");
  let downloadCustomDir = $state("");
  let downloadLoadingVersions = $state(false);
  let downloading = $state(false);
  let downloadDone = $state("");

  onMount(() => {
    selectedInstanceId = eligibleInstances[0]?.id ?? "";
    if (selectedInstanceId) {
      const instance = eligibleInstances[0];
      filterVersion = instance?.gameVersion ?? "";
      filterLoader = instance?.loaderKind ?? "";
      void loadInstalled();
    }
    void runCatalogSearch(true);
  });

  async function selectInstance(event: Event): Promise<void> {
    selectedInstanceId = (event.currentTarget as HTMLSelectElement).value;
    const instance = selectedInstance();
    filterVersion = instance?.gameVersion ?? filterVersion;
    filterLoader = instance?.loaderKind ?? filterLoader;
    preview = null;
    queued = false;
    updates = null;
    updateError = "";
    updateQueued = false;
    datapackImportOpen = false;
    resourceError = "";
    await loadInstalled();
    if (tab === "catalog") void runCatalogSearch(true);
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
    return kind === "resourcepack" ? t("resources.kind.resourcepack") : kind === "shader" ? t("resources.kind.shader") : kind === "mod" ? t("resources.kind.mod") : t("resources.kind.datapack");
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
      resourceError = t("resources.files.noWorld");
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

  async function deleteResource(resource: InstanceResource): Promise<void> {
    resourceError = "";
    try {
      await runtime.deleteInstanceResource(resource.id);
      pendingResourceDelete = null;
      resources = await runtime.listInstanceResources(selectedInstanceId);
    } catch (error) {
      resourceError = error instanceof Error ? error.message : String(error);
    }
  }

  function selectCatalogType(type: ModrinthProjectType): void {
    catalogType = type;
    catalogPage = null;
    catalogHits = [];
    catalogError = "";
    packPreview = null;
    packDone = "";
    resourceInstallDone = "";
    preview = null;
    queued = false;
    void runCatalogSearch(true);
  }

  function selectedInstance(): ManagedInstance | undefined {
    return eligibleInstances.find((candidate) => candidate.id === selectedInstanceId);
  }

  const CATALOG_PAGE_SIZE = 20;

  /** 目录查询：browse=true 为重载（搜索词可为空 = 热门浏览），false 为加载更多。 */
  async function runCatalogSearch(fresh: boolean): Promise<void> {
    if (fresh) {
      catalogSearching = true;
      catalogPage = null;
      catalogHits = [];
      packPreview = null;
      packDone = "";
      resourceInstallDone = "";
      preview = null;
      queued = false;
    } else {
      loadingMore = true;
    }
    catalogError = "";
    const instance = selectedInstance();
    const gameVersion =
      filterVersion.trim() ||
      (catalogType === "modpack" ? "" : (instance?.gameVersion ?? ""));
    const loader =
      filterLoader || (catalogType === "mod" ? (instance?.loaderKind ?? "") : "");
    try {
      const page = await runtime.searchModrinthMods({
        query: catalogQuery.trim(),
        gameVersion: catalogType === "modpack" ? "" : gameVersion,
        loader: catalogType === "mod" ? loader : "",
        index: catalogQuery.trim() ? "relevance" : sortIndex,
        offset: fresh ? 0 : catalogHits.length,
        limit: CATALOG_PAGE_SIZE,
        projectType: catalogType,
        category: filterCategory,
      });
      catalogPage = page;
      catalogHits = fresh ? page.hits : [...catalogHits, ...page.hits];
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      catalogSearching = false;
      loadingMore = false;
    }
  }

  function searchCatalog(event?: SubmitEvent): void {
    event?.preventDefault();
    void runCatalogSearch(true);
  }

  function applyFilters(): void {
    void runCatalogSearch(true);
  }

  function formatDownloads(value: number): string {
    if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
    if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
    return String(value);
  }

  function formatDate(value: string | null): string {
    return value ? value.slice(0, 10) : "";
  }

  function latestVersion(versions: string[]): string {
    return versions.length > 0 ? versions[versions.length - 1] ?? "" : "";
  }

  async function createPreview(project: ModrinthProjectSummary): Promise<void> {
    previewingProject = project.projectId;
    catalogError = "";
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
      catalogError = error instanceof Error ? error.message : String(error);
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
    catalogError = "";
    try {
      preview = await runtime.previewModrinthInstall(
        selectedInstanceId,
        preview.plan.rootProjectId,
        selectedOptionalProjects,
      );
      optionalSelectionDirty = false;
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      previewingProject = "";
    }
  }

  async function confirm(): Promise<void> {
    if (!preview || optionalSelectionDirty) return;
    submitting = true;
    catalogError = "";
    try {
      await runtime.confirmContentPreview(preview.id);
      await onTasksChanged();
      queued = true;
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      submitting = false;
    }
  }

  async function previewPack(project: ModrinthProjectSummary): Promise<void> {
    packPreviewing = project.projectId;
    catalogError = "";
    packDone = "";
    try {
      packPreview = await runtime.previewOnlineModpack(project.projectId);
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      packPreviewing = "";
    }
  }

  async function confirmPackInstall(): Promise<void> {
    if (!packPreview) return;
    packInstalling = true;
    catalogError = "";
    try {
      const report = await runtime.installModpack(packPreview.id);
      packDone = report.packName;
      packPreview = null;
      await onTasksChanged();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      packInstalling = false;
    }
  }

  async function installResourceToInstance(project: ModrinthProjectSummary): Promise<void> {
    const instance = selectedInstance();
    if (!instance || (catalogType !== "shader" && catalogType !== "resourcepack")) return;
    resourceInstalling = project.projectId;
    catalogError = "";
    resourceInstallDone = "";
    try {
      await runtime.installOnlineResource(instance.id, catalogType, project.projectId);
      resourceInstallDone = `${project.title} → ${instance.name}`;
      await loadInstalled();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      resourceInstalling = "";
    }
  }

  function loaderName(kind: string): string {
    return LOADER_NAMES[kind] ?? kind;
  }

  function defaultFileExtension(): string {
    return catalogType === "modpack" ? ".mrpack" : catalogType === "mod" ? ".jar" : ".zip";
  }

  async function openDownloadDialog(project: ModrinthProjectSummary): Promise<void> {
    downloadTarget = project;
    downloadVersions = [];
    downloadVersionId = "";
    downloadFileName = "";
    downloadLoadingVersions = true;
    catalogError = "";
    downloadDone = "";
    const instance = selectedInstance();
    downloadDest = instance ? "instance" : "custom";
    try {
      const versions = await runtime.listModrinthVersions(
        project.projectId,
        catalogType === "modpack" ? undefined : filterVersion || undefined,
        catalogType === "mod" ? filterLoader || undefined : undefined,
      );
      if (versions.length === 0) {
        catalogError = t("resources.download.noVersions");
        downloadTarget = null;
        return;
      }
      downloadVersions = versions;
      selectDownloadVersion(versions[0]?.id ?? "");
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
      downloadTarget = null;
    } finally {
      downloadLoadingVersions = false;
    }
  }

  function selectDownloadVersion(versionId: string): void {
    downloadVersionId = versionId;
    const version = downloadVersions.find((candidate) => candidate.id === versionId);
    const slug = downloadTarget?.slug ?? "download";
    downloadFileName = `${slug}-${version?.versionNumber ?? ""}${defaultFileExtension()}`;
  }

  function downloadTargetDir(): string {
    const instance = selectedInstance();
    if (downloadDest === "instance" && instance) {
      const sub = catalogType === "mod" ? "mods" : catalogType === "shader" ? "shaderpacks" : catalogType === "resourcepack" ? "resourcepacks" : "modpacks";
      return `${instance.rootDirectory}\\${sub}`;
    }
    return downloadCustomDir;
  }

  async function pickDownloadDir(): Promise<void> {
    const selected = await runtime.pickDirectory();
    if (selected) downloadCustomDir = selected;
  }

  async function confirmDownload(): Promise<void> {
    const targetDir = downloadTargetDir();
    if (!downloadVersionId || !targetDir || !downloadFileName.trim()) return;
    downloading = true;
    catalogError = "";
    downloadDone = "";
    try {
      const path = await runtime.downloadModrinthFile(
        downloadVersionId,
        targetDir,
        downloadFileName.trim(),
      );
      downloadDone = path;
      downloadTarget = null;
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      downloading = false;
    }
  }

  function bytes(value: number): string {
    if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MiB`;
    if (value >= 1024) return `${(value / 1024).toFixed(1)} KiB`;
    return `${value} B`;
  }
</script>

<AppShell
  pageTitle={t("nav.resources")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="resources"
  {onNavigate}
  connectionStatus={catalogError ? t("resources.connection.offline") : t("resources.connection.online")}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content resource-content">
    <header class="resource-heading">
      <div>
        <h1>{t("resources.heading.title")}</h1>
      </div>
      <nav class="resource-tabs" aria-label={t("resources.tabs.aria")}>
        <button class:active={tab === "catalog"} onclick={() => { tab = "catalog"; }}>{t("resources.tabs.catalog")}</button>
        <button class:active={tab === "instances"} onclick={() => { tab = "instances"; }}>{t("resources.tabs.instances")}</button>
      </nav>
    </header>

    {#if tab === "catalog"}
      <div class="catalog-chips" role="group" aria-label={t("resources.catalog.typeAria")}>
        {#each CATALOG_TYPES as catalogTypeOption}
          <button
            class:active={catalogType === catalogTypeOption.key}
            aria-pressed={catalogType === catalogTypeOption.key}
            onclick={() => selectCatalogType(catalogTypeOption.key)}
          >{t(catalogTypeOption.labelKey)}</button>
        {/each}
      </div>

      {#if catalogType !== "modpack" && eligibleInstances.length === 0}
        <div class="catalog-instance-hint">
          <span>{t("resources.catalog.needInstance")}</span>
          <button class="inline-link" onclick={() => onNavigate("instances")}>{t("resources.catalog.createInstance")}</button>
        </div>
      {/if}

      <form class="catalog-searchbar" onsubmit={(event) => searchCatalog(event)}>
        <label class="catalog-searchbox">
          <span class="sr-live">{t("resources.catalog.searchLabel")}</span>
          <Icon name="search" size={15} />
          <input bind:value={catalogQuery} type="search" aria-label={t("resources.catalog.searchLabel")} placeholder={t("resources.catalog.searchPlaceholder")} oninput={() => { if (!catalogQuery.trim()) applyFilters(); }} />
        </label>
        <button class="button primary" disabled={catalogSearching || (catalogType === "mod" && eligibleInstances.length === 0)}>{catalogSearching ? t("resources.catalog.searching") : t("resources.catalog.searchSubmit")}</button>
      </form>

      <div class="catalog-filters" role="group" aria-label={t("resources.catalog.filtersAria")}>
        {#if catalogType !== "modpack" && eligibleInstances.length > 0}
          <label class="catalog-filter">
            <span>{t("resources.instanceLabel")}</span>
            <select value={selectedInstanceId} onchange={(event) => void selectInstance(event)} aria-label={t("resources.instanceLabel")}>
              {#each eligibleInstances as instance}
                <option value={instance.id}>{instance.name}</option>
              {/each}
            </select>
          </label>
          <label class="catalog-filter">
            <span>{t("resources.catalog.filterVersion")}</span>
            <input
              value={filterVersion}
              oninput={(event) => { filterVersion = (event.currentTarget as HTMLInputElement).value; }}
              onchange={() => applyFilters()}
              list="catalog-version-options"
              aria-label={t("resources.catalog.filterVersion")}
              placeholder={t("resources.catalog.filterVersionAll")}
            />
            <datalist id="catalog-version-options">
              {#each [...new Set(instances.map((instance) => instance.gameVersion))] as version}
                <option value={version}></option>
              {/each}
            </datalist>
          </label>
        {/if}
        {#if catalogType === "mod"}
          <label class="catalog-filter">
            <span>{t("resources.catalog.filterLoader")}</span>
            <select value={filterLoader} onchange={(event) => { filterLoader = (event.currentTarget as HTMLSelectElement).value; applyFilters(); }} aria-label={t("resources.catalog.filterLoader")}>
              <option value="">{t("resources.catalog.filterLoaderAll")}</option>
              {#each Object.entries(LOADER_NAMES) as [kind, name]}
                <option value={kind}>{name}</option>
              {/each}
            </select>
          </label>
        {/if}
        <label class="catalog-filter">
          <span>{t("resources.catalog.filterCategory")}</span>
          <select value={filterCategory} onchange={(event) => { filterCategory = (event.currentTarget as HTMLSelectElement).value; applyFilters(); }} aria-label={t("resources.catalog.filterCategory")}>
            <option value="">{t("resources.catalog.filterCategoryAll")}</option>
            {#each CATALOG_CATEGORIES as category}
              <option value={category.value}>{t(category.labelKey)}</option>
            {/each}
          </select>
        </label>
        <label class="catalog-filter">
          <span>{t("resources.catalog.filterSort")}</span>
          <select value={sortIndex} onchange={(event) => { sortIndex = (event.currentTarget as HTMLSelectElement).value as typeof sortIndex; applyFilters(); }} aria-label={t("resources.catalog.filterSort")}>
            <option value="downloads">{t("resources.catalog.sortDownloads")}</option>
            <option value="updated">{t("resources.catalog.sortUpdated")}</option>
            <option value="relevance">{t("resources.catalog.sortRelevance")}</option>
          </select>
        </label>
        {#if catalogType === "modpack"}
          <span class="catalog-cf-inline">
            {t("resources.catalog.cfHint")}
            <button class="inline-link" onclick={() => onNavigate("instances")}>{t("resources.catalog.cfImport")}</button>
          </span>
        {/if}
      </div>

      {#if catalogError}
        <div class="error-block content-search-error" role="alert">
          <strong>{t("resources.catalog.errorTitle")}</strong>
          <span>{catalogError}</span>
        </div>
      {/if}
      {#if packDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.catalog.packDone").replace("{name}", packDone)}</strong><span>{t("resources.catalog.packDoneHint")}</span></div>
          <button class="button primary" onclick={() => onNavigate("home")}>{t("resources.catalog.viewHome")}</button>
        </div>
      {/if}
      {#if resourceInstallDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.catalog.resourceDone").replace("{name}", resourceInstallDone)}</strong></div>
        </div>
      {/if}

      {#if packPreview}
        <section class="content-preview" aria-labelledby="pack-preview-title">
          <header><h2 id="pack-preview-title">{t("resources.catalog.packPreviewTitle")}</h2></header>
          <div class="content-plan-list">
            <article class="content-plan-row">
              <div>
                <strong>{packPreview.preview.name} {packPreview.preview.version}</strong>
                <small>Minecraft {packPreview.preview.gameVersion} · {loaderName(packPreview.preview.loaderKind)} {packPreview.preview.loaderVersion}</small>
              </div>
              <span>{t("resources.catalog.packFiles").replace("{count}", String(packPreview.preview.fileCount)).replace("{size}", bytes(packPreview.preview.totalBytes))}</span>
            </article>
          </div>
          <div class="content-preview-actions">
            <button class="button primary" disabled={packInstalling} onclick={() => void confirmPackInstall()}>{packInstalling ? t("resources.catalog.installing") : t("resources.catalog.confirmInstall")}</button>
            <button class="button ghost" disabled={packInstalling} onclick={() => { packPreview = null; }}>{t("common.cancel")}</button>
          </div>
        </section>
      {/if}

      {#if downloadDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.download.done")}</strong><span>{downloadDone}</span></div>
        </div>
      {/if}

      {#if catalogSearching && catalogHits.length === 0}
        <div class="content-loading" aria-live="polite"><span>{t("resources.catalog.searching")}</span></div>
      {:else if catalogPage && catalogHits.length === 0}
        <div class="content-search-empty">{t("resources.catalog.noResults")}</div>
      {:else if catalogHits.length > 0}
        <div class="content-result-list" aria-label={t("resources.catalog.resultAria")}>
          {#each catalogHits as project}
            <article class="content-result-card">
              <div class="result-icon" aria-hidden="true">
                {#if project.iconUrl}
                  <img src={project.iconUrl} alt="" loading="lazy" />
                {:else}
                  <span>{project.title.slice(0, 1)}</span>
                {/if}
              </div>
              <div class="result-main">
                <div class="result-title-line">
                  <strong>{project.title}</strong>
                  {#if project.author}<span class="result-author">by {project.author}</span>{/if}
                  {#if latestVersion(project.versions)}
                    <span class="result-version-badge">{latestVersion(project.versions)}</span>
                  {/if}
                </div>
                <p>{project.description}</p>
                {#if uiLanguage() === "zh-CN" || uiLanguage() === "zh-TW"}
                  {@const mcmod = mcmodEntryFor(project.slug)}
                  <div class="result-mcmod">
                    {#if mcmod}
                      <strong>{mcmod.zhName}</strong>
                      <span>{mcmod.zhDescription}</span>
                      <button class="inline-link mcmod-link" onclick={() => void runtime.openExternalUrl(mcmod.mcmodUrl)}>{t("resources.catalog.mcmodLink")}</button>
                    {:else}
                      <button class="inline-link mcmod-link mcmod-fallback" onclick={() => void runtime.openExternalUrl(mcmodSearchUrl(project.title))}>{t("resources.catalog.mcmodSearch")}</button>
                    {/if}
                  </div>
                {/if}
              </div>
              <div class="result-side">
                <span class="result-downloads">{t("resources.catalog.downloads").replace("{count}", formatDownloads(project.downloads))}</span>
                {#if project.dateModified}<span class="result-date">{formatDate(project.dateModified)}</span>{/if}
                <div class="result-actions">
                  {#if catalogType === "mod"}
                    <button class="button compact" disabled={Boolean(previewingProject)} onclick={() => void createPreview(project)}>
                      {previewingProject === project.projectId ? t("resources.catalog.parsing") : t("resources.catalog.viewPlan")}
                    </button>
                  {:else if catalogType === "modpack"}
                    <button class="button compact" disabled={Boolean(packPreviewing) || packInstalling} onclick={() => void previewPack(project)}>
                      {packPreviewing === project.projectId ? t("resources.catalog.parsing") : t("resources.catalog.install")}
                    </button>
                  {:else}
                    <button class="button compact" disabled={Boolean(resourceInstalling) || !selectedInstanceId} onclick={() => void installResourceToInstance(project)}>
                      {resourceInstalling === project.projectId ? t("resources.catalog.installing") : t("resources.catalog.install")}
                    </button>
                  {/if}
                  <button class="button ghost compact" disabled={downloading} onclick={() => void openDownloadDialog(project)}>{t("resources.download.button")}</button>
                </div>
              </div>
            </article>
          {/each}
        </div>
        {#if catalogPage && catalogHits.length < catalogPage.totalHits}
          <div class="catalog-loadmore">
            <button class="button ghost" disabled={loadingMore} onclick={() => void runCatalogSearch(false)}>
              {loadingMore ? t("resources.catalog.loadingMore") : t("resources.catalog.loadMore").replace("{count}", String(catalogPage.totalHits - catalogHits.length))}
            </button>
          </div>
        {/if}
      {/if}

      {#if preview}
        <section class="content-preview" aria-labelledby="content-preview-title">
          <header><h2 id="content-preview-title">{t("resources.preview.title")}</h2></header>
          <div class="content-plan-list">
            {#each preview.plan.entries as entry}
              <article class="content-plan-row">
                <div><strong>{entry.projectTitle}</strong><small>{entry.versionNumber} · {entry.file.filename} · {bytes(entry.file.size)}</small></div>
                <span>{entry.projectId === preview.plan.rootProjectId ? t("resources.preview.role.target") : selectedOptionalProjects.includes(entry.projectId) ? t("resources.preview.role.optional") : t("resources.preview.role.required")}</span>
              </article>
            {/each}
          </div>
          {#if preview.plan.optionalDependencies.length > 0}
            <fieldset class="optional-content-list">
              <legend>{t("resources.preview.optionalLegend")}</legend>
              {#each preview.plan.optionalDependencies as dependency}
                {#if dependency.projectId}
                  <label>
                    <input
                      type="checkbox"
                      checked={selectedOptionalProjects.includes(dependency.projectId)}
                      onchange={(event) => toggleOptional(dependency.projectId!, (event.currentTarget as HTMLInputElement).checked)}
                    />
                    <span><strong>{dependency.title}</strong><small>{t("resources.preview.optionalDeclaredBy").replace("{id}", dependency.requiredByProjectId)}</small></span>
                  </label>
                {/if}
              {/each}
            </fieldset>
          {/if}
          {#if preview.plan.incompatibleDependencies.length > 0}
            <div class="warning-panel"><strong>{t("resources.preview.incompatibleTitle")}</strong><span>{t("resources.preview.incompatibleBody")}</span></div>
          {/if}
          <div class="content-preview-actions">
            {#if optionalSelectionDirty}
              <button class="button" disabled={Boolean(previewingProject)} onclick={() => void applyOptionalSelection()}>{t("resources.preview.applyOptional")}</button>
            {/if}
            <button class="button primary" disabled={submitting || optionalSelectionDirty} onclick={() => void confirm()}>{submitting ? t("resources.submitting") : t("resources.preview.confirm")}</button>
          </div>
        </section>
      {/if}

      {#if queued}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.queuedTitle")}</strong></div>
          <button class="button primary" onclick={onOpenTasks}>{t("resources.viewTasks")}</button>
        </div>
      {/if}
    {:else}
      {#if eligibleInstances.length === 0}
        <section class="resource-empty">
          <Icon name="compass" size={28} />
          <h2>{t("resources.empty.title")}</h2>
          <p>{t("resources.empty.description")}</p>
        </section>
      {:else}
        <label class="resource-instance-field">
          <span>{t("resources.instanceLabel")}</span>
          <select value={selectedInstanceId} onchange={(event) => void selectInstance(event)}>
            {#each eligibleInstances as instance}
              <option value={instance.id}>{t("resources.instanceOption").replace("{name}", instance.name).replace("{version}", instance.gameVersion).replace("{loader}", loaderName(instance.loaderKind)).replace("{loaderVersion}", instance.loaderVersion ?? "")}</option>
            {/each}
          </select>
        </label>

        <section class="local-content-section" aria-labelledby="local-content-title">
          <header>
            <div><h2 id="local-content-title">{t("resources.local.title")}</h2></div>
            <div class="local-content-actions">
              <button class="button ghost compact" disabled={localLoading || checkingUpdates} onclick={() => void checkUpdates()}>{checkingUpdates ? t("resources.local.checking") : t("resources.local.checkUpdates")}</button>
              <button class="button ghost compact" disabled={localLoading} onclick={() => void loadInstalled()}>{t("resources.local.refresh")}</button>
            </div>
          </header>
          {#if localError}
            <div class="error-block" role="alert"><strong>{t("resources.local.errorTitle")}</strong><span>{localError}</span></div>
          {:else if localLoading}
            <div class="content-loading" aria-live="polite"><span>{t("resources.local.loading")}</span></div>
          {:else if installed.length === 0}
            <div class="local-content-empty">{t("resources.local.empty")}</div>
          {:else}
            <div class="installed-content-list">
              {#each installed as entry}
                <article class="installed-content-row">
                  <div><strong>{entry.projectTitle}</strong><small>{entry.versionNumber} · {entry.fileName}</small></div>
                  <span>{entry.autoUpdateEnabled ? t("resources.local.autoUpdateOn") : t("resources.local.autoUpdateOff")}</span>
                </article>
              {/each}
            </div>
          {/if}
          <label class="auto-update-toggle">
            <input
              type="checkbox"
              checked={autoUpdate}
              aria-label={t("resources.autoUpdate.title")}
              onchange={(event) => void toggleAutoUpdate((event.currentTarget as HTMLInputElement).checked)}
            />
            <span><strong>{t("resources.autoUpdate.title")}</strong><small>{t("resources.autoUpdate.description")}</small></span>
          </label>
          {#if updateError}
            <div class="error-block" role="alert"><strong>{t("resources.updates.errorTitle")}</strong><span>{updateError}</span></div>
          {/if}
          {#if updates !== null}
            <div class="content-update-panel" aria-label={t("resources.updates.panelAria")}>
              {#if updates.length === 0}
                <div class="local-content-empty">{t("resources.updates.none")}</div>
              {:else}
                <div class="content-update-heading">
                  <span>{t("resources.updates.count").replace("{count}", String(updates.length))}</span>
                  {#if autoUpdate && updates.length > 1}
                    <button class="button primary compact" disabled={updateSubmitting} onclick={() => void planUpdates((updates ?? []).map((update) => update.projectId))}>{updateSubmitting ? t("resources.submitting") : t("resources.updates.updateAll")}</button>
                  {/if}
                </div>
                <div class="installed-content-list">
                  {#each updates as update}
                    <article class="installed-content-row">
                      <div><strong>{update.projectTitle}</strong><small>{update.currentVersionNumber} → {update.latestVersionNumber}</small></div>
                      <button class="button compact" disabled={updateSubmitting} onclick={() => void planUpdates([update.projectId])}>{t("resources.updates.updateOne")}</button>
                    </article>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
          {#if updateQueued}
            <div class="content-queued" role="status">
              <div><strong>{t("resources.updates.queuedTitle")}</strong></div>
              <button class="button primary" onclick={onOpenTasks}>{t("resources.viewTasks")}</button>
            </div>
          {/if}
        </section>

        <section class="local-content-section" aria-labelledby="instance-resource-title">
          <header>
            <div><h2 id="instance-resource-title">{t("resources.files.title")}</h2><p>{t("resources.files.description")}</p></div>
            <div class="local-content-actions">
              <button class="button ghost compact" disabled={importing} onclick={() => void importResource("resourcepack")}>{t("resources.files.importResourcepack")}</button>
              <button class="button ghost compact" disabled={importing} onclick={() => void importResource("shader")}>{t("resources.files.importShader")}</button>
              <button class="button ghost compact" disabled={importing} onclick={openDatapackImport}>{t("resources.files.importDatapack")}</button>
            </div>
          </header>
          {#if resourceError}
            <div class="error-block" role="alert"><strong>{t("resources.files.errorTitle")}</strong><span>{resourceError}</span></div>
          {/if}
          {#if datapackImportOpen}
            <div class="datapack-import-form" role="group" aria-label={t("resources.datapack.groupAria")}>
              <label>
                <span>{t("resources.datapack.worldLabel")}</span>
                <select value={selectedWorld} onchange={(event) => { selectedWorld = (event.currentTarget as HTMLSelectElement).value; }}>
                  {#each worlds as world}
                    <option value={world}>{world}</option>
                  {/each}
                </select>
              </label>
              <div class="local-content-actions">
                <button class="button primary compact" disabled={importing || !selectedWorld} onclick={() => void importResource("datapack", selectedWorld)}>{importing ? t("resources.datapack.importing") : t("resources.datapack.pickAndImport")}</button>
                <button class="button ghost compact" disabled={importing} onclick={() => { datapackImportOpen = false; }}>{t("common.cancel")}</button>
              </div>
            </div>
          {/if}
          {#if resources.length === 0}
            <div class="local-content-empty">{t("resources.files.empty")}</div>
          {:else}
            <div class="installed-content-list">
              {#each resources as resource}
                <article class="installed-content-row">
                  <div>
                    <strong>{resource.displayName}</strong>
                    <small>{kindLabel(resource.kind)}{resource.worldName ? t("resources.files.worldSuffix").replace("{world}", resource.worldName) : ""} · {resource.fileName}</small>
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
                    {#if pendingResourceDelete === resource.id}
                      <button class="button danger-subtle compact" onclick={() => void deleteResource(resource)}>{t("common.confirmDelete")}</button>
                      <button class="button ghost compact" onclick={() => { pendingResourceDelete = null; }}>{t("common.cancel")}</button>
                    {:else}
                      <button class="button danger-subtle compact" aria-label={t("resources.files.deleteAria").replace("{name}", resource.displayName)} onclick={() => { pendingResourceDelete = resource.id; }}>{t("common.delete")}</button>
                    {/if}
                  </div>
                </article>
              {/each}
            </div>
          {/if}
        </section>
      {/if}
    {/if}
  </main>

  {#if downloadTarget}
    <div class="modal-backdrop" role="presentation">
      <div class="confirmation-dialog download-dialog" role="dialog" aria-modal="true" aria-labelledby="download-dialog-title" tabindex="-1" onkeydown={(event) => { if (event.key === "Escape" && !downloading) downloadTarget = null; }}>
        <header>
          <h2 id="download-dialog-title">{t("resources.download.title").replace("{name}", downloadTarget.title)}</h2>
          <p>{t("resources.download.description")}</p>
        </header>
        {#if downloadLoadingVersions}
          <div class="content-loading" aria-live="polite"><span>{t("resources.download.loadingVersions")}</span></div>
        {:else}
          <div class="download-form">
            <label>
              <span>{t("resources.download.versionLabel")}</span>
              <select value={downloadVersionId} onchange={(event) => selectDownloadVersion((event.currentTarget as HTMLSelectElement).value)} aria-label={t("resources.download.versionAria")}>
                {#each downloadVersions as version}
                  <option value={version.id}>{version.versionNumber}{version.versionType !== "release" ? ` (${version.versionType})` : ""}</option>
                {/each}
              </select>
            </label>
            <label>
              <span>{t("resources.download.fileNameLabel")}</span>
              <input bind:value={downloadFileName} type="text" aria-label={t("resources.download.fileNameAria")} />
            </label>
            <div class="download-dest" role="radiogroup" aria-label={t("resources.download.destAria")}>
              {#if selectedInstance()}
                <label class="download-dest-option">
                  <input type="radio" name="download-dest" checked={downloadDest === "instance"} onchange={() => { downloadDest = "instance"; }} />
                  <span>{t("resources.download.destInstance").replace("{name}", selectedInstance()?.name ?? "")}</span>
                </label>
              {/if}
              <label class="download-dest-option">
                <input type="radio" name="download-dest" checked={downloadDest === "custom"} onchange={() => { downloadDest = "custom"; }} />
                <span>{t("resources.download.destCustom")}</span>
              </label>
              {#if downloadDest === "custom"}
                <div class="download-dir-row">
                  <button class="button ghost compact" onclick={() => void pickDownloadDir()}>{t("resources.download.pickDir")}</button>
                  <span class="download-dir">{downloadCustomDir || t("resources.download.noDir")}</span>
                </div>
              {/if}
            </div>
          </div>
          <div class="confirmation-actions">
            <button class="button" data-dialog-autofocus disabled={downloading} onclick={() => { downloadTarget = null; }}>{t("common.cancel")}</button>
            <button class="button primary" disabled={downloading || !downloadVersionId || !downloadFileName.trim() || (downloadDest === "custom" && !downloadCustomDir)} onclick={() => void confirmDownload()}>{downloading ? t("resources.download.running") : t("resources.download.confirm")}</button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</AppShell>
