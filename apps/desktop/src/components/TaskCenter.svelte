<script lang="ts">
  import { installStageLabel } from "../installation";
  import type {
    ContentInstallStage,
    ContentInstallTask,
    InstallTask,
    MoyuRuntime,
    OnboardingSelection,
    RecoveryDecision,
    TaskState,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    tasks: InstallTask[];
    contentTasks: ContentInstallTask[];
    onBack: () => void;
    onOpenResources: () => void;
    onTasksChanged: () => Promise<void>;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    tasks,
    contentTasks,
    onBack,
    onOpenResources,
    onTasksChanged,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingTask = $state("");
  let errorMessage = $state("");
  const contentStages: ContentInstallStage[] = [
    "prepare",
    "downloadFiles",
    "verifyFiles",
    "commitFiles",
    "indexContent",
  ];

  async function resolveRecovery(taskId: string, decision: RecoveryDecision): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.resolveInstallTaskRecovery(taskId, decision);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function retryTask(taskId: string): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.retryInstallTask(taskId);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function resolveContentRecovery(
    taskId: string,
    decision: RecoveryDecision,
  ): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.resolveContentTaskRecovery(taskId, decision);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function retryContentTask(taskId: string): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.retryContentTask(taskId);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  function stateLabel(state: TaskState): string {
    const labels: Record<TaskState, string> = {
      queued: "已排队",
      running: "正在运行",
      committing: "正在提交",
      paused: "已暂停",
      awaitingRecovery: "等待恢复确认",
      failed: "失败",
      completed: "已完成",
      cancelled: "已取消",
    };
    return labels[state];
  }

  function contentStageLabel(stage: ContentInstallStage): string {
    const labels: Record<ContentInstallStage, string> = {
      prepare: "准备",
      downloadFiles: "下载文件",
      verifyFiles: "校验文件",
      commitFiles: "提交文件",
      indexContent: "写入索引",
    };
    return labels[stage];
  }

  function rootContentTitle(task: ContentInstallTask): string {
    return task.plan.entries.find(
      (entry) => entry.projectId === task.plan.rootProjectId,
    )?.projectTitle ?? task.plan.rootProjectId;
  }
</script>

<AppShell
  pageTitle="任务中心"
  dataDirectory={settings.dataDirectory}
  activeNavigation="tasks"
  navigationTargets={["home", "resources"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : undefined}
  connectionStatus="本地任务队列"
  taskStatus={`${[...tasks, ...contentTasks].filter((task) => !["completed", "cancelled"].includes(task.state)).length} 个未完成任务`}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content task-center-content">
    <header class="task-center-heading">
      <button class="button ghost compact" onclick={onBack}>返回首页</button>
      <div><h1>任务中心</h1><p>所有长任务使用同一持久化队列；未知总量时不会伪造百分比。</p></div>
    </header>

    {#if errorMessage}
      <div class="error-block" role="alert"><strong>无法更新任务</strong><span>{errorMessage}</span></div>
    {/if}

    {#if tasks.length === 0 && contentTasks.length === 0}
      <section class="task-empty"><Icon name="task" size={28} /><h2>没有任务</h2><p>安装、迁移、备份和修复会统一出现在这里。</p></section>
    {:else}
      <div class="task-list">
        {#each tasks as task}
          <article class:recovery={task.state === "awaitingRecovery"} class="task-card">
            <header>
              <div><strong>{task.plan.instanceName}</strong><small>安装 Minecraft 实例</small></div>
              <span class="task-state">{stateLabel(task.state)}</span>
            </header>
            {#if task.state === "awaitingRecovery"}
              <div class="recovery-copy">
                <Icon name="info" size={16} />
                <div><strong>上次运行在安装提交前中断</strong><p>继续只会把任务放回队列，不会立即联网。放弃只清理此任务的专用暂存区，不删除共享文件、Java 或已提交实例。</p></div>
              </div>
              <div class="task-buttons">
                <button class="button primary" disabled={changingTask === task.id} onclick={() => void resolveRecovery(task.id, "resume")}>继续任务</button>
                <button class="button ghost" disabled={changingTask === task.id} onclick={() => void resolveRecovery(task.id, "discard")}>放弃并清理临时文件</button>
              </div>
            {:else}
              <ol class="task-stage-list">
                {#each task.plan.stages as stage, index}
                  <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{installStageLabel(stage)}</b></li>
                {/each}
              </ol>
              {#if task.state === "queued"}
                <p class="task-boundary">计划与暂存区已建立，正在等待统一任务调度器分配执行槽。</p>
              {:else if task.state === "running" || task.state === "committing"}
                <div class="task-progress" aria-label={`已完成 ${task.progress.completedBytes} 字节${task.progress.totalBytes === null ? "，总量未知" : `，共 ${task.progress.totalBytes} 字节`}`}>
                  <div class="progress-track">
                    <span style:width={task.progress.totalBytes && task.progress.totalBytes > 0 ? `${Math.min(100, task.progress.completedBytes / task.progress.totalBytes * 100)}%` : "24%"}></span>
                  </div>
                  <p>{task.progress.currentItem ?? "正在处理"}</p>
                </div>
              {:else if task.state === "failed"}
                <div class="error-block task-error" role="alert">
                  <strong>安装任务未完成</strong>
                  <span>尚未发布可启动实例；已校验共享文件不会被删除。</span>
                  <span>{task.progress.errorSummary ?? "请查看详情后重试。"}</span>
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void retryTask(task.id)}>重试未完成内容</button>
                </div>
              {/if}
            {/if}
            <details><summary>任务路径</summary><code>{task.stagingDirectory}</code></details>
          </article>
        {/each}
        {#each contentTasks as task}
          <article class:recovery={task.state === "awaitingRecovery"} class="task-card content-task-card">
            <header>
              <div><strong>{rootContentTitle(task)}</strong><small>安装 Modrinth 内容</small></div>
              <span class="task-state">{stateLabel(task.state)}</span>
            </header>
            {#if task.state === "awaitingRecovery"}
              <div class="recovery-copy">
                <Icon name="info" size={16} />
                <div><strong>上次运行在内容提交前中断</strong><p>继续会重新进入统一队列并复用已校验缓存。放弃只清理该任务的暂存区，不删除已安装模组或共享缓存。</p></div>
              </div>
              <div class="task-buttons">
                <button class="button primary" disabled={changingTask === task.id} onclick={() => void resolveContentRecovery(task.id, "resume")}>继续任务</button>
                <button class="button ghost" disabled={changingTask === task.id} onclick={() => void resolveContentRecovery(task.id, "discard")}>放弃并清理临时文件</button>
              </div>
            {:else}
              <ol class="task-stage-list content-stage-list">
                {#each contentStages as stage, index}
                  <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{contentStageLabel(stage)}</b></li>
                {/each}
              </ol>
              {#if task.state === "queued"}
                <p class="task-boundary">模组与依赖计划已持久化，正在等待与游戏安装任务共享的执行槽。</p>
              {:else if task.state === "running" || task.state === "committing"}
                <div class="task-progress" aria-label={`已完成 ${task.progress.completedBytes} 字节${task.progress.totalBytes === null ? "，总量未知" : `，共 ${task.progress.totalBytes} 字节`}`}>
                  <div class="progress-track">
                    <span style:width={task.progress.totalBytes && task.progress.totalBytes > 0 ? `${Math.min(100, task.progress.completedBytes / task.progress.totalBytes * 100)}%` : "0%"}></span>
                  </div>
                  <p>{task.progress.currentItem ?? "正在处理内容"}</p>
                </div>
              {:else if task.state === "failed"}
                <div class="error-block task-error" role="alert">
                  <strong>内容安装任务未完成</strong>
                  <span>本次新增文件已经补偿撤销；已有模组和世界存档保持原样。</span>
                  <span>{task.progress.errorSummary ?? "请查看详情后重试。"}</span>
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void retryContentTask(task.id)}>重试未完成内容</button>
                </div>
              {/if}
            {/if}
            <details><summary>任务路径</summary><code>{task.stagingDirectory}</code></details>
          </article>
        {/each}
      </div>
    {/if}
  </main>
</AppShell>
