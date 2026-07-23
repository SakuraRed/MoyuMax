<script lang="ts">
  import { onMount } from "svelte";

  import { formatBytes } from "../installation";
  import type {
    JavaDeleteOutcome,
    JavaEnvironment,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
    ReferencingInstance,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onBack: () => void;
    onOpenHome: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    onBack,
    onOpenHome,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let environments = $state<JavaEnvironment[]>([]);
  let deletedEnvironments = $state<JavaEnvironment[]>([]);
  let instances = $state<ManagedInstance[]>([]);
  let errorMessage = $state("");
  let notice = $state("");
  let busy = $state("");
  let deleteTarget = $state<JavaEnvironment | null>(null);
  let deleteAffected = $state<ReferencingInstance[]>([]);
  let assignTarget = $state("");
  let assignInstance = $state("");

  onMount(() => {
    void refresh();
  });

  async function refresh(): Promise<void> {
    errorMessage = "";
    try {
      [environments, deletedEnvironments, instances] = await Promise.all([
        runtime.listJavaEnvironments(),
        runtime.listDeletedJavaEnvironments(),
        runtime.listInstances(),
      ]);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function distributionName(environment: JavaEnvironment): string {
    return environment.distribution === "azulZulu" ? "Azul Zulu" : environment.distribution;
  }

  function statusLabel(environment: JavaEnvironment): string {
    if (!environment.healthy && environment.status === "ready") return "文件缺失";
    const labels: Record<string, string> = {
      planned: "已计划",
      installing: "安装中",
      ready: "已就绪",
      missing: "缺失",
      failed: "失败",
      deleted: "已删除",
    };
    return labels[environment.status] ?? environment.status;
  }

  async function verify(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const healthy = await runtime.verifyJavaEnvironment(environment.id);
      notice = healthy
        ? `${distributionName(environment)} ${environment.fullVersion} 验证通过。`
        : `${distributionName(environment)} ${environment.fullVersion} 缺少环境文件，可在原任务中恢复或重新安装。`;
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function openLocation(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      await runtime.openJavaLocation(environment.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function requestDelete(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const outcome: JavaDeleteOutcome = await runtime.deleteJavaEnvironment(
        environment.id,
        false,
      );
      if (outcome.kind === "requiresConfirmation") {
        deleteTarget = environment;
        deleteAffected = outcome.instances;
      } else {
        notice = `${distributionName(environment)} ${environment.fullVersion} 已删除，实例与其他环境未受影响。`;
        await refresh();
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    busy = deleteTarget.id;
    errorMessage = "";
    try {
      await runtime.deleteJavaEnvironment(deleteTarget.id, true);
      notice = `${distributionName(deleteTarget)} ${deleteTarget.fullVersion} 已删除；受影响的 ${deleteAffected.length} 个实例在恢复或更换环境前无法启动。`;
      deleteTarget = null;
      deleteAffected = [];
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function restore(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const restored = await runtime.restoreJavaEnvironment(environment.id);
      notice = `${distributionName(restored)} ${restored.fullVersion} 已恢复，引用实例已指向该环境。`;
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function applyAssignment(environment: JavaEnvironment): Promise<void> {
    if (assignTarget !== environment.id) return;
    if (!assignInstance) return;
    busy = environment.id;
    errorMessage = "";
    try {
      await runtime.setInstanceJavaEnvironment(assignInstance, environment.id);
      const instance = instances.find((entry) => entry.id === assignInstance);
      notice = `已为「${instance?.name ?? assignInstance}」指派 ${distributionName(environment)} ${environment.fullVersion}。`;
      assignTarget = "";
      assignInstance = "";
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }
</script>

<AppShell
  pageTitle="设置 · Java 环境"
  dataDirectory={settings.dataDirectory}
  activeNavigation="settings"
  navigationTargets={["home"]}
  onNavigate={(target) => target === "home" ? onOpenHome() : undefined}
  connectionStatus="本地环境管理"
  taskStatus={`${environments.length} 个可用环境`}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content java-content" data-scroll-region="main">
    <header class="task-center-heading">
      <button class="button ghost compact" onclick={onBack}>返回首页</button>
      <div>
        <h1>Java 环境</h1>
        <p>托管 Azul Zulu 环境按“发行版、完整补丁版本、架构”全局去重；删除实例不会自动删除环境。</p>
      </div>
    </header>

    {#if errorMessage}
      <div class="error-block" role="alert"><strong>操作未完成</strong><span>{errorMessage}</span></div>
    {/if}
    {#if notice}
      <div class="java-notice" role="status">{notice}</div>
    {/if}

    {#if environments.length === 0 && deletedEnvironments.length === 0}
      <section class="task-empty"><Icon name="box" size={28} /><h2>还没有托管环境</h2><p>安装第一个游戏时，MoyuMax 会自动选择并安装兼容的 Azul Zulu 环境。</p></section>
    {:else}
      <div class="task-list">
        {#each environments as environment}
          <article class="task-card java-card">
            <header>
              <div>
                <strong>{distributionName(environment)} {environment.fullVersion}</strong>
                <small>{environment.architecture} · {formatBytes(environment.sizeBytes)}</small>
              </div>
              <span class="task-state" class:java-missing={!environment.healthy}>{statusLabel(environment)}</span>
            </header>
            <p class="java-home"><code>{environment.homeDirectory}</code></p>
            {#if environment.referencingInstances.length > 0}
              <p class="java-refs">
                引用实例:{environment.referencingInstances.map((entry) => entry.name).join("、")}
              </p>
            {:else}
              <p class="java-refs muted">没有实例引用该环境。</p>
            {/if}
            <div class="task-buttons">
              <button class="button ghost compact" disabled={busy === environment.id} onclick={() => void verify(environment)}>验证</button>
              <button class="button ghost compact" disabled={busy === environment.id} onclick={() => void openLocation(environment)}>打开位置</button>
              <button
                class="button ghost compact"
                disabled={busy === environment.id || instances.length === 0}
                onclick={() => {
                  assignTarget = assignTarget === environment.id ? "" : environment.id;
                  assignInstance = instances[0]?.id ?? "";
                }}
              >设为实例环境</button>
              <button class="button danger-subtle compact" disabled={busy === environment.id} onclick={() => void requestDelete(environment)}>删除</button>
            </div>
            {#if assignTarget === environment.id}
              <div class="java-assign" role="group" aria-label="选择目标实例">
                <label>
                  目标实例
                  <select bind:value={assignInstance}>
                    {#each instances as instance}
                      <option value={instance.id}>{instance.name}（{instance.gameVersion} {instance.loaderKind}）</option>
                    {/each}
                  </select>
                </label>
                <button class="button primary compact" disabled={busy === environment.id || !assignInstance} onclick={() => void applyAssignment(environment)}>确认指派</button>
                <small>只会接受主版本一致的环境；数据库与磁盘运行时清单同步更新。</small>
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}

    {#if deletedEnvironments.length > 0}
      <section class="java-deleted" aria-label="已删除的环境">
        <h2>已删除的环境</h2>
        <p>已删除环境的引用记录被保留；恢复由你主动触发，获取同一发行版与主版本线的最新可用补丁。</p>
        <div class="task-list">
          {#each deletedEnvironments as environment}
            <article class="task-card java-card deleted">
              <header>
                <div>
                  <strong>{distributionName(environment)} {environment.fullVersion}</strong>
                  <small>{environment.architecture}</small>
                </div>
                <span class="task-state">已删除</span>
              </header>
              {#if environment.referencingInstances.length > 0}
                <p class="java-refs">仍被引用:{environment.referencingInstances.map((entry) => entry.name).join("、")}</p>
              {/if}
              <div class="task-buttons">
                <button class="button primary compact" disabled={busy === environment.id} onclick={() => void restore(environment)}>一键恢复</button>
              </div>
            </article>
          {/each}
        </div>
      </section>
    {/if}
  </main>

  {#if deleteTarget}
    <div class="modal-backdrop" role="presentation">
      <div class="confirmation-dialog" role="dialog" aria-modal="true" aria-labelledby="delete-java-title">
        <header>
          <h2 id="delete-java-title">删除 Java 环境</h2>
          <p>此 Java 环境仍被实例使用。</p>
        </header>
        <div class="confirmation-impact danger-impact" role="note">
          <strong>删除后，以下实例将无法直接启动，直到恢复或选择其他环境:</strong>
          {#each deleteAffected as instance}
            <span>「{instance.name}」</span>
          {/each}
          <span>删除实例本身不会自动删除 Java；删除环境也不会删除实例。</span>
        </div>
        <div class="confirmation-actions">
          <button class="button ghost" onclick={() => { deleteTarget = null; deleteAffected = []; }}>取消</button>
          <button class="button danger-subtle" disabled={busy === deleteTarget.id} onclick={() => void confirmDelete()}>
            删除 {distributionName(deleteTarget)} {deleteTarget.fullVersion}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
