<script lang="ts">
  import { onMount } from "svelte";

  import {
    applyUiPreferences,
    t,
    UI_CONTRASTS,
    UI_LANGUAGES,
    UI_MOTIONS,
    UI_THEMES,
    uiContrast,
    uiLanguage,
    uiMotion,
    uiTheme,
    type UiContrast,
    type UiLanguage,
    type UiMotion,
    type UiTheme,
  } from "../i18n.svelte";
  import { formatBytes } from "../installation";
  import type {
    AccountSummary,
    JavaDeleteOutcome,
    JavaEnvironment,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
    ReferencingInstance,
    ReleaseInfo,
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
  let cliEnabled = $state(false);
  let updateChecks = $state(true);
  let checkingUpdates = $state(false);
  let updateResult = $state<"none" | ReleaseInfo | null>("none");
  let downloading = $state(false);
  let downloadedPath = $state("");
  let uiPreferencesLoaded = $state(false);

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
        const [backupSettings, cliState, updateChecksState] = await Promise.all([
          runtime.getWorldBackupSettings(),
          runtime.getCliEnabled(),
          runtime.getUpdateChecksEnabled(),
        ]);
        backupInterval = backupSettings.intervalMinutes;
        backupKeep = backupSettings.keepCount;
        cliEnabled = cliState;
        updateChecks = updateChecksState;
        backupSettingsLoaded = true;
      }
      if (!uiPreferencesLoaded) {
        const preferences = await runtime.getUiPreferences();
        applyStoredUiPreferences(preferences);
        uiPreferencesLoaded = true;
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  // 仅接受受支持的偏好值，非法存储值保持当前默认不变。
  function applyStoredUiPreferences(stored: {
    theme: string;
    language: string;
    motion: string;
    contrast: string;
  }): void {
    const nextTheme = UI_THEMES.find((entry) => entry.value === stored.theme)?.value;
    const nextLanguage = UI_LANGUAGES.find((entry) => entry.value === stored.language)?.value;
    const nextMotion = UI_MOTIONS.find((entry) => entry.value === stored.motion)?.value;
    const nextContrast = UI_CONTRASTS.find((entry) => entry.value === stored.contrast)?.value;
    applyUiPreferences({
      theme: nextTheme,
      language: nextLanguage,
      motion: nextMotion,
      contrast: nextContrast,
    });
  }

  async function selectTheme(value: UiTheme): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiTheme(value);
      applyUiPreferences({ theme: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectLanguage(value: UiLanguage): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiLanguage(value);
      applyUiPreferences({ language: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectMotion(value: UiMotion): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiMotion(value);
      applyUiPreferences({ motion: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectContrast(value: UiContrast): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiContrast(value);
      applyUiPreferences({ contrast: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function toggleCli(checked: boolean): Promise<void> {
    const previous = cliEnabled;
    cliEnabled = checked;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setCliEnabled(checked);
      notice = checked ? t("settings.dev.cliEnabled") : t("settings.dev.cliDisabled");
    } catch (error) {
      cliEnabled = previous;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function toggleUpdateChecks(checked: boolean): Promise<void> {
    const previous = updateChecks;
    updateChecks = checked;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setUpdateChecksEnabled(checked);
    } catch (error) {
      updateChecks = previous;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function checkUpdates(): Promise<void> {
    checkingUpdates = true;
    errorMessage = "";
    notice = "";
    updateResult = "none";
    downloadedPath = "";
    try {
      updateResult = await runtime.checkForUpdates();
      if (updateResult === null) {
        notice = t("settings.update.upToDate");
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      checkingUpdates = false;
    }
  }

  async function downloadUpdate(): Promise<void> {
    if (updateResult === "none" || updateResult === null) return;
    downloading = true;
    errorMessage = "";
    notice = "";
    downloadedPath = "";
    try {
      downloadedPath = await runtime.downloadUpdateInstaller(updateResult);
      notice = t("settings.update.downloadDone");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      downloading = false;
    }
  }

  async function openDownloaded(): Promise<void> {
    if (!downloadedPath) return;
    errorMessage = "";
    try {
      await runtime.openUpdateLocation(downloadedPath);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function accountKindLabel(account: AccountSummary): string {
    return account.kind === "offline" ? t("settings.accounts.kind.offline") : t("settings.accounts.kind.authlib");
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
      notice = t("settings.accounts.created");
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
      notice = t("settings.accounts.loggedIn");
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
      notice = t("settings.accounts.defaultSwitched").replace("{name}", account.username);
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
      notice = t("settings.accounts.sessionRefreshed").replace("{name}", account.username);
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
      notice = t("settings.accounts.removed").replace("{name}", account.username);
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
      errorMessage = t("settings.backup.intervalInvalid");
      return;
    }
    try {
      await runtime.setWorldBackupIntervalMinutes(Math.floor(backupInterval));
      notice = backupInterval === 0 ? t("settings.backup.intervalDisabled") : t("settings.backup.intervalSaved").replace("{minutes}", String(Math.floor(backupInterval)));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveBackupKeep(): Promise<void> {
    errorMessage = "";
    notice = "";
    if (!Number.isFinite(backupKeep) || backupKeep < 1 || backupKeep > 100) {
      errorMessage = t("settings.backup.keepInvalid");
      return;
    }
    try {
      await runtime.setWorldBackupKeepCount(Math.floor(backupKeep));
      notice = t("settings.backup.keepSaved").replace("{count}", String(Math.floor(backupKeep)));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  function distributionName(environment: JavaEnvironment): string {
    return environment.distribution === "azulZulu" ? "Azul Zulu" : environment.distribution;
  }

  function statusLabel(environment: JavaEnvironment): string {
    if (!environment.healthy && environment.status === "ready") return t("settings.java.status.missingFiles");
    const keys: Record<string, string> = {
      planned: "settings.java.status.planned",
      installing: "settings.java.status.installing",
      ready: "settings.java.status.ready",
      missing: "settings.java.status.missing",
      failed: "settings.java.status.failed",
      deleted: "settings.java.status.deleted",
    };
    const key = keys[environment.status];
    return key ? t(key) : environment.status;
  }

  async function verify(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const healthy = await runtime.verifyJavaEnvironment(environment.id);
      notice = healthy
        ? t("settings.java.verifyOk").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion)
        : t("settings.java.verifyMissing").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion);
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
        notice = t("settings.java.deleted").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion);
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
      notice = t("settings.java.deletedWithRefs")
        .replace("{distribution}", distributionName(deleteTarget))
        .replace("{version}", deleteTarget.fullVersion)
        .replace("{count}", String(deleteAffected.length));
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
      notice = t("settings.java.restored").replace("{distribution}", distributionName(restored)).replace("{version}", restored.fullVersion);
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
      notice = t("settings.java.assigned")
        .replace("{name}", instance?.name ?? assignInstance)
        .replace("{distribution}", distributionName(environment))
        .replace("{version}", environment.fullVersion);
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
  pageTitle={t("settings.pageTitle")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="settings"
  navigationTargets={["home"]}
  onNavigate={(target) => target === "home" ? onOpenHome() : undefined}
  connectionStatus={t("settings.connectionStatus")}
  taskStatus={t("settings.envCount").replace("{count}", String(environments.length))}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content java-content" data-scroll-region="main">
    <header class="task-center-heading">
      <button class="button ghost compact" onclick={onBack}>{t("settings.back")}</button>
      <div>
        <h1>{t("settings.java.heading")}</h1>
        <p>{t("settings.java.description")}</p>
      </div>
    </header>

    {#if errorMessage}
      <div class="error-block" role="alert"><strong>{t("settings.error.title")}</strong><span>{errorMessage}</span></div>
    {/if}
    {#if notice}
      <div class="java-notice" role="status">{notice}</div>
    {/if}

    <section class="backup-settings" aria-labelledby="appearance-title">
      <header>
        <div>
          <h2 id="appearance-title">{t("appearance.title")}</h2>
          <p>{t("appearance.description")}</p>
        </div>
      </header>
      <div class="backup-settings-grid">
        <div>
          <span>{t("appearance.themeLabel")}</span>
          <div class="screenshot-filters" role="group" aria-label={t("appearance.themeAria")}>
            {#each UI_THEMES as themeOption}
              <button
                class="filter-chip"
                class:active={uiTheme() === themeOption.value}
                onclick={() => void selectTheme(themeOption.value)}
              >{t(themeOption.labelKey)}</button>
            {/each}
          </div>
        </div>
        <div>
          <span>{t("appearance.languageLabel")}</span>
          <div class="screenshot-filters" role="group" aria-label={t("appearance.languageAria")}>
            {#each UI_LANGUAGES as languageOption}
              <button
                class="filter-chip"
                class:active={uiLanguage() === languageOption.value}
                onclick={() => void selectLanguage(languageOption.value)}
              >{languageOption.label}</button>
            {/each}
          </div>
        </div>
        <div>
          <span>{t("appearance.motionLabel")}</span>
          <div class="screenshot-filters" role="group" aria-label={t("appearance.motionAria")}>
            {#each UI_MOTIONS as motionOption}
              <button
                class="filter-chip"
                class:active={uiMotion() === motionOption.value}
                onclick={() => void selectMotion(motionOption.value)}
              >{t(motionOption.labelKey)}</button>
            {/each}
          </div>
        </div>
        <div>
          <span>{t("appearance.contrastLabel")}</span>
          <div class="screenshot-filters" role="group" aria-label={t("appearance.contrastAria")}>
            {#each UI_CONTRASTS as contrastOption}
              <button
                class="filter-chip"
                class:active={uiContrast() === contrastOption.value}
                onclick={() => void selectContrast(contrastOption.value)}
              >{t(contrastOption.labelKey)}</button>
            {/each}
          </div>
        </div>
      </div>
    </section>

    <section class="backup-settings" aria-labelledby="accounts-title">
      <header>
        <div>
          <h2 id="accounts-title">{t("settings.accounts.title")}</h2>
          <p>{t("settings.accounts.description")}</p>
        </div>
        <div class="local-content-actions">
          <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = addForm === "offline" ? "" : "offline"; }}>{t("settings.accounts.addOffline")}</button>
          <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = addForm === "authlib" ? "" : "authlib"; }}>{t("settings.accounts.addAuthlib")}</button>
        </div>
      </header>
      {#if addForm === "offline"}
        <div class="account-form" role="group" aria-label={t("settings.accounts.addOffline")}>
          <label>
            <span>{t("settings.accounts.offlineNameLabel")}</span>
            <input bind:value={offlineName} type="text" aria-label={t("settings.accounts.offlineNameAria")} placeholder={t("settings.accounts.offlineNamePlaceholder")} />
          </label>
          <div class="local-content-actions">
            <button class="button primary compact" disabled={busy !== "" || !offlineName.trim()} onclick={() => void submitOffline()}>{busy === "add-account" ? t("settings.accounts.creating") : t("settings.accounts.createOffline")}</button>
            <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = ""; }}>{t("common.cancel")}</button>
          </div>
        </div>
      {:else if addForm === "authlib"}
        <div class="account-form" role="group" aria-label={t("settings.accounts.addAuthlib")}>
          <label>
            <span>{t("settings.accounts.serverLabel")}</span>
            <select bind:value={authlibServerChoice} aria-label={t("settings.accounts.serverLabel")}>
              <option value="littleskin">LittleSkin（littleskin.cn）</option>
              <option value="custom">{t("settings.accounts.serverCustom")}</option>
            </select>
          </label>
          {#if authlibServerChoice === "custom"}
            <label>
              <span>{t("settings.accounts.serverUrlLabel")}</span>
              <input bind:value={authlibUrl} type="text" aria-label={t("settings.accounts.serverUrlAria")} placeholder={t("settings.accounts.serverUrlPlaceholder")} />
            </label>
          {/if}
          <label>
            <span>{t("settings.accounts.usernameLabel")}</span>
            <input bind:value={authlibUser} type="text" aria-label={t("settings.accounts.usernameAria")} autocomplete="username" />
          </label>
          <label>
            <span>{t("settings.accounts.passwordLabel")}</span>
            <input bind:value={authlibPass} type="password" aria-label={t("settings.accounts.passwordAria")} autocomplete="current-password" />
          </label>
          <div class="local-content-actions">
            <button
              class="button primary compact"
              disabled={busy !== "" || !authlibUser.trim() || !authlibPass || (authlibServerChoice === "custom" && !authlibUrl.trim())}
              onclick={() => void submitAuthlib()}
            >{busy === "add-account" ? t("settings.accounts.loggingIn") : t("settings.accounts.loginAndAdd")}</button>
            <button class="button ghost compact" disabled={busy !== ""} onclick={() => { addForm = ""; }}>{t("common.cancel")}</button>
          </div>
        </div>
      {/if}
      {#if accounts.length === 0}
        <div class="backup-empty-row">{t("settings.accounts.empty")}</div>
      {:else}
        <div class="backup-list">
          {#each accounts as account}
            <article class="backup-row">
              <div>
                <div class="backup-title-line">
                  <h3>{account.username}</h3>
                  <span>{accountKindLabel(account)}</span>
                  {#if account.isDefault}<span>{t("settings.accounts.defaultBadge")}</span>{/if}
                  {#if account.sessionState === "expired"}<span class="account-expired">{t("settings.accounts.sessionExpired")}</span>{/if}
                </div>
                <p>{account.kind === "offline" ? t("settings.accounts.offlineNote") : t("settings.accounts.authlibNote").replace("{url}", account.serverUrl ?? "")}</p>
              </div>
              <div class="backup-side">
                {#if !account.isDefault}
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => void makeDefault(account)}>{t("settings.accounts.makeDefault")}</button>
                {/if}
                {#if account.kind === "authlib"}
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => void refreshSession(account)}>{busy === account.id ? t("settings.accounts.refreshing") : t("settings.accounts.refreshSession")}</button>
                {/if}
                {#if pendingAccountRemove === account.id}
                  <button class="button danger-subtle compact" disabled={busy !== ""} onclick={() => void removeAccount(account)}>{t("settings.accounts.confirmRemove")}</button>
                  <button class="button ghost compact" disabled={busy !== ""} onclick={() => { pendingAccountRemove = null; }}>{t("common.cancel")}</button>
                {:else}
                  <button class="button danger-subtle compact" aria-label={t("settings.accounts.removeAria").replace("{name}", account.username)} disabled={busy !== ""} onclick={() => { pendingAccountRemove = account.id; }}>{t("settings.accounts.remove")}</button>
                {/if}
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="backup-settings" aria-labelledby="update-title">
      <header>
        <div>
          <h2 id="update-title">{t("settings.update.title")}</h2>
          <p>{t("settings.update.description")}</p>
        </div>
        <div class="local-content-actions">
          <button class="button ghost compact" disabled={checkingUpdates || !updateChecks} onclick={() => void checkUpdates()}>
            {checkingUpdates ? t("settings.update.checking") : t("settings.update.check")}
          </button>
        </div>
      </header>
      <div class="update-current">
        <span>{t("settings.update.currentVersion")}</span>
        <strong>0.1.0-preview.1</strong>
      </div>
      <label class="auto-update-toggle">
        <input
          type="checkbox"
          checked={updateChecks}
          aria-label={t("settings.update.promptLabel")}
          onchange={(event) => void toggleUpdateChecks((event.currentTarget as HTMLInputElement).checked)}
        />
        <span>
          <strong>{t("settings.update.promptLabel")}</strong>
          <small>{t("settings.update.promptHint")}</small>
        </span>
      </label>
      {#if updateResult !== "none" && updateResult !== null}
        <div class="update-result" role="status">
          <div class="backup-title-line">
            <h3>{updateResult.tag}</h3>
            <span>{t("settings.update.available")}</span>
          </div>
          {#if updateResult.notes}
            <p class="update-notes">{updateResult.notes}</p>
          {/if}
          {#if updateResult.minAppVersion}
            <p class="update-notes">{t("settings.update.minVersion").replace("{version}", updateResult.minAppVersion)}</p>
          {/if}
          <div class="local-content-actions">
            <button
              class="button primary compact"
              disabled={downloading || !updateResult.installer}
              onclick={() => void downloadUpdate()}
            >{downloading ? t("settings.update.downloading") : t("settings.update.download")}</button>
            {#if downloadedPath}
              <button class="button ghost compact" onclick={() => void openDownloaded()}>{t("settings.update.openLocation")}</button>
            {/if}
          </div>
          {#if downloadedPath}
            <small class="update-path">{downloadedPath}</small>
          {/if}
        </div>
      {/if}
    </section>

    <section class="backup-settings" aria-labelledby="dev-title">
      <header>
        <div>
          <h2 id="dev-title">{t("settings.dev.title")}</h2>
          <p>{t("settings.dev.description")}</p>
        </div>
      </header>
      <label class="auto-update-toggle">
        <input
          type="checkbox"
          checked={cliEnabled}
          aria-label={t("settings.dev.cliLabel")}
          onchange={(event) => void toggleCli((event.currentTarget as HTMLInputElement).checked)}
        />
        <span>
          <strong>{t("settings.dev.cliLabel")}</strong>
          <small>{t("settings.dev.cliHint")}</small>
          <small class="dev-risk">{t("settings.dev.riskWarning")}</small>
        </span>
      </label>
      {#if cliEnabled}
        <div class="dev-usage">
          <code>moyumax-desktop.exe --cli instances list</code>
          <small>{t("settings.dev.usageHint")}</small>
        </div>
      {/if}
    </section>

    <section class="backup-settings" aria-labelledby="backup-settings-title">
      <header>
        <div>
          <h2 id="backup-settings-title">{t("settings.backup.title")}</h2>
          <p>{t("settings.backup.description")}</p>
        </div>
      </header>
      <div class="backup-settings-grid">
        <label>
          <span>{t("settings.backup.intervalLabel")}</span>
          <input
            type="number"
            min="0"
            max="1440"
            aria-label={t("settings.backup.intervalAria")}
            bind:value={backupInterval}
            onchange={() => void saveBackupInterval()}
          />
        </label>
        <label>
          <span>{t("settings.backup.keepLabel")}</span>
          <input
            type="number"
            min="1"
            max="100"
            aria-label={t("settings.backup.keepAria")}
            bind:value={backupKeep}
            onchange={() => void saveBackupKeep()}
          />
        </label>
      </div>
    </section>

    {#if environments.length === 0 && deletedEnvironments.length === 0}
      <section class="task-empty"><Icon name="box" size={28} /><h2>{t("settings.java.emptyTitle")}</h2><p>{t("settings.java.emptyDescription")}</p></section>
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
                {t("settings.java.refs").replace("{names}", environment.referencingInstances.map((entry) => entry.name).join(t("settings.java.namesSeparator")))}
              </p>
            {:else}
              <p class="java-refs muted">{t("settings.java.noRefs")}</p>
            {/if}
            <div class="task-buttons">
              <button class="button ghost compact" disabled={busy === environment.id} onclick={() => void verify(environment)}>{t("settings.java.verify")}</button>
              <button class="button ghost compact" disabled={busy === environment.id} onclick={() => void openLocation(environment)}>{t("settings.java.openLocation")}</button>
              <button
                class="button ghost compact"
                disabled={busy === environment.id || instances.length === 0}
                onclick={() => {
                  assignTarget = assignTarget === environment.id ? "" : environment.id;
                  assignInstance = instances[0]?.id ?? "";
                }}
              >{t("settings.java.assign")}</button>
              <button class="button danger-subtle compact" disabled={busy === environment.id} onclick={() => void requestDelete(environment)}>{t("settings.java.delete")}</button>
            </div>
            {#if assignTarget === environment.id}
              <div class="java-assign" role="group" aria-label={t("settings.java.assignAria")}>
                <label>
                  {t("settings.java.assignTarget")}
                  <select bind:value={assignInstance}>
                    {#each instances as instance}
                      <option value={instance.id}>{t("settings.java.instanceOption").replace("{name}", instance.name).replace("{version}", instance.gameVersion).replace("{loader}", instance.loaderKind)}</option>
                    {/each}
                  </select>
                </label>
                <button class="button primary compact" disabled={busy === environment.id || !assignInstance} onclick={() => void applyAssignment(environment)}>{t("settings.java.assignConfirm")}</button>
                <small>{t("settings.java.assignHint")}</small>
              </div>
            {/if}
          </article>
        {/each}
      </div>
    {/if}

    {#if deletedEnvironments.length > 0}
      <section class="java-deleted" aria-label={t("settings.java.deletedSectionTitle")}>
        <h2>{t("settings.java.deletedSectionTitle")}</h2>
        <p>{t("settings.java.deletedSectionDescription")}</p>
        <div class="task-list">
          {#each deletedEnvironments as environment}
            <article class="task-card java-card deleted">
              <header>
                <div>
                  <strong>{distributionName(environment)} {environment.fullVersion}</strong>
                  <small>{environment.architecture}</small>
                </div>
                <span class="task-state">{t("settings.java.status.deleted")}</span>
              </header>
              {#if environment.referencingInstances.length > 0}
                <p class="java-refs">{t("settings.java.stillReferenced").replace("{names}", environment.referencingInstances.map((entry) => entry.name).join(t("settings.java.namesSeparator")))}</p>
              {/if}
              <div class="task-buttons">
                <button class="button primary compact" disabled={busy === environment.id} onclick={() => void restore(environment)}>{t("settings.java.restore")}</button>
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
          <h2 id="delete-java-title">{t("settings.java.deleteDialogTitle")}</h2>
          <p>{t("settings.java.deleteDialogInUse")}</p>
        </header>
        <div class="confirmation-impact danger-impact" role="note">
          <strong>{t("settings.java.deleteDialogImpact")}</strong>
          {#each deleteAffected as instance}
            <span>{t("settings.java.deleteDialogInstance").replace("{name}", instance.name)}</span>
          {/each}
          <span>{t("settings.java.deleteDialogNote")}</span>
        </div>
        <div class="confirmation-actions">
          <button class="button ghost" onclick={() => { deleteTarget = null; deleteAffected = []; }}>{t("common.cancel")}</button>
          <button class="button danger-subtle" disabled={busy === deleteTarget.id} onclick={() => void confirmDelete()}>
            {t("settings.java.deleteDialogConfirm").replace("{distribution}", distributionName(deleteTarget)).replace("{version}", deleteTarget.fullVersion)}
          </button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
