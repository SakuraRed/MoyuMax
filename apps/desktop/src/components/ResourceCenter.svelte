<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import {
    isFavorite,
    listFavorites,
    toggleFavorite,
    type FavoriteProject,
    type FavoriteProjectInput,
  } from "../favorites.svelte";
  import { mcmodEntryFor, mcmodSearchUrl } from "../mcmod-zh";
  import {
    buildVersionGroups,
    compareGameVersionsDescending,
    formatGameVersionRange,
    SNAPSHOT_GROUP_KEY,
    UNKNOWN_GROUP_KEY,
    versionGameTags,
    versionOptionLabel,
  } from "../version-groups";
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
  import VersionPicker from "./VersionPicker.svelte";

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
  /** 远程目录搜索失败即视为离线:标题栏网络点置灰黄并给就地 banner。 */
  let catalogOffline = $state(false);
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
  let packProjectRef = $state<ModrinthProjectSummary | null>(null);
  let packVersions = $state<ModrinthVersionSummary[]>([]);
  let packVersionId = $state("");
  let resourceInstalling = $state("");
  let resourceInstallDone = $state("");
  let resourceInstallTarget = $state<ModrinthProjectSummary | null>(null);
  let resourceVersions = $state<ModrinthVersionSummary[]>([]);
  let resourceVersionId = $state("");
  let resourceVersionsLoading = $state(false);
  let previewProjectRef = $state<ModrinthProjectSummary | null>(null);
  let previewVersions = $state<ModrinthVersionSummary[]>([]);
  let previewVersionId = $state("");
  let downloadTarget = $state<ModrinthProjectSummary | null>(null);
  let downloadVersions = $state<ModrinthVersionSummary[]>([]);
  let downloadVersionId = $state("");
  let downloadFileName = $state("");
  let downloadDest = $state<"instance" | "custom">("instance");
  let downloadCustomDir = $state("");
  let downloadLoadingVersions = $state(false);
  let downloading = $state(false);
  let downloadDone = $state("");

  // modal 根节点:打开后把焦点交给标了 data-dialog-autofocus 的取消按钮。
  let previewDialog = $state<HTMLElement | null>(null);
  let packDialog = $state<HTMLElement | null>(null);
  let resourceDialog = $state<HTMLElement | null>(null);
  let downloadDialog = $state<HTMLElement | null>(null);

  // ---- 资源详情副视图：简介卡 + 版本筛选 + 按 MC 版本分组的文件列表 ----
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

  /** 筛选 chip 命中规则：归并后的大版本 chip 以前缀匹配（1.21 覆盖 1.21.1）。 */
  function gameVersionMatchesFilter(gameVersion: string, filter: string): boolean {
    return gameVersion === filter || gameVersion.startsWith(`${filter}.`);
  }

  /** 不同游戏版本数 ≥9 时按大版本（1.21/1.20…）归并筛选 chip。 */
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
    const instance = selectedInstance();
    // 对齐 PCL-CE:按 MC 精确版本分组(多加载器模组为 加载器×版本),
    // 整合包版本只归最高 MC 版本,匹配实例的组置顶为「所选版本」。
    return buildVersionGroups(filtered, {
      kind: detailType,
      target:
        instance && detailType !== "modpack"
          ? { gameVersion: instance.gameVersion, loaderKind: instance.loaderKind }
          : null,
      collapseLoaders: detailLoaderFilter !== "",
    }).map((group) => ({
      key: group.key,
      isSelected: group.recommended,
      versions: group.versions,
    }));
  });

  /** 分组标题:快照与其他组用本地化名称,其余为版本/加载器×版本键。 */
  function groupLabel(key: string): string {
    if (key === SNAPSHOT_GROUP_KEY) return t("resources.versions.snapshotGroup");
    if (key === UNKNOWN_GROUP_KEY) return t("resources.versions.otherGroup");
    return key;
  }

  /** 折叠卡规则：单组默认展开；带目标实例时「所选版本」组自动展开。 */
  function detailGroupOpen(group: DetailVersionGroup, groupCount: number): boolean {
    return detailOpenOverrides[group.key] ?? (groupCount === 1 || group.isSelected);
  }

  function toggleDetailGroup(group: DetailVersionGroup, groupCount: number): void {
    detailOpenOverrides = {
      ...detailOpenOverrides,
      [group.key]: !detailGroupOpen(group, groupCount),
    };
  }

  /** 详情右栏兼容性结论：存在匹配所选实例游戏版本（模组还要求加载器）的文件版本。 */
  const detailCompatible = $derived.by(() => {
    const instance = selectedInstance();
    if (!instance || detailType === "modpack" || detailVersions.length === 0) return false;
    return detailVersions.some(
      (version) =>
        version.gameVersions.includes(instance.gameVersion) &&
        (detailType !== "mod" || version.loaders.includes(instance.loaderKind)),
    );
  });

  const modalOpen = $derived(
    Boolean(preview || packPreview || resourceInstallTarget || downloadTarget),
  );

  /** 工具行第二行的自动过滤说明：非整合包且选中实例时按实例版本/加载器收窄结果。 */
  const autoFilterNote = $derived.by(() => {
    const instance = selectedInstance();
    if (!instance || catalogType === "modpack") return "";
    if (catalogType === "mod") {
      return t("resources.catalog.filterNote")
        .replace("{name}", instance.name)
        .replace("{version}", filterVersion.trim() || instance.gameVersion)
        .replace("{loader}", loaderName(filterLoader || instance.loaderKind));
    }
    return t("resources.catalog.filterNoteVersion")
      .replace("{name}", instance.name)
      .replace("{version}", filterVersion.trim() || instance.gameVersion);
  });

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

  /** 文件列表加载失败后的重试：保留当前筛选状态，仅重新拉取版本列表。 */
  async function reloadDetailVersions(): Promise<void> {
    const project = detailProject;
    if (!project) return;
    detailVersionsLoading = true;
    detailVersionsError = "";
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

  /** 详情文件行主操作：整合包走安装预览；模组走安装计划；光影/资源包直接装该版本；无实例则自由下载。 */
  function runDetailVersionAction(version: ModrinthVersionSummary): void {
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
      void installResourceVersion(project, version.id);
      return;
    }
    void openDetailDownloadDialog(version);
  }

  /** 文件行按钮文案：有可用实例（或整合包）为安装，否则为自由下载。 */
  function detailVersionActionLabel(): string {
    if (detailType === "modpack") return t("resources.catalog.install");
    return selectedInstance() ? t("resources.catalog.install") : t("resources.download.button");
  }

  /** 详情右栏「安装到」按钮：模组进安装计划，光影/资源包进版本确认。 */
  function installDetailProject(): void {
    const project = detailProject;
    if (!project) return;
    if (detailType === "mod") {
      void createPreview(project);
    } else {
      void openResourceInstall(project);
    }
  }

  /** 详情页已选定具体版本的直接安装（光影/资源包）。 */
  async function installResourceVersion(project: ModrinthProjectSummary, versionId: string): Promise<void> {
    const instance = selectedInstance();
    if (!instance || (detailType !== "shader" && detailType !== "resourcepack")) return;
    resourceInstalling = project.projectId;
    catalogError = "";
    resourceInstallDone = "";
    try {
      await runtime.installOnlineResource(instance.id, detailType, project.projectId, versionId);
      resourceInstallDone = `${project.title} → ${instance.name}`;
      await loadInstalled();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      resourceInstalling = "";
    }
  }

  async function openDetailDownloadDialog(version: ModrinthVersionSummary): Promise<void> {
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
    await tick();
    focusDialog(downloadDialog);
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
    closePreview();
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
    closePackPreview();
    resourceInstallDone = "";
    resourceInstallTarget = null;
    closePreview();
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
      closePackPreview();
      resourceInstallDone = "";
      resourceInstallTarget = null;
      closePreview();
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
      catalogOffline = false;
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
      catalogOffline = true;
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

  function focusDialog(dialog: HTMLElement | null): void {
    dialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function closePreview(): void {
    preview = null;
    previewProjectRef = null;
    previewVersions = [];
    previewVersionId = "";
    selectedOptionalProjects = [];
    optionalSelectionDirty = false;
  }

  function closePackPreview(): void {
    packPreview = null;
    packPreviewIcon = "";
    packProjectRef = null;
    packVersions = [];
    packVersionId = "";
    packDone = "";
  }

  async function createPreview(project: ModrinthProjectSummary): Promise<void> {
    previewingProject = project.projectId;
    catalogError = "";
    queued = false;
    selectedOptionalProjects = [];
    previewProjectRef = project;
    previewVersionId = "";
    previewVersions = [];
    try {
      // 版本列表全量拉取,按 MC 版本分组展示(与实例匹配的组置顶推荐),不再按实例过滤。
      const [previewResult, versions] = await Promise.all([
        runtime.previewModrinthInstall(selectedInstanceId, project.projectId, []),
        runtime.listModrinthVersions(project.projectId).catch(() => [] as ModrinthVersionSummary[]),
      ]);
      preview = previewResult;
      previewVersions = versions;
      optionalSelectionDirty = false;
      await tick();
      focusDialog(previewDialog);
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      previewingProject = "";
    }
  }

  /** 切换安装计划的目标版本：重置可选依赖选择并按选定版本重新解析。 */
  async function selectPreviewVersion(versionId: string): Promise<void> {
    if (!previewProjectRef || versionId === previewVersionId) return;
    previewVersionId = versionId;
    previewingProject = previewProjectRef.projectId;
    catalogError = "";
    selectedOptionalProjects = [];
    try {
      preview = await runtime.previewModrinthInstall(
        selectedInstanceId,
        previewProjectRef.projectId,
        [],
        versionId || undefined,
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
        previewVersionId || undefined,
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
      closePreview();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      submitting = false;
    }
  }

  function handlePreviewDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !submitting && !previewingProject) {
      event.preventDefault();
      closePreview();
    }
  }

  async function previewPack(project: ModrinthProjectSummary): Promise<void> {
    packPreviewing = project.projectId;
    catalogError = "";
    packDone = "";
    packProjectRef = project;
    packVersions = [];
    packVersionId = "";
    try {
      const [previewResult, versions] = await Promise.all([
        runtime.previewOnlineModpack(project.projectId),
        runtime.listModrinthVersions(project.projectId).catch(() => [] as ModrinthVersionSummary[]),
      ]);
      packPreview = previewResult;
      packVersions = versions;
      packPreviewIcon = project.iconUrl ?? "";
      await tick();
      focusDialog(packDialog);
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      packPreviewing = "";
    }
  }

  /** 切换整合包版本：按选定版本重新下载并解析包。 */
  async function selectPackVersion(versionId: string): Promise<void> {
    if (!packProjectRef || versionId === packVersionId) return;
    packVersionId = versionId;
    packPreviewing = packProjectRef.projectId;
    catalogError = "";
    try {
      packPreview = await runtime.previewOnlineModpack(
        packProjectRef.projectId,
        versionId || undefined,
      );
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
      closePackPreview();
      // closePackPreview 会重置 packDone，关闭 modal 后再回填完成文案。
      packDone = report.packName;
      await onTasksChanged();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      packInstalling = false;
    }
  }

  function handlePackDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !packInstalling && !packPreviewing) {
      event.preventDefault();
      closePackPreview();
    }
  }

  /** 打开光影/资源包安装确认：先取版本列表，用户选定版本后再安装。 */
  async function openResourceInstall(project: ModrinthProjectSummary): Promise<void> {
    const instance = selectedInstance();
    const installType = catalogView === "detail" ? detailType : catalogType;
    if (!instance || (installType !== "shader" && installType !== "resourcepack")) return;
    resourceInstallTarget = project;
    resourceVersions = [];
    resourceVersionId = "";
    resourceVersionsLoading = true;
    catalogError = "";
    resourceInstallDone = "";
    try {
      // 全量版本,按 MC 版本分组,与实例匹配的组置顶推荐。
      const versions = await runtime.listModrinthVersions(project.projectId);
      if (versions.length === 0) {
        catalogError = t("resources.download.noVersions");
        resourceInstallTarget = null;
        return;
      }
      resourceVersions = versions;
      const groups = buildVersionGroups(versions, {
        kind: installType,
        target: { gameVersion: instance.gameVersion, loaderKind: instance.loaderKind },
      });
      resourceVersionId = groups[0]?.versions[0]?.id ?? versions[0]?.id ?? "";
      await tick();
      focusDialog(resourceDialog);
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
      resourceInstallTarget = null;
    } finally {
      resourceVersionsLoading = false;
    }
  }

  async function confirmResourceInstall(): Promise<void> {
    const instance = selectedInstance();
    const project = resourceInstallTarget;
    const installType = catalogView === "detail" ? detailType : catalogType;
    if (!instance || !project || !resourceVersionId || (installType !== "shader" && installType !== "resourcepack")) return;
    resourceInstalling = project.projectId;
    catalogError = "";
    resourceInstallDone = "";
    try {
      await runtime.installOnlineResource(instance.id, installType, project.projectId, resourceVersionId);
      resourceInstallDone = `${project.title} → ${instance.name}`;
      resourceInstallTarget = null;
      await loadInstalled();
    } catch (error) {
      catalogError = error instanceof Error ? error.message : String(error);
    } finally {
      resourceInstalling = "";
    }
  }

  function handleResourceDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !resourceInstalling) {
      event.preventDefault();
      resourceInstallTarget = null;
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
      // 全量版本,按 MC 版本分组展示,与实例匹配的组置顶推荐。
      const versions = await runtime.listModrinthVersions(project.projectId);
      if (versions.length === 0) {
        catalogError = t("resources.download.noVersions");
        downloadTarget = null;
        return;
      }
      downloadVersions = versions;
      const groups = buildVersionGroups(versions, {
        kind: catalogType,
        target: instance ? { gameVersion: instance.gameVersion, loaderKind: instance.loaderKind } : null,
      });
      selectDownloadVersion(groups[0]?.versions[0]?.id ?? versions[0]?.id ?? "");
      await tick();
      focusDialog(downloadDialog);
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

  /** 选择自定义目录:每次都立即拉起系统目录选择器(对齐 PCL 的另存为),进程内记忆上次所选。 */
  async function selectCustomDest(): Promise<void> {
    downloadDest = "custom";
    if (!downloadCustomDir) await pickDownloadDir();
  }

  async function confirmDownload(): Promise<void> {
    if (downloadDest === "custom" && !downloadCustomDir) {
      await pickDownloadDir();
      if (!downloadCustomDir) return;
    }
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

  function handleDownloadDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !downloading) {
      event.preventDefault();
      downloadTarget = null;
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
  online={!catalogOffline}
  connectionStatus={catalogOffline ? t("resources.connection.offline") : t("resources.connection.online")}
  instanceCount={instances.length}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content resource-content">
    <nav class="tabs" aria-label={t("resources.tabs.aria")}>
      <button class:on={tab === "catalog"} aria-pressed={tab === "catalog"} onclick={() => { tab = "catalog"; }}>{t("resources.tabs.catalog")}</button>
      <button class:on={tab === "favorites"} aria-pressed={tab === "favorites"} onclick={() => { tab = "favorites"; }}>{t("resources.tabs.favorites")}</button>
      <button class:on={tab === "instances"} aria-pressed={tab === "instances"} onclick={() => { tab = "instances"; }}>{t("resources.tabs.instances")}</button>
    </nav>

    {#if tab === "catalog"}
      {#if catalogView === "list"}
      <div class="res-toolbar">
        <form onsubmit={(event) => searchCatalog(event)}>
          <input
            class="input res-search"
            bind:value={catalogQuery}
            type="search"
            aria-label={t("resources.catalog.searchLabel")}
            placeholder={t("resources.catalog.searchPlaceholder")}
            oninput={() => { if (!catalogQuery.trim()) applyFilters(); }}
          />
          <button class="btn primary search-submit" disabled={catalogSearching || (catalogType === "mod" && eligibleInstances.length === 0)}>{catalogSearching ? t("resources.catalog.searching") : t("resources.catalog.searchSubmit")}</button>
        </form>
        <div class="row spread">
          <div class="seg" role="group" aria-label={t("resources.catalog.typeAria")}>
            {#each CATALOG_TYPES as catalogTypeOption}
              <button
                class:on={catalogType === catalogTypeOption.key}
                aria-pressed={catalogType === catalogTypeOption.key}
                onclick={() => selectCatalogType(catalogTypeOption.key)}
              >{t(catalogTypeOption.labelKey)}</button>
            {/each}
          </div>
          {#if catalogType !== "modpack" && eligibleInstances.length > 0}
            <div class="row">
              <span class="dim">{t("resources.catalog.installTo")}</span>
              <select class="input inst-select" value={selectedInstanceId} onchange={(event) => void selectInstance(event)} aria-label={t("resources.instanceLabel")}>
                {#each eligibleInstances as instance}
                  <option value={instance.id}>{instance.name}（{instance.gameVersion} · {loaderName(instance.loaderKind)}）</option>
                {/each}
              </select>
            </div>
          {/if}
        </div>
        <div class="row spread">
          <span class="src-note">{t("resources.catalog.sourceNote")}</span>
          {#if autoFilterNote}<span class="src-note">{autoFilterNote}</span>{/if}
        </div>
        <div class="res-filters" role="group" aria-label={t("resources.catalog.filtersAria")}>
          {#if catalogType !== "modpack" && eligibleInstances.length > 0}
            <label class="res-filter">
              <span>{t("resources.catalog.filterVersion")}</span>
              <input
                class="input"
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
            <label class="res-filter">
              <span>{t("resources.catalog.filterLoader")}</span>
              <select class="input" value={filterLoader} onchange={(event) => { filterLoader = (event.currentTarget as HTMLSelectElement).value; applyFilters(); }} aria-label={t("resources.catalog.filterLoader")}>
                <option value="">{t("resources.catalog.filterLoaderAll")}</option>
                {#each Object.entries(LOADER_NAMES) as [kind, name]}
                  <option value={kind}>{name}</option>
                {/each}
              </select>
            </label>
          {/if}
          <label class="res-filter">
            <span>{t("resources.catalog.filterCategory")}</span>
            <select class="input" value={filterCategory} onchange={(event) => { filterCategory = (event.currentTarget as HTMLSelectElement).value; applyFilters(); }} aria-label={t("resources.catalog.filterCategory")}>
              <option value="">{t("resources.catalog.filterCategoryAll")}</option>
              {#each CATALOG_CATEGORIES as category}
                <option value={category.value}>{t(category.labelKey)}</option>
              {/each}
            </select>
          </label>
          <label class="res-filter">
            <span>{t("resources.catalog.filterSort")}</span>
            <select class="input" value={sortIndex} onchange={(event) => { sortIndex = (event.currentTarget as HTMLSelectElement).value as typeof sortIndex; applyFilters(); }} aria-label={t("resources.catalog.filterSort")}>
              <option value="downloads">{t("resources.catalog.sortDownloads")}</option>
              <option value="updated">{t("resources.catalog.sortUpdated")}</option>
              <option value="relevance">{t("resources.catalog.sortRelevance")}</option>
            </select>
          </label>
          {#if catalogType === "modpack"}
            <span class="src-note">
              {t("resources.catalog.cfHint")}
              <button class="inline-link" onclick={() => onNavigate("instances")}>{t("resources.catalog.cfImport")}</button>
            </span>
          {/if}
        </div>
      </div>

      {#if catalogType !== "modpack" && eligibleInstances.length === 0}
        <div class="banner info" style="margin-bottom:16px">
          <span>{t("resources.catalog.needInstance")}</span>
          <div class="b-act"><button class="btn small secondary" onclick={() => onNavigate("instances")}>{t("resources.catalog.createInstance")}</button></div>
        </div>
      {/if}
      {/if}

      {#if catalogOffline && catalogView === "list"}
        <div class="banner warn" style="margin-bottom:16px" role="status">
          <span>{t("resources.catalog.offlineBanner")}</span>
        </div>
      {/if}

      {#if catalogError && !modalOpen && catalogView === "list"}
        <div class="err-block" role="alert">
          <div class="err-line"><b>{catalogOffline ? t("resources.catalog.errorTitle") : t("resources.error.title")}</b></div>
          <div class="err-line">{catalogOffline ? t("resources.error.impactOffline") : t("resources.error.impactGeneric")}</div>
          <div class="row">
            <button class="btn small primary" onclick={() => void runCatalogSearch(true)}>{t("resources.error.retry")}</button>
            <button class="btn small ghost" onclick={() => { tab = "instances"; }}>{t("resources.error.openLocal")}</button>
          </div>
          <details class="adv">
            <summary>{t("resources.error.techSummary")}</summary>
            <div class="adv-body"><div class="mono dim" style="overflow-wrap:anywhere">{catalogError}</div></div>
          </details>
        </div>
      {/if}
      {#if packDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.catalog.packDone").replace("{name}", packDone)}</strong><span>{t("resources.catalog.packDoneHint")}</span></div>
          <button class="btn primary" onclick={() => onNavigate("home")}>{t("resources.catalog.viewHome")}</button>
        </div>
      {/if}
      {#if resourceInstallDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.catalog.resourceDone").replace("{name}", resourceInstallDone)}</strong></div>
        </div>
      {/if}
      {#if downloadDone}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.download.done")}</strong><span>{downloadDone}</span></div>
        </div>
      {/if}

      {#if catalogView === "list"}
      {#if catalogSearching && catalogHits.length === 0}
        <section class="panel" aria-live="polite" aria-label={t("resources.catalog.searching")}>
          {#each [0, 1, 2, 3] as index}
            <div class="res-row skel-row" aria-hidden="true">
              <div class="skel" style="width:44px;height:44px;flex:none"></div>
              <div class="col" style="flex:1;gap:8px">
                <div class="skel" style="height:14px;width:{34 - index * 5}%"></div>
                <div class="skel" style="height:12px;width:72%"></div>
              </div>
            </div>
          {/each}
        </section>
      {:else if catalogPage && catalogHits.length === 0}
        <section class="panel pad">
          <div class="muted" role="status">{t("resources.catalog.noResults")}</div>
        </section>
      {:else if catalogHits.length > 0}
        <section class="panel" aria-label={t("resources.catalog.resultAria")}>
          {#each catalogHits as project}
            <article class="res-row">
              <div class="cube" aria-hidden="true">
                {#if project.iconUrl}
                  <img src={project.iconUrl} alt="" loading="lazy" />
                {:else}
                  {project.title.slice(0, 1)}
                {/if}
              </div>
              <div class="rr-main">
                <div class="row">
                  <span class="rr-name">{project.title}</span>
                  {#if project.author}<span class="dim">{t("resources.catalog.byAuthor").replace("{author}", project.author)}</span>{/if}
                  {#if project.versions.length > 0}
                    <span class="tag neutral version-range-badge" title={project.versions.join("、")}>{formatGameVersionRange(project.versions)}</span>
                  {/if}
                </div>
                <div class="rr-desc">{project.description}</div>
                {#if uiLanguage() === "zh-CN" || uiLanguage() === "zh-TW"}
                  {@const mcmod = mcmodEntryFor(project.slug)}
                  <div class="rr-mcmod">
                    {#if mcmod}
                      <strong>{mcmod.zhName}</strong>
                      <span>{mcmod.zhDescription}</span>
                      <button class="inline-link" onclick={() => void runtime.openExternalUrl(mcmod.mcmodUrl)}>{t("resources.catalog.mcmodLink")}</button>
                    {:else}
                      <button class="inline-link" onclick={() => void runtime.openExternalUrl(mcmodSearchUrl(project.title))}>{t("resources.catalog.mcmodSearch")}</button>
                    {/if}
                  </div>
                {/if}
              </div>
              <div class="rr-meta">
                <div class="dim">{t("resources.catalog.downloads").replace("{count}", formatDownloads(project.downloads))}</div>
                {#if project.dateModified}<div class="dim">{formatDate(project.dateModified)}</div>{/if}
              </div>
              <div class="rr-actions">
                {#if catalogType === "mod"}
                  <button class="btn small primary" disabled={Boolean(previewingProject) || eligibleInstances.length === 0} onclick={() => void createPreview(project)}>
                    {previewingProject === project.projectId ? t("resources.catalog.parsing") : t("resources.catalog.install")}
                  </button>
                {:else if catalogType === "modpack"}
                  <button class="btn small primary" disabled={Boolean(packPreviewing) || packInstalling} onclick={() => void previewPack(project)}>
                    {packPreviewing === project.projectId ? t("resources.catalog.parsing") : t("resources.catalog.install")}
                  </button>
                {:else}
                  <button class="btn small primary" disabled={Boolean(resourceInstalling) || resourceVersionsLoading || !selectedInstanceId} onclick={() => void openResourceInstall(project)}>
                    {resourceInstalling === project.projectId ? t("resources.catalog.installing") : t("resources.catalog.install")}
                  </button>
                {/if}
                <button class="btn small ghost" onclick={() => void openDetail(project, catalogType)}>{t("resources.detail.open")}</button>
                <button class="btn small ghost" disabled={downloading} onclick={() => void openDownloadDialog(project)}>{t("resources.download.button")}</button>
                <button
                  class="btn small ghost fav-btn"
                  class:fav-on={isFavorite(project.projectId)}
                  aria-pressed={isFavorite(project.projectId)}
                  aria-label={t("resources.detail.favoriteToggleAria").replace("{name}", project.title)}
                  onclick={() => toggleProjectFavorite(project, catalogType)}
                ><Icon name="star" size={14} /></button>
              </div>
            </article>
          {/each}
        </section>
        {#if catalogPage && catalogHits.length < catalogPage.totalHits}
          <div class="res-loadmore">
            <button class="btn secondary" disabled={loadingMore} onclick={() => void runCatalogSearch(false)}>
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
          <button class="btn small ghost detail-back" onclick={closeDetail}>
            <Icon name="arrow-left" size={14} />
            {t("resources.detail.back")}
          </button>

          <div class="detail-grid">
            <div class="col" style="gap:16px">
              <section class="panel pad" aria-labelledby="detail-intro-title">
                <div class="row" style="align-items:flex-start;gap:16px;flex-wrap:wrap">
                  <div class="cube large" aria-hidden="true">
                    {#if project.iconUrl}
                      <img src={project.iconUrl} alt="" loading="lazy" />
                    {:else}
                      {project.title.slice(0, 1)}
                    {/if}
                  </div>
                  <div style="flex:1 1 200px;min-width:0">
                    <h2 class="detail-name" id="detail-intro-title">{project.title}</h2>
                    {#if zhRegion && detailMcmod}
                      <strong class="detail-zh-name">{detailMcmod.zhName}</strong>
                    {/if}
                    {#if project.author}
                      <div class="kv"><span class="k">{t("resources.detail.kvAuthor")}</span><span>{project.author}</span></div>
                    {/if}
                    <div class="kv"><span class="k">{t("resources.detail.kvSource")}</span><span>{t("resources.detail.sourceValue")}</span></div>
                    {#if project.description}
                      <div class="kv"><span class="k">{t("resources.detail.kvIntro")}</span><span>{project.description}</span></div>
                    {/if}
                    {#if zhRegion && detailMcmod && detailMcmod.zhDescription !== project.description}
                      <div class="kv"><span class="k">{t("resources.detail.kvZh")}</span><span>{detailMcmod.zhDescription}</span></div>
                    {/if}
                    {#if project.downloads > 0 || project.dateModified}
                      <div class="detail-stats">
                        <span>{t("resources.catalog.downloads").replace("{count}", formatDownloads(project.downloads))}</span>
                        {#if project.dateModified}<span>{formatDate(project.dateModified)}</span>{/if}
                      </div>
                    {/if}
                    <div class="detail-actions">
                      <button class="btn small ghost" onclick={() => void runtime.openExternalUrl(modrinthProjectUrl(project))}>{t("resources.detail.openModrinth")}</button>
                      {#if zhRegion && detailMcmod}
                        <button class="btn small ghost" onclick={() => void runtime.openExternalUrl(detailMcmod.mcmodUrl)}>{t("resources.detail.openMcmod")}</button>
                      {/if}
                      <button class="btn small ghost" onclick={() => void copyDetailText("name")}>{detailCopied === "name" ? t("resources.detail.copied") : t("resources.detail.copyName")}</button>
                      <button class="btn small ghost" onclick={() => void copyDetailText("link")}>{detailCopied === "link" ? t("resources.detail.copied") : t("resources.detail.copyLink")}</button>
                      <button
                        class="btn small ghost fav-text-btn"
                        class:fav-on={isFavorite(project.projectId)}
                        aria-pressed={isFavorite(project.projectId)}
                        aria-label={t("resources.detail.favoriteToggleAria").replace("{name}", project.title)}
                        onclick={() => toggleProjectFavorite(project, detailType)}
                      >
                        <Icon name="star" size={14} />
                        {isFavorite(project.projectId) ? t("resources.detail.favoriteRemove") : t("resources.detail.favoriteAdd")}
                      </button>
                    </div>
                  </div>
                </div>
              </section>

              <section class="panel pad" aria-labelledby="detail-filter-title">
                <div class="panel-title" id="detail-filter-title">{t("resources.detail.filterTitle")}</div>
                <div class="seg wrap" role="group" aria-label={t("resources.detail.gameFilterAria")} style="margin-top:8px">
                  <button class:on={detailGameFilter === ""} aria-pressed={detailGameFilter === ""} onclick={() => { detailGameFilter = ""; }}>{t("resources.catalog.filterVersionAll")}</button>
                  {#each detailGameOptions as option}
                    <button class:on={detailGameFilter === option} aria-pressed={detailGameFilter === option} onclick={() => { detailGameFilter = option; }}>{option}</button>
                  {/each}
                </div>
                {#if detailType === "mod"}
                  <div class="seg wrap" role="group" aria-label={t("resources.detail.loaderFilterAria")} style="margin-top:8px">
                    <button class:on={detailLoaderFilter === ""} aria-pressed={detailLoaderFilter === ""} onclick={() => { detailLoaderFilter = ""; }}>{t("resources.catalog.filterLoaderAll")}</button>
                    {#each Object.entries(LOADER_NAMES) as [kind, name]}
                      <button class:on={detailLoaderFilter === kind} aria-pressed={detailLoaderFilter === kind} onclick={() => { detailLoaderFilter = kind; }}>{name}</button>
                    {/each}
                  </div>
                {/if}
              </section>

              <section class="panel pad" aria-labelledby="detail-files-title">
                <h2 class="panel-title" id="detail-files-title" style="margin:0">{t("resources.detail.filesTitle")}</h2>
                {#if detailVersionsLoading}
                  <div class="col" style="gap:8px;margin-top:12px" aria-live="polite" aria-label={t("resources.download.loadingVersions")}>
                    <div class="skel" style="height:14px;width:40%"></div>
                    <div class="skel" style="height:34px;width:100%"></div>
                    <div class="skel" style="height:34px;width:100%"></div>
                  </div>
                {:else if detailVersionsError}
                  <div class="err-block" role="alert" style="margin-top:12px;margin-bottom:0">
                    <div class="err-line"><b>{t("resources.error.title")}</b></div>
                    <div class="err-line">{t("resources.error.impactGeneric")}</div>
                    <div class="row">
                      <button class="btn small primary" onclick={() => void reloadDetailVersions()}>{t("resources.error.retry")}</button>
                      <button class="btn small ghost" onclick={closeDetail}>{t("resources.detail.back")}</button>
                    </div>
                    <details class="adv">
                      <summary>{t("resources.error.techSummary")}</summary>
                      <div class="adv-body"><div class="mono dim" style="overflow-wrap:anywhere">{detailVersionsError}</div></div>
                    </details>
                  </div>
                {:else if detailGroups.length === 0}
                  <div class="muted" style="padding:10px 0">{t("resources.detail.filesEmpty")}</div>
                {:else}
                  <div class="dv-groups" style="margin-top:8px">
                    {#each detailGroups as group}
                      {@const open = detailGroupOpen(group, detailGroups.length)}
                      <div class="dv-group">
                        <button class="dv-group-head" aria-expanded={open} onclick={() => toggleDetailGroup(group, detailGroups.length)}>
                          <span class="dv-chevron">{open ? "▾" : "▸"}</span>
                          <strong>{group.key === SNAPSHOT_GROUP_KEY || group.key === UNKNOWN_GROUP_KEY ? groupLabel(group.key) : `Minecraft ${group.key}`}</strong>
                          {#if group.isSelected}
                            <span class="tag accent">{t("resources.detail.selectedGroup")}</span>
                          {/if}
                          <span class="dim" style="margin-left:auto">{t("resources.detail.groupCount").replace("{count}", String(group.versions.length))}</span>
                        </button>
                        {#if open}
                          <div class="dv-file-list">
                            {#each group.versions as version}
                              <article class="detail-file-row">
                                <div class="dv-file-main">
                                  <strong>{versionOptionLabel(version)}</strong>
                                  <small>{formatDate(version.datePublished)} · {t("resources.catalog.downloads").replace("{count}", formatDownloads(version.downloads))}</small>
                                  <small class="dv-tags">{versionGameTags(version)}{#if detailType === "mod" && version.loaders.length > 0} · {version.loaders.map(loaderName).join("、")}{/if}</small>
                                </div>
                                <button
                                  class="btn small secondary"
                                  disabled={Boolean(previewingProject) || Boolean(packPreviewing) || Boolean(resourceInstalling) || packInstalling}
                                  onclick={() => runDetailVersionAction(version)}
                                >{detailVersionActionLabel()}</button>
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

            <div class="col" style="gap:16px">
              <section class="panel pad" aria-labelledby="detail-install-title">
                <div class="panel-title" id="detail-install-title">{t("resources.detail.installTitle")}</div>
                {#if detailType === "modpack"}
                  <p class="muted" style="margin:8px 0 0;font-size:13px">{t("resources.detail.modpackHint")}</p>
                  <button class="btn primary" style="width:100%;margin-top:14px" disabled={Boolean(packPreviewing) || packInstalling} onclick={() => void previewPack(project)}>
                    {packPreviewing ? t("resources.catalog.parsing") : t("resources.catalog.install")}
                  </button>
                {:else if selectedInstance()}
                  {@const instance = selectedInstance()!}
                  {#if detailVersions.length > 0}
                    <div class="row" style="margin-top:8px">
                      {#if detailCompatible}
                        <span class="tag ok"><span class="cdot"></span>{t("resources.detail.compat")}</span>
                        <span class="dim">{t("resources.detail.compatNote").replace("{version}", instance.gameVersion).replace("{loader}", loaderName(instance.loaderKind))}</span>
                      {:else}
                        <span class="tag warn"><span class="cdot"></span>{t("resources.detail.noMatch")}</span>
                      {/if}
                    </div>
                  {/if}
                  <label class="field" style="margin-top:14px">
                    <span class="field-label">{t("resources.detail.installTarget")}</span>
                    <select class="input" value={selectedInstanceId} onchange={(event) => void selectInstance(event)} aria-label={t("resources.detail.installTarget")}>
                      {#each eligibleInstances as candidate}
                        <option value={candidate.id}>{candidate.name}（{candidate.gameVersion} · {loaderName(candidate.loaderKind)}）</option>
                      {/each}
                    </select>
                    <span class="help">{t("resources.detail.installTargetHelp")}</span>
                  </label>
                  <button
                    class="btn primary"
                    style="width:100%;margin-top:14px"
                    disabled={Boolean(previewingProject) || Boolean(resourceInstalling) || resourceVersionsLoading}
                    onclick={() => installDetailProject()}
                  >{t("resources.detail.installTo").replace("{name}", instance.name)}</button>
                {:else}
                  <p class="muted" style="margin:8px 0 0;font-size:13px">{t("resources.catalog.needInstance")}</p>
                  <button class="btn secondary" style="width:100%;margin-top:14px" onclick={() => onNavigate("instances")}>{t("resources.catalog.createInstance")}</button>
                {/if}
              </section>
            </div>
          </div>
        </div>
      {/if}

      {#if queued}
        <div class="content-queued" role="status">
          <div><strong>{t("resources.queuedTitle")}</strong></div>
          <button class="btn primary" onclick={onOpenTasks}>{t("resources.viewTasks")}</button>
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

  {#if preview}
    <div class="modal-mask">
      <div
        class="modal"
        style="width:560px"
        role="dialog"
        aria-modal="true"
        aria-labelledby="preview-dialog-title"
        tabindex="-1"
        bind:this={previewDialog}
        onkeydown={handlePreviewDialogKeydown}
      >
        <h3 id="preview-dialog-title">{t("resources.preview.confirmTitle").replace("{name}", preview.plan.instanceName)}</h3>
        <div class="m-body">
          {#if previewVersions.length > 0}
            <div class="field" style="margin-bottom:12px">
              <span class="field-label">{t("resources.download.versionLabel")}</span>
              <VersionPicker
                versions={previewVersions}
                kind="mod"
                target={selectedInstance() ? { gameVersion: selectedInstance()!.gameVersion, loaderKind: selectedInstance()!.loaderKind } : null}
                value={previewVersionId}
                showAuto
                disabled={Boolean(previewingProject)}
                ariaLabel={t("resources.download.versionAria")}
                onSelect={(versionId) => void selectPreviewVersion(versionId)}
              />
            </div>
          {/if}
          {#each preview.plan.entries as entry}
            {@const isRoot = entry.projectId === preview.plan.rootProjectId}
            {@const isOptional = preview.plan.optionalDependencies.some((dependency) => dependency.projectId === entry.projectId)}
            {#if !isOptional}
              <div class="install-line">
                <span class="tag {isRoot ? "neutral" : "accent"}" style="flex:none">{isRoot ? t("resources.preview.role.root") : t("resources.preview.role.requiredShort")}</span>
                <div style="flex:1;min-width:0">
                  <div style="font-weight:600"><span>{entry.projectTitle}</span> <span class="dim">{entry.versionNumber}</span></div>
                  <div class="dim">{isRoot ? `${entry.file.filename} · ${bytes(entry.file.size)}` : t("resources.preview.requiredNote")}</div>
                </div>
                {#if !isRoot}<span class="ck on">✓</span>{/if}
              </div>
            {/if}
          {/each}
          {#each preview.plan.optionalDependencies as dependency}
            {#if dependency.projectId}
              <label class="install-line optional-line">
                <span class="tag neutral" style="flex:none">{t("resources.preview.role.optionalShort")}</span>
                <div style="flex:1;min-width:0">
                  <div style="font-weight:600">{dependency.title}</div>
                  <div class="dim">{t("resources.preview.optionalDeclaredBy").replace("{id}", dependency.requiredByProjectId)}</div>
                </div>
                <input
                  type="checkbox"
                  class="ck-input"
                  checked={selectedOptionalProjects.includes(dependency.projectId)}
                  disabled={Boolean(previewingProject)}
                  onchange={(event) => toggleOptional(dependency.projectId!, (event.currentTarget as HTMLInputElement).checked)}
                />
              </label>
            {/if}
          {/each}
          {#if preview.plan.incompatibleDependencies.length > 0}
            <div class="banner warn" style="margin-top:12px"><span><strong>{t("resources.preview.incompatibleTitle")}</strong> {t("resources.preview.incompatibleBody")}</span></div>
          {/if}
          <p style="margin-top:14px">{t("resources.preview.impact").replace("{name}", preview.plan.instanceName)}</p>
          {#if preview.plan.entries.some((entry) => !entry.file.sha1 && !entry.file.sha512)}
            <div class="banner warn" style="margin-top:14px;align-items:flex-start">
              <span>{t("resources.preview.unverified")}</span>
            </div>
          {/if}
          {#if catalogError}
            <div class="banner danger" style="margin-top:14px" role="alert"><span>{catalogError}</span></div>
          {/if}
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={submitting} onclick={closePreview}>{t("common.cancel")}</button>
          {#if optionalSelectionDirty}
            <button class="btn secondary" disabled={Boolean(previewingProject)} onclick={() => void applyOptionalSelection()}>{t("resources.preview.applyOptional")}</button>
          {/if}
          <button class="btn danger-soft" disabled={submitting || optionalSelectionDirty || Boolean(previewingProject)} onclick={() => void confirm()}>{submitting ? t("resources.submitting") : t("resources.preview.confirmAnyway")}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if packPreview}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pack-dialog-title"
        tabindex="-1"
        bind:this={packDialog}
        onkeydown={handlePackDialogKeydown}
      >
        <h3 id="pack-dialog-title">{t("resources.catalog.packPreviewTitle")}</h3>
        <div class="m-body">
          {#if packVersions.length > 0}
            <div class="field" style="margin-bottom:12px">
              <span class="field-label">{t("resources.download.versionLabel")}</span>
              <VersionPicker
                versions={packVersions}
                kind="modpack"
                value={packVersionId}
                showAuto
                disabled={Boolean(packPreviewing) || packInstalling}
                ariaLabel={t("resources.download.versionAria")}
                onSelect={(versionId) => void selectPackVersion(versionId)}
              />
            </div>
          {/if}
          <div class="install-line">
            <span class="tag accent" style="flex:none">{t("resources.catalog.type.modpack")}</span>
            <div style="flex:1;min-width:0">
              <div style="font-weight:600">{packPreview.preview.name} {packPreview.preview.version}</div>
              <div class="dim">Minecraft {packPreview.preview.gameVersion} · {loaderName(packPreview.preview.loaderKind)} {packPreview.preview.loaderVersion}</div>
            </div>
          </div>
          <p class="dim" style="margin-top:12px">{t("resources.catalog.packFiles").replace("{count}", String(packPreview.preview.fileCount)).replace("{size}", bytes(packPreview.preview.totalBytes))}</p>
          <p style="margin-top:10px">{t("resources.catalog.packImpact")}</p>
          {#if catalogError}
            <div class="banner danger" style="margin-top:14px" role="alert"><span>{catalogError}</span></div>
          {/if}
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={packInstalling} onclick={closePackPreview}>{t("common.cancel")}</button>
          <button class="btn primary" disabled={packInstalling || Boolean(packPreviewing)} onclick={() => void confirmPackInstall()}>{packInstalling ? t("resources.catalog.installing") : t("resources.catalog.confirmInstall")}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if resourceInstallTarget}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="resource-dialog-title"
        tabindex="-1"
        bind:this={resourceDialog}
        onkeydown={handleResourceDialogKeydown}
      >
        <h3 id="resource-dialog-title">{t("resources.catalog.resourceInstallTitle").replace("{name}", resourceInstallTarget.title)}</h3>
        <div class="m-body">
          {#if resourceVersionsLoading}
            <div class="col" style="gap:8px" aria-live="polite" aria-label={t("resources.download.loadingVersions")}>
              <div class="skel" style="height:14px;width:40%"></div>
              <div class="skel" style="height:34px;width:100%"></div>
            </div>
          {:else}
            <div class="field">
              <span class="field-label">{t("resources.download.versionLabel")}</span>
              <VersionPicker
                versions={resourceVersions}
                kind={catalogView === "detail" ? detailType : catalogType}
                target={selectedInstance() ? { gameVersion: selectedInstance()!.gameVersion, loaderKind: selectedInstance()!.loaderKind } : null}
                value={resourceVersionId}
                disabled={Boolean(resourceInstalling)}
                ariaLabel={t("resources.download.versionAria")}
                onSelect={(versionId) => { resourceVersionId = versionId; }}
              />
            </div>
            {#if selectedInstance()}
              <p class="dim" style="margin-top:12px">{t("resources.catalog.resourceTarget").replace("{name}", selectedInstance()!.name)}</p>
            {/if}
            {#if catalogError}
              <div class="banner danger" style="margin-top:14px" role="alert"><span>{catalogError}</span></div>
            {/if}
          {/if}
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={Boolean(resourceInstalling)} onclick={() => { resourceInstallTarget = null; }}>{t("common.cancel")}</button>
          <button class="btn primary" disabled={Boolean(resourceInstalling) || resourceVersionsLoading || !resourceVersionId} onclick={() => void confirmResourceInstall()}>{resourceInstalling ? t("resources.catalog.installing") : t("resources.catalog.confirmInstall")}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if downloadTarget}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="download-dialog-title"
        tabindex="-1"
        bind:this={downloadDialog}
        onkeydown={handleDownloadDialogKeydown}
      >
        <h3 id="download-dialog-title">{t("resources.download.title").replace("{name}", downloadTarget.title)}</h3>
        <div class="m-body">
          <p class="dim">{t("resources.download.description")}</p>
          {#if downloadLoadingVersions}
            <div class="col" style="gap:8px;margin-top:12px" aria-live="polite" aria-label={t("resources.download.loadingVersions")}>
              <div class="skel" style="height:14px;width:40%"></div>
              <div class="skel" style="height:34px;width:100%"></div>
            </div>
          {:else}
            <div class="download-form">
              <div class="field">
                <span class="field-label">{t("resources.download.versionLabel")}</span>
                <VersionPicker
                  versions={downloadVersions}
                  kind={catalogType}
                  target={selectedInstance() ? { gameVersion: selectedInstance()!.gameVersion, loaderKind: selectedInstance()!.loaderKind } : null}
                  value={downloadVersionId}
                  ariaLabel={t("resources.download.versionAria")}
                  onSelect={selectDownloadVersion}
                />
              </div>
              <label class="field">
                <span class="field-label">{t("resources.download.fileNameLabel")}</span>
                <input class="input" bind:value={downloadFileName} type="text" aria-label={t("resources.download.fileNameAria")} />
              </label>
              <div class="download-dest" role="radiogroup" aria-label={t("resources.download.destAria")}>
                {#if selectedInstance()}
                  <label class="download-dest-option">
                    <input type="radio" name="download-dest" checked={downloadDest === "instance"} onchange={() => { downloadDest = "instance"; }} />
                    <span>{t("resources.download.destInstance").replace("{name}", selectedInstance()?.name ?? "")}</span>
                  </label>
                {/if}
                <label class="download-dest-option">
                  <input type="radio" name="download-dest" checked={downloadDest === "custom"} onchange={() => void selectCustomDest()} />
                  <span>{t("resources.download.destCustom")}</span>
                </label>
                {#if downloadDest === "custom"}
                  <div class="download-dir-row">
                    <button class="btn small ghost" onclick={() => void pickDownloadDir()}>{t("resources.download.pickDir")}</button>
                    <span class="download-dir">{downloadCustomDir || t("resources.download.noDir")}</span>
                  </div>
                {/if}
              </div>
            </div>
            {#if catalogError}
              <div class="banner danger" style="margin-top:14px" role="alert"><span>{catalogError}</span></div>
            {/if}
          {/if}
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={downloading} onclick={() => { downloadTarget = null; }}>{t("common.cancel")}</button>
          <button class="btn primary" disabled={downloading || downloadLoadingVersions || !downloadVersionId || !downloadFileName.trim()} onclick={() => void confirmDownload()}>{downloading ? t("resources.download.running") : t("resources.download.confirm")}</button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

<style>
  /* ---- 目录工具行（mockup 05 窗口 1） ---- */
  .res-toolbar {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 16px;
  }
  .res-toolbar form {
    display: flex;
    gap: 10px;
  }
  .res-search {
    flex: 1;
    min-width: 0;
    height: 40px;
    font-size: 14px;
  }
  .res-toolbar .row {
    flex-wrap: wrap;
  }
  .res-toolbar .seg {
    flex-wrap: wrap;
  }
  .search-submit {
    flex: none;
  }
  .inst-select {
    height: 34px;
    max-width: 280px;
  }
  .src-note {
    font-size: 12px;
    color: var(--text-3);
  }
  .res-filters {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .res-filter {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-3);
  }
  .res-filter .input {
    height: 32px;
    font-size: 12.5px;
  }
  .res-filter input.input {
    width: 110px;
  }

  /* ---- 结果行（mockup 05 res-row） ---- */
  .res-row {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    padding: 12px 14px;
  }
  .res-row + .res-row {
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .rr-main {
    flex: 1 1 240px;
    min-width: 0;
  }
  .rr-main > .row {
    flex-wrap: wrap;
    row-gap: 2px;
  }
  .rr-name {
    font-size: 13.5px;
    font-weight: 600;
  }
  .version-range-badge {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rr-desc {
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 2px;
  }
  .rr-mcmod {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .rr-mcmod strong {
    color: var(--accent);
    font-weight: 600;
  }
  .rr-meta {
    text-align: right;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .rr-actions {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex-wrap: wrap;
    max-width: 250px;
  }
  .fav-btn.fav-on,
  .fav-text-btn.fav-on {
    color: var(--accent);
  }
  .fav-btn.fav-on :global(svg),
  .fav-text-btn.fav-on :global(svg) {
    fill: currentColor;
  }
  .res-loadmore {
    display: flex;
    justify-content: center;
    margin-top: 14px;
  }
  .skel-row {
    pointer-events: none;
  }

  /* ---- 详情副视图（mockup 05 窗口 2） ---- */
  .detail-view {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .detail-back {
    align-self: flex-start;
  }
  .detail-grid {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 16px;
    align-items: start;
  }
  .detail-grid > .col {
    min-width: 0;
  }
  @media (max-width: 1100px) {
    .detail-grid {
      grid-template-columns: 1fr;
    }
  }
  .detail-name {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
  }
  .detail-zh-name {
    color: var(--accent);
    font-size: 13px;
  }
  .kv {
    display: flex;
    gap: 12px;
    padding: 6px 0;
    font-size: 13px;
  }
  .kv .k {
    width: 88px;
    flex: none;
    color: var(--text-3);
    font-size: 12.5px;
  }
  .kv span:last-child {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .detail-stats {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
    margin-top: 8px;
    color: var(--text-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .detail-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .seg.wrap {
    flex-wrap: wrap;
  }
  .field-label {
    font-size: 12.5px;
    color: var(--text-2);
    font-weight: 600;
  }

  /* ---- 版本分组与文件行 ---- */
  .dv-group + .dv-group {
    margin-top: 6px;
  }
  .dv-group-head {
    display: flex;
    align-items: center;
    gap: 10px;
    row-gap: 4px;
    flex-wrap: wrap;
    width: 100%;
    padding: 9px 0;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-family: var(--font);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }
  .dv-chevron {
    width: 12px;
    flex: none;
    color: var(--text-3);
  }
  .dv-file-list {
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    overflow: hidden;
    margin: 4px 0 8px;
  }
  .detail-file-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  .detail-file-row:last-child {
    border-bottom: none;
  }
  .dv-file-main {
    min-width: 0;
  }
  .dv-file-main strong,
  .dv-file-main small {
    display: block;
  }
  .dv-file-main small {
    margin-top: 2px;
    color: var(--text-3);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }
  .dv-file-main .dv-tags {
    color: var(--text-2);
    white-space: normal;
    overflow-wrap: anywhere;
  }

  /* ---- 安装确认 modal（mockup 05 窗口 3） ---- */
  .install-line {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    font-size: 13px;
  }
  .install-line:first-of-type {
    border-top: none;
  }
  .optional-line {
    cursor: pointer;
  }
  .ck {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: var(--r);
    border: 1.5px solid var(--glass-border);
    background: rgba(0, 0, 0, 0.22);
    display: grid;
    place-items: center;
  }
  .ck.on {
    background: var(--accent);
    border-color: transparent;
    color: var(--accent-ink);
    font-size: 11px;
    font-weight: 700;
  }
  .ck-input {
    width: 18px;
    height: 18px;
    flex: none;
    margin: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  /* ---- 错误五段结构 ---- */
  .err-block {
    display: flex;
    flex-direction: column;
    gap: 10px;
    border: 1px solid rgba(232, 104, 95, 0.35);
    background: var(--danger-soft);
    border-radius: var(--r);
    padding: 14px 16px;
    margin-bottom: 16px;
  }
  .err-line {
    font-size: 13px;
    color: var(--text-1);
  }

  /* ---- 状态卡（入队/完成） ---- */
  .content-queued {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding: 20px 24px;
    background: var(--glass);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    margin-bottom: 16px;
  }
  .content-queued strong {
    display: block;
    font-size: 13.5px;
  }
  .content-queued span {
    color: var(--text-2);
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  /* ---- 自由下载表单 ---- */
  .download-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 12px;
  }
  .download-dest {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .download-dest-option {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .download-dir-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-left: 26px;
  }
  .download-dir {
    color: var(--text-3);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
</style>
