<script lang="ts">
  import { onMount } from "svelte";

  import { t } from "../i18n.svelte";
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
  let pendingResourceDelete = $state<string | null>(null);
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
    return kind === "resourcepack" ? t("resources.kind.resourcepack") : kind === "shader" ? t("resources.kind.shader") : t("resources.kind.datapack");
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
  pageTitle={t("nav.resources")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="resources"
  navigationTargets={["home", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus={searchError ? t("resources.connection.offline") : t("resources.connection.online")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content resource-content">
    <header class="resource-heading">
      <div>
        <h1>{t("resources.heading.title")}</h1>
        <p>{t("resources.heading.description")}</p>
      </div>
      <button class="button ghost compact" onclick={onBack}>{t("settings.back")}</button>
    </header>

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
          <div><h2 id="local-content-title">{t("resources.local.title")}</h2><p>{t("resources.local.description")}</p></div>
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
            <div><strong>{t("resources.updates.queuedTitle")}</strong><span>{t("resources.updates.queuedBody")}</span></div>
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

      <section class="remote-content-section" aria-labelledby="remote-content-title">
        <header><h2 id="remote-content-title">{t("resources.remote.title")}</h2><p>{t("resources.remote.description")}</p></header>
        <form class="content-search" onsubmit={(event) => void search(event)}>
          <label>
            <span class="sr-live">{t("resources.remote.searchLabel")}</span>
            <Icon name="search" size={15} />
            <input bind:value={query} type="search" aria-label={t("resources.remote.searchLabel")} placeholder={t("resources.remote.searchPlaceholder")} />
          </label>
          <button class="button primary" disabled={searching || !query.trim()}>{searching ? t("resources.remote.searching") : t("resources.remote.searchSubmit")}</button>
        </form>

        {#if searchError}
          <div class="error-block content-search-error" role="alert">
            <strong>{t("resources.remote.errorTitle")}</strong>
            <span>{searchError}</span>
          </div>
        {/if}

        {#if searchPage && searchPage.hits.length === 0}
          <div class="content-search-empty">{t("resources.remote.noResults")}</div>
        {:else if searchPage}
          <div class="content-result-list" aria-label={t("resources.remote.resultAria")}>
            {#each searchPage.hits as project}
              <article class="content-result-card">
                <div>
                  <strong>{project.title}</strong>
                  <p>{project.description}</p>
                  <small>{t("resources.remote.projectLine").replace("{id}", project.projectId).replace("{downloads}", project.downloads.toLocaleString())}</small>
                </div>
                <button class="button" disabled={Boolean(previewingProject)} onclick={() => void createPreview(project)}>
                  {previewingProject === project.projectId ? t("resources.remote.parsing") : t("resources.remote.viewPlan")}
                </button>
              </article>
            {/each}
          </div>
        {/if}
      </section>

      {#if preview}
        <section class="content-preview" aria-labelledby="content-preview-title">
          <header><h2 id="content-preview-title">{t("resources.preview.title")}</h2><p>{t("resources.preview.description")}</p></header>
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
          <div><strong>{t("resources.queuedTitle")}</strong><span>{t("resources.queuedBody")}</span></div>
          <button class="button primary" onclick={onOpenTasks}>{t("resources.viewTasks")}</button>
        </div>
      {/if}
    {/if}
  </main>
</AppShell>
