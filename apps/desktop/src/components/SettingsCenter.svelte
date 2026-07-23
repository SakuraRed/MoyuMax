<script lang="ts">
  import { onMount } from "svelte";

  import { formatBytes } from "../installation";
  import type {
    AccountSummary,
    JavaDeleteOutcome,
    JavaEnvironment,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
    ReferencingInstance,
  } from "../runtime";
  import { LITTLESKIN_YGGDRASIL_URL } from "../runtime";
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
  let backupInterval = $state(30);
  let backupKeep = $state(20);
  let backupSettingsLoaded = $state(false);
  let accounts = $state<AccountSummary[]>([]);
  let addForm = $state<"" | "offline" | "authlib">("");
  let offlineName = $state("");
  let authlibServerChoice = $state("littleskin");
  let authlibUrl = $state("");
  let authlibUser = $state("");
  let authlibPass = $state("");
  let pendingAccountRemove = $state<string | null>(null);

  onMount(() => {
    void refresh();
  });

  async function refresh(): Promise<void> {
    errorMessage = "";
    try {
      [environments, deletedEnvironments, instances, accounts] = await Promise.all([
        runtime.listJavaEnvironments(),
        runtime.listDeletedJavaEnvironments(),
        runtime.listInstances(),
        runtime.listAccounts(),
      ]);
      if (!backupSettingsLoaded) {
        const backupSettings = await runtime.getWorldBackupSettings();
        backupInterval = backupSettings.intervalMinutes;
        backupKeep = backupSettings.keepCount;
        backupSettingsLoaded = true;
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function accountKindLabel(account: AccountSummary): string {
    return account.kind === "offline" ? "离线" : "外置";
  }

  async function submitOffline(): Promise<void> {
    busy = "add-account";
    errorMessage = "";
    notice = "";
    try {
      await runtime.addOfflineAccount(offlineName.trim());
      offlineName = "";
      addForm = "";
      accounts = await runtime.listAccounts();
      notice = "离线账户已创建";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function submitAuthlib(): Promise<void> {
    busy = "add-account";
    errorMessage = "";
    notice = "";
    const serverUrl = authlibServerChoice === "littleskin" ? LITTLESKIN_YGGDRASIL_URL : authlibUrl.trim();
    try {
      await runtime.addAuthlibAccount(serverUrl, authlibUser.trim(), authlibPass);
      authlibUser = "";
      authlibPass = "";
      authlibUrl = "";
      addForm = "";
      accounts = await runtime.listAccounts();
      notice = "外置账户已登录，令牌仅保存在本地";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function makeDefault(account: AccountSummary): Promise<void> {
    errorMessage = "";
    notice = "";
    try {
      await runtime.setDefaultAccount(account.id);
      accounts = await runtime.listAccounts();
      notice = `默认账户已切换为「${account.username}」`;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function refreshSession(account: AccountSummary): Promise<void> {
    busy = account.id;
    errorMessage = "";
    notice = "";
    try {
      await runtime.refreshAccountSession(account.id);
      accounts = await runtime.listAccounts();
      notice = `「${account.username}」会话已刷新`;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      accounts = await runtime.listAccounts();
    } finally {
      busy = "";
    }
  }

  async function removeAccount(account: AccountSummary): Promise<void> {
    busy = account.id;
    errorMessage = "";
    notice = "";
    try {
      await runtime.removeAccount(account.id);
      pendingAccountRemove = null;
      accounts = await runtime.listAccounts();
      notice = `已移除账户「${account.username}」`;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function saveBackupInterval(): Promise<void> {
    errorMessage = "";
    notice = "";
    if (!Number.isFinite(backupInterval) || backupInterval < 0 || backupInterval > 1440) {
      errorMessage = "备份间隔必须在 0 到 1440 分钟之间";
      return;
    }
    try {
      await runtime.setWorldBackupIntervalMinutes(Math.floor(backupInterval));
      notice = backupInterval === 0 ? "已关闭运行期间定时备份" : `运行期间每 ${Math.floor(backupInterval)} 分钟创建增量备份`;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveBackupKeep(): Promise<void> {
    errorMessage = "";
    notice = "";
    if (!Number.isFinite(backupKeep) || backupKeep < 1 || backupKeep > 100) {
      errorMessage = "备份保留数量必须在 1 到 100 之间";
      return;
    }
    try {
      await runtime.setWorldBackupKeepCount(Math.floor(backupKeep));
      notice = `每个实例最多保留 ${Math.floor(backupKeep)} 个备份`;
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

    <section class="backup-settings" aria-labelledby="accounts-title">
      <header>
        <div>
          <h2 id="accounts-title">账户</h2>
          <p>离线账户仅用于本地单人游戏；外置账户经 Authlib Injector 认证且只保存令牌。Microsoft 登录在应用注册完成后提供。</p>
        </div>
        <div class="local-content-actions">
          <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = addForm === "offline" ? "" : "offline"; }}>添加离线账户</button>
          <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = addForm === "authlib" ? "" : "authlib"; }}>添加外置账户</button>
        </div>
      </header>
      {#if addForm === "offline"}
        <div class="account-form" role="group" aria-label="添加离线账户">
          <label>
            <span>玩家名（3-16 位字母、数字或下划线）</span>
            <input bind:value={offlineName} type="text" aria-label="离线玩家名" placeholder="Steve_2026" />
          </label>
          <div class="local-content-actions">
            <button class="button primary compact" disabled={busy !== "" || !offlineName.trim()} onclick={() => void submitOffline()}>{busy === "add-account" ? "正在创建" : "创建离线账户"}</button>
            <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = ""; }}>取消</button>
          </div>
        </div>
      {:else if addForm === "authlib"}
        <div class="account-form" role="group" aria-label="添加外置账户">
          <label>
            <span>认证服务器</span>
            <select bind:value={authlibServerChoice} aria-label="认证服务器">
              <option value="littleskin">LittleSkin（littleskin.cn）</option>
              <option value="custom">Authlib Injector · 自定义地址</option>
            </select>
          </label>
          {#if authlibServerChoice === "custom"}
            <label>
              <span>服务器地址（https）</span>
              <input bind:value={authlibUrl} type="text" aria-label="认证服务器地址" placeholder="https://example.com/api/yggdrasil" />
            </label>
          {/if}
          <label>
            <span>用户名</span>
            <input bind:value={authlibUser} type="text" aria-label="外置账户用户名" autocomplete="username" />
          </label>
          <label>
            <span>密码（只用于本次登录，不会保存）</span>
            <input bind:value={authlibPass} type="password" aria-label="外置账户密码" autocomplete="current-password" />
          </label>
          <div class="local-content-actions">
            <button
              class="button primary compact"
              disabled={busy !== "" || !authlibUser.trim() || !authlibPass || (authlibServerChoice === "custom" && !authlibUrl.trim())}
              onclick={() => void submitAuthlib()}
            >{busy === "add-account" ? "正在登录" : "登录并添加"}</button>
            <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = ""; }}>取消</button>
          </div>
        </div>
      {/if}
      {#if accounts.length === 0}
        <div class="backup-empty-row">还没有账户。启动游戏时会自动创建一个本地离线账户。</div>
      {:else}
        <div class="backup-list">
          {#each accounts as account}
            <article class="backup-row">
              <div>
                <div class="backup-title-line">
                  <h3>{account.username}</h3>
                  <span>{accountKindLabel(account)}</span>
                  {#if account.isDefault}<span>默认</span>{/if}
                  {#if account.sessionState === "expired"}<span class="account-expired">会话已过期</span>{/if}
                </div>
                <p>{account.kind === "offline" ? "离线模式 · 无法加入正版服务器" : `${account.serverUrl ?? ""} · 令牌仅保存在本地`}</p>
              </div>
              <div class="backup-side">
                {#if !account.isDefault}
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => void makeDefault(account)}>设为默认</button>
                {/if}
                {#if account.kind === "authlib"}
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => void refreshSession(account)}>{busy === account.id ? "正在刷新" : "刷新会话"}</button>
                {/if}
                {#if pendingAccountRemove === account.id}
                  <button class="button danger-subtle compact" disabled={busy !== ""} onclick={() => void removeAccount(account)}>确认移除</button>
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => { pendingAccountRemove = null; }}>取消</button>
                {:else}
                  <button class="button danger-subtle compact" aria-label={`移除账户 ${account.username}`} disabled={busy !== ""} onclick={() => { pendingAccountRemove = account.id; }}>移除</button>
                {/if}
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="backup-settings" aria-labelledby="backup-settings-title">
      <header>
        <div>
          <h2 id="backup-settings-title">世界备份</h2>
          <p>启动前与退出后始终创建完整备份；游戏运行期间按间隔创建只含变化的增量备份。</p>
        </div>
      </header>
      <div class="backup-settings-grid">
        <label>
          <span>运行期间备份间隔（分钟，0 关闭）</span>
          <input
            type="number"
            min="0"
            max="1440"
            aria-label="运行期间备份间隔（分钟）"
            bind:value={backupInterval}
            onchange={() => void saveBackupInterval()}
          />
        </label>
        <label>
          <span>每个实例保留备份数量（1–100）</span>
          <input
            type="number"
            min="1"
            max="100"
            aria-label="每个实例保留备份数量"
            bind:value={backupKeep}
            onchange={() => void saveBackupKeep()}
          />
        </label>
      </div>
    </section>

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
