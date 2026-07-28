<script lang="ts">
  import { onMount, tick } from "svelte";

  import { formatBytes, installStageLabel, taskProgressAriaLabel } from "../installation";
  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    ContentInstallStage,
    ContentInstallTask,
    InstallStage,
    InstallTask,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    RecoveryDecision,
    SourcePolicy,
    TaskSourceDetail,
    TaskState,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Fish from "./Fish.svelte";

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

  type TaskKind = "install" | "content";
  type AnyTask = InstallTask | ContentInstallTask;

  let {
    runtime,
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
  let pendingDeleteTask = $state("");
  let errorMessage = $state("");
  let statusMessage = $state("");
  let speedLimitBytes = $state(0);
  let downloadConcurrency = $state(0);
  let sourcePolicy = $state<SourcePolicy>({ kind: "mirrorFirst" });
  let expandedTask = $state("");
  let recoveryBusy = $state(false);
  let recoveryDialog = $state<HTMLElement | null>(null);

  // 全局限速档位：0 表示不限速，其余映射到现有任意字节限速能力。
  const SPEED_PRESETS = [
    { bytes: 0, label: "" },
    { bytes: 5 * 1024 * 1024, label: "5 MB/s" },
    { bytes: 10 * 1024 * 1024, label: "10 MB/s" },
  ];

  const contentStages: ContentInstallStage[] = [
    "prepare",
    "downloadFiles",
    "verifyFiles",
    "commitFiles",
    "indexContent",
  ];

  onMount(async () => {
    try {
      speedLimitBytes = await runtime.getDownloadSpeedLimit();
    } catch {
      // 限速读取失败不阻塞任务中心。
    }
    try {
      downloadConcurrency = await runtime.getDownloadConcurrency();
    } catch {
      // 连接数读取失败时详情区省略对应取值。
    }
    try {
      sourcePolicy = await runtime.getDownloadSourcePolicy();
    } catch {
      // 来源策略读取失败时按默认镜像优先展示。
    }
  });

  const allTasks = $derived([...tasks, ...contentTasks]);
  const activeTaskCount = $derived(
    allTasks.filter((task) => !["completed", "cancelled"].includes(task.state)).length,
  );
  const runningCount = $derived(
    allTasks.filter((task) => ["running", "committing"].includes(task.state)).length,
  );
  const pausedCount = $derived(allTasks.filter((task) => task.state === "paused").length);
  const failedCount = $derived(allTasks.filter((task) => task.state === "failed").length);
  const summaryText = $derived(
    [
      runningCount > 0
        ? t("tasks.summary.running").replace("{count}", String(runningCount))
        : "",
      pausedCount > 0 ? t("tasks.summary.paused").replace("{count}", String(pausedCount)) : "",
      failedCount > 0 ? t("tasks.summary.failed").replace("{count}", String(failedCount)) : "",
    ]
      .filter((part) => part.length > 0)
      .join(" · "),
  );
  const recoveryInstall = $derived(tasks.filter((task) => task.state === "awaitingRecovery"));
  const recoveryContent = $derived(
    contentTasks.filter((task) => task.state === "awaitingRecovery"),
  );
  const recoveryCount = $derived(recoveryInstall.length + recoveryContent.length);
  const customLimitLabel = $derived(
    speedLimitBytes > 0 && !SPEED_PRESETS.some((preset) => preset.bytes === speedLimitBytes)
      ? `${Math.round(speedLimitBytes / 1024 / 1024)} MB/s`
      : "",
  );

  $effect(() => {
    if (recoveryCount > 0) {
      void tick().then(() => {
        recoveryDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
      });
    }
  });

  // ---- 任务操作(沿用现有 runtime 语义) ----
  async function runTaskAction(taskId: string, action: () => Promise<void>): Promise<void> {
    changingTask = taskId;
    errorMessage = "";
    statusMessage = "";
    try {
      await action();
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      changingTask = "";
    }
  }

  async function pauseOne(taskId: string, kind: TaskKind): Promise<void> {
    await runTaskAction(taskId, () => runtime.pauseTask(taskId, kind));
  }

  async function resumeOne(taskId: string, kind: TaskKind): Promise<void> {
    await runTaskAction(taskId, () => runtime.resumeTask(taskId, kind));
  }

  async function cancelOne(taskId: string, kind: TaskKind): Promise<void> {
    await runTaskAction(taskId, () => runtime.cancelTask(taskId, kind));
  }

  async function deleteOne(taskId: string, kind: TaskKind): Promise<void> {
    await runTaskAction(taskId, async () => {
      await runtime.deleteTask(taskId, kind);
      pendingDeleteTask = "";
    });
  }

  async function movePriority(task: AnyTask, kind: TaskKind, direction: 1 | -1): Promise<void> {
    await runTaskAction(task.id, () => runtime.setTaskPriority(task.id, kind, task.priority + direction));
  }

  async function retryOne(taskId: string, kind: TaskKind): Promise<void> {
    await runTaskAction(taskId, () =>
      kind === "install" ? runtime.retryInstallTask(taskId) : runtime.retryContentTask(taskId),
    );
  }

  async function applySpeedLimit(bytes: number): Promise<void> {
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.setDownloadSpeedLimit(bytes);
      speedLimitBytes = bytes;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
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

  /** 恢复询问 modal 的两个选择对所有待恢复任务统一生效。 */
  async function resolveRecoveryAll(decision: RecoveryDecision): Promise<void> {
    recoveryBusy = true;
    errorMessage = "";
    try {
      for (const task of recoveryInstall) {
        await runtime.resolveInstallTaskRecovery(task.id, decision);
      }
      for (const task of recoveryContent) {
        await runtime.resolveContentTaskRecovery(task.id, decision);
      }
      await onTasksChanged();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      recoveryBusy = false;
    }
  }

  async function copyDiagnostics(task: AnyTask, kind: TaskKind): Promise<void> {
    errorMessage = "";
    statusMessage = "";
    const lines = [
      cardTitle(task, kind),
      `${stateLabel(task.state)} · ${failedStageLabel(task, kind)}`,
      task.progress.errorSummary ?? "",
      task.id,
    ].filter((line) => line.length > 0);
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      statusMessage = t("tasks.copied");
    } catch {
      errorMessage = t("tasks.copyFailed");
    }
  }

  // ---- 标签与文案 ----
  function isTerminalState(state: TaskState): boolean {
    return state === "failed" || state === "completed" || state === "cancelled";
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

  function stateTone(state: TaskState): string {
    const tones: Record<TaskState, string> = {
      queued: "info",
      running: "accent",
      committing: "accent",
      paused: "warn",
      awaitingRecovery: "warn",
      failed: "danger",
      completed: "ok",
      cancelled: "neutral",
    };
    return tones[state];
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
    return (
      task.plan.entries.find((entry) => entry.projectId === task.plan.rootProjectId)
        ?.projectTitle ?? task.plan.rootProjectId
    );
  }

  function cardTitle(task: AnyTask, kind: TaskKind): string {
    if (kind === "install") {
      return t("tasks.card.install").replace("{name}", task.plan.instanceName);
    }
    const content = task as ContentInstallTask;
    if (content.plan.isUpdate) {
      return t("tasks.card.contentUpdate")
        .replace("{name}", content.plan.instanceName)
        .replace("{count}", String(content.plan.entries.length));
    }
    return t("tasks.card.contentInstall").replace("{name}", rootContentTitle(content));
  }

  function taskPercent(task: AnyTask): number | null {
    const total = task.progress.totalBytes;
    if (!total || total <= 0) return null;
    return Math.min(100, Math.round((task.progress.completedBytes / total) * 100));
  }

  interface StepItem {
    label: string;
    status: "done" | "current" | "todo";
  }

  function stepStatus(state: TaskState, index: number, currentIndex: number): StepItem["status"] {
    if (state === "completed" || index < currentIndex) return "done";
    if (index === currentIndex && state !== "cancelled" && state !== "failed") return "current";
    return "todo";
  }

  function stepItems(task: AnyTask, kind: TaskKind): StepItem[] {
    const current = task.currentStage;
    if (kind === "install") {
      const stages = (task as InstallTask).plan.stages;
      const currentIndex = current ? stages.indexOf(current as InstallStage) : -1;
      return stages.map((stage, index) => ({
        label: installStageLabel(stage),
        status: stepStatus(task.state, index, currentIndex),
      }));
    }
    const currentIndex = current ? contentStages.indexOf(current as ContentInstallStage) : -1;
    return contentStages.map((stage, index) => ({
      label: contentStageLabel(stage),
      status: stepStatus(task.state, index, currentIndex),
    }));
  }

  /** 进度/暂停/排队等状态下的用户语言说明行。 */
  function metaLine(task: AnyTask, kind: TaskKind): string {
    const pct = taskPercent(task);
    switch (task.state) {
      case "queued":
        return t(kind === "install" ? "tasks.queued.installBoundary" : "tasks.queued.contentBoundary");
      case "running":
      case "committing": {
        const item =
          task.progress.currentItem ??
          t(kind === "install" ? "tasks.progress.processing" : "tasks.progress.processingContent");
        if (pct !== null) return `${pct}% · ${item}`;
        return `${item} · ${t("tasks.progress.indetNote")}`;
      }
      case "paused":
        return pct !== null
          ? t("tasks.paused.at").replace("{pct}", String(pct))
          : t("tasks.paused.unknown");
      case "completed":
        return t("tasks.progress.downloaded").replace(
          "{size}",
          formatBytes(task.progress.completedBytes),
        );
      case "cancelled":
        return t("tasks.cancelled.hint");
      case "awaitingRecovery":
        return t("tasks.recovery.cardHint");
      default:
        return "";
    }
  }

  function failedStageLabel(task: AnyTask, kind: TaskKind): string {
    const current = task.currentStage;
    if (current) {
      return kind === "install"
        ? installStageLabel(current as InstallStage)
        : contentStageLabel(current as ContentInstallStage);
    }
    return t(kind === "install" ? "tasks.install.kindLabel" : "tasks.content.kindLabel");
  }

  function failedConclusion(task: AnyTask, kind: TaskKind): string {
    return t("tasks.failed.conclusionBold").replace("{stage}", failedStageLabel(task, kind));
  }

  function failedImpact(task: AnyTask, kind: TaskKind): string {
    return t(kind === "install" ? "tasks.failed.installImpact" : "tasks.failed.contentImpact").replace(
      "{name}",
      task.plan.instanceName,
    );
  }

  /** 卡片右侧来源行：只展示任务真实的最终来源。 */
  function sourceRightLabel(task: AnyTask): string | null {
    const label = task.progress.sourceDetail?.finalLabel;
    return label ? t("tasks.source.prefix").replace("{label}", label) : null;
  }

  function sourceDetailLine(task: AnyTask): string | null {
    const detail = task.progress.sourceDetail;
    if (!detail) return null;
    const parts = [t("tasks.source.prefix").replace("{label}", detail.finalLabel)];
    const failed = detail.attempts.find((attempt) => typeof attempt.outcome !== "string");
    if (failed) parts.push(t("tasks.source.fallback").replace("{label}", failed.label));
    if (detail.segmented) {
      parts.push(t("tasks.source.segmented").replace("{count}", String(detail.segmentCount)));
    }
    if (detail.effectiveConnections && detail.effectiveConnections > 0) {
      parts.push(
        t("tasks.source.connections").replace("{count}", String(detail.effectiveConnections)),
      );
    }
    if (detail.degradedReason) {
      parts.push(t("tasks.source.degraded").replace("{reason}", detail.degradedReason));
    }
    return parts.join(" · ");
  }

  // ---- 详情展开(kv2) ----
  function policyShortLabel(): string {
    if (sourcePolicy.kind === "officialFirst") return t("tasks.detail.policy.officialFirst");
    if (sourcePolicy.kind === "custom") return t("tasks.detail.policy.custom");
    return t("tasks.detail.policy.mirrorFirst");
  }

  function failedAttempts(detail: TaskSourceDetail | null | undefined): { label: string }[] {
    return detail?.attempts.filter((attempt) => typeof attempt.outcome !== "string") ?? [];
  }

  function effectiveConnections(detail: TaskSourceDetail | null | undefined): number {
    return detail?.effectiveConnections ?? downloadConcurrency;
  }

  function switchNote(detail: TaskSourceDetail): string {
    const joiner = uiLanguage() === "en" ? ", " : "、";
    const failed = failedAttempts(detail)
      .map((attempt) => attempt.label)
      .join(joiner);
    return t("tasks.detail.switchNote")
      .replace("{failed}", failed)
      .replace("{final}", detail.finalLabel);
  }

  function toggleDetail(taskId: string): void {
    expandedTask = expandedTask === taskId ? "" : taskId;
  }

  function speedPresetLabel(preset: (typeof SPEED_PRESETS)[number]): string {
    return preset.bytes === 0 ? t("tasks.limit.unlimited") : preset.label;
  }

  /** 恢复 modal 中单个任务的进度与来源说明。 */
  function recoveryLine(task: AnyTask): string {
    const parts: string[] = [];
    const pct = taskPercent(task);
    const total = task.progress.totalBytes;
    if (pct !== null && total && total > 0) {
      parts.push(
        t("tasks.recovery.modalProgress")
          .replace("{pct}", String(pct))
          .replace("{completed}", formatBytes(task.progress.completedBytes))
          .replace("{total}", formatBytes(total)),
      );
    } else {
      parts.push(
        t("tasks.recovery.modalProgressUnknown").replace(
          "{completed}",
          formatBytes(task.progress.completedBytes),
        ),
      );
    }
    const source = task.progress.sourceDetail?.finalLabel;
    if (source) parts.push(t("tasks.recovery.modalSource").replace("{label}", source));
    parts.push(t("tasks.recovery.modalResumable"));
    return parts.join(" · ");
  }
</script>

<AppShell
  pageTitle={t("tasks.title")}
  activeNavigation="tasks"
  taskCount={activeTaskCount}
  connectionStatus={t("tasks.connectionStatus")}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content task-page">
    <h1 class="sr-live">{t("tasks.title")}</h1>

    <div class="row spread task-toolbar">
      <div class="row toolbar-controls">
        <button class="btn small secondary" disabled={pauseChanging} onclick={() => void togglePaused()}>
          {tasksPaused ? t("tasks.global.resumeAll") : t("tasks.global.pauseAll")}
        </button>
        <div class="seg" role="group" aria-label={t("tasks.limit.groupAria")}>
          {#each SPEED_PRESETS as preset}
            <button
              class:on={speedLimitBytes === preset.bytes}
              aria-pressed={speedLimitBytes === preset.bytes}
              onclick={() => void applySpeedLimit(preset.bytes)}
            >{speedPresetLabel(preset)}</button>
          {/each}
          {#if customLimitLabel}
            <button class="on" aria-pressed="true" disabled>{customLimitLabel}</button>
          {/if}
        </div>
      </div>
      {#if summaryText}<span class="dim">{summaryText}</span>{/if}
    </div>

    {#if tasksPaused}
      <div class="banner warn pause-banner" role="status"><span>{t("tasks.global.paused")}</span></div>
    {/if}

    {#if tasks.length === 0 && contentTasks.length === 0}
      <div class="empty-stage">
        <Fish variant="tank" />
        <h2 class="empty-title">{t("tasks.empty.title")}</h2>
        <p class="muted empty-desc">{t("tasks.empty.description")}</p>
      </div>
    {:else}
      {#each tasks as task (task.id)}
        {@render taskCard(task, "install")}
      {/each}
      {#each contentTasks as task (task.id)}
        {@render taskCard(task, "content")}
      {/each}
    {/if}
  </main>

  {#if errorMessage}
    <div class="toast" role="alert" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{errorMessage}</span></div>
  {:else if statusMessage}
    <div class="toast" role="status" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{statusMessage}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{statusMessage || errorMessage}</div>

  {#if recoveryCount > 0}
    <div class="modal-mask">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="recovery-modal-title"
        tabindex="-1"
        bind:this={recoveryDialog}
      >
        <h3 id="recovery-modal-title">{t("tasks.recovery.modalTitle")}</h3>
        <div class="m-body">
          <p>{t("tasks.recovery.modalIntro")}</p>
          <div class="panel recovery-list">
            {#each recoveryInstall as task (task.id)}
              <div class="recovery-item">
                <div class="recovery-name">{cardTitle(task, "install")}</div>
                <div class="dim">{t("tasks.recovery.installTitle")}</div>
                <div class="dim">{recoveryLine(task)}</div>
              </div>
            {/each}
            {#each recoveryContent as task (task.id)}
              <div class="recovery-item">
                <div class="recovery-name">{cardTitle(task, "content")}</div>
                <div class="dim">{t("tasks.recovery.contentTitle")}</div>
                <div class="dim">{recoveryLine(task)}</div>
              </div>
            {/each}
          </div>
          <p class="recovery-question">{t("tasks.recovery.modalQuestion")}</p>
          <p class="dim recovery-note">{t("tasks.recovery.modalDiscardNote")}</p>
        </div>
        <div class="m-acts">
          <button class="btn danger-soft" disabled={recoveryBusy} onclick={() => void resolveRecoveryAll("discard")}>
            {t("tasks.recovery.discard")}
          </button>
          <button class="btn primary" data-dialog-autofocus disabled={recoveryBusy} onclick={() => void resolveRecoveryAll("resume")}>
            {t("tasks.recovery.resume")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

{#snippet taskCard(task: AnyTask, kind: TaskKind)}
  {@const pct = taskPercent(task)}
  {@const sourceLine = sourceDetailLine(task)}
  {@const steps = stepItems(task, kind)}
  {@const detail = task.progress.sourceDetail}
  <section class="panel task-card">
    <div class="row spread">
      <span class="task-name">{cardTitle(task, kind)}</span>
      <span class="tag {stateTone(task.state)} task-state">{stateLabel(task.state)}</span>
    </div>

    {#if task.state === "failed"}
      <div class="err-block" role="alert">
        <div class="err-line"><b>{failedConclusion(task, kind)}</b>{t("tasks.failed.conclusionTail")}</div>
        <div class="err-line">{failedImpact(task, kind)}</div>
        <div class="row err-actions">
          <button class="btn small primary" disabled={changingTask === task.id} onclick={() => void retryOne(task.id, kind)}>
            {t("tasks.failed.retry")}
          </button>
          <button class="btn small ghost" onclick={() => void copyDiagnostics(task, kind)}>
            {t("tasks.failed.copyDiagnostics")}
          </button>
          {#if pendingDeleteTask === task.id}
            <button class="btn small danger" disabled={changingTask === task.id} onclick={() => void deleteOne(task.id, kind)}>
              {t("common.confirmDelete")}
            </button>
            <button class="btn small ghost" disabled={changingTask === task.id} onclick={() => { pendingDeleteTask = ""; }}>
              {t("common.cancel")}
            </button>
          {:else}
            <button class="btn small ghost" disabled={changingTask === task.id} onclick={() => { pendingDeleteTask = task.id; }}>
              {t("common.delete")}
            </button>
          {/if}
        </div>
        {#if task.progress.errorSummary}
          <details class="adv">
            <summary>{t("tasks.failed.techCode")}</summary>
            <div class="adv-body"><div class="mono">{task.progress.errorSummary}</div></div>
          </details>
        {/if}
      </div>
    {:else}
      {#if steps.length > 0}
        <div class="steps">
          {#each steps as step, index}
            {#if index > 0}<span>›</span>{/if}
            {#if step.status === "current"}
              <b>{step.label}</b>
            {:else if step.status === "done"}
              <span class="done">{step.label}</span>
            {:else}
              <span>{step.label}</span>
            {/if}
          {/each}
        </div>
      {/if}

      {#if ["running", "committing"].includes(task.state)}
        {#if pct !== null}
          <div class="progress" aria-label={taskProgressAriaLabel(task.progress)}><i style="width:{pct}%"></i></div>
        {:else}
          <div class="progress indet" aria-label={taskProgressAriaLabel(task.progress)}><i></i></div>
        {/if}
      {:else if ["paused", "completed"].includes(task.state) && pct !== null}
        <div class="progress" aria-label={taskProgressAriaLabel(task.progress)}><i style="width:{pct}%"></i></div>
      {/if}

      <div class="row spread">
        <span class="dim">{metaLine(task, kind)}</span>
        {#if sourceRightLabel(task)}<span class="dim">{sourceRightLabel(task)}</span>{/if}
      </div>
    {/if}

    {#if sourceLine}
      <div class="task-source">{sourceLine}</div>
    {/if}

    {#if expandedTask === task.id}
      <div class="task-detail">
        <div class="kv2">
          <span class="k">{t("tasks.detail.source")}</span>
          <span>{detail?.finalLabel ?? policyShortLabel()}</span>
        </div>
        {#if effectiveConnections(detail) > 0}
          <div class="kv2">
            <span class="k">{t("tasks.detail.connections")}</span>
            <span>
              {t("tasks.detail.connectionsValue").replace("{count}", String(effectiveConnections(detail)))}{#if detail?.segmented}{t("tasks.detail.segmentedSuffix")}{/if}
            </span>
          </div>
        {/if}
        {#if detail && detail.segmented && detail.segmentCount > 0}
          <div class="kv2">
            <span class="k">{t("tasks.detail.segments")}</span>
            <span>{t("tasks.detail.segmentsValue").replace("{count}", String(detail.segmentCount))}</span>
          </div>
        {/if}
        {#if detail && failedAttempts(detail).length > 0}
          <div class="kv2">
            <span class="k">{t("tasks.detail.switch")}</span>
            <span>{switchNote(detail)}</span>
          </div>
        {/if}
        {#if detail?.degradedReason}
          <div class="kv2">
            <span class="k">{t("tasks.detail.degraded")}</span>
            <span>{detail.degradedReason}</span>
          </div>
        {/if}
        <details class="adv detail-about">
          <summary>{t("tasks.detail.aboutTitle")}</summary>
          <div class="adv-body"><span class="muted detail-about-copy">{t("tasks.detail.aboutBody")}</span></div>
        </details>
      </div>
    {/if}

    {#if task.state !== "awaitingRecovery"}
      <div class="row card-actions">
        {#if task.state === "running" || task.state === "queued"}
          <button class="btn small secondary" disabled={changingTask === task.id} onclick={() => void pauseOne(task.id, kind)}>
            {t("tasks.action.pause")}
          </button>
        {/if}
        {#if task.state === "paused"}
          <button class="btn small primary" disabled={changingTask === task.id} onclick={() => void resumeOne(task.id, kind)}>
            {t("tasks.action.resume")}
          </button>
        {/if}
        {#if task.state === "queued"}
          <button class="btn small ghost" aria-label={t("tasks.action.moveUpAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, kind, 1)}>
            {t("tasks.action.moveUp")}
          </button>
          <button class="btn small ghost" aria-label={t("tasks.action.moveDownAria")} disabled={changingTask === task.id} onclick={() => void movePriority(task, kind, -1)}>
            {t("tasks.action.moveDown")}
          </button>
          <span class="dim">{t("tasks.priority").replace("{value}", String(task.priority))}</span>
        {/if}
        {#if ["queued", "running", "paused"].includes(task.state)}
          <button class="btn small ghost" disabled={changingTask === task.id} onclick={() => void cancelOne(task.id, kind)}>
            {t("tasks.action.cancel")}
          </button>
        {/if}
        {#if isTerminalState(task.state) && task.state !== "failed"}
          {#if pendingDeleteTask === task.id}
            <button class="btn small danger" disabled={changingTask === task.id} onclick={() => void deleteOne(task.id, kind)}>
              {t("common.confirmDelete")}
            </button>
            <button class="btn small ghost" disabled={changingTask === task.id} onclick={() => { pendingDeleteTask = ""; }}>
              {t("common.cancel")}
            </button>
          {:else}
            <button class="btn small danger-soft" disabled={changingTask === task.id} onclick={() => { pendingDeleteTask = task.id; }}>
              {t("common.delete")}
            </button>
          {/if}
        {/if}
        <button class="btn small ghost" aria-expanded={expandedTask === task.id} onclick={() => toggleDetail(task.id)}>
          {t("tasks.action.details")}
        </button>
      </div>
    {/if}
  </section>
{/snippet}

<style>
  .task-page {
    display: flex;
    flex-direction: column;
  }
  .task-toolbar {
    margin-bottom: 16px;
    flex-wrap: wrap;
    row-gap: 10px;
  }
  .toolbar-controls {
    flex-wrap: wrap;
  }
  .pause-banner {
    margin-bottom: 16px;
  }
  .seg button:disabled {
    opacity: 1;
    cursor: default;
  }

  .task-card {
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .task-card + .task-card {
    margin-top: 16px;
  }
  .task-name {
    font-size: 14px;
    font-weight: 600;
  }
  .task-state {
    padding: 5px 12px;
  }

  .steps {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-start;
    gap: 4px 6px;
    font-size: 11.5px;
    color: var(--text-3);
  }
  .steps b {
    color: var(--accent);
    font-weight: 600;
  }
  .steps .done {
    color: var(--text-2);
  }

  .err-block {
    border: 1px solid rgba(232, 104, 95, 0.3);
    background: var(--danger-soft);
    border-radius: var(--r);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .err-block .err-line {
    font-size: 12.5px;
    color: #f0c3bf;
  }
  .err-block .err-line b {
    color: var(--text-1);
  }
  .err-actions {
    flex-wrap: wrap;
  }

  .task-source {
    margin: 0;
    background: var(--info-soft);
    border-radius: var(--r);
  }

  .task-detail {
    margin-top: 2px;
  }
  .kv2 {
    display: flex;
    gap: 12px;
    padding: 7px 0;
    font-size: 13px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .kv2:first-of-type {
    border-top: none;
  }
  .kv2 .k {
    width: 110px;
    flex: none;
    color: var(--text-3);
    font-size: 12.5px;
  }
  .detail-about {
    margin-top: 6px;
  }
  .detail-about-copy {
    font-size: 12.5px;
  }

  .card-actions {
    justify-content: flex-end;
    flex-wrap: wrap;
  }

  .empty-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }
  .empty-title {
    font-size: 16px;
    font-weight: 600;
  }
  .empty-desc {
    font-size: 12.5px;
  }

  .recovery-list {
    margin-top: 10px;
    padding: 12px 16px;
    box-shadow: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .recovery-name {
    font-weight: 600;
    font-size: 13.5px;
  }
  .recovery-question {
    margin-top: 12px;
  }
  .recovery-note {
    margin-top: 8px;
  }
</style>
