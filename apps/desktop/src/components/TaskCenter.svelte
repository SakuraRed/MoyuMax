<script lang="ts">
  import { onMount } from "svelte";

  import { installStageLabel, taskProgressAriaLabel } from "../installation";
  import { t } from "../i18n.svelte";
  import type {
    ContentInstallStage,
    ContentInstallTask,
    InstallTask,
    MoyuRuntime,
    NavigationKey,
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
    tasksPaused: boolean;
    onTasksChanged: () => Promise<void>;
    onToggleTasksPaused: () => Promise<void>;
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
    tasksPaused,
    onTasksChanged,
    onToggleTasksPaused,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let changingTask = $state("");
  let pauseChanging = $state(false);
  let errorMessage = $state("");
  let speedLimitMib = $state("");
  let speedLimitBytes = $state(0);

  onMount(async () => {
    try {
      const limit = await runtime.getDownloadSpeedLimit();
      speedLimitBytes = limit;
      speedLimitMib = limit > 0 ? String(Math.round(limit / 1024 / 1024)) : "";
    } catch {
      // 限速读取失败不阻塞任务中心。
    }
  });

  async function pauseOne(taskId: string, kind: "install" | "content"): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.pauseTask(taskId, kind);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function resumeOne(taskId: string, kind: "install" | "content"): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    try {
      await runtime.resumeTask(taskId, kind);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function movePriority(
    task: InstallTask | ContentInstallTask,
    kind: "install" | "content",
    direction: 1 | -1,
  ): Promise<void> {
    changingTask = task.id;
    errorMessage = "";
    try {
      await runtime.setTaskPriority(task.id, kind, task.priority + direction);
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function applySpeedLimit(): Promise<void> {
    errorMessage = "";
    const value = speedLimitMib.trim() === "" ? 0 : Number(speedLimitMib);
    if (!Number.isFinite(value) || value < 0) {
      errorMessage = t("tasks.limit.invalid");
      return;
    }
    try {
      const bytesPerSec = Math.round(value * 1024 * 1024);
      await runtime.setDownloadSpeedLimit(bytesPerSec);
      speedLimitBytes = bytesPerSec;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }
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

  async function togglePaused(): Promise<void> {
    pauseChanging = true;
    errorMessage = "";
    try {
      await onToggleTasksPaused();
    } finally {
      pauseChanging = false;
    }
  }

  function stateLabel(state: TaskState): string {
    const keys: Record<TaskState, string> = {
      queued: "tasks.state.queued",
      running: "tasks.state.running",
      committing: "tasks.state.committing",
      paused: "tasks.state.paused",
      awaitingRecovery: "tasks.state.awaitingRecovery",
      failed: "tasks.state.failed",
      completed: "tasks.state.completed",
      cancelled: "tasks.state.cancelled",
    };
    return t(keys[state]);
  }

  function contentStageLabel(stage: ContentInstallStage): string {
    const keys: Record<ContentInstallStage, string> = {
      prepare: "tasks.contentStage.prepare",
      downloadFiles: "tasks.contentStage.downloadFiles",
      verifyFiles: "tasks.contentStage.verifyFiles",
      commitFiles: "tasks.contentStage.commitFiles",
      indexContent: "tasks.contentStage.indexContent",
    };
    return t(keys[stage]);
  }

  function rootContentTitle(task: ContentInstallTask): string {
    return task.plan.entries.find(
      (entry) => entry.projectId === task.plan.rootProjectId,
    )?.projectTitle ?? task.plan.rootProjectId;
  }

  function sourceDetailLine(task: InstallTask | ContentInstallTask): string | null {
    const detail = task.progress.sourceDetail;
    if (!detail) return null;
    const parts = [t("tasks.source.prefix").replace("{label}", detail.finalLabel)];
    const failed = detail.attempts.find(
      (attempt) => typeof attempt.outcome !== "string",
    );
    if (failed) parts.push(t("tasks.source.fallback").replace("{label}", failed.label));
    if (detail.segmented) parts.push(t("tasks.source.segmented").replace("{count}", String(detail.segmentCount)));
    if (detail.effectiveConnections && detail.effectiveConnections > 0) {
      parts.push(t("tasks.source.connections").replace("{count}", String(detail.effectiveConnections)));
    }
    if (detail.degradedReason) parts.push(t("tasks.source.degraded").replace("{reason}", detail.degradedReason));
    return parts.join(" · ");
  }
</script>

<AppShell
  pageTitle={t("tasks.title")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="tasks"
  {onNavigate}
  connectionStatus={t("tasks.connectionStatus")}
  taskStatus={t("home.taskStatus.pending").replace("{count}", String([...tasks, ...contentTasks].filter((task) => !["completed", "cancelled"].includes(task.state)).length))}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content task-center-content">
    <header class="task-center-heading">
      <div><h1>{t("tasks.title")}</h1></div>
    </header>

    {#if errorMessage}
      <div class="error-block" role="alert"><strong>{t("tasks.errorTitle")}</strong><span>{errorMessage}</span></div>
    {/if}

    <div class="task-global-bar" class:paused={tasksPaused}>
      <span>
        {#if tasksPaused}
          {t("tasks.global.paused")}
        {:else}
          {t("tasks.global.running")}
        {/if}
      </span>
      <button class="button ghost compact" disabled={pauseChanging} onclick={() => void togglePaused()}>
        {tasksPaused ? t("tasks.global.resumeAll") : t("tasks.global.pauseAll")}
      </button>
    </div>

    <div class="task-limit-bar" role="group" aria-label={t("tasks.limit.groupAria")}>
      <span>
        {#if speedLimitBytes > 0}
          {t("tasks.limit.current").replace("{speed}", String(Math.round(speedLimitBytes / 1024 / 1024)))}
        {:else}
          {t("tasks.limit.unlimited")}
        {/if}
      </span>
      <label class="task-limit-field">
        {t("tasks.limit.fieldLabel")}
        <input
          value={speedLimitMib}
          inputmode="decimal"
          placeholder={t("tasks.limit.placeholder")}
          oninput={(event) => (speedLimitMib = event.currentTarget.value)}
        />
      </label>
      <button class="button ghost compact" onclick={() => void applySpeedLimit()}>{t("tasks.limit.apply")}</button>
    </div>

    {#if tasks.length === 0 && contentTasks.length === 0}
      <section class="task-empty"><Icon name="task" size={28} /><h2>{t("tasks.empty.title")}</h2><p>{t("tasks.empty.description")}</p></section>
    {:else}
      <div class="task-list">
        {#each tasks as task}
          <article class:recovery={task.state === "awaitingRecovery"} class="task-card">
            <header>
              <div><strong>{task.plan.instanceName}</strong><small>{t("tasks.install.kindLabel")}</small></div>
              <span class="task-state">{stateLabel(task.state)}</span>
            </header>
            {#if task.state === "awaitingRecovery"}
              <div class="recovery-copy">
                <Icon name="info" size={16} />
                <div><strong>{t("tasks.recovery.installTitle")}</strong><p>{t("tasks.recovery.installBody")}</p></div>
              </div>
              <div class="task-buttons">
                <button class="button primary" disabled={changingTask === task.id} onclick={() => void resolveRecovery(task.id, "resume")}>{t("tasks.recovery.resume")}</button>
                <button class="button ghost" disabled={changingTask === task.id} onclick={() => void resolveRecovery(task.id, "discard")}>{t("tasks.recovery.discard")}</button>
              </div>
            {:else}
              <ol class="task-stage-list">
                {#each task.plan.stages as stage, index}
                  <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{installStageLabel(stage)}</b></li>
                {/each}
              </ol>
              {#if task.state === "queued"}
                <p class="task-boundary">{t("tasks.queued.installBoundary")}</p>
              {:else if task.state === "running" || task.state === "committing"}
                <div class="task-progress" aria-label={taskProgressAriaLabel(task.progress)}>
                  <div class="progress-track">
                    <span style:width={task.progress.totalBytes && task.progress.totalBytes > 0 ? `${Math.min(100, task.progress.completedBytes / task.progress.totalBytes * 100)}%` : "24%"}></span>
                  </div>
                  <p>{task.progress.currentItem ?? t("tasks.progress.processing")}</p>
                </div>
              {:else if task.state === "failed"}
                <div class="error-block task-error" role="alert">
                  <strong>{t("tasks.failed.installTitle")}</strong>
                  <span>{t("tasks.failed.installBody")}</span>
                  <span>{task.progress.errorSummary ?? t("tasks.failed.retryHint")}</span>
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void retryTask(task.id)}>{t("tasks.failed.retry")}</button>
                </div>
              {/if}
              <div class="task-actions">
                {#if task.state === "running" || task.state === "queued"}
                  <button class="button ghost compact" disabled={changingTask === task.id} onclick={() => void pauseOne(task.id, "install")}>{t("tasks.action.pause")}</button>
                {/if}
                {#if task.state === "paused"}
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void resumeOne(task.id, "install")}>{t("tasks.action.resume")}</button>
                {/if}
                {#if task.state === "queued"}
                  <button class="button ghost compact" aria-label={t("tasks.action.moveUpAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, "install", 1)}>{t("tasks.action.moveUp")}</button>
                  <button class="button ghost compact" aria-label={t("tasks.action.moveDownAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, "install", -1)}>{t("tasks.action.moveDown")}</button>
                  <span class="task-priority">{t("tasks.priority").replace("{value}", String(task.priority))}</span>
                {/if}
              </div>
            {/if}
            {#if sourceDetailLine(task)}
              <p class="task-source">{sourceDetailLine(task)}</p>
            {/if}
            <details><summary>{t("tasks.paths")}</summary><code>{task.stagingDirectory}</code></details>
          </article>
        {/each}
        {#each contentTasks as task}
          <article class:recovery={task.state === "awaitingRecovery"} class="task-card content-task-card">
            <header>
              <div><strong>{rootContentTitle(task)}</strong><small>{t("tasks.content.kindLabel")}</small></div>
              <span class="task-state">{stateLabel(task.state)}</span>
            </header>
            {#if task.state === "awaitingRecovery"}
              <div class="recovery-copy">
                <Icon name="info" size={16} />
                <div><strong>{t("tasks.recovery.contentTitle")}</strong><p>{t("tasks.recovery.contentBody")}</p></div>
              </div>
              <div class="task-buttons">
                <button class="button primary" disabled={changingTask === task.id} onclick={() => void resolveContentRecovery(task.id, "resume")}>{t("tasks.recovery.resume")}</button>
                <button class="button ghost" disabled={changingTask === task.id} onclick={() => void resolveContentRecovery(task.id, "discard")}>{t("tasks.recovery.discard")}</button>
              </div>
            {:else}
              <ol class="task-stage-list content-stage-list">
                {#each contentStages as stage, index}
                  <li class:current={task.currentStage === stage}><span>{index + 1}</span><b>{contentStageLabel(stage)}</b></li>
                {/each}
              </ol>
              {#if task.state === "queued"}
                <p class="task-boundary">{t("tasks.queued.contentBoundary")}</p>
              {:else if task.state === "running" || task.state === "committing"}
                <div class="task-progress" aria-label={taskProgressAriaLabel(task.progress)}>
                  <div class="progress-track">
                    <span style:width={task.progress.totalBytes && task.progress.totalBytes > 0 ? `${Math.min(100, task.progress.completedBytes / task.progress.totalBytes * 100)}%` : "0%"}></span>
                  </div>
                  <p>{task.progress.currentItem ?? t("tasks.progress.processingContent")}</p>
                </div>
              {:else if task.state === "failed"}
                <div class="error-block task-error" role="alert">
                  <strong>{t("tasks.failed.contentTitle")}</strong>
                  <span>{t("tasks.failed.contentBody")}</span>
                  <span>{task.progress.errorSummary ?? t("tasks.failed.retryHint")}</span>
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void retryContentTask(task.id)}>{t("tasks.failed.retry")}</button>
                </div>
              {/if}
              <div class="task-actions">
                {#if task.state === "running" || task.state === "queued"}
                  <button class="button ghost compact" disabled={changingTask === task.id} onclick={() => void pauseOne(task.id, "content")}>{t("tasks.action.pause")}</button>
                {/if}
                {#if task.state === "paused"}
                  <button class="button primary compact" disabled={changingTask === task.id} onclick={() => void resumeOne(task.id, "content")}>{t("tasks.action.resume")}</button>
                {/if}
                {#if task.state === "queued"}
                  <button class="button ghost compact" aria-label={t("tasks.action.moveUpAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, "content", 1)}>{t("tasks.action.moveUp")}</button>
                  <button class="button ghost compact" aria-label={t("tasks.action.moveDownAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, "content", -1)}>{t("tasks.action.moveDown")}</button>
                  <span class="task-priority">{t("tasks.priority").replace("{value}", String(task.priority))}</span>
                {/if}
              </div>
            {/if}
            {#if sourceDetailLine(task)}
              <p class="task-source">{sourceDetailLine(task)}</p>
            {/if}
            <details><summary>{t("tasks.paths")}</summary><code>{task.stagingDirectory}</code></details>
          </article>
        {/each}
      </div>
    {/if}
  </main>
</AppShell>
