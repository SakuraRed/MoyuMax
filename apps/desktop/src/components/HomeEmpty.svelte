<script lang="ts">
  import type { InstallTask, OnboardingSelection } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    settings: OnboardingSelection;
    tasks: InstallTask[];
    notice: string;
    onInstall: () => void;
    onOpenTasks: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    settings,
    tasks,
    notice,
    onInstall,
    onOpenTasks,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  const activeTasks = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)),
  );
</script>

<AppShell
  pageTitle="首页"
  dataDirectory={settings.dataDirectory}
  searchVisible
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content home-empty">
    <div class="empty-graphic" aria-hidden="true"></div>
    <h1>从安装第一个游戏开始</h1>
    <p>推荐稳定版会自动配好 Java、加载器和隔离环境。你不需要打开文件资源管理器、命令行或 Java 官网。</p>
    <button class="button primary large" onclick={onInstall}>安装第一个游戏</button>
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
  </main>

  {#if notice}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{notice}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{notice}</div>
</AppShell>
