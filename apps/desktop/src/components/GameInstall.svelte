<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";

  import {
    defaultInstanceName,
    formatBytes,
    installStageLabel,
    recommendedFabricLoader,
    recommendedVersion,
    taskProgressAriaLabel,
  } from "../installation";
  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    FabricLoaderSummary,
    GameVersionSummary,
    InstallPreview,
    InstallTask,
    LoaderChoice,
    MoyuRuntime,
    OnboardingSelection,
    VersionCatalog,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onBack: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  type InstallView = "loading" | "configure" | "previewing" | "confirm" | "queueing" | "queued";

  let {
    runtime,
    settings,
    onBack,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let view = $state<InstallView>("loading");
  let catalog = $state<VersionCatalog | null>(null);
  let selectedVersion = $state<GameVersionSummary | null>(null);
  let fabricLoaders = $state<FabricLoaderSummary[]>([]);
  let quiltLoaders = $state<FabricLoaderSummary[]>([]);
  let forgeVersions = $state<FabricLoaderSummary[]>([]);
  let neoforgeVersions = $state<FabricLoaderSummary[]>([]);
  let loader = $state<LoaderChoice>({ kind: "vanilla" });
  let instanceName = $state("");
  let nameEdited = $state(false);
  let preview = $state<InstallPreview | null>(null);
  let task = $state<InstallTask | null>(null);
  let errorMessage = $state("");
  let loaderMessage = $state("");
  let showOlderVersions = $state(false);
  let pageRoot: HTMLElement | undefined = $state();
  let loaderRequestSequence = 0;
  let taskPoll: ReturnType<typeof setInterval> | undefined;
  let taskPollRunning = false;

  const visibleVersions = $derived(
    (catalog?.versions ?? [])
      .filter((version) => version.releaseType === "release")
      .slice(0, showOlderVersions ? 24 : 3),
  );

  $effect(() => {
    view;
    void tick().then(() => {
      pageRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });
  });

  onMount(() => {
    void loadCatalog();
  });

  onDestroy(() => {
    if (taskPoll) clearInterval(taskPoll);
  });

  async function loadCatalog(): Promise<void> {
    view = "loading";
    errorMessage = "";
    try {
      catalog = await runtime.getGameVersionCatalog();
      const recommended = recommendedVersion(catalog.versions);
      if (!recommended) throw new Error(t("install.error.noVersions"));
      selectedVersion = recommended;
      await loadLoaders(recommended, true);
      view = "configure";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function selectVersion(version: GameVersionSummary): Promise<void> {
    selectedVersion = version;
    loader = { kind: "vanilla" };
    fabricLoaders = [];
    quiltLoaders = [];
    forgeVersions = [];
    neoforgeVersions = [];
    updateGeneratedName();
    await loadLoaders(version, false);
  }

  async function loadLoaders(version: GameVersionSummary, selectRecommended: boolean): Promise<void> {
    await Promise.all([
      loadFabric(version, selectRecommended),
      loadQuilt(version),
      loadForge(version),
      loadNeoForge(version),
    ]);
  }

  async function loadFabric(version: GameVersionSummary, selectRecommended: boolean): Promise<void> {
    const requestSequence = ++loaderRequestSequence;
    loaderMessage = t("install.loader.querying");
    try {
      const compatibleLoaders = await runtime.getFabricLoaders(version.id);
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      fabricLoaders = compatibleLoaders;
      const recommended = recommendedFabricLoader(fabricLoaders);
      if (recommended && selectRecommended) {
        loader = { kind: "fabric", version: recommended.version };
      }
      loaderMessage = fabricLoaders.length === 0 ? t("install.loader.noneAvailable") : "";
      updateGeneratedName();
    } catch (error) {
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      fabricLoaders = [];
      loaderMessage = t("install.loader.metadataUnavailable").replace("{error}", error instanceof Error ? error.message : String(error));
      updateGeneratedName();
    }
  }

  async function loadQuilt(version: GameVersionSummary): Promise<void> {
    try {
      const compatibleLoaders = await runtime.getQuiltLoaders(version.id);
      if (selectedVersion?.id !== version.id) return;
      quiltLoaders = compatibleLoaders;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      quiltLoaders = [];
    }
  }

  async function loadForge(version: GameVersionSummary): Promise<void> {
    try {
      const versions = await runtime.getForgeVersions(version.id);
      if (selectedVersion?.id !== version.id) return;
      forgeVersions = versions;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      forgeVersions = [];
    }
  }

  async function loadNeoForge(version: GameVersionSummary): Promise<void> {
    try {
      const versions = await runtime.getNeoForgeVersions(version.id);
      if (selectedVersion?.id !== version.id) return;
      neoforgeVersions = versions;
      updateGeneratedName();
    } catch {
      if (selectedVersion?.id !== version.id) return;
      neoforgeVersions = [];
    }
  }

  function selectVanilla(): void {
    loader = { kind: "vanilla" };
    updateGeneratedName();
  }

  function selectFabric(): void {
    const recommended = recommendedFabricLoader(fabricLoaders);
    if (!recommended) return;
    loader = { kind: "fabric", version: recommended.version };
    updateGeneratedName();
  }

  function selectQuilt(): void {
    const recommended = recommendedFabricLoader(quiltLoaders);
    if (!recommended) return;
    loader = { kind: "quilt", version: recommended.version };
    updateGeneratedName();
  }

  function selectForge(): void {
    const recommended = recommendedFabricLoader(forgeVersions);
    if (!recommended) return;
    loader = { kind: "forge", version: recommended.version };
    updateGeneratedName();
  }

  function selectNeoForge(): void {
    const recommended = recommendedFabricLoader(neoforgeVersions);
    if (!recommended) return;
    loader = { kind: "neoforge", version: recommended.version };
    updateGeneratedName();
  }

  function selectFabricVersion(version: string): void {
    loader = { kind: "fabric", version };
    updateGeneratedName();
  }

  function selectQuiltVersion(version: string): void {
    loader = { kind: "quilt", version };
    updateGeneratedName();
  }

  function selectForgeVersion(version: string): void {
    loader = { kind: "forge", version };
    updateGeneratedName();
  }

  function selectNeoForgeVersion(version: string): void {
    loader = { kind: "neoforge", version };
    updateGeneratedName();
  }

  function updateGeneratedName(): void {
    if (!nameEdited && selectedVersion) {
      instanceName = defaultInstanceName(selectedVersion.id, loader);
    }
  }

  async function createPreview(): Promise<void> {
    if (!selectedVersion || instanceName.trim() === "") return;
    view = "previewing";
    errorMessage = "";
    try {
      preview = await runtime.previewInstall({
        instanceName: instanceName.trim(),
        gameVersion: selectedVersion,
        loader,
        isolation: "full",
      });
      view = "confirm";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function confirmInstall(): Promise<void> {
    if (!preview) return;
    view = "queueing";
    errorMessage = "";
    try {
      task = await runtime.confirmInstallPreview(preview.id);
      view = "queued";
      startTaskPolling();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "confirm";
    }
  }

  function startTaskPolling(): void {
    if (taskPoll) clearInterval(taskPoll);
    taskPoll = setInterval(() => void refreshCurrentTask(), 400);
  }

  async function refreshCurrentTask(): Promise<void> {
    if (!task || taskPollRunning) return;
    taskPollRunning = true;
    try {
      const refreshed = (await runtime.getInstallTasks()).find((candidate) => candidate.id === task?.id);
      if (refreshed) task = refreshed;
      if (refreshed && ["completed", "failed", "cancelled"].includes(refreshed.state)) {
        if (taskPoll) clearInterval(taskPoll);
        taskPoll = undefined;
      }
    } finally {
      taskPollRunning = false;
    }
  }

  function taskStateLabel(current: InstallTask): string {
    if (current.state === "completed") return t("install.taskState.completed");
    if (current.state === "failed") return t("install.taskState.failed");
    if (current.state === "committing") return t("install.taskState.committing");
    if (current.state === "running") return t("install.taskState.running");
    return t("install.taskState.waiting");
  }

  function returnToConfiguration(): void {
    preview = null;
    view = "configure";
  }

  function releaseDescription(version: GameVersionSummary): string {
    const date = new Date(version.releaseTime);
    const releaseDate = Number.isNaN(date.valueOf())
      ? t("install.version.unknownDate")
      : new Intl.DateTimeFormat(uiLanguage(), { dateStyle: "medium" }).format(date);
    return t("install.version.releaseLine").replace("{date}", releaseDate) + (version.recommended ? t("install.version.stableSuffix") : "");
  }
</script>

<AppShell
  pageTitle={view === "confirm" || view === "queueing" ? t("install.pageTitle.confirm") : view === "queued" ? t("install.pageTitle.queued") : t("install.pageTitle.configure")}
  dataDirectory={settings.dataDirectory}
  activeNavigation={view === "queued" ? "tasks" : "instances"}
  connectionStatus={catalog?.source === "cache" ? t("install.connection.cache") : t("install.connection.online")}
  taskStatus={task ? t("install.taskStatus.active").replace("{state}", taskStateLabel(task)) : t("shell.status.noTasks")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content install-content" bind:this={pageRoot}>
    {#if view === "loading"}
      <section class="install-loading" aria-live="polite">
        <div class="loading-line wide"></div>
        <div class="loading-line"></div>
        <strong>{t("install.loading.title")}</strong>
        <span>{t("install.loading.description")}</span>
      </section>
    {:else if view === "configure"}
      <div class="install-scroll" data-scroll-region="main">
        <header class="install-heading">
          <button class="button ghost compact" aria-label={t("settings.back")} onclick={onBack}>{t("install.back")}</button>
          <div>
            <h1>{t("home.empty.installFirst")}</h1>
            <p>{t("install.heading.description")}</p>
          </div>
        </header>

        {#if catalog?.source === "cache"}
          <div class="info-banner" role="status">
            <Icon name="info" size={16} />
            <span>{t("install.cacheBanner")}</span>
          </div>
        {/if}
        {#if errorMessage}
          <div class="error-block" role="alert">
            <strong>{t("install.error.title")}</strong>
            <span>{t("install.error.body")}</span>
            <span>{errorMessage}</span>
            <button class="button ghost compact" onclick={() => void loadCatalog()}>{t("install.error.retry")}</button>
          </div>
        {/if}

        {#if selectedVersion}
          <section class="install-section" aria-labelledby="game-version-heading">
            <div class="section-number">1</div>
            <div class="section-content">
              <h2 id="game-version-heading">{t("install.version.heading")}</h2>
              <div class="install-choice-list" role="radiogroup" aria-label={t("install.version.heading")}>
                {#each visibleVersions as version, index}
                  <button
                    class:selected={selectedVersion.id === version.id}
                    class="install-choice-row"
                    role="radio"
                    aria-checked={selectedVersion.id === version.id}
                    data-autofocus={index === 0 ? "true" : undefined}
                    onclick={() => void selectVersion(version)}
                  >
                    <span class="radio-mark"></span>
                    <span class="choice-copy">
                      <strong>{version.id}{#if version.recommended}<em>{t("install.version.recommended")}</em>{/if}</strong>
                      <small>{releaseDescription(version)}</small>
                    </span>
                  </button>
                {/each}
              </div>
              <button class="inline-link version-toggle" onclick={() => showOlderVersions = !showOlderVersions}>
                {showOlderVersions ? t("install.version.showLess") : t("install.version.showMore")}
              </button>
            </div>
          </section>

          <section class="install-section" aria-labelledby="loader-heading">
            <div class="section-number">2</div>
            <div class="section-content">
              <h2 id="loader-heading">{t("install.loader.heading")}</h2>
              <div class="loader-grid" role="radiogroup" aria-label={t("install.loader.groupAria")}>
                <button class:selected={loader.kind === "vanilla"} class="loader-card" role="radio" aria-checked={loader.kind === "vanilla"} onclick={selectVanilla}>
                  <span class="radio-mark"></span><strong>{t("install.loader.none")}</strong><small>{t("home.loader.vanilla")}</small>
                </button>
                <button class:selected={loader.kind === "forge"} class="loader-card" role="radio" aria-checked={loader.kind === "forge"} disabled={forgeVersions.length === 0} onclick={selectForge}>
                  <span class="radio-mark"></span><strong>Forge</strong><small>{recommendedFabricLoader(forgeVersions)?.version ?? t("install.loader.unavailable")}{t("install.loader.recommendedSuffix")}</small>
                </button>
                <button class:selected={loader.kind === "neoforge"} class="loader-card" role="radio" aria-checked={loader.kind === "neoforge"} disabled={neoforgeVersions.length === 0} onclick={selectNeoForge}>
                  <span class="radio-mark"></span><strong>NeoForge</strong><small>{recommendedFabricLoader(neoforgeVersions)?.version ?? t("install.loader.unavailable")}{t("install.loader.recommendedSuffix")}</small>
                </button>
                <button class:selected={loader.kind === "fabric"} class="loader-card" role="radio" aria-checked={loader.kind === "fabric"} disabled={fabricLoaders.length === 0} onclick={selectFabric}>
                  <span class="radio-mark"></span><strong>Fabric</strong><small>{recommendedFabricLoader(fabricLoaders)?.version ?? t("install.loader.unavailable")}{t("install.loader.recommendedSuffix")}</small>
                </button>
                <button class:selected={loader.kind === "quilt"} class="loader-card" role="radio" aria-checked={loader.kind === "quilt"} disabled={quiltLoaders.length === 0} onclick={selectQuilt}>
                  <span class="radio-mark"></span><strong>Quilt</strong><small>{recommendedFabricLoader(quiltLoaders)?.version ?? t("install.loader.unavailable")}{t("install.loader.recommendedSuffix")}</small>
                </button>
              </div>
              {#if loader.kind === "fabric"}
                <label class="loader-version-field">
                  {t("install.loader.fabricField")}
                  <select value={loader.version} onchange={(event) => selectFabricVersion(event.currentTarget.value)}>
                    {#each fabricLoaders as candidate}
                      <option value={candidate.version}>{candidate.version}{candidate.recommended ? t("install.loader.recommendedTag") : ""}</option>
                    {/each}
                  </select>
                  <small>{t("install.loader.fabricHint").replace("{version}", selectedVersion.id)}</small>
                </label>
              {/if}
              {#if loader.kind === "quilt"}
                <label class="loader-version-field">
                  {t("install.loader.quiltField")}
                  <select value={loader.version} onchange={(event) => selectQuiltVersion(event.currentTarget.value)}>
                    {#each quiltLoaders as candidate}
                      <option value={candidate.version}>{candidate.version}{candidate.recommended ? t("install.loader.recommendedTag") : ""}</option>
                    {/each}
                  </select>
                  <small>{t("install.loader.quiltHint").replace("{version}", selectedVersion.id)}</small>
                </label>
              {/if}
              {#if loader.kind === "forge"}
                <label class="loader-version-field">
                  {t("install.loader.forgeField")}
                  <select value={loader.version} onchange={(event) => selectForgeVersion(event.currentTarget.value)}>
                    {#each forgeVersions as candidate}
                      <option value={candidate.version}>{candidate.version}{candidate.recommended ? t("install.loader.recommendedTag") : ""}</option>
                    {/each}
                  </select>
                  <small>{t("install.loader.forgeHint").replace("{version}", selectedVersion.id)}</small>
                </label>
              {/if}
              {#if loader.kind === "neoforge"}
                <label class="loader-version-field">
                  {t("install.loader.neoforgeField")}
                  <select value={loader.version} onchange={(event) => selectNeoForgeVersion(event.currentTarget.value)}>
                    {#each neoforgeVersions as candidate}
                      <option value={candidate.version}>{candidate.version}{candidate.recommended ? t("install.loader.recommendedTag") : ""}</option>
                    {/each}
                  </select>
                  <small>{t("install.loader.neoforgeHint").replace("{version}", selectedVersion.id)}</small>
                </label>
              {/if}
              {#if loaderMessage}<p class="hint">{loaderMessage}</p>{/if}
            </div>
          </section>

          <section class="install-section" aria-labelledby="instance-name-heading">
            <div class="section-number">3</div>
            <div class="section-content">
              <h2 id="instance-name-heading">{t("install.name.heading")}</h2>
              <div class="install-form-card">
                <label>
                  {t("install.name.label")}
                  <input
                    value={instanceName}
                    maxlength="120"
                    oninput={(event) => {
                      nameEdited = true;
                      instanceName = event.currentTarget.value;
                    }}
                  />
                  <small>{t("install.name.hint")}</small>
                </label>
                <div class="managed-location">
                  <span>{t("install.name.locationLabel")}</span>
                  <code>{settings.dataDirectory}\instances\&lt;{t("install.name.instanceId")}&gt;</code>
                  <small>{t("install.name.locationHint")}</small>
                </div>
              </div>
            </div>
          </section>

          <footer class="install-actions">
            <button class="button primary large" disabled={instanceName.trim() === ""} onclick={() => void createPreview()}>
              {t("install.action.preview")} <Icon name="arrow-right" size={14} />
            </button>
            <p>{t("install.action.previewHint")}</p>
          </footer>
        {/if}
      </div>
    {:else if view === "previewing"}
      <section class="install-loading" aria-live="polite">
        <div class="loading-line wide"></div>
        <div class="loading-line"></div>
        <strong>{t("install.previewing.title")}</strong>
        <span>{t("install.previewing.description")}</span>
      </section>
    {:else if (view === "confirm" || view === "queueing") && preview}
      <div class="install-scroll confirm-layout" data-scroll-region="main">
        <header class="install-heading">
          <button class="button ghost compact" disabled={view === "queueing"} onclick={returnToConfiguration}>{t("install.confirm.backEdit")}</button>
          <div><h1>{t("install.confirm.heading")}</h1><p>{t("install.confirm.description")}</p></div>
        </header>
        {#if errorMessage}
          <div class="error-block" role="alert"><strong>{t("install.confirm.errorTitle")}</strong><span>{errorMessage}</span></div>
        {/if}
        <dl class="install-summary">
          <div><dt>{t("install.confirm.versionLabel")}</dt><dd>{preview.gameVersion}</dd><span>{t("install.confirm.versionNote")}</span></div>
          <div><dt>{t("install.confirm.loaderLabel")}</dt><dd>{preview.loaderName}{preview.loaderVersion ? ` ${preview.loaderVersion}` : ""}</dd><span>{t("install.confirm.loaderNote")}</span></div>
          <div><dt>Java</dt><dd>Azul Zulu {preview.javaVersion} · {preview.javaArchitecture}</dd><span>{t("install.confirm.javaNote")}</span></div>
          <div><dt>{t("install.confirm.isolationLabel")}</dt><dd>{t("install.confirm.isolationValue")}</dd><span>{t("install.confirm.isolationNote")}</span></div>
          <div><dt>{t("install.confirm.downloadLabel")}</dt><dd>{formatBytes(preview.estimatedDownloadBytes)}</dd><span>{t("install.confirm.downloadNote")}</span></div>
        </dl>
        <div class="stage-preview">
          <h2>{t("install.confirm.stagesTitle")}</h2>
          <ol>
            {#each ["prepare", "downloadGameFiles", "verifyFiles", "installGameEnvironment", "applyLoader", "commitChanges", "createRollbackPoint"] as stage}
              <li>{installStageLabel(stage as import("../runtime").InstallStage)}</li>
            {/each}
          </ol>
        </div>
        <footer class="install-actions confirm-actions">
          <button class="button primary large" data-autofocus="true" disabled={view === "queueing"} onclick={() => void confirmInstall()}>
            {view === "queueing" ? t("install.confirm.creating") : t("install.confirm.start")}
          </button>
          <p>{t("install.confirm.stagingHint")}</p>
        </footer>
      </div>
    {:else if view === "queued" && task}
      <section class="queued-result" aria-live="polite">
        <span class="done-mark"><Icon name={task.state === "completed" ? "check" : "task"} size={18} /></span>
        <h1>{task.state === "completed" ? t("install.queued.titleCompleted") : task.state === "failed" ? t("install.queued.titleFailed") : task.state === "queued" ? t("install.queued.titleQueued") : t("install.queued.titleRunning")}</h1>
        <p>{task.state === "completed" ? t("install.queued.bodyCompleted") : task.state === "failed" ? t("install.queued.bodyFailed") : t("install.queued.bodyDefault")}</p>
        <div class="queued-task-card">
          <div><strong>{task.plan.instanceName}</strong><span>{taskStateLabel(task)}</span></div>
          <ol>
            {#each task.plan.stages as stage, index}
              <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{installStageLabel(stage)}</b></li>
            {/each}
          </ol>
          {#if task.state === "running" || task.state === "committing"}
            <div class="queued-progress" aria-label={taskProgressAriaLabel(task.progress)}>
              <div class="progress-track"><span style:width={task.progress.totalBytes && task.progress.totalBytes > 0 ? `${Math.min(100, task.progress.completedBytes / task.progress.totalBytes * 100)}%` : "24%"}></span></div>
              <small>{task.progress.currentItem ?? t("tasks.progress.processing")}</small>
            </div>
          {:else if task.state === "failed"}
            <div class="error-block task-error" role="alert"><strong>{t("install.queued.failedTitle")}</strong><span>{task.progress.errorSummary ?? t("install.queued.failedHint")}</span></div>
          {/if}
          <small>{t("install.queued.staging")}<code>{task.stagingDirectory}</code></small>
        </div>
        <button class="button primary" data-autofocus="true" onclick={onBack}>{t("settings.back")}</button>
      </section>
    {/if}
  </main>
</AppShell>
