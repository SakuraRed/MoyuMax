<script lang="ts">
  import { onMount, tick } from "svelte";

  import { t } from "../i18n.svelte";
  import type {
    ContentInstallTask,
    CrashReport,
    InstallTask,
    InstalledModpack,
    LaunchSession,
    ManagedInstance,
    ModpackUpdateReport,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    WorldBackupSummary,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    tasks: InstallTask[];
    contentTasks: ContentInstallTask[];
    instances: ManagedInstance[];
    launchSessions: LaunchSession[];
    crashReports: CrashReport[];
    notice: string;
    onInstall: () => void;
    onOpenTasks: () => void;
    onOpenCrash: (report: CrashReport) => void;
    onStateChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    tasks,
    contentTasks,
    instances,
    launchSessions,
    crashReports,
    notice,
    onInstall,
    onOpenTasks,
    onOpenCrash,
    onStateChanged,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingInstance = $state<string | null>(null);
  let actionMessage = $state("");
  let actionError = $state("");
  let recycleCandidate = $state<ManagedInstance | null>(null);
  let recycleDialog = $state<HTMLElement | null>(null);
  let modpacks = $state<Record<string, InstalledModpack>>({});
  let updatingPack = $state<string | null>(null);
  let packReport = $state<ModpackUpdateReport | null>(null);
  let defaultAccountName = $state("");
  let homeRoot: HTMLElement | undefined = $state();

  onMount(async () => {
    await tick();
    homeRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    try {
      const accounts = await runtime.listAccounts();
      defaultAccountName = accounts.find((account) => account.isDefault)?.username ?? "";
    } catch {
      defaultAccountName = "";
    }
  });

  $effect(() => {
    void loadModpacks(instances);
  });

  async function loadModpacks(list: ManagedInstance[]): Promise<void> {
    const next: Record<string, InstalledModpack> = {};
    for (const instance of list) {
      const pack = await runtime.getInstanceModpack(instance.id).catch(() => null);
      if (pack) next[instance.id] = pack;
    }
    modpacks = next;
  }

  async function updatePack(instance: ManagedInstance): Promise<void> {
    updatingPack = instance.id;
    actionMessage = "";
    actionError = "";
    packReport = null;
    try {
      const path = await runtime.pickModpackFile();
      if (!path) return;
      packReport = await runtime.updateModpack(instance.id, path);
      actionMessage = t("modpack.updateDone")
        .replace("{name}", packReport.packName)
        .replace("{from}", packReport.fromVersion)
        .replace("{to}", packReport.toVersion);
      if (packReport.keptUserModified.length > 0) {
        actionMessage += t("modpack.keptNote").replace("{files}", packReport.keptUserModified.join("、"));
      }
      await loadModpacks(instances);
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      updatingPack = null;
    }
  }

  const activeTasks = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );
  const activeLaunches = $derived(
    launchSessions.filter((session) =>
      ["starting", "running"].includes(session.state),
    ),
  );
  const activeContentTasks = $derived(
    contentTasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );

  function activeSession(instanceId: string): LaunchSession | undefined {
    return launchSessions.find(
      (session) =>
        session.instanceId === instanceId &&
        ["starting", "running"].includes(session.state),
    );
  }

  function latestSession(instanceId: string): LaunchSession | undefined {
    return launchSessions.find((session) => session.instanceId === instanceId);
  }

  function crashReportForSession(session: LaunchSession | undefined): CrashReport | undefined {
    if (!session || !["failed", "interrupted"].includes(session.state)) return undefined;
    return crashReports.find((report) => report.launchSessionId === session.id);
  }

  const LOADER_DISPLAY: Record<string, string> = {
    fabric: "Fabric",
    quilt: "Quilt",
    forge: "Forge",
    neoforge: "NeoForge",
  };

  function loaderLabel(instance: ManagedInstance): string {
    const name = LOADER_DISPLAY[instance.loaderKind];
    if (name) {
      return `${name}${instance.loaderVersion ? ` ${instance.loaderVersion}` : ""}`;
    }
    return instance.loaderKind === "vanilla" ? t("home.loader.vanilla") : instance.loaderKind;
  }

  function sessionStateLabel(state: LaunchSession["state"]): string {
    switch (state) {
      case "starting":
        return t("home.state.starting");
      case "running":
        return t("home.state.running");
      case "completed":
        return t("home.state.completed");
      case "failed":
        return t("home.state.failed");
      case "stopped":
        return t("home.state.stopped");
      case "interrupted":
        return t("home.state.interrupted");
    }
  }

  function backupStateLabel(backup: WorldBackupSummary | null | undefined): string {
    if (!backup) return t("home.backup.none");
    switch (backup.state) {
      case "ready":
        return t("home.backup.ready");
      case "skipped":
        return t("home.backup.skipped");
      case "failed":
        return t("home.backup.failed");
      case "staging":
        return t("home.backup.staging");
    }
  }

  async function start(instance: ManagedInstance): Promise<void> {
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.startInstance(instance.id);
      actionMessage = t("home.action.starting").replace("{name}", instance.name);
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = null;
    }
  }

  async function stop(instance: ManagedInstance): Promise<void> {
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.stopInstance(instance.id);
      actionMessage = t("home.action.stopRequested").replace("{name}", instance.name);
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = null;
    }
  }

  async function askRecycle(instance: ManagedInstance): Promise<void> {
    actionMessage = "";
    actionError = "";
    recycleCandidate = instance;
    await tick();
    recycleDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
  }

  function cancelRecycle(): void {
    if (changingInstance === recycleCandidate?.id) return;
    recycleCandidate = null;
  }

  async function recycleInstance(): Promise<void> {
    const instance = recycleCandidate;
    if (!instance) return;
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.recycleInstance(instance.id);
      recycleCandidate = null;
      actionMessage = t("home.action.recycled").replace("{name}", instance.name);
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = null;
    }
  }

  function handleRecycleDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelRecycle();
      return;
    }
    if (event.key !== "Tab" || !recycleDialog) return;
    const controls = [...recycleDialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
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
</script>

<AppShell
  pageTitle={t("nav.home")}
  dataDirectory={settings.dataDirectory}
  searchVisible
  onNavigate={onNavigate}
  taskStatus={activeLaunches.length > 0 ? t("home.taskStatus.running").replace("{count}", String(activeLaunches.length)) : activeTasks.length + activeContentTasks.length > 0 ? t("home.taskStatus.pending").replace("{count}", String(activeTasks.length + activeContentTasks.length)) : t("shell.status.noTasks")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  {#if instances.length === 0}
    <main class="content home-empty" bind:this={homeRoot}>
      <div class="empty-graphic" aria-hidden="true"></div>
      <h1>{t("home.empty.title")}</h1>
      <p>{t("home.empty.description")}</p>
      <button class="button primary large" data-autofocus="true" onclick={onInstall}>{t("home.empty.installFirst")}</button>
      <small>
        {t("home.empty.altPrefix")} <button class="inline-link" onclick={onInstall}>{t("home.empty.importModpack")}</button>
      </small>
      {#if activeTasks.length > 0}
        {#each activeTasks.slice(0, 1) as task}
          <button class="home-task-summary" onclick={onOpenTasks}>
            <span><strong>{task.plan.instanceName}</strong><small>{task.state === "awaitingRecovery" ? t("home.task.awaitingRecovery") : t("home.task.installQueued")}</small></span>
            <span>{t("home.task.count").replace("{count}", String(activeTasks.length))} <Icon name="arrow-right" size={14} /></span>
          </button>
        {/each}
      {/if}
      {#if activeTasks.length === 0 && activeContentTasks.length > 0}
        {#each activeContentTasks.slice(0, 1) as task}
          <button class="home-task-summary" onclick={onOpenTasks}>
            <span><strong>{task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)?.projectTitle ?? t("home.task.modrinthContent")}</strong><small>{task.state === "awaitingRecovery" ? t("home.task.awaitingRecovery") : t("home.task.contentQueued")}</small></span>
            <span>{t("home.task.count").replace("{count}", String(activeContentTasks.length))} <Icon name="arrow-right" size={14} /></span>
          </button>
        {/each}
      {/if}
    </main>
  {:else}
    <main class="content home-content" bind:this={homeRoot}>
      <div class="home-scroll" data-scroll-region="main">
        <header class="home-heading">
          <div>
            <h1>{t("home.heading.title")}</h1>
          </div>
          <button class="button" onclick={onInstall}>{t("home.heading.installOther")}</button>
        </header>

        <section class="instance-list" aria-label={t("home.listAria")}>
          {#each instances as instance, index}
            {@const active = activeSession(instance.id)}
            {@const latest = latestSession(instance.id)}
            {@const crashReport = crashReportForSession(latest)}
            {@const pack = modpacks[instance.id]}
            <article class:running={active?.state === "running"} class:crashed={Boolean(crashReport)} class:instance-hero={index === 0} class="instance-card">
              <div class="instance-cover" class:hero-cover={index === 0} aria-hidden="true">{instance.name.slice(0, 1)}</div>
              <div class="instance-copy">
                <div class="instance-title-line">
                  <h2>{instance.name}</h2>
                  <span class:active={Boolean(active)} class="instance-state">
                    {active ? sessionStateLabel(active.state) : instance.state === "ready" ? t("home.instance.ready") : instance.state}
                  </span>
                </div>
                <p>{t("home.instance.summary").replace("{version}", instance.gameVersion).replace("{loader}", loaderLabel(instance))}</p>
                {#if pack}
                  <small class="modpack-badge">{pack.packName} {pack.packVersion} · {pack.provider === "modrinth" ? "Modrinth" : "CurseForge"}</small>
                {/if}
                {#if latest && !active}
                  <small class="latest-session">{t("home.instance.latestSession")}<span>{sessionStateLabel(latest.state)}</span>{#if latest.exitCode !== null}{t("home.instance.exitCode").replace("{code}", String(latest.exitCode))}{/if}</small>
                  <small class="latest-backups">{t("home.instance.latestBackups").replace("{pre}", backupStateLabel(latest.preLaunchBackup)).replace("{post}", backupStateLabel(latest.postExitBackup))}</small>
                {/if}
                {#if index === 0}
                  <small class="default-account">{t("home.instance.defaultAccount").replace("{name}", defaultAccountName || t("home.instance.localAccount"))}</small>
                {/if}
              </div>
              <div class="instance-actions">
                {#if active}
                  <button
                    class="button"
                    disabled={changingInstance === instance.id}
                    onclick={() => void stop(instance)}
                  >{changingInstance === instance.id ? t("home.launch.stopping") : t("home.launch.stop")}</button>
                {:else}
                  <button
                    class="button primary large"
                    data-autofocus={index === 0 ? "true" : undefined}
                    disabled={changingInstance === instance.id || instance.state !== "ready"}
                    onclick={() => void start(instance)}
                  ><Icon name="play" size={14} />{changingInstance === instance.id ? t("home.launch.starting") : t("home.launch.start")}</button>
                {/if}
                {#if crashReport && !active}
                  <button class="button crash-report-button" onclick={() => onOpenCrash(crashReport)}>{t("home.launch.crashReport")}</button>
                {/if}
                {#if pack && !active}
                  <button
                    class="button ghost"
                    disabled={updatingPack !== null}
                    onclick={() => void updatePack(instance)}
                  >{updatingPack === instance.id ? t("modpack.updating") : t("modpack.update")}</button>
                {/if}
                {#if !active}
                  <button
                    class="button danger-subtle"
                    aria-label={t("home.launch.recycleAria").replace("{name}", instance.name)}
                    disabled={changingInstance === instance.id}
                    onclick={() => void askRecycle(instance)}
                  >{t("home.launch.recycle")}</button>
                {/if}
                <span>{t("home.launch.managedJava")}</span>
              </div>
            </article>
          {/each}
        </section>

        {#if activeTasks.length > 0}
          {#each activeTasks.slice(0, 1) as task}
            <button class="home-task-summary home-task-wide" onclick={onOpenTasks}>
              <span><strong>{task.plan.instanceName}</strong><small>{task.state === "awaitingRecovery" ? t("home.task.awaitingRecovery") : t("home.task.installProcessing")}</small></span>
              <span>{t("home.task.count").replace("{count}", String(activeTasks.length))} <Icon name="arrow-right" size={14} /></span>
            </button>
          {/each}
        {/if}
        {#if activeTasks.length === 0 && activeContentTasks.length > 0}
          {#each activeContentTasks.slice(0, 1) as task}
            <button class="home-task-summary home-task-wide" onclick={onOpenTasks}>
              <span><strong>{task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)?.projectTitle ?? t("home.task.modrinthContent")}</strong><small>{task.state === "awaitingRecovery" ? t("home.task.awaitingRecovery") : t("home.task.contentProcessing")}</small></span>
              <span>{t("home.task.count").replace("{count}", String(activeContentTasks.length))} <Icon name="arrow-right" size={14} /></span>
            </button>
          {/each}
        {/if}
      </div>
    </main>
  {/if}

  {#if actionError}
    <div class="toast danger-toast" role="alert"><Icon name="info" size={16} /><span>{actionError}</span></div>
  {:else if notice || actionMessage}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{actionMessage || notice}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{actionMessage || actionError || notice}</div>

  {#if recycleCandidate}
    <div class="modal-backdrop">
      <div
        class="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="recycle-confirm-title"
        tabindex="-1"
        bind:this={recycleDialog}
        onkeydown={handleRecycleDialogKeydown}
      >
        <header>
          <h2 id="recycle-confirm-title">{t("home.recycle.title").replace("{name}", recycleCandidate.name)}</h2>
          <p>{t("home.recycle.description")}</p>
        </header>
        <div class="confirmation-impact">
          <strong>{t("home.recycle.impactTitle")}</strong>
          <span>{t("home.recycle.impactBody")}</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingInstance === recycleCandidate.id} onclick={cancelRecycle}>{t("common.cancel")}</button>
          <button class="button danger" disabled={changingInstance === recycleCandidate.id} onclick={() => void recycleInstance()}>
            {changingInstance === recycleCandidate.id ? t("home.recycle.moving") : t("home.launch.recycle")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
