<script lang="ts">
  import { onMount } from "svelte";

  import { markAvatarFailed, shellAccount, skinAvatarUrl } from "../accounts.svelte";
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
  import { pushToast } from "../toast.svelte";

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
  let heroPackKey = "";
  let defaultAccountName = $state("");
  let homeRoot: HTMLElement | undefined = $state();
  // 首页实例切换:本地记忆选中实例;未选或失效时回退最近运行/首个。
  let selectedHeroId = $state(
    (typeof localStorage !== "undefined" && localStorage.getItem("moyumax.home.heroId")) || "",
  );

  const hero = $derived.by(() => {
    if (instances.length === 0) return null;
    const selected = instances.find((instance) => instance.id === selectedHeroId);
    if (selected) return selected;
    const withSession = instances.find((instance) =>
      launchSessions.some((session) => session.instanceId === instance.id),
    );
    return withSession ?? instances[0] ?? null;
  });
  const heroIndex = $derived(hero ? instances.findIndex((instance) => instance.id === hero.id) : -1);

  function switchHero(delta: number): void {
    if (instances.length < 2 || !hero) return;
    const next = (heroIndex + delta + instances.length) % instances.length;
    selectedHeroId = instances[next]?.id ?? "";
    if (typeof localStorage !== "undefined") {
      localStorage.setItem("moyumax.home.heroId", selectedHeroId);
    }
  }

  // hero 统计:累计游玩时长(结束会话求和 + 运行中会话计到现在)、最后启动、启动次数。
  const heroStats = $derived.by(() => {
    if (!hero) return null;
    const now = Date.now() / 1000;
    const sessions = launchSessions.filter((session) => session.instanceId === hero.id);
    let totalSeconds = 0;
    let lastLaunch = 0;
    for (const session of sessions) {
      const end = session.endedAtUnixSeconds ?? (["starting", "running"].includes(session.state) ? now : null);
      if (end) totalSeconds += Math.max(0, end - session.startedAtUnixSeconds);
      lastLaunch = Math.max(lastLaunch, session.startedAtUnixSeconds);
    }
    return { totalSeconds, lastLaunch, count: sessions.length };
  });

  function playTimeLabel(totalSeconds: number): string {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    if (hours > 0) return t("home.session.durationHm").replace("{h}", String(hours)).replace("{m}", String(minutes));
    if (minutes > 0) return t("home.session.durationM").replace("{m}", String(minutes));
    return t("home.hero.lessThanMinute");
  }

  function relativeLabel(unixSeconds: number): string {
    const delta = Math.max(0, Date.now() / 1000 - unixSeconds);
    if (delta < 3600) return t("home.hero.agoMinutes").replace("{n}", String(Math.max(1, Math.floor(delta / 60))));
    if (delta < 86400) return t("home.hero.agoHours").replace("{n}", String(Math.floor(delta / 3600)));
    return t("home.hero.agoDays").replace("{n}", String(Math.floor(delta / 86400)));
  }

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

  $effect(() => {
    if (actionError) pushToast({ tone: "danger", title: actionError });
  });
  $effect(() => {
    if (actionMessage) pushToast({ tone: "ok", title: actionMessage });
  });
  $effect(() => {
    if (notice) pushToast({ tone: "info", title: notice });
  });

  async function loadHeroPack(instanceId: string): Promise<void> {
    if (!instanceId) {
      heroPack = null;
      heroIcon = "";
      heroInstallingPack = false;
      heroPackKey = "";
      return;
    }
    const [pack, installing] = await Promise.all([
      runtime.getInstanceModpack(instanceId).catch(() => null),
      runtime.isModpackInstalling(instanceId).catch(() => false),
    ]);
    // 图标是大体积 data URL,包身份未变时跳过重取,避免轮询期主线程反复编码。
    const key = `${instanceId}|${pack?.packName ?? ""}|${pack?.packVersion ?? ""}|${installing}`;
    if (key === heroPackKey) return;
    heroPackKey = key;
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
    onNavigate("accounts");
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
      <section class="panel hero-card">
        <div class="cube hero-cube" aria-hidden="true">
          {#if heroIcon}<img src={heroIcon} alt="" />{:else}{hero.name.slice(0, 1)}{/if}
        </div>
        <div class="hero-meta">
          <div class="row" style="gap:10px">
            <h1>{hero.name}</h1>
            {#if heroInstallingPack}
              <span class="tag info"><span class="cdot"></span>{t("modpack.stateInstalling")}</span>
            {:else if heroSession}
              <span class="tag accent"><span class="cdot"></span>{t("home.state.running")}</span>
            {:else if hero.state === "ready"}
              <span class="tag ok"><span class="cdot"></span>{t("home.instance.ready")}</span>
            {:else}
              <span class="tag neutral"><span class="cdot"></span>{hero.state}</span>
            {/if}
          </div>
          <div class="ver">
            Minecraft {hero.gameVersion} · {loaderLabel(hero)}{#if heroPack} · {heroPack.packName} {heroPack.packVersion}{/if}
          </div>
          <div class="hero-stats">
            {#if heroStats && heroStats.count > 0}
              <span>{t("home.hero.playTime").replace("{time}", playTimeLabel(heroStats.totalSeconds))}</span>
              <span class="hero-stat-sep" aria-hidden="true">·</span>
              <span>{t("home.hero.lastLaunch").replace("{time}", relativeLabel(heroStats.lastLaunch))}</span>
              <span class="hero-stat-sep" aria-hidden="true">·</span>
              <span>{t("home.hero.launchCount").replace("{count}", String(heroStats.count))}</span>
            {:else}
              <span>{t("home.hero.noSessions")}</span>
            {/if}
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
              <button class="btn ghost" onclick={() => onManageInstance(hero!)}>{t("home.launch.manage")}</button>
            {/if}
          </div>
        </div>
        {#if instances.length > 1}
          <div class="hero-switch">
            <button class="hero-switch-btn" aria-label={t("home.hero.switchPrev")} onclick={() => switchHero(-1)}>‹</button>
            <span class="dim">{t("home.hero.position").replace("{current}", String(heroIndex + 1)).replace("{total}", String(instances.length))}</span>
            <button class="hero-switch-btn" aria-label={t("home.hero.switchNext")} onclick={() => switchHero(1)}>›</button>
          </div>
        {/if}
      </section>
      {#if heroInstallingPack}
        <div class="banner info" role="status" style="margin-top:16px"><span>{t("modpack.installingHint")}</span></div>
      {/if}

      <div class="home-grid" style="margin-top:16px">
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
    position: relative;
    padding: 32px 36px;
    display: flex;
    gap: 26px;
    align-items: center;
    min-height: 208px;
    flex-wrap: wrap;
  }
  .hero-cube {
    width: 96px;
    height: 96px;
    font-size: 34px;
    flex: none;
  }
  .hero-meta {
    flex: 1 1 220px;
    min-width: 0;
  }
  .hero-meta h1 {
    font-size: 26px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hero-meta .ver {
    color: var(--text-2);
    font-size: 13px;
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hero-stats {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    color: var(--text-3);
    font-size: 12.5px;
    flex-wrap: wrap;
  }
  .hero-stat-sep {
    color: var(--text-3);
  }
  .hero-switch {
    position: absolute;
    top: 14px;
    right: 14px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hero-switch-btn {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.18);
    color: var(--text-2);
    cursor: pointer;
    font-size: 14px;
  }
  .hero-switch-btn:hover {
    background: var(--glass-strong);
    color: var(--text-1);
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
    border-radius: 4px;
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
