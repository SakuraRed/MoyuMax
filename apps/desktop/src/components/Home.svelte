<script lang="ts">
  import { onMount, tick } from "svelte";

  import type {
    ContentInstallTask,
    InstallTask,
    LaunchSession,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
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
    notice: string;
    onInstall: () => void;
    onOpenTasks: () => void;
    onOpenResources: () => void;
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
    notice,
    onInstall,
    onOpenTasks,
    onOpenResources,
    onStateChanged,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingInstance = $state<string | null>(null);
  let actionMessage = $state("");
  let actionError = $state("");
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
</script>

<AppShell
  pageTitle="首页"
  dataDirectory={settings.dataDirectory}
  searchVisible
  navigationTargets={["resources", "tasks"]}
  onNavigate={(target) => target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
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
            <article class:running={active?.state === "running"} class="instance-card">
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
</AppShell>
