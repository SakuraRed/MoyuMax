<script lang="ts">
  import { onMount } from "svelte";

  import { markAvatarFailed, requestSettingsPage, shellAccount, skinAvatarUrl } from "../accounts.svelte";
  import { t } from "../i18n.svelte";
  import type {
    ContentInstallTask,
    CrashReport,
    InstallTask,
    InstalledModpack,
    LaunchSession,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    WorldBackupSummary,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Fish from "./Fish.svelte";

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
    onManageInstance: (instance: ManagedInstance, tab?: string) => void;
    onStateChanged: () => Promise<void>;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    tasks,
    contentTasks,
    instances,
    launchSessions,
    crashReports,
    notice,
    onInstall,
    onOpenTasks,
    onOpenCrash,
    onManageInstance,
    onStateChanged,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingInstance = $state(false);
  let actionMessage = $state("");
  let actionError = $state("");
  let heroPack = $state<InstalledModpack | null>(null);
  let heroIcon = $state("");
  let heroInstallingPack = $state(false);
  let defaultAccountName = $state("");
  let homeRoot: HTMLElement | undefined = $state();

  // 主卡片实例:最近运行过的实例,否则列表首个。
  const hero = $derived.by(() => {
    if (instances.length === 0) return null;
    const withSession = instances.find((instance) =>
      launchSessions.some((session) => session.instanceId === instance.id),
    );
    return withSession ?? instances[0] ?? null;
  });

  onMount(async () => {
    homeRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    try {
      const accounts = await runtime.listAccounts();
      defaultAccountName = accounts.find((account) => account.isDefault)?.username ?? "";
    } catch {
      defaultAccountName = "";
    }
  });

  $effect(() => {
    void loadHeroPack(hero?.id ?? "");
  });

  async function loadHeroPack(instanceId: string): Promise<void> {
    if (!instanceId) {
      heroPack = null;
      heroIcon = "";
      heroInstallingPack = false;
      return;
    }
    const [pack, installing] = await Promise.all([
      runtime.getInstanceModpack(instanceId).catch(() => null),
      runtime.isModpackInstalling(instanceId).catch(() => false),
    ]);
    heroPack = pack;
    heroInstallingPack = installing;
    heroIcon = pack ? ((await runtime.getModpackIconDataUrl(instanceId).catch(() => null)) ?? "") : "";
  }

  const activeTasks = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );
  const activeContentTasks = $derived(
    contentTasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );
  const heroSession = $derived(
    hero
      ? launchSessions.find(
          (session) =>
            session.instanceId === hero.id && ["starting", "running"].includes(session.state),
        )
      : undefined,
  );

  // 需要处理:有未查看崩溃报告的实例(按会话去重,最多 3 条)。
  const issues = $derived.by(() => {
    const seen = new Set<string>();
    const rows: { report: CrashReport; instanceName: string }[] = [];
    for (const session of launchSessions) {
      if (!["failed", "interrupted"].includes(session.state)) continue;
      const report = crashReports.find((candidate) => candidate.launchSessionId === session.id);
      if (!report || seen.has(session.instanceId)) continue;
      seen.add(session.instanceId);
      rows.push({
        report,
        instanceName:
          instances.find((instance) => instance.id === session.instanceId)?.name ??
          session.instanceId,
      });
      if (rows.length >= 3) break;
    }
    return rows;
  });

  const lastSession = $derived(launchSessions[0]);
  const lastSessionInstance = $derived(
    lastSession
      ? (instances.find((instance) => instance.id === lastSession.instanceId)?.name ??
          lastSession.instanceId)
      : "",
  );

  function taskPercent(task: InstallTask): number | null {
    const total = task.progress.totalBytes;
    if (!total || total <= 0) return null;
    return Math.min(100, Math.round((task.progress.completedBytes / total) * 100));
  }

  function sessionDuration(session: LaunchSession): string {
    if (!session.endedAtUnixSeconds) return "";
    const seconds = Math.max(0, session.endedAtUnixSeconds - session.startedAtUnixSeconds);
    const hours = Math.floor(seconds / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (hours > 0) return t("home.session.durationHm").replace("{h}", String(hours)).replace("{m}", String(minutes));
    return t("home.session.durationM").replace("{m}", String(Math.max(1, minutes)));
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

  function identityLabel(): string {
    const account = shellAccount();
    if (account.kind === "microsoft") {
      return t("home.action.identityMicrosoft").replace("{name}", account.name);
    }
    if (account.kind === "authlib") {
      return t("home.action.identityAuthlib").replace("{name}", account.name);
    }
    return t("home.action.identityOffline");
  }

  function openAccounts(): void {
    requestSettingsPage("accounts");
    onNavigate("settings");
  }

  async function startHero(): Promise<void> {
    if (!hero) return;
    changingInstance = true;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.startInstance(hero.id);
      actionMessage = t("home.action.starting")
        .replace("{name}", hero.name)
        .replace("{identity}", identityLabel());
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = false;
    }
  }

  async function stopHero(): Promise<void> {
    if (!hero) return;
    changingInstance = true;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.stopInstance(hero.id);
      actionMessage = t("home.action.stopRequested").replace("{name}", hero.name);
      await onStateChanged();
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
    } finally {
      changingInstance = false;
    }
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
</script>

<AppShell
  pageTitle={t("nav.home")}
  activeNavigation="home"
  taskCount={activeTasks.length + activeContentTasks.length}
  instanceCount={instances.length}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  {#if !hero}
    <main class="content" style="display:flex" bind:this={homeRoot}>
      <div class="empty-stage">
        <Fish variant="tank" />
        <h1 style="font-size:20px">{t("home.empty.title")}</h1>
        <p class="muted" style="max-width:44ch;text-align:center">{t("home.empty.description")}</p>
        <button class="btn primary large" data-autofocus="true" onclick={onInstall}>{t("home.empty.installFirst")}</button>
        <small class="dim">
          {t("home.empty.altPrefix")} <button class="inline-link" onclick={onInstall}>{t("home.empty.importModpack")}</button>
        </small>
      </div>
    </main>
  {:else}
    <main class="content" bind:this={homeRoot}>
      <div class="home-grid">
        <div class="col" style="gap:16px">
          <section class="panel hero-card">
            <div class="cube large" aria-hidden="true">
              {#if heroIcon}<img src={heroIcon} alt="" />{:else}{hero.name.slice(0, 1)}{/if}
            </div>
            <div class="hero-meta" style="flex:1">
              <h1>{hero.name}</h1>
              <div class="ver">
                Minecraft {hero.gameVersion} · {loaderLabel(hero)}{#if heroPack} · {heroPack.packName} {heroPack.packVersion}{/if}
              </div>
              <div class="launch-row">
                {#if heroSession}
                  <button class="btn danger-soft large" disabled={changingInstance} onclick={() => void stopHero()}>
                    {changingInstance ? t("home.launch.stopping") : t("home.hero.stop")}
                  </button>
                  <button class="btn secondary" onclick={() => onManageInstance(hero!, "logs")}>{t("home.launch.logs")}</button>
                {:else}
                  <button
                    class="btn primary large"
                    data-autofocus="true"
                    disabled={changingInstance || hero.state !== "ready" || heroInstallingPack}
                    onclick={() => void startHero()}
                  >{changingInstance ? t("home.launch.starting") : t("home.hero.launch")}</button>
                  <button class="acct-chip" title={t("home.hero.accountChipTitle")} onclick={openAccounts}>
                    {#if shellAccount().loaded && shellAccount().kind !== null}
                      {@const account = shellAccount()}
                      {@const avatarUrl = account.avatarFailed ? "" : skinAvatarUrl(account.playerUuid, account.kind)}
                      <span class="avatar">
                        {#if avatarUrl}<img src={avatarUrl} alt="" onerror={() => markAvatarFailed()} />{:else}{account.name.slice(0, 1) || "?"}{/if}
                      </span>
                      <div>
                        <div style="font-size:12.5px;font-weight:600">{account.name}</div>
                        <div style="font-size:11px;color:var(--text-3)">{t("home.hero.accountChipHint")}</div>
                      </div>
                    {:else}
                      <span class="avatar">?</span>
                      <div>
                        <div style="font-size:12.5px;font-weight:600">{defaultAccountName || t("home.instance.localAccount")}</div>
                        <div style="font-size:11px;color:var(--text-3)">{t("home.hero.accountChipHint")}</div>
                      </div>
                    {/if}
                  </button>
                {/if}
              </div>
            </div>
            {#if heroInstallingPack}
              <span class="tag info"><span class="cdot"></span>{t("modpack.stateInstalling")}</span>
            {:else if heroSession}
              <span class="tag accent"><span class="cdot"></span>{t("home.state.running")}</span>
            {:else if hero.state === "ready"}
              <span class="tag ok"><span class="cdot"></span>{t("home.instance.ready")}</span>
            {:else}
              <span class="tag neutral"><span class="cdot"></span>{hero.state}</span>
            {/if}
          </section>
          {#if heroInstallingPack}
            <div class="banner info" role="status"><span>{t("modpack.installingHint")}</span></div>
          {/if}

          {#if issues.length > 0}
            <section class="panel pad">
              <div class="row spread">
                <div class="panel-title">{t("home.issues.title")}</div>
                <span class="dim">{t("home.issues.count").replace("{count}", String(issues.length))}</span>
              </div>
              {#each issues as issue}
                <div class="issue-row">
                  <span class="tag danger" style="flex:none;margin-top:2px">{t("home.issues.crashTag")}</span>
                  <div class="lr-main">
                    <div class="lr-name">{t("home.issues.crashName").replace("{name}", issue.instanceName)}</div>
                    <div class="lr-sub">{issue.report.summary || t("home.issues.crashHint")}</div>
                  </div>
                  <button class="btn small secondary" onclick={() => onOpenCrash(issue.report)}>{t("home.issues.viewDiagnostics")}</button>
                </div>
              {/each}
            </section>
          {/if}
        </div>

        <div class="col" style="gap:16px">
          <section class="panel pad">
            <div class="row spread">
              <div class="panel-title">{t("home.tasks.title")}</div>
              <button class="btn small ghost" onclick={onOpenTasks}>{t("home.tasks.openCenter")}</button>
            </div>
            {#if activeTasks.length === 0 && activeContentTasks.length === 0}
              <div class="dim" style="padding:8px 0">{t("shell.status.noTasks")}</div>
            {/if}
            {#each activeTasks.slice(0, 3) as task}
              {@const pct = taskPercent(task)}
              <div class="mini-task">
                <div class="row spread">
                  <span style="font-size:13px;font-weight:600">{task.plan.instanceName}</span>
                  {#if pct !== null}<span class="dim">{pct}%</span>{:else}<span class="tag info">{t("home.tasks.queued")}</span>{/if}
                </div>
                <div class="dim" style="margin:2px 0 6px">{task.progress.currentItem ?? t("home.tasks.processing")}</div>
                {#if pct !== null}<div class="progress"><i style="width:{pct}%"></i></div>{/if}
              </div>
            {/each}
            {#each activeContentTasks.slice(0, Math.max(0, 3 - activeTasks.length)) as task}
              <div class="mini-task">
                <div class="row spread">
                  <span style="font-size:13px;font-weight:600">{task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)?.projectTitle ?? t("home.task.modrinthContent")}</span>
                  <span class="tag info">{t("home.tasks.queued")}</span>
                </div>
                <div class="dim" style="margin-top:2px">{task.plan.instanceName}</div>
              </div>
            {/each}
          </section>

          {#if lastSession}
            <section class="panel pad">
              <div class="panel-title">{t("home.session.title")}</div>
              <div class="col" style="gap:8px;margin-top:8px">
                <div class="row spread">
                  <span class="muted">{lastSessionInstance}</span>
                  {#if lastSession.state === "completed"}
                    <span class="tag ok">{t("home.session.exitedClean")}</span>
                  {:else if lastSession.state === "stopped"}
                    <span class="tag neutral">{t("home.state.stopped")}</span>
                  {:else if ["failed", "interrupted"].includes(lastSession.state)}
                    <span class="tag danger">{t("home.session.exitedAbnormal")}</span>
                  {:else}
                    <span class="tag accent">{t("home.state.running")}</span>
                  {/if}
                </div>
                {#if sessionDuration(lastSession)}
                  <div class="row spread"><span class="muted">{t("home.session.playTime")}</span><span>{sessionDuration(lastSession)}</span></div>
                {/if}
                <div class="row spread">
                  <span class="muted">{t("home.session.backups")}</span>
                  <span class="muted">{t("home.instance.latestBackups").replace("{pre}", backupStateLabel(lastSession.preLaunchBackup)).replace("{post}", backupStateLabel(lastSession.postExitBackup))}</span>
                </div>
              </div>
            </section>
          {/if}
        </div>
      </div>
    </main>
  {/if}

  {#if actionError}
    <div class="toast" role="alert" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{actionError}</span></div>
  {:else if notice || actionMessage}
    <div class="toast" role="status" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{actionMessage || notice}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{actionMessage || actionError || notice}</div>
</AppShell>

<style>
  .empty-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
  }
  .home-grid {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 16px;
  }
  @media (max-width: 1100px) {
    .home-grid {
      grid-template-columns: 1fr;
    }
  }
  .hero-card {
    padding: 26px 28px;
    display: flex;
    gap: 22px;
    align-items: center;
  }
  .hero-meta h1 {
    font-size: 22px;
  }
  .hero-meta .ver {
    color: var(--text-2);
    font-size: 13px;
    margin-top: 2px;
  }
  .launch-row {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 18px;
    flex-wrap: wrap;
  }
  .acct-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    padding: 6px 12px 6px 6px;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.18);
    color: var(--text-1);
    font-family: var(--font);
    text-align: left;
  }
  .acct-chip:hover {
    background: var(--glass-strong);
  }
  .acct-chip .avatar {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: linear-gradient(135deg, #3fd8c2, #2e82b4);
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    color: var(--accent-ink);
    overflow: hidden;
    flex: none;
  }
  .acct-chip .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .issue-row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 10px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .issue-row:first-of-type {
    border-top: none;
  }
  .mini-task {
    padding: 10px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .mini-task:first-of-type {
    border-top: none;
  }
</style>
