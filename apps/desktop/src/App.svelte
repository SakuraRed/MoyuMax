<script lang="ts">
  import { onMount, tick } from "svelte";

  import AppShell from "./components/AppShell.svelte";
  import CloseDialog from "./components/CloseDialog.svelte";
  import CrashCenter from "./components/CrashCenter.svelte";
  import DataCenter from "./components/DataCenter.svelte";
  import GameInstall from "./components/GameInstall.svelte";
  import Home from "./components/Home.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import ResourceCenter from "./components/ResourceCenter.svelte";
  import SettingsCenter from "./components/SettingsCenter.svelte";
  import TaskCenter from "./components/TaskCenter.svelte";
  import { impactRequiresConfirmation, routeCloseRequest } from "./close-flow";
  import { applyUiPreferences } from "./i18n.svelte";
  import { createRuntime } from "./runtime";
  import type {
    BootstrapState,
    ContentInstallTask,
    CrashReport,
    ExitImpact,
    InstallTask,
    LaunchSession,
    ManagedInstance,
    OnboardingSelection,
    PendingIntent,
    WindowCloseAction,
  } from "./runtime";
  import { isRestorablePage, sanitizeShellState } from "./shell-state";

  type Phase = "loading" | "onboarding" | "home" | "install" | "resources" | "tasks" | "data" | "crash" | "settings" | "fatal";

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
  let tasksPaused = $state(false);
  let closeDialog = $state<{
    open: boolean;
    mode: "choice" | "impact";
    impact: ExitImpact | null;
    busy: boolean;
    errorMessage: string;
    pendingRemember: boolean;
  }>({ open: false, mode: "choice", impact: null, busy: false, errorMessage: "", pendingRemember: false });
  let shellReady = false;
  let persistTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void initialize();
    const unregisterClose = runtime.onCloseRequested(() => void handleCloseRequested());
    const unregisterIntent = runtime.onPendingIntent(() => void consumePendingIntent());
    const scrollListener = (event: Event) => {
      const target = event.target;
      if (target instanceof HTMLElement && target.matches("[data-scroll-region='main']")) {
        scheduleShellPersist(target.scrollTop);
      }
    };
    document.addEventListener("scroll", scrollListener, true);
    const statePoll = setInterval(() => {
      if (document.hidden) return;
      if (phase === "home") void refreshHomeStateSilently();
      else if (phase === "tasks") void refreshTasksSilently();
    }, 1_000);
    return () => {
      unregisterClose();
      unregisterIntent();
      document.removeEventListener("scroll", scrollListener, true);
      clearInterval(statePoll);
      clearTimeout(persistTimer);
    };
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
        startupKind,
        shellState,
        pendingIntent,
        paused,
        uiPreferences,
      ] =
        await Promise.all([
          runtime.getBootstrapState(),
          runtime.getInstallTasks(),
          runtime.getContentInstallTasks(),
          runtime.listInstances(),
          runtime.listLaunchSessions(),
          runtime.listCrashReports(),
          runtime.getWindowStartupKind(),
          runtime.getShellState(),
          runtime.takePendingIntent(),
          runtime.getTasksPaused(),
          runtime.getUiPreferences(),
        ]);
      bootstrap = bootstrapState;
      settings = bootstrap.settings ?? bootstrap.defaults;
      tasks = initialTasks;
      contentTasks = initialContentTasks;
      instances = initialInstances;
      launchSessions = initialSessions;
      crashReports = initialCrashReports;
      tasksPaused = paused;
      applyUiPreferences({
        theme: uiPreferences.theme === "light" || uiPreferences.theme === "dark" ? uiPreferences.theme : "system",
        language: uiPreferences.language === "zh-TW" || uiPreferences.language === "en" ? uiPreferences.language : "zh-CN",
        motion: uiPreferences.motion === "reduce" ? "reduce" : "system",
        contrast: uiPreferences.contrast === "high" ? "high" : "standard",
      });
      if (bootstrap.requiresOnboarding) {
        phase = "onboarding";
      } else if (startupKind === "wake") {
        // 托盘唤醒:恢复上次非敏感页面与滚动位置;敏感或未知页面回退首页。
        const restored = sanitizeShellState(shellState);
        phase = restored?.page ?? "home";
        if (restored && restored.scrollTop > 0) {
          await tick();
          const region = document.querySelector<HTMLElement>("[data-scroll-region='main']");
          if (region) region.scrollTop = restored.scrollTop;
        }
      } else {
        phase = "home";
      }
      shellReady = true;
      if (pendingIntent) await handlePendingIntent(pendingIntent);
    } catch (error) {
      fatalMessage = error instanceof Error ? error.message : String(error);
      phase = "fatal";
    }
  }

  async function consumePendingIntent(): Promise<void> {
    const intent = await runtime.takePendingIntent();
    if (intent) await handlePendingIntent(intent);
  }

  async function handlePendingIntent(intent: PendingIntent): Promise<void> {
    if (intent.kind === "exitRequested") {
      await openCloseDialog("impact");
      return;
    }
    phase = "home";
    notice = "";
    try {
      await runtime.startInstance(intent.instanceId);
      await refreshHomeState();
    } catch (error) {
      notice = `无法启动实例：${error instanceof Error ? error.message : String(error)}`;
    }
  }

  async function handleCloseRequested(): Promise<void> {
    if (closeDialog.open) return;
    try {
      const [behavior, impact] = await Promise.all([
        runtime.getWindowCloseBehavior(),
        runtime.getExitImpact(),
      ]);
      switch (routeCloseRequest(behavior, impact)) {
        case "minimize":
          await runtime.resolveWindowClose({ action: "minimize", remember: false });
          return;
        case "exit":
          await runtime.confirmExit();
          return;
        case "impact-dialog":
          await openCloseDialog("impact", impact);
          return;
        default:
          await openCloseDialog("choice", impact);
      }
    } catch {
      // 读取关闭行为失败时回退到询问,不做任何不可逆操作。
      await openCloseDialog("choice");
    }
  }

  async function openCloseDialog(mode: "choice" | "impact", knownImpact?: ExitImpact): Promise<void> {
    const impact = knownImpact ?? (await runtime.getExitImpact());
    closeDialog = { open: true, mode, impact, busy: false, errorMessage: "", pendingRemember: false };
  }

  async function confirmCloseChoice(choice: WindowCloseAction, remember: boolean): Promise<void> {
    if (choice === "minimize") {
      closeDialog = { ...closeDialog, open: false };
      await runtime.resolveWindowClose({ action: "minimize", remember });
      return;
    }
    const impact = closeDialog.impact ?? (await runtime.getExitImpact());
    if (impactRequiresConfirmation(impact)) {
      closeDialog = { ...closeDialog, mode: "impact", impact, errorMessage: "", pendingRemember: remember };
      return;
    }
    closeDialog = { ...closeDialog, open: false };
    await runtime.resolveWindowClose({ action: "exit", remember });
  }

  async function confirmExitNow(): Promise<void> {
    closeDialog = { ...closeDialog, busy: true, errorMessage: "" };
    try {
      // 影响确认后仍需持久化用户在选择阶段勾选的“记住退出”。
      if (closeDialog.pendingRemember) {
        await runtime.setWindowCloseBehavior("exit");
      }
      await runtime.confirmExit();
      // 真实桌面端此时进程已退出；浏览器测试环境需要显式关闭对话框。
      closeDialog = { ...closeDialog, open: false, busy: false, errorMessage: "" };
    } catch (error) {
      closeDialog = {
        ...closeDialog,
        busy: false,
        errorMessage: error instanceof Error ? error.message : String(error),
      };
    }
  }

  function cancelCloseDialog(): void {
    if (closeDialog.busy) return;
    closeDialog = { ...closeDialog, open: false, errorMessage: "" };
  }

  function scheduleShellPersist(scrollTop: number): void {
    clearTimeout(persistTimer);
    persistTimer = setTimeout(() => void persistShellState(scrollTop), 400);
  }

  async function persistShellState(scrollTop?: number): Promise<void> {
    if (!shellReady || !isRestorablePage(phase)) return;
    const currentScroll =
      scrollTop ??
      document.querySelector<HTMLElement>("[data-scroll-region='main']")?.scrollTop ??
      0;
    try {
      await runtime.persistShellState({ page: phase, scrollTop: Math.round(currentScroll) });
    } catch {
      // 持久化失败不影响界面使用,下次唤醒按首页处理。
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

  function openData(): void {
    notice = "";
    phase = "data";
  }

  function openSettings(): void {
    notice = "";
    phase = "settings";
  }

  async function toggleTasksPaused(): Promise<void> {
    try {
      if (tasksPaused) {
        await runtime.resumeAllTasks();
      } else {
        await runtime.pauseAllTasks();
      }
      tasksPaused = await runtime.getTasksPaused();
      await refreshTasks();
    } catch (error) {
      notice = `无法更新任务暂停状态：${error instanceof Error ? error.message : String(error)}`;
    }
  }

  async function refreshTasks(): Promise<void> {
    [tasks, contentTasks, tasksPaused] = await Promise.all([
      runtime.getInstallTasks(),
      runtime.getContentInstallTasks(),
      runtime.getTasksPaused(),
    ]);
  }

  async function refreshHomeState(): Promise<void> {
    const [nextTasks, nextContentTasks, nextInstances, nextSessions, nextCrashReports, paused] = await Promise.all([
      runtime.getInstallTasks(),
      runtime.getContentInstallTasks(),
      runtime.listInstances(),
      runtime.listLaunchSessions(),
      runtime.listCrashReports(),
      runtime.getTasksPaused(),
    ]);
    tasks = nextTasks;
    contentTasks = nextContentTasks;
    instances = nextInstances;
    launchSessions = nextSessions;
    crashReports = nextCrashReports;
    tasksPaused = paused;
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
      [tasks, contentTasks, tasksPaused] = await Promise.all([
        runtime.getInstallTasks(),
        runtime.getContentInstallTasks(),
        runtime.getTasksPaused(),
      ]);
    } catch {
      // 可交互页面保持可用，显式进入任务中心时再显示读取错误。
    }
  }

  $effect(() => {
    if (shellReady && isRestorablePage(phase)) {
      void persistShellState(0);
    }
  });
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
    onOpenData={openData}
    onOpenSettings={openSettings}
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
    {tasksPaused}
    onBack={() => void returnHome()}
    onOpenResources={() => phase = "resources"}
    onTasksChanged={refreshTasks}
    onToggleTasksPaused={toggleTasksPaused}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "data" && settings}
  <DataCenter
    {runtime}
    {settings}
    onBack={() => void returnHome()}
    onOpenResources={() => phase = "resources"}
    onOpenTasks={() => phase = "tasks"}
    onInstancesChanged={refreshHomeState}
    onMinimize={() => runtime.minimizeWindow()}
    onToggleMaximize={() => runtime.toggleMaximizeWindow()}
    onClose={() => runtime.closeWindow()}
  />
{:else if phase === "settings" && settings}
  <SettingsCenter
    {runtime}
    {settings}
    onBack={() => void returnHome()}
    onOpenHome={() => void returnHome()}
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

{#if closeDialog.open && closeDialog.impact}
  <!-- display:contents 包装让对话框继承 .window 上的设计令牌,自身不产生布局盒 -->
  <div class="window" style="display: contents">
    <CloseDialog
      mode={closeDialog.mode}
      impact={closeDialog.impact}
      busy={closeDialog.busy}
      errorMessage={closeDialog.errorMessage}
      onCancel={cancelCloseDialog}
      onConfirm={(choice, remember) => void confirmCloseChoice(choice, remember)}
      onConfirmExit={() => void confirmExitNow()}
      onForceExit={() => void runtime.forceExit()}
    />
  </div>
{/if}
