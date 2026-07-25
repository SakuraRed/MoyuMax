<script lang="ts">
  import { onMount } from "svelte";

  import { t, uiLanguage } from "../i18n.svelte";
  import type {
    BackupState,
    BackupTrigger,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    WorldBackupSummary,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    instances: ManagedInstance[];
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    instances,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  interface BackupGroup {
    instance: ManagedInstance;
    backups: WorldBackupSummary[];
  }

  let groups = $state<BackupGroup[]>([]);
  let loading = $state(true);
  let errorMessage = $state("");
  let notice = $state("");
  let busy = $state("");
  let rollbackCandidate = $state<WorldBackupSummary | null>(null);
  let deleteCandidate = $state<WorldBackupSummary | null>(null);
  let manualInstanceId = $state("");

  onMount(() => void refresh());

  async function refresh(): Promise<void> {
    loading = true;
    errorMessage = "";
    try {
      const all = await runtime.listWorldBackups();
      const result: BackupGroup[] = [];
      for (const instance of instances) {
        const [worlds, backups] = await Promise.all([
          runtime.listInstanceWorlds(instance.id).catch(() => [] as string[]),
          Promise.resolve(all.filter((backup) => backup.instanceId === instance.id)),
        ]);
        // 无世界且没有备份记录的实例不显示(无世界跳过)。
        if (worlds.length === 0 && backups.length === 0) continue;
        result.push({ instance, backups });
      }
      groups = result;
      if (!manualInstanceId && result.length > 0) {
        manualInstanceId = result[0]?.instance.id ?? "";
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  async function createManual(): Promise<void> {
    if (!manualInstanceId) return;
    busy = "manual";
    errorMessage = "";
    notice = "";
    try {
      const backup = await runtime.createManualWorldBackup(manualInstanceId);
      if (backup.state === "skipped") {
        notice = t("backups.manualSkipped");
      } else {
        notice = t("backups.manualDone");
      }
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function confirmRollback(): Promise<void> {
    if (!rollbackCandidate) return;
    busy = rollbackCandidate.id;
    errorMessage = "";
    try {
      await runtime.rollbackWorldBackup(rollbackCandidate.id);
      rollbackCandidate = null;
      notice = t("backups.rollbackDone");
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteCandidate) return;
    busy = deleteCandidate.id;
    errorMessage = "";
    try {
      await runtime.deleteWorldBackup(deleteCandidate.id);
      deleteCandidate = null;
      notice = t("backups.deleteDone");
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  function triggerLabel(trigger: BackupTrigger): string {
    switch (trigger) {
      case "preLaunch":
        return t("data.backups.trigger.preLaunch");
      case "postExit":
        return t("data.backups.trigger.postExit");
      case "manual":
        return t("data.backups.trigger.manual");
      default:
        return t("data.backups.trigger.scheduled");
    }
  }

  function stateLabel(state: BackupState): string {
    switch (state) {
      case "ready":
        return t("data.backups.state.ready");
      case "skipped":
        return t("data.backups.state.skipped");
      case "failed":
        return t("data.backups.state.failed");
      default:
        return t("data.backups.state.interrupted");
    }
  }

  function formatTime(unixSeconds: number): string {
    return new Intl.DateTimeFormat(uiLanguage(), {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(unixSeconds * 1000));
  }

  function formatBytes(value: number): string {
    if (value >= 1024 * 1024 * 1024) return `${(value / 1024 / 1024 / 1024).toFixed(1)} GiB`;
    if (value >= 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MiB`;
    if (value >= 1024) return `${(value / 1024).toFixed(1)} KiB`;
    return `${value} B`;
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape" && busy === "") {
      rollbackCandidate = null;
      deleteCandidate = null;
    }
  }}
/>

<AppShell
  pageTitle={t("backups.pageTitle")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="data"
  {onNavigate}
  connectionStatus={t("data.connectionStatus")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content backup-content">
    <div class="backup-scroll" data-scroll-region="main">
      <header class="data-heading">
        <div>
          <h1>{t("backups.heading")}</h1>
          <p>{t("backups.description")}</p>
        </div>
        <div class="local-content-actions">
          <select value={manualInstanceId} onchange={(event) => { manualInstanceId = (event.currentTarget as HTMLSelectElement).value; }} aria-label={t("backups.manualTarget")}>
            {#each groups as group}
              <option value={group.instance.id}>{group.instance.name}</option>
            {/each}
          </select>
          <button class="button primary compact" disabled={busy !== "" || !manualInstanceId} onclick={() => void createManual()}>{busy === "manual" ? t("backups.manualRunning") : t("backups.manualRun")}</button>
          <button class="button ghost compact" onclick={() => onNavigate("settings")}>{t("backups.openSettings")}</button>
        </div>
      </header>

      {#if errorMessage}
        <div class="error-block" role="alert"><strong>{t("backups.errorTitle")}</strong><span>{errorMessage}</span></div>
      {/if}
      {#if notice}
        <div class="java-notice" role="status">{notice}</div>
      {/if}

      {#if loading}
        <div class="content-loading" aria-live="polite"><span>{t("backups.loading")}</span></div>
      {:else if groups.length === 0}
        <section class="task-empty">
          <Icon name="box" size={28} />
          <h2>{t("backups.emptyTitle")}</h2>
          <p>{t("backups.emptyDescription")}</p>
        </section>
      {:else}
        {#each groups as group}
          <section class="backup-settings" aria-labelledby="backup-group-{group.instance.id}">
            <header>
              <div>
                <h2 id="backup-group-{group.instance.id}">{group.instance.name}</h2>
                <p>{t("backups.groupSummary").replace("{count}", String(group.backups.length)).replace("{size}", formatBytes(group.backups.reduce((sum, backup) => sum + backup.archiveBytes, 0)))}</p>
              </div>
            </header>
            {#if group.backups.length === 0}
              <div class="local-content-empty">{t("backups.groupEmpty")}</div>
            {:else}
              <div class="backup-list">
                {#each group.backups as backup}
                  <article class="backup-row">
                    <div>
                      <div class="backup-title-line">
                        <h3>{formatTime(backup.createdAtUnixSeconds)}</h3>
                        <span>{triggerLabel(backup.trigger)}</span>
                        <span class:account-expired={backup.state === "failed"}>{stateLabel(backup.state)}</span>
                      </div>
                      <p>{t("backups.rowSummary").replace("{worlds}", String(backup.worldCount)).replace("{size}", formatBytes(backup.archiveBytes))}{#if backup.errorSummary} · {backup.errorSummary}{/if}</p>
                    </div>
                    <div class="backup-side">
                      <button class="button ghost compact" disabled={busy !== "" || backup.state !== "ready"} onclick={() => { rollbackCandidate = backup; deleteCandidate = null; }}>{t("backups.rollback")}</button>
                      {#if deleteCandidate?.id === backup.id}
                        <button class="button danger-subtle compact" disabled={busy !== ""} onclick={() => void confirmDelete()}>{t("backups.deleteConfirm")}</button>
                        <button class="button ghost compact" disabled={busy !== ""} onclick={() => { deleteCandidate = null; }}>{t("common.cancel")}</button>
                      {:else}
                        <button class="button danger-subtle compact" aria-label={t("backups.deleteAria")} disabled={busy !== ""} onclick={() => { deleteCandidate = backup; rollbackCandidate = null; }}>{t("backups.delete")}</button>
                      {/if}
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          </section>
        {/each}
      {/if}
    </div>
  </main>

  {#if rollbackCandidate}
    <div class="modal-backdrop" role="presentation">
      <div
        class="confirmation-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rollback-dialog-title"
        tabindex="-1"
        onkeydown={(event) => { if (event.key === "Escape" && busy === "") rollbackCandidate = null; }}
      >
        <header>
          <h2 id="rollback-dialog-title">{t("backups.rollbackTitle")}</h2>
          <p>{t("backups.rollbackBody").replace("{time}", formatTime(rollbackCandidate.createdAtUnixSeconds)).replace("{name}", rollbackCandidate.instanceName)}</p>
        </header>
        <div class="confirmation-actions">
          <button class="button" data-dialog-autofocus disabled={busy !== ""} onclick={() => { rollbackCandidate = null; }}>{t("common.cancel")}</button>
          <button class="button primary" disabled={busy !== ""} onclick={() => void confirmRollback()}>{busy !== "" ? t("backups.rollbackRunning") : t("backups.rollbackConfirm")}</button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>
