<script lang="ts">
  import { onMount } from "svelte";

  import AppShell from "./components/AppShell.svelte";
  import GameInstall from "./components/GameInstall.svelte";
  import HomeEmpty from "./components/HomeEmpty.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import TaskCenter from "./components/TaskCenter.svelte";
  import { createRuntime } from "./runtime";
  import type { BootstrapState, InstallTask, OnboardingSelection } from "./runtime";

  type Phase = "loading" | "onboarding" | "home" | "install" | "tasks" | "fatal";

  const runtime = createRuntime();
  let phase = $state<Phase>("loading");
  let bootstrap = $state<BootstrapState | null>(null);
  let settings = $state<OnboardingSelection | null>(null);
  let fatalMessage = $state("");
  let notice = $state("");
  let tasks = $state<InstallTask[]>([]);

  onMount(() => {
    void initialize();
    const taskPoll = setInterval(() => {
      if (phase === "home" || phase === "tasks") void refreshTasksSilently();
    }, 750);
    return () => clearInterval(taskPoll);
  });

  async function initialize(): Promise<void> {
    try {
      bootstrap = await runtime.getBootstrapState();
      settings = bootstrap.settings ?? bootstrap.defaults;
      tasks = await runtime.getInstallTasks();
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
      tasks = await runtime.getInstallTasks();
      notice = "";
    } catch (error) {
      notice = `无法刷新任务：${error instanceof Error ? error.message : String(error)}`;
    }
    phase = "home";
  }

  async function refreshTasks(): Promise<void> {
    tasks = await runtime.getInstallTasks();
  }

  async function refreshTasksSilently(): Promise<void> {
    try {
      tasks = await runtime.getInstallTasks();
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
  <HomeEmpty
    {settings}
    {tasks}
    {notice}
    onInstall={openInstaller}
    onOpenTasks={() => phase = "tasks"}
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
{:else if phase === "tasks" && settings}
  <TaskCenter
    {runtime}
    {settings}
    {tasks}
    onBack={() => void returnHome()}
    onTasksChanged={refreshTasks}
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
