<script lang="ts">
  import { onMount } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import {
    isFavorite,
    listFavorites,
    toggleFavorite,
    type FavoriteProject,
    type FavoriteProjectInput,
  } from "../favorites.svelte";
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

  let tab = $state<"catalog" | "instances" | "favorites">("catalog");
  let catalogView = $state<"list" | "detail">("list");
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
  let packPreviewIcon = $state("");
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

  // ---- 资源详情副视图（PCL 3.5）：简介卡 + 版本筛选 + 按 MC 版本分组的文件列表 ----
  const UNKNOWN_GROUP_KEY = "unknown";
  let detailProject = $state<ModrinthProjectSummary | null>(null);
  let detailType = $state<ModrinthProjectType>("mod");
  let detailVersions = $state<ModrinthVersionSummary[]>([]);
  let detailVersionsLoading = $state(false);
  let detailVersionsError = $state("");
  let detailGameFilter = $state("");
  let detailLoaderFilter = $state("");
  let detailOpenOverrides = $state<Record<string, boolean>>({});
  let detailCopied = $state<"name" | "link" | "">("");

  interface DetailVersionGroup {
    key: string;
    isSelected: boolean;
    versions: ModrinthVersionSummary[];
  }

  function compareGameVersionsDescending(left: string, right: string): number {
    const leftParts = left.split(".");
    const rightParts = right.split(".");
    const length = Math.max(leftParts.length, rightParts.length);
    for (let index = 0; index < length; index += 1) {
      const a = leftParts[index] ?? "";
      const b = rightParts[index] ?? "";
      const aNumber = Number(a);
      const bNumber = Number(b);
      if (a === b) continue;
      if (!Number.isNaN(aNumber) && !Number.isNaN(bNumber) && aNumber !== bNumber) {
        return bNumber - aNumber;
      }
      if (!Number.isNaN(aNumber)) return -1;
      if (!Number.isNaN(bNumber)) return 1;
      return b.localeCompare(a);
    }
    return 0;
  }

  /** 筛选 chip 命中规则：归并后的大版本 chip 以前缀匹配（1.21 覆盖 1.21.1）。 */
  function gameVersionMatchesFilter(gameVersion: string, filter: string): boolean {
    return gameVersion === filter || gameVersion.startsWith(`${filter}.`);
  }

  /** PCL 规则：不同游戏版本数 ≥9 时按大版本（1.21/1.20…）归并筛选 chip。 */
  const detailGameOptions = $derived.by(() => {
    const distinct = [
      ...new Set(detailVersions.flatMap((version) => version.gameVersions)),
    ].sort(compareGameVersionsDescending);
    if (distinct.length < 9) return distinct;
    const merged = new Set<string>();
    for (const version of distinct) {
      const parts = version.split(".");
      const isTripleNumeric = parts.length >= 3 && parts.slice(0, 2).every((part) => /^\d+$/.test(part));
      merged.add(isTripleNumeric ? parts.slice(0, 2).join(".") : version);
    }
    return [...merged].sort(compareGameVersionsDescending);
  });

  const detailGroups = $derived.by((): DetailVersionGroup[] => {
    const filtered = detailVersions.filter((version) => {
      const gameMatch =
        detailGameFilter === "" ||
        version.gameVersions.some((candidate) => gameVersionMatchesFilter(candidate, detailGameFilter));
      const loaderMatch =
        detailType !== "mod" || detailLoaderFilter === "" || version.loaders.includes(detailLoaderFilter);
      return gameMatch && loaderMatch;
    });
    const buckets = new Map<string, ModrinthVersionSummary[]>();
    for (const version of filtered) {
      const targets = version.gameVersions.length > 0 ? version.gameVersions : [UNKNOWN_GROUP_KEY];
      for (const target of targets) {
        if (detailGameFilter !== "" && !gameVersionMatchesFilter(target, detailGameFilter)) continue;
        const bucket = buckets.get(target) ?? [];
        bucket.push(version);
        buckets.set(target, bucket);
      }
    }
    const selectedGameVersion = detailType === "modpack" ? "" : (selectedInstance()?.gameVersion ?? "");
    return [...buckets.entries()]
      .sort((a, b) => {
        if (a[0] === UNKNOWN_GROUP_KEY) return 1;
        if (b[0] === UNKNOWN_GROUP_KEY) return -1;
        return compareGameVersionsDescending(a[0], b[0]);
      })
      .map(([key, versions]) => ({
        key,
        isSelected: key === selectedGameVersion,
        versions: [...versions].sort((a, b) => b.datePublished.localeCompare(a.datePublished)),
      }));
  });

  /** PCL 折叠卡规则：单组默认展开；带目标实例时「所选版本」组自动展开。 */
  function detailGroupOpen(group: DetailVersionGroup, groupCount: number): boolean {
    return detailOpenOverrides[group.key] ?? (groupCount === 1 || group.isSelected);
  }

  function toggleDetailGroup(group: DetailVersionGroup, groupCount: number): void {
    detailOpenOverrides = {
      ...detailOpenOverrides,
      [group.key]: !detailGroupOpen(group, groupCount),
    };
  }

  async function openDetail(project: ModrinthProjectSummary, type: ModrinthProjectType): Promise<void> {
    detailProject = project;
    detailType = type;
    // 下载/安装流程复用目录侧状态（catalogType 与 detailType 保持一致）。
    catalogType = type;
    catalogView = "detail";
    detailCopied = "";
    detailVersions = [];
    detailVersionsError = "";
    detailOpenOverrides = {};
    const instance = selectedInstance();
    detailGameFilter = type === "modpack" ? "" : (instance?.gameVersion ?? "");
    detailLoaderFilter = type === "mod" ? (instance?.loaderKind ?? "") : "";
    detailVersionsLoading = true;
    catalogError = "";
    try {
      detailVersions = await runtime.listModrinthVersions(project.projectId);
    } catch (error) {
      detailVersionsError = error instanceof Error ? error.message : String(error);
    } finally {
      detailVersionsLoading = false;
    }
  }

  function closeDetail(): void {
    catalogView = "list";
    detailProject = null;
  }

  function modrinthProjectUrl(project: ModrinthProjectSummary): string {
    return `https://modrinth.com/project/${project.slug}`;
  }

  async function copyDetailText(kind: "name" | "link"): Promise<void> {
    const project = detailProject;
    if (!project) return;
    const text = kind === "name" ? project.title : modrinthProjectUrl(project);
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      fallbackCopyText(text);
    }
    detailCopied = kind;
    window.setTimeout(() => {
      if (detailCopied === kind) detailCopied = "";
    }, 1600);
  }

  function fallbackCopyText(text: string): void {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }

  function favoriteInputFor(project: ModrinthProjectSummary, type: ModrinthProjectType): FavoriteProjectInput {
    return {
      projectId: project.projectId,
      slug: project.slug,
      title: project.title,
      iconUrl: project.iconUrl,
      type,
    };
  }

  function toggleProjectFavorite(project: ModrinthProjectSummary, type: ModrinthProjectType): void {
    toggleFavorite(favoriteInputFor(project, type));
  }

  /** 收藏夹行内「下载」：重建摘要并进入详情（收藏不含描述与统计，简介区如实留空）。 */
  function openFavoriteDetail(favorite: FavoriteProject): void {
    tab = "catalog";
    void openDetail(
      {
        projectId: favorite.projectId,
        slug: favorite.slug,
        title: favorite.title,
        description: "",
        downloads: 0,
        clientSide: "",
        serverSide: "",
        iconUrl: favorite.iconUrl,
        author: null,
        dateModified: null,
        versions: [],
      },
      favorite.type,
    );
  }

  /** 详情文件行下载：整合包走安装预览；有实例走既有安装流；否则打开已定版本的自由下载。 */
  function downloadDetailVersion(version: ModrinthVersionSummary): void {
    const project = detailProject;
    if (!project) return;
    if (detailType === "modpack") {
      void previewPack(project);
      return;
    }
    const instance = selectedInstance();
    if (instance && detailType === "mod") {
      void createPreview(project);
      return;
    }
    if (instance) {
      void installResourceToInstance(project, version.id);
      return;
    }
    openDetailDownloadDialog(version);
  }

  function openDetailDownloadDialog(version: ModrinthVersionSummary): void {
    const project = detailProject;
    if (!project) return;
    downloadTarget = project;
    downloadVersions = [version];
    downloadVersionId = version.id;
    downloadLoadingVersions = false;
    catalogError = "";
    downloadDone = "";
    downloadDest = selectedInstance() ? "instance" : "custom";
    const slug = project.slug || "download";
    downloadFileName = `${slug}-${version.versionNumber}${defaultFileExtension()}`;
  }

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
    packPreviewIcon = "";
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
      packPreviewIcon = "";
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
      packPreviewIcon = project.iconUrl ?? "";
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
      // 包内图标优先；没有内置图标时回填在线项目图标。
      if (packPreviewIcon) {
        const installed = await runtime.getInstanceModpack(report.instanceId).catch(() => null);
        if (!installed?.iconUrl) {
          await runtime.setModpackIconUrl(report.instanceId, packPreviewIcon).catch(() => {});
        }
      }
      packDone = report.packName;
      packPreview = null;
      packPreviewIcon = "";
      await onTasksChanged();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      packInstalling = false;
    }
  }

  async function installResourceToInstance(project: ModrinthProjectSummary, versionId?: string): Promise<void> {
    const instance = selectedInstance();
    if (!instance || (catalogType !== "shader" && catalogType !== "resourcepack")) return;
    resourceInstalling = project.projectId;
    catalogError = "";
    resourceInstallDone = "";
    try {
      await runtime.installOnlineResource(instance.id, catalogType, project.projectId, versionId);
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
        <button class:active={tab === "favorites"} onclick={() => { tab = "favorites"; }}>{t("resources.tabs.favorites")}</button>
        <button class:active={tab === "instances"} onclick={() => { tab = "instances"; }}>{t("resources.tabs.instances")}</button>
      </nav>
    </header>

    {#if tab === "catalog"}
      {#if catalogView === "list"}
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
      {/if}

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
            <button class="button ghost" disabled={packInstalling} onclick={() => { packPreview = null; packPreviewIcon = ""; }}>{t("common.cancel")}</button>
          </div>
        </section>
      {/if}

      {#if downloadDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.download.done")}</strong><span>{downloadDone}</span></div>
        </div>
      {/if}

      {#if catalogView === "list"}
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
                  <button class="button ghost compact" onclick={() => void openDetail(project, catalogType)}>{t("resources.detail.open")}</button>
                  <button class="button ghost compact" disabled={downloading} onclick={() => void openDownloadDialog(project)}>{t("resources.download.button")}</button>
                  <button
                    class="button ghost compact favorite-toggle"
                    class:favorite-active={isFavorite(project.projectId)}
                    aria-pressed={isFavorite(project.projectId)}
                    aria-label={t("resources.detail.favoriteToggleAria").replace("{name}", project.title)}
                    onclick={() => toggleProjectFavorite(project, catalogType)}
                  ><Icon name="star" size={14} /></button>
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
      {:else if detailProject}
        {@const project = detailProject}
        {@const detailMcmod = mcmodEntryFor(project.slug)}
        {@const zhRegion = uiLanguage() === "zh-CN" || uiLanguage() === "zh-TW"}
        <div class="detail-view">
          <button class="button ghost compact detail-back" onclick={closeDetail}>
            <Icon name="arrow-left" size={14} />
            {t("resources.detail.back")}
          </button>

          <section class="content-preview detail-intro" aria-labelledby="detail-intro-title">
            <div class="detail-intro-main">
              <div class="result-icon detail-intro-icon" aria-hidden="true">
                {#if project.iconUrl}
                  <img src={project.iconUrl} alt="" loading="lazy" />
                {:else}
                  <span>{project.title.slice(0, 1)}</span>
                {/if}
              </div>
              <div class="detail-intro-copy">
                <h2 id="detail-intro-title">{project.title}</h2>
                {#if zhRegion && detailMcmod}
                  <strong class="detail-zh-name">{detailMcmod.zhName}</strong>
                {/if}
                {#if project.author}
                  <span class="result-author">by {project.author}</span>
                {/if}
                {#if project.description}
                  <p>{project.description}</p>
                {/if}
                {#if zhRegion && detailMcmod && detailMcmod.zhDescription !== project.description}
                  <p class="detail-zh-desc">{detailMcmod.zhDescription}</p>
                {/if}
                {#if project.downloads > 0 || project.dateModified}
                  <div class="detail-stats">
                    <span>{t("resources.catalog.downloads").replace("{count}", formatDownloads(project.downloads))}</span>
                    {#if project.dateModified}<span>{formatDate(project.dateModified)}</span>{/if}
                    <span>{t("resources.detail.source")}</span>
                  </div>
                {/if}
              </div>
            </div>
            <div class="detail-actions">
              <button class="button ghost compact" onclick={() => void runtime.openExternalUrl(modrinthProjectUrl(project))}>{t("resources.detail.openModrinth")}</button>
              {#if zhRegion && detailMcmod}
                <button class="button ghost compact" onclick={() => void runtime.openExternalUrl(detailMcmod.mcmodUrl)}>{t("resources.detail.openMcmod")}</button>
              {/if}
              <button class="button ghost compact" onclick={() => void copyDetailText("name")}>{detailCopied === "name" ? t("resources.detail.copied") : t("resources.detail.copyName")}</button>
              <button class="button ghost compact" onclick={() => void copyDetailText("link")}>{detailCopied === "link" ? t("resources.detail.copied") : t("resources.detail.copyLink")}</button>
              <button
                class="button ghost compact favorite-toggle"
                class:favorite-active={isFavorite(project.projectId)}
                aria-pressed={isFavorite(project.projectId)}
                aria-label={t("resources.detail.favoriteToggleAria").replace("{name}", project.title)}
                onclick={() => toggleProjectFavorite(project, detailType)}
              >
                <Icon name="star" size={14} />
                {isFavorite(project.projectId) ? t("resources.detail.favoriteRemove") : t("resources.detail.favoriteAdd")}
              </button>
            </div>
          </section>

          <section class="content-preview detail-filter" aria-labelledby="detail-filter-title">
            <header><h2 id="detail-filter-title">{t("resources.detail.filterTitle")}</h2></header>
            <div class="catalog-chips" role="group" aria-label={t("resources.detail.gameFilterAria")}>
              <button class:active={detailGameFilter === ""} aria-pressed={detailGameFilter === ""} onclick={() => { detailGameFilter = ""; }}>{t("resources.catalog.filterVersionAll")}</button>
              {#each detailGameOptions as option}
                <button class:active={detailGameFilter === option} aria-pressed={detailGameFilter === option} onclick={() => { detailGameFilter = option; }}>{option}</button>
              {/each}
            </div>
            {#if detailType === "mod"}
              <div class="catalog-chips" role="group" aria-label={t("resources.detail.loaderFilterAria")}>
                <button class:active={detailLoaderFilter === ""} aria-pressed={detailLoaderFilter === ""} onclick={() => { detailLoaderFilter = ""; }}>{t("resources.catalog.filterLoaderAll")}</button>
                {#each Object.entries(LOADER_NAMES) as [kind, name]}
                  <button class:active={detailLoaderFilter === kind} aria-pressed={detailLoaderFilter === kind} onclick={() => { detailLoaderFilter = kind; }}>{name}</button>
                {/each}
              </div>
            {/if}
          </section>

          <section class="content-preview detail-files" aria-labelledby="detail-files-title">
            <header><h2 id="detail-files-title">{t("resources.detail.filesTitle")}</h2></header>
            {#if detailVersionsLoading}
              <div class="content-loading" aria-live="polite"><span>{t("resources.download.loadingVersions")}</span></div>
            {:else if detailVersionsError}
              <div class="error-block" role="alert"><strong>{t("resources.catalog.errorTitle")}</strong><span>{detailVersionsError}</span></div>
            {:else if detailGroups.length === 0}
              <div class="local-content-empty">{t("resources.detail.filesEmpty")}</div>
            {:else}
              <div class="version-groups">
                {#each detailGroups as group}
                  {@const open = detailGroupOpen(group, detailGroups.length)}
                  <div class="version-group">
                    <button class="version-group-head" aria-expanded={open} onclick={() => toggleDetailGroup(group, detailGroups.length)}>
                      <span class="group-chevron" class:open={open}></span>
                      <strong>{group.key === UNKNOWN_GROUP_KEY ? t("resources.detail.unknownGroup") : `Minecraft ${group.key}`}</strong>
                      {#if group.isSelected}
                        <span class="detail-selected-badge">{t("resources.detail.selectedGroup")}</span>
                      {/if}
                      <small>{t("resources.detail.groupCount").replace("{count}", String(group.versions.length))}</small>
                    </button>
                    {#if open}
                      <div class="detail-file-list">
                        {#each group.versions as version}
                          <article class="detail-file-row">
                            <div class="detail-file-main">
                              <strong>{version.versionNumber}{version.versionType !== "release" ? ` (${version.versionType})` : ""}</strong>
                              <small>{formatDate(version.datePublished)} · {t("resources.catalog.downloads").replace("{count}", formatDownloads(version.downloads))}</small>
                            </div>
                            <button
                              class="button compact"
                              disabled={Boolean(previewingProject) || Boolean(packPreviewing) || Boolean(resourceInstalling) || packInstalling}
                              onclick={() => downloadDetailVersion(version)}
                            >{t("resources.download.button")}</button>
                          </article>
                        {/each}
                      </div>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        </div>
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
    {:else if tab === "favorites"}
      {@const favoriteList = listFavorites()}
      {#if favoriteList.length === 0}
        <section class="resource-empty">
          <Icon name="star" size={28} />
          <h2>{t("resources.favorites.empty")}</h2>
          <p>{t("resources.favorites.emptyHint")}</p>
          <button class="button primary" onclick={() => { tab = "catalog"; }}>{t("resources.favorites.goSearch")}</button>
        </section>
      {:else}
        <div class="favorites-list" aria-label={t("resources.favorites.aria")}>
          {#each CATALOG_TYPES as favoriteType}
            {@const group = favoriteList.filter((candidate) => candidate.type === favoriteType.key)}
            {#if group.length > 0}
              <section class="local-content-section favorites-group" aria-labelledby={`favorites-group-${favoriteType.key}`}>
                <header>
                  <div><h2 id={`favorites-group-${favoriteType.key}`}>{t(favoriteType.labelKey)}</h2></div>
                  <span class="favorites-count">{t("resources.favorites.count").replace("{count}", String(group.length))}</span>
                </header>
                <div class="installed-content-list">
                  {#each group as favorite}
                    {@const zhEntry = uiLanguage() === "zh-CN" || uiLanguage() === "zh-TW" ? mcmodEntryFor(favorite.slug) : null}
                    <article class="installed-content-row favorites-row">
                      <div class="result-icon favorites-icon" aria-hidden="true">
                        {#if favorite.iconUrl}
                          <img src={favorite.iconUrl} alt="" loading="lazy" />
                        {:else}
                          <span>{favorite.title.slice(0, 1)}</span>
                        {/if}
                      </div>
                      <div class="favorites-main">
                        <strong>{zhEntry?.zhName ?? favorite.title}</strong>
                        <small>{#if zhEntry}{favorite.title} · {/if}{t(favoriteType.labelKey)}</small>
                      </div>
                      <div class="resource-row-actions">
                        <button class="button compact" onclick={() => openFavoriteDetail(favorite)}>{t("resources.download.button")}</button>
                        <button class="button danger-subtle compact" onclick={() => toggleFavorite(favorite)}>{t("resources.favorites.remove")}</button>
                      </div>
                    </article>
                  {/each}
                </div>
              </section>
            {/if}
          {/each}
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
