<script lang="ts">
  import { onMount, tick } from "svelte";

  import { refreshShellAccount, skinAvatarUrl } from "../accounts.svelte";
  import { t } from "../i18n.svelte";
  import type {
    AccountSummary,
    ContentInstallTask,
    DeviceCodeInfo,
    InstallTask,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import { LITTLESKIN_YGGDRASIL_URL } from "../runtime";
  import AppShell from "./AppShell.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    tasks: InstallTask[];
    contentTasks: ContentInstallTask[];
    instances: ManagedInstance[];
    notice: string;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    tasks,
    contentTasks,
    instances,
    notice,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let accounts = $state<AccountSummary[]>([]);
  let loaded = $state(false);
  let busy = $state("");
  let errorMessage = $state("");
  let statusMessage = $state("");
  // 添加菜单默认展开(mockup 固定展开态),点菜单外、Escape 或选中类型后收起。
  let addMenuOpen = $state(true);
  let addForm = $state<"" | "offline" | "authlib">("");
  let offlineName = $state("");
  let authlibServerChoice = $state<"littleskin" | "custom">("littleskin");
  let authlibUrl = $state("");
  let authlibUser = $state("");
  let authlibPass = $state("");
  let savePassword = $state(false);
  let vaultOpen = $state(false);
  let vaultChecked = $state(false);
  let vaultDialog = $state<HTMLElement | null>(null);
  let reloginTarget = $state<AccountSummary | null>(null);
  let pendingRemove = $state<string | null>(null);
  let msLogin = $state<DeviceCodeInfo | null>(null);
  let msCodeCopied = $state(false);
  let avatarFailedIds = $state<string[]>([]);
  // 组件挂载时刻:早于挂载的点击(如触发导航进入本页的那次)不应收起菜单。
  let mountedAt = 0;

  const activeTaskCount = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)).length +
      contentTasks.filter((task) => !["completed", "cancelled"].includes(task.state)).length,
  );
  const microsoftAccounts = $derived(accounts.filter((account) => account.kind === "microsoft"));
  const offlineAccounts = $derived(accounts.filter((account) => account.kind === "offline"));
  const thirdPartyAccounts = $derived(accounts.filter((account) => account.kind === "authlib"));

  onMount(() => {
    mountedAt = performance.now();
    void loadAccounts();
    const unsubscribe = runtime.onMicrosoftDeviceLogin((event) => {
      if (event.state === "completed" && event.account) {
        msLogin = null;
        msCodeCopied = false;
        statusMessage = t("settings.accounts.loggedIn");
        void finalizeLogin();
      } else if (event.state === "failed") {
        msLogin = null;
        reloginTarget = null;
        errorMessage = event.message ?? t("settings.accounts.deviceCode.failed");
      } else if (event.state === "cancelled") {
        msLogin = null;
        msCodeCopied = false;
        reloginTarget = null;
      }
    });
    return unsubscribe;
  });

  /** 登录完成后收敛列表;重新登录场景下移除被替换的旧账户。 */
  async function finalizeLogin(): Promise<void> {
    const stale = reloginTarget;
    reloginTarget = null;
    if (stale) {
      await runtime.removeAccount(stale.id).catch(() => {});
    }
    await loadAccounts();
    void refreshShellAccount(runtime);
  }

  async function loadAccounts(): Promise<void> {
    const firstLoad = !loaded;
    try {
      accounts = await runtime.listAccounts();
      // 菜单默认展开仅用于无账户的引导态;已有账户时收起,避免遮挡行操作。
      if (firstLoad) addMenuOpen = accounts.length === 0;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loaded = true;
    }
  }

  // ---- 添加菜单 ----
  function toggleAddMenu(): void {
    addMenuOpen = !addMenuOpen;
  }

  function handleWindowClick(event: MouseEvent): void {
    if (!addMenuOpen) return;
    if (event.timeStamp <= mountedAt) return;
    if (event.target instanceof HTMLElement && event.target.closest("[data-add-menu-root]")) return;
    addMenuOpen = false;
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && addMenuOpen) addMenuOpen = false;
  }

  function selectAdd(type: "microsoft" | "offline" | "authlib" | "littleskin"): void {
    addMenuOpen = false;
    if (type === "microsoft") {
      void startMicrosoftLogin();
      return;
    }
    reloginTarget = null;
    if (type === "offline") {
      addForm = addForm === "offline" ? "" : "offline";
      return;
    }
    authlibServerChoice = type === "littleskin" ? "littleskin" : "custom";
    addForm = "authlib";
  }

  // ---- Microsoft 设备码登录(沿用现有流程) ----
  async function startMicrosoftLogin(): Promise<void> {
    busy = "add-account";
    errorMessage = "";
    statusMessage = "";
    msCodeCopied = false;
    try {
      msLogin = await runtime.startMicrosoftDeviceLogin();
    } catch (error) {
      reloginTarget = null;
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function cancelMicrosoftLogin(): Promise<void> {
    try {
      await runtime.cancelMicrosoftDeviceLogin();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function copyDeviceCode(): Promise<void> {
    if (!msLogin) return;
    try {
      await navigator.clipboard.writeText(msLogin.userCode);
      msCodeCopied = true;
    } catch {
      errorMessage = t("settings.accounts.deviceCode.copyFailed");
    }
  }

  async function openVerificationLink(): Promise<void> {
    if (!msLogin) return;
    try {
      await runtime.openExternalUrl(msLogin.verificationUri);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  // ---- 离线/外置添加(沿用现有逻辑) ----
  async function submitOffline(): Promise<void> {
    busy = "add-account";
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.addOfflineAccount(offlineName.trim());
      offlineName = "";
      addForm = "";
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
      statusMessage = t("settings.accounts.created");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  /** 勾选本地保存密码时先弹风险确认;未勾选直接登录(只保存令牌)。 */
  async function requestAuthlibSubmit(): Promise<void> {
    if (savePassword) {
      vaultChecked = false;
      vaultOpen = true;
      await tick();
      vaultDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
      return;
    }
    await doSubmitAuthlib();
  }

  function cancelVault(): void {
    if (busy === "add-account") return;
    vaultOpen = false;
  }

  async function confirmVault(): Promise<void> {
    vaultOpen = false;
    await doSubmitAuthlib();
  }

  function handleVaultKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelVault();
      return;
    }
    if (event.key !== "Tab" || !vaultDialog) return;
    const controls = [...vaultDialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const first = controls.at(0);
    const last = controls.at(-1);
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  async function doSubmitAuthlib(): Promise<void> {
    busy = "add-account";
    errorMessage = "";
    statusMessage = "";
    const serverUrl =
      authlibServerChoice === "littleskin" ? LITTLESKIN_YGGDRASIL_URL : authlibUrl.trim();
    try {
      await runtime.addAuthlibAccount(serverUrl, authlibUser.trim(), authlibPass);
      const stale = reloginTarget;
      reloginTarget = null;
      if (stale) {
        await runtime.removeAccount(stale.id).catch(() => {});
      }
      authlibUser = "";
      authlibPass = "";
      authlibUrl = "";
      savePassword = false;
      addForm = "";
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
      statusMessage = t("settings.accounts.loggedIn");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  // ---- 行操作(沿用现有逻辑) ----
  async function makeDefault(account: AccountSummary): Promise<void> {
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.setDefaultAccount(account.id);
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
      statusMessage = t("settings.accounts.defaultSwitched").replace("{name}", account.username);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function refreshSession(account: AccountSummary): Promise<void> {
    busy = account.id;
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.refreshAccountSession(account.id);
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
      statusMessage = t("settings.accounts.sessionRefreshed").replace("{name}", account.username);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
    } finally {
      busy = "";
    }
  }

  async function removeAccount(account: AccountSummary): Promise<void> {
    busy = account.id;
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.removeAccount(account.id);
      pendingRemove = null;
      accounts = await runtime.listAccounts();
      void refreshShellAccount(runtime);
      statusMessage = t("settings.accounts.removed").replace("{name}", account.username);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  /** 会话过期:外置账户预填服务器与用户名重走登录;Microsoft 重新发起设备码。 */
  async function startRelogin(account: AccountSummary): Promise<void> {
    if (account.kind === "microsoft") {
      reloginTarget = account;
      await startMicrosoftLogin();
      return;
    }
    reloginTarget = account;
    authlibServerChoice = account.serverUrl === LITTLESKIN_YGGDRASIL_URL ? "littleskin" : "custom";
    authlibUrl = authlibServerChoice === "custom" ? (account.serverUrl ?? "") : "";
    authlibUser = account.username;
    authlibPass = "";
    savePassword = false;
    addForm = "authlib";
    addMenuOpen = false;
  }

  // ---- 展示辅助 ----
  function avatarUrlFor(account: AccountSummary): string {
    if (avatarFailedIds.includes(account.id)) return "";
    return skinAvatarUrl(account.playerUuid, account.kind);
  }

  function markAvatarFailed(accountId: string): void {
    avatarFailedIds = [...avatarFailedIds, accountId];
  }

  function serverLabel(account: AccountSummary): string {
    if (!account.serverUrl) return "";
    if (account.serverUrl === LITTLESKIN_YGGDRASIL_URL) return "LittleSkin";
    try {
      return new URL(account.serverUrl).hostname;
    } catch {
      return account.serverUrl;
    }
  }

  function relativeTime(unixSeconds: number): string {
    const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
    if (diff < 60) return t("accounts.time.justNow");
    const minutes = Math.floor(diff / 60);
    if (minutes < 60) return t("accounts.time.minutesAgo").replace("{count}", String(minutes));
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return t("accounts.time.hoursAgo").replace("{count}", String(hours));
    return t("accounts.time.daysAgo").replace("{count}", String(Math.floor(hours / 24)));
  }

  function subLine(account: AccountSummary): string {
    if (account.kind === "offline") return t("accounts.sub.offline");
    if (account.sessionState === "expired") return t("accounts.sub.expired");
    const validated = account.lastValidatedAtUnixSeconds
      ? t("accounts.sub.validated").replace("{time}", relativeTime(account.lastValidatedAtUnixSeconds))
      : t("accounts.sub.neverValidated");
    return `${validated} · ${t("accounts.sub.tokenLocal")}`;
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<AppShell
  pageTitle={t("shell.account.pageTitle")}
  activeNavigation="accounts"
  taskCount={activeTaskCount}
  instanceCount={instances.length}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content acct-content">
    <div class="row spread acct-head">
      <div style="min-width:0;max-width:64ch">
        <h1 class="acct-title">{t("accounts.title")}</h1>
        <div class="panel-desc" style="margin-top:2px">{t("accounts.description")}</div>
      </div>
      <div class="add-root" data-add-menu-root>
        <button class="btn primary" aria-expanded={addMenuOpen} onclick={toggleAddMenu}>{t("accounts.add")}</button>
        {#if addMenuOpen}
          <div class="add-menu" aria-label={t("accounts.menuAria")}>
            <button class="am-item" aria-label={t("accounts.addMicrosoft")} disabled={busy !== "" || msLogin !== null} onclick={() => selectAdd("microsoft")}>
              <span class="am-name">Microsoft <span class="tag accent tag-mini">{t("accounts.type.microsoft.recommended")}</span></span>
              <span class="am-desc">{t("accounts.type.microsoft.desc")}</span>
            </button>
            <button class="am-item" aria-label={t("accounts.addOffline")} disabled={busy !== "" || msLogin !== null} onclick={() => selectAdd("offline")}>
              <span class="am-name">{t("accounts.type.offline.name")}</span>
              <span class="am-desc">{t("accounts.type.offline.desc")}</span>
            </button>
            <button class="am-item" aria-label={t("accounts.addAuthlib")} disabled={busy !== "" || msLogin !== null} onclick={() => selectAdd("authlib")}>
              <span class="am-name">{t("accounts.type.authlib.name")}</span>
              <span class="am-desc">{t("accounts.type.authlib.desc")}</span>
            </button>
            <button class="am-item" aria-label={t("accounts.addLittleSkin")} disabled={busy !== "" || msLogin !== null} onclick={() => selectAdd("littleskin")}>
              <span class="am-name">{t("accounts.type.littleskin.name")}</span>
              <span class="am-desc">{t("accounts.type.littleskin.desc")}</span>
            </button>
            <button class="am-item" disabled>
              <span class="am-name">{t("accounts.type.mcunion.name")}</span>
              <span class="am-desc">{t("accounts.type.mcunion.desc")}</span>
            </button>
          </div>
        {/if}
      </div>
    </div>

    {#if msLogin}
      <div class="panel pad device-code-panel" role="group" aria-label={t("settings.accounts.addMicrosoft")}>
        <p class="panel-desc">{t("settings.accounts.deviceCode.instruction")}</p>
        <div class="device-code-row">
          <strong class="device-code" aria-label={t("settings.accounts.deviceCode.codeLabel")}>{msLogin.userCode}</strong>
          <button class="btn small secondary" onclick={() => void copyDeviceCode()}>{msCodeCopied ? t("settings.accounts.deviceCode.copied") : t("settings.accounts.deviceCode.copy")}</button>
        </div>
        <div class="device-code-row">
          <span class="device-code-uri">{msLogin.verificationUri}</span>
          <button class="btn small ghost" onclick={() => void openVerificationLink()}>{t("settings.accounts.deviceCode.openLink")}</button>
        </div>
        <div class="device-code-row">
          <span class="dim" role="status">{t("settings.accounts.deviceCode.waiting")}</span>
          <button class="btn small ghost" onclick={() => void cancelMicrosoftLogin()}>{t("common.cancel")}</button>
        </div>
      </div>
    {/if}

    {#if addForm === "offline"}
      <div class="panel pad acct-form" role="group" aria-label={t("settings.accounts.addOffline")}>
        <div class="field">
          <label for="offline-account-name">{t("settings.accounts.offlineNameLabel")}</label>
          <input id="offline-account-name" class="input" bind:value={offlineName} type="text" aria-label={t("settings.accounts.offlineNameAria")} placeholder={t("settings.accounts.offlineNamePlaceholder")} />
        </div>
        <div class="row">
          <button class="btn small primary" disabled={busy !== "" || !offlineName.trim()} onclick={() => void submitOffline()}>{busy === "add-account" ? t("settings.accounts.creating") : t("settings.accounts.createOffline")}</button>
          <button class="btn small ghost" disabled={busy !== ""} onclick={() => { addForm = ""; }}>{t("common.cancel")}</button>
        </div>
      </div>
    {:else if addForm === "authlib"}
      <div class="panel pad acct-form" role="group" aria-label={t("settings.accounts.addAuthlib")}>
        <div class="field">
          <label for="authlib-server">{t("settings.accounts.serverLabel")}</label>
          <select id="authlib-server" class="input" bind:value={authlibServerChoice} aria-label={t("settings.accounts.serverLabel")}>
            <option value="littleskin">LittleSkin（littleskin.cn）</option>
            <option value="custom">{t("settings.accounts.serverCustom")}</option>
          </select>
        </div>
        {#if authlibServerChoice === "custom"}
          <div class="field">
            <label for="authlib-server-url">{t("settings.accounts.serverUrlLabel")}</label>
            <input id="authlib-server-url" class="input" bind:value={authlibUrl} type="text" aria-label={t("settings.accounts.serverUrlAria")} placeholder={t("settings.accounts.serverUrlPlaceholder")} />
          </div>
        {/if}
        <div class="field">
          <label for="authlib-username">{t("settings.accounts.usernameLabel")}</label>
          <input id="authlib-username" class="input" bind:value={authlibUser} type="text" aria-label={t("settings.accounts.usernameAria")} autocomplete="username" />
        </div>
        <div class="field">
          <label for="authlib-password">{t("settings.accounts.passwordLabel")}</label>
          <input id="authlib-password" class="input" bind:value={authlibPass} type="password" aria-label={t("settings.accounts.passwordAria")} autocomplete="current-password" />
        </div>
        <label class="save-password">
          <input type="checkbox" bind:checked={savePassword} aria-label={t("accounts.savePassword")} />
          <span>{t("accounts.savePassword")}</span>
        </label>
        <div class="row">
          <button
            class="btn small primary"
            disabled={busy !== "" || !authlibUser.trim() || !authlibPass || (authlibServerChoice === "custom" && !authlibUrl.trim())}
            onclick={() => void requestAuthlibSubmit()}
          >{busy === "add-account" ? t("settings.accounts.loggingIn") : t("settings.accounts.loginAndAdd")}</button>
          <button class="btn small ghost" disabled={busy !== ""} onclick={() => { addForm = ""; reloginTarget = null; }}>{t("common.cancel")}</button>
        </div>
      </div>
    {/if}

    {#if !loaded}
      <section class="panel pad">
        <div class="skel" style="height:14px;width:35%;margin-bottom:12px"></div>
        <div class="skel" style="height:14px;width:60%"></div>
      </section>
    {:else if accounts.length === 0}
      <section class="panel pad">
        <span class="muted">{t("settings.accounts.empty")}</span>
      </section>
    {:else}
      {#if microsoftAccounts.length > 0}
        <section class="panel pad acct-group-panel" aria-label={t("accounts.group.microsoft")}>
          <div class="acct-group">{t("accounts.group.microsoft")}</div>
          {#each microsoftAccounts as account (account.id)}
            {@render accountRow(account)}
          {/each}
        </section>
      {/if}
      {#if offlineAccounts.length > 0}
        <section class="panel pad acct-group-panel" aria-label={t("accounts.group.offline")}>
          <div class="acct-group">{t("accounts.group.offline")}</div>
          {#each offlineAccounts as account (account.id)}
            {@render accountRow(account)}
          {/each}
        </section>
      {/if}
      {#if thirdPartyAccounts.length > 0}
        <section class="panel pad acct-group-panel" aria-label={t("accounts.group.thirdParty")}>
          <div class="acct-group">{t("accounts.group.thirdParty")}</div>
          {#each thirdPartyAccounts as account (account.id)}
            {@render accountRow(account)}
          {/each}
        </section>
      {/if}
    {/if}
  </main>

  {#if errorMessage}
    <div class="toast" role="alert" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{errorMessage}</span></div>
  {:else if statusMessage || notice}
    <div class="toast" role="status" style="position:absolute;right:20px;bottom:20px;z-index:35"><span>{statusMessage || notice}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{statusMessage || errorMessage || notice}</div>

  {#if vaultOpen}
    <div class="modal-mask">
      <div
        class="modal"
        style="width:560px"
        role="dialog"
        aria-modal="true"
        aria-labelledby="vault-modal-title"
        tabindex="-1"
        bind:this={vaultDialog}
        onkeydown={handleVaultKeydown}
      >
        <h3 id="vault-modal-title">{t("accounts.vault.title").replace("{name}", authlibUser.trim())}</h3>
        <div class="m-body">
          <p>{t("accounts.vault.body1")}</p>
          <p style="margin-top:10px">{t("accounts.vault.body2")}</p>
          <label class="risk-check">
            <input type="checkbox" class="risk-box" bind:checked={vaultChecked} aria-label={t("accounts.vault.check")} />
            <span class="rc-text">{t("accounts.vault.check")}</span>
          </label>
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={busy === "add-account"} onclick={cancelVault}>{t("common.cancel")}</button>
          <button class="btn primary" disabled={!vaultChecked || busy === "add-account"} onclick={() => void confirmVault()}>
            {busy === "add-account" ? t("settings.accounts.loggingIn") : t("accounts.vault.confirm")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

{#snippet accountRow(account: AccountSummary)}
  {@const avatarUrl = avatarUrlFor(account)}
  {@const server = serverLabel(account)}
  <div class="list-row acct-row">
    <span class="acct-avatar {account.kind}" class:expired={account.sessionState === "expired"}>
      {#if avatarUrl}
        <img src={avatarUrl} alt="" onerror={() => markAvatarFailed(account.id)} />
      {:else}
        {account.username.slice(0, 1) || "?"}
      {/if}
    </span>
    <div class="lr-main">
      <div class="lr-name">
        {account.username}
        {#if account.isDefault}<span class="tag accent tag-mini">{t("settings.accounts.defaultBadge")}</span>{/if}
        {#if account.kind === "authlib" && server}<span class="dim server-tag">· {server}</span>{/if}
      </div>
      <div class="lr-sub">{subLine(account)}</div>
    </div>
    {#if account.kind === "offline"}
      <span class="tag neutral acct-status">{t("accounts.status.offline")}</span>
    {:else if account.sessionState === "expired"}
      <span class="tag danger acct-status"><span class="cdot"></span>{t("settings.accounts.sessionExpired")}</span>
    {:else}
      <span class="tag ok acct-status"><span class="cdot"></span>{t("accounts.status.valid")}</span>
    {/if}
    <div class="row acct-acts">
      {#if account.kind !== "offline" && account.sessionState === "expired"}
        <button class="btn small primary" disabled={busy !== "" || msLogin !== null} onclick={() => void startRelogin(account)}>{t("accounts.action.relogin")}</button>
      {/if}
      {#if account.kind !== "offline"}
        <button class="btn small ghost" disabled={busy !== ""} onclick={() => void refreshSession(account)}>{busy === account.id ? t("settings.accounts.refreshing") : t("settings.accounts.refreshSession")}</button>
      {/if}
      {#if !account.isDefault}
        <button class="btn small ghost" disabled={busy !== ""} onclick={() => void makeDefault(account)}>{t("settings.accounts.makeDefault")}</button>
      {/if}
      {#if pendingRemove === account.id}
        <button class="btn small danger" disabled={busy !== ""} onclick={() => void removeAccount(account)}>{t("settings.accounts.confirmRemove")}</button>
        <button class="btn small ghost" disabled={busy !== ""} onclick={() => { pendingRemove = null; }}>{t("common.cancel")}</button>
      {:else}
        <button class="btn small ghost" aria-label={t("settings.accounts.removeAria").replace("{name}", account.username)} disabled={busy !== ""} onclick={() => { pendingRemove = account.id; }}>{t("settings.accounts.remove")}</button>
      {/if}
    </div>
  </div>
{/snippet}

<style>
  .acct-head {
    margin-bottom: 18px;
  }
  .acct-title {
    font-size: 17px;
    font-weight: 600;
    margin-bottom: 4px;
  }

  /* 添加账户的展开菜单 */
  .add-root {
    position: relative;
    flex: none;
  }
  .add-menu {
    position: absolute;
    top: 44px;
    right: 0;
    width: 240px;
    z-index: 10;
    background: rgba(18, 36, 46, 0.97);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    box-shadow: var(--shadow-2);
    padding: 6px;
  }
  .am-item {
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    background: transparent;
    border-radius: var(--r);
    color: var(--text-1);
    font-family: var(--font);
    cursor: pointer;
  }
  .am-item:hover:not(:disabled) {
    background: var(--glass-strong);
  }
  .am-item:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .am-name {
    font-size: 13px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .am-desc {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .tag-mini {
    height: 18px;
    padding: 0 7px;
    font-size: 10.5px;
  }

  .acct-form {
    margin-bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .acct-form select.input {
    width: 100%;
    min-width: 0;
  }
  .acct-form input.input {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  .save-password {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
    cursor: pointer;
  }
  .save-password input {
    margin-top: 2px;
    accent-color: var(--accent);
  }

  /* 设备码面板 */
  .device-code-panel {
    margin-bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .device-code-row {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .device-code {
    font-family: var(--mono);
    font-size: 18px;
    letter-spacing: 0.08em;
    overflow-wrap: anywhere;
  }
  .device-code-uri {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-2);
    overflow-wrap: anywhere;
    min-width: 0;
  }

  /* 分组账户列表 */
  .acct-group-panel {
    margin-bottom: 14px;
  }
  .acct-group {
    font-size: 11px;
    color: var(--text-3);
    letter-spacing: 0.08em;
    padding: 2px 2px 8px;
  }
  .acct-row {
    padding-left: 0;
    padding-right: 0;
    flex-wrap: wrap;
    row-gap: 8px;
  }
  .acct-avatar {
    width: 38px;
    height: 38px;
    border-radius: 4px;
    flex: none;
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 14px;
    background: var(--glass-strong);
    color: var(--text-2);
    overflow: hidden;
  }
  .acct-avatar.microsoft {
    background: linear-gradient(135deg, #3fd8c2, #2e82b4);
    color: var(--accent-ink);
  }
  .acct-avatar.expired {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .acct-avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    image-rendering: pixelated;
  }
  .acct-status {
    flex: none;
  }
  .acct-acts {
    flex-wrap: wrap;
    gap: 6px;
  }
  .server-tag {
    font-weight: 400;
    margin-left: 4px;
  }

  /* 风险确认复选框(默认不勾选) */
  .risk-check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-top: 14px;
    cursor: pointer;
  }
  .risk-check .risk-box {
    width: 18px;
    height: 18px;
    flex: none;
    margin-top: 1px;
    accent-color: var(--accent);
  }
  .risk-check .rc-text {
    font-size: 12.5px;
    color: var(--text-2);
  }

  /* 窄窗口/高倍放大:行内文本由省略号降级为换行,避免横向裁剪 */
  @media (max-width: 900px) {
    .acct-row .lr-name,
    .acct-row .lr-sub {
      white-space: normal;
      overflow-wrap: anywhere;
    }
  }
</style>
