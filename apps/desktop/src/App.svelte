<script lang="ts">
  import { onMount } from "svelte";

  import AppShell from "./components/AppShell.svelte";
  import CrashCenter from "./components/CrashCenter.svelte";
  import GameInstall from "./components/GameInstall.svelte";
  import Home from "./components/Home.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import ResourceCenter from "./components/ResourceCenter.svelte";
  import TaskCenter from "./components/TaskCenter.svelte";
  import { createRuntime } from "./runtime";
  import type {
    BootstrapState,
    ContentInstallTask,
    CrashReport,
    InstallTask,
    LaunchSession,
    ManagedInstance,
    OnboardingSelection,
  } from "./runtime";

  type Phase = "loading" | "onboarding" | "home" | "install" | "resources" | "tasks" | "crash" | "fatal";

  const runtime = createRuntime();
  let phase = $state<Phase>("loading");
  let bootstrap = $state<BootstrapState | null>(null);
  let settings = $state<OnboardingSelection | null>(null);
  let fatalMessage = $state("");
  let notice = $state("");
  let tasks = $state<InstallTask[]>([]);
  let contentTasks = $state<ContentInstallTask[]>([]);
  let instances = $state<ManagedInstance[]>([]);
  let launchSessions = $state<LaunchSession[]>([]);
  let crashReports = $state<CrashReport[]>([]);
  let selectedCrashReport = $state<CrashReport | null>(null);
  let homeRefreshRunning = false;

  onMount(() => {
    void initialize();
    const statePoll = setInterval(() => {
      if (phase === "home") void refreshHomeStateSilently();
      else if (phase === "tasks") void refreshTasksSilently();
    }, 1_000);
    return () => clearInterval(statePoll);
  });

  async function initialize(): Promise<void> {
    try {
      const [
        bootstrapState,
        initialTasks,
        initialContentTasks,
        initialInstances,
        initialSessions,
        initialCrashReports,
      ] =
        await Promise.all([
          runtime.getBootstrapState(),
          runtime.getInstallTasks(),
          runtime.getContentInstallTasks(),
          runtime.listInstances(),
          runtime.listLaunchSessions(),
          runtime.listCrashReports(),
        ]);
      bootstrap = bootstrapState;
      settings = bootstrap.settings ?? bootstrap.defaults;
      tasks = initialTasks;
      contentTasks = initialContentTasks;
      instances = initialInstances;
      launchSessions = initialSessions;
      crashReports = initialCrashReports;
      phase = bootstrap.requiresOnboarding ? "onboarding" : "home";
    } catch (error) {
      fatalMessage = error instanceof Error ? error.message : String(error);
      phase = "fatal";
    }
  }

  async function persistOnboarding(selection: OnboardingSelection): Promise<void> {
    await runtime.completeOnboarding(selection);
    settings = selection;
  }

  async function skipOnboarding(): Promise<void> {
    await runtime.skipOnboarding();
    if (bootstrap) settings = { ...bootstrap.defaults };
  }

  function startUsing(selection: OnboardingSelection): void {
    settings = selection;
    phase = "home";
  }

  function openInstaller(): void {
    notice = "";
    phase = "install";
  }

  async function returnHome(): Promise<void> {
    try {
      await refreshHomeState();
      notice = "";
    } catch (error) {
      notice = `无法刷新任务：${error instanceof Error ? error.message : String(error)}`;
    }
    selectedCrashReport = null;
    phase = "home";
  }

  function openCrashReport(report: CrashReport): void {
    selectedCrashReport = report;
    notice = "";
    phase = "crash";
  }

  async function refreshTasks(): Promise<void> {
    [tasks, contentTasks] = await Promise.all([
      runtime.getInstallTasks(),
      runtime.getContentInstallTasks(),
    ]);
  }

  async function refreshHomeState(): Promise<void> {
    const [nextTasks, nextContentTasks, nextInstances, nextSessions, nextCrashReports] = await Promise.all([
      runtime.getInstallTasks(),
      runtime.getContentInstallTasks(),
      runtime.listInstances(),
      runtime.listLaunchSessions(),
      runtime.listCrashReports(),
    ]);
    tasks = nextTasks;
    contentTasks = nextContentTasks;
    instances = nextInstances;
    launchSessions = nextSessions;
    crashReports = nextCrashReports;
  }

  async function refreshHomeStateSilently(): Promise<void> {
    if (homeRefreshRunning) return;
    homeRefreshRunning = true;
    try {
      await refreshHomeState();
    } catch {
      // 本地实例列表保持最后一次成功快照，显式操作失败时由首页显示原因。
    } finally {
      homeRefreshRunning = false;
    }
  }

  async function refreshTasksSilently(): Promise<void> {
    try {
      [tasks, contentTasks] = await Promise.all([
        runtime.getInstallTasks(),
        runtime.getContentInstallTasks(),
      ]);
    } catch {
      // 可交互页面保持可用，显式进入任务中心时再显示读取错误。
    }
  }
</script>

{#if phase === "loading"}
  <AppShell
    pageTitle="正在启动"
    dataDirectory="本地状态"
    navigationDisabled
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  >
    <main class="content loading-state" aria-live="polite">
      <div class="loading-line wide"></div>
      <div class="loading-line"></div>
      <span>正在读取本地状态…</span>
    </main>
  </AppShell>
{:else if phase === "onboarding" && bootstrap}
  <Onboarding
    {bootstrap}
    onPersist={persistOnboarding}
    onSkip={skipOnboarding}
    onStart={startUsing}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "home" && settings}
  <Home
    {runtime}
    {settings}
    {tasks}
    {contentTasks}
    {instances}
    {launchSessions}
    {crashReports}
    {notice}
    onInstall={openInstaller}
    onOpenTasks={() => phase = "tasks"}
    onOpenResources={() => phase = "resources"}
    onOpenCrash={openCrashReport}
    onStateChanged={refreshHomeState}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "install" && settings}
  <GameInstall
    {runtime}
    {settings}
    onBack={() => void returnHome()}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "resources" && settings}
  <ResourceCenter
    {runtime}
    {settings}
    {instances}
    onBack={() => void returnHome()}
    onOpenTasks={() => phase = "tasks"}
    onTasksChanged={refreshTasks}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "tasks" && settings}
  <TaskCenter
    {runtime}
    {settings}
    {tasks}
    {contentTasks}
    onBack={() => void returnHome()}
    onOpenResources={() => phase = "resources"}
    onTasksChanged={refreshTasks}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "crash" && settings && selectedCrashReport}
  <CrashCenter
    {runtime}
    {settings}
    report={selectedCrashReport}
    instance={instances.find((instance) => instance.id === selectedCrashReport?.instanceId) ?? null}
    onBack={() => void returnHome()}
    onOpenResources={() => phase = "resources"}
    onOpenTasks={() => phase = "tasks"}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else}
  <AppShell
    pageTitle="无法启动"
    dataDirectory="本地状态不可用"
    navigationDisabled
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  >
    <main class="content fatal-state" role="alert">
      <div class="error-block">
        <strong>MoyuMax 无法读取本地状态</strong>
        <span>游戏实例和数据没有被修改。</span>
        <span>请确认当前用户能够访问应用数据目录，然后重新启动。</span>
        <details><summary>技术详情</summary><code>{fatalMessage}</code></details>
      </div>
    </main>
  </AppShell>
{/if}
