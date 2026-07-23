<script lang="ts">
  import { onMount, tick } from "svelte";

  import type {
    ContentInstallTask,
    CrashReport,
    InstallTask,
    LaunchSession,
    ManagedInstance,
    MoyuRuntime,
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
    onOpenResources: () => void;
    onOpenData: () => void;
    onOpenCrash: (report: CrashReport) => void;
    onStateChanged: () => Promise<void>;
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
    onOpenResources,
    onOpenData,
    onOpenCrash,
    onStateChanged,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingInstance = $state<string | null>(null);
  let actionMessage = $state("");
  let actionError = $state("");
  let recycleCandidate = $state<ManagedInstance | null>(null);
  let recycleDialog = $state<HTMLElement | null>(null);
  let homeRoot: HTMLElement | undefined = $state();

  onMount(async () => {
    await tick();
    homeRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
  });

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

  function loaderLabel(instance: ManagedInstance): string {
    if (instance.loaderKind === "fabric") {
      return `Fabric${instance.loaderVersion ? ` ${instance.loaderVersion}` : ""}`;
    }
    return instance.loaderKind === "vanilla" ? "原版" : instance.loaderKind;
  }

  function sessionStateLabel(state: LaunchSession["state"]): string {
    switch (state) {
      case "starting":
        return "正在启动";
      case "running":
        return "正在运行";
      case "completed":
        return "正常退出";
      case "failed":
        return "异常退出";
      case "stopped":
        return "已停止";
      case "interrupted":
        return "启动器中断";
    }
  }

  function backupStateLabel(backup: WorldBackupSummary | null | undefined): string {
    if (!backup) return "未记录";
    switch (backup.state) {
      case "ready":
        return "已备份";
      case "skipped":
        return "无世界";
      case "failed":
        return "备份失败";
      case "staging":
        return "正在备份";
    }
  }

  async function start(instance: ManagedInstance): Promise<void> {
    changingInstance = instance.id;
    actionMessage = "";
    actionError = "";
    try {
      await runtime.startInstance(instance.id);
      actionMessage = `正在以本地离线身份启动「${instance.name}」`;
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
      actionMessage = `已请求停止「${instance.name}」`;
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
      actionMessage = `已将「${instance.name}」移入回收站，可在数据页恢复`;
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
  pageTitle="首页"
  dataDirectory={settings.dataDirectory}
  searchVisible
  navigationTargets={["resources", "tasks", "data"]}
  onNavigate={(target) => target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : target === "data" ? onOpenData() : undefined}
  taskStatus={activeLaunches.length > 0 ? `${activeLaunches.length} 个游戏正在运行` : activeTasks.length + activeContentTasks.length > 0 ? `${activeTasks.length + activeContentTasks.length} 个未完成任务` : "无活动任务"}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  {#if instances.length === 0}
    <main class="content home-empty" bind:this={homeRoot}>
      <div class="empty-graphic" aria-hidden="true"></div>
      <h1>从安装第一个游戏开始</h1>
      <p>推荐稳定版会自动配好 Java、加载器和隔离环境。你不需要打开文件资源管理器、命令行或 Java 官网。</p>
      <button class="button primary large" data-autofocus="true" onclick={onInstall}>安装第一个游戏</button>
      <small>
        也可以 <button class="inline-link" disabled>导入整合包</button> 或
        <button class="inline-link" disabled>从其他启动器迁移</button>（第二个公开版本提供）
      </small>
      {#if activeTasks.length > 0}
        {#each activeTasks.slice(0, 1) as task}
          <button class="home-task-summary" onclick={onOpenTasks}>
            <span><strong>{task.plan.instanceName}</strong><small>{task.state === "awaitingRecovery" ? "等待恢复确认" : "安装任务已排队"}</small></span>
            <span>{activeTasks.length} 个任务 <Icon name="arrow-right" size={14} /></span>
          </button>
        {/each}
      {/if}
      {#if activeTasks.length === 0 && activeContentTasks.length > 0}
        {#each activeContentTasks.slice(0, 1) as task}
          <button class="home-task-summary" onclick={onOpenTasks}>
            <span><strong>{task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)?.projectTitle ?? "Modrinth 内容"}</strong><small>{task.state === "awaitingRecovery" ? "等待恢复确认" : "内容安装任务已排队"}</small></span>
            <span>{activeContentTasks.length} 个任务 <Icon name="arrow-right" size={14} /></span>
          </button>
        {/each}
      {/if}
    </main>
  {:else}
    <main class="content home-content" bind:this={homeRoot}>
      <div class="home-scroll">
        <header class="home-heading">
          <div>
            <h1>继续游戏</h1>
            <p>本地实例无需联网即可启动。首版使用明确标注的本地离线身份。</p>
          </div>
          <button class="button" onclick={onInstall}>安装其他版本</button>
        </header>

        <section class="instance-list" aria-label="本地游戏实例">
          {#each instances as instance, index}
            {@const active = activeSession(instance.id)}
            {@const latest = latestSession(instance.id)}
            {@const crashReport = crashReportForSession(latest)}
            <article class:running={active?.state === "running"} class:crashed={Boolean(crashReport)} class="instance-card">
              <div class="instance-cover" aria-hidden="true">{instance.name.slice(0, 1)}</div>
              <div class="instance-copy">
                <div class="instance-title-line">
                  <h2>{instance.name}</h2>
                  <span class:active={Boolean(active)} class="instance-state">
                    {active ? sessionStateLabel(active.state) : instance.state === "ready" ? "可启动" : instance.state}
                  </span>
                </div>
                <p>Minecraft {instance.gameVersion} · {loaderLabel(instance)} · 完全隔离</p>
                <small>本地离线身份：MoyuMaxPlayer</small>
                {#if latest && !active}
                  <small class="latest-session">最近会话：<span>{sessionStateLabel(latest.state)}</span>{#if latest.exitCode !== null} · 退出码 {latest.exitCode}{/if}</small>
                  <small class="latest-backups">世界备份：启动前 {backupStateLabel(latest.preLaunchBackup)} · 退出后 {backupStateLabel(latest.postExitBackup)}</small>
                {/if}
              </div>
              <div class="instance-actions">
                {#if active}
                  <button
                    class="button"
                    disabled={changingInstance === instance.id}
                    onclick={() => void stop(instance)}
                  >{changingInstance === instance.id ? "正在停止" : "停止游戏"}</button>
                {:else}
                  <button
                    class="button primary large"
                    data-autofocus={index === 0 ? "true" : undefined}
                    disabled={changingInstance === instance.id || instance.state !== "ready"}
                    onclick={() => void start(instance)}
                  ><Icon name="play" size={14} />{changingInstance === instance.id ? "正在启动" : "启动游戏"}</button>
                {/if}
                {#if crashReport && !active}
                  <button class="button crash-report-button" onclick={() => onOpenCrash(crashReport)}>查看崩溃报告</button>
                {/if}
                {#if !active}
                  <button
                    class="button danger-subtle"
                    aria-label={`将“${instance.name}”移入回收站`}
                    disabled={changingInstance === instance.id}
                    onclick={() => void askRecycle(instance)}
                  >移入回收站</button>
                {/if}
                <span>启动前将使用托管 Java</span>
              </div>
            </article>
          {/each}
        </section>

        {#if activeTasks.length > 0}
          {#each activeTasks.slice(0, 1) as task}
            <button class="home-task-summary home-task-wide" onclick={onOpenTasks}>
              <span><strong>{task.plan.instanceName}</strong><small>{task.state === "awaitingRecovery" ? "等待恢复确认" : "安装任务正在处理"}</small></span>
              <span>{activeTasks.length} 个任务 <Icon name="arrow-right" size={14} /></span>
            </button>
          {/each}
        {/if}
        {#if activeTasks.length === 0 && activeContentTasks.length > 0}
          {#each activeContentTasks.slice(0, 1) as task}
            <button class="home-task-summary home-task-wide" onclick={onOpenTasks}>
              <span><strong>{task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)?.projectTitle ?? "Modrinth 内容"}</strong><small>{task.state === "awaitingRecovery" ? "等待恢复确认" : "内容安装任务正在处理"}</small></span>
              <span>{activeContentTasks.length} 个任务 <Icon name="arrow-right" size={14} /></span>
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
          <h2 id="recycle-confirm-title">将“{recycleCandidate.name}”移入回收站？</h2>
          <p>实例会从首页隐藏，但文件、存档和配置会保留 30 天。</p>
        </header>
        <div class="confirmation-impact">
          <strong>托管 Java 不会被删除</strong>
          <span>共享游戏基础文件也会继续保留；你可以随时从数据页恢复到原位置。</span>
        </div>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={changingInstance === recycleCandidate.id} onclick={cancelRecycle}>取消</button>
          <button class="button danger" disabled={changingInstance === recycleCandidate.id} onclick={() => void recycleInstance()}>
            {changingInstance === recycleCandidate.id ? "正在移动" : "移入回收站"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
