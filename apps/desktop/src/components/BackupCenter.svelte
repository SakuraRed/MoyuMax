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
  import { pushToast } from "../toast.svelte";
  import AppShell from "./AppShell.svelte";

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
    try {
      const backup = await runtime.createManualWorldBackup(manualInstanceId);
      pushToast({
        tone: backup.state === "skipped" ? "warn" : "ok",
        title: backup.state === "skipped" ? t("backups.manualSkipped") : t("backups.manualDone"),
      });
      await refresh();
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
    } finally {
      busy = "";
    }
  }

  async function confirmRollback(): Promise<void> {
    if (!rollbackCandidate) return;
    busy = rollbackCandidate.id;
    try {
      await runtime.rollbackWorldBackup(rollbackCandidate.id);
      rollbackCandidate = null;
      pushToast({ tone: "ok", title: t("backups.rollbackDone") });
      await refresh();
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
    } finally {
      busy = "";
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteCandidate) return;
    busy = deleteCandidate.id;
    try {
      await runtime.deleteWorldBackup(deleteCandidate.id);
      deleteCandidate = null;
      pushToast({ tone: "ok", title: t("backups.deleteDone") });
      await refresh();
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
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

  function stateTag(state: BackupState): string {
    if (state === "ready") return "ok";
    if (state === "failed") return "danger";
    return "neutral";
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
  activeNavigation="data"
  onBack={() => onNavigate("data")}
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content">
    <div class="row spread" style="margin-bottom:16px">
      <div>
        <h1 class="page-title">{t("backups.heading")}</h1>
        <p class="muted" style="margin-top:2px">{t("backups.description")}</p>
      </div>
      <div class="row">
        <select
          class="input"
          value={manualInstanceId}
          onchange={(event) => { manualInstanceId = (event.currentTarget as HTMLSelectElement).value; }}
          aria-label={t("backups.manualTarget")}
        >
          {#each groups as group}
            <option value={group.instance.id}>{group.instance.name}</option>
          {/each}
        </select>
        <button class="btn primary" disabled={busy !== "" || !manualInstanceId} onclick={() => void createManual()}>{busy === "manual" ? t("backups.manualRunning") : t("backups.manualRun")}</button>
        <button class="btn ghost" onclick={() => onNavigate("settings")}>{t("backups.openSettings")}</button>
      </div>
    </div>

    {#if errorMessage}
      <div class="banner danger" role="alert" style="margin-bottom:16px">
        <div><strong>{t("backups.errorTitle")}</strong><div>{errorMessage}</div></div>
      </div>
    {/if}

    {#if loading}
      <div class="skel" style="height:120px;margin-bottom:16px"></div>
      <div class="skel" style="height:120px"></div>
    {:else if groups.length === 0}
      <div class="empty-stage">
        <h2 style="font-size:17px">{t("backups.emptyTitle")}</h2>
        <p class="muted" style="max-width:44ch;text-align:center">{t("backups.emptyDescription")}</p>
      </div>
    {:else}
      {#each groups as group}
        <section class="panel pad" style="margin-bottom:16px" aria-labelledby="backup-group-{group.instance.id}">
          <div class="row spread">
            <div>
              <h2 class="panel-title" id="backup-group-{group.instance.id}">{group.instance.name}</h2>
              <div class="panel-desc" style="margin-top:2px">{t("backups.groupSummary").replace("{count}", String(group.backups.length)).replace("{size}", formatBytes(group.backups.reduce((sum, backup) => sum + backup.archiveBytes, 0)))}</div>
            </div>
          </div>
          {#if group.backups.length === 0}
            <p class="dim" style="padding:10px 0">{t("backups.groupEmpty")}</p>
          {:else}
            <div style="margin-top:8px">
              {#each group.backups as backup}
                <div class="tl-item">
                  <span class="tl-dot" class:failed={backup.state === "failed"}></span>
                  <div class="lr-main">
                    <div class="lr-name"><span>{triggerLabel(backup.trigger)}</span> <span class="dim" style="font-weight:400">{formatTime(backup.createdAtUnixSeconds)}</span></div>
                    <div class="lr-sub">{t("backups.rowSummary").replace("{worlds}", String(backup.worldCount)).replace("{size}", formatBytes(backup.archiveBytes))}{#if backup.errorSummary} · {backup.errorSummary}{/if}</div>
                  </div>
                  <span class="tag {stateTag(backup.state)}">{stateLabel(backup.state)}</span>
                  <div class="row" style="gap:4px">
                    <button class="btn small ghost" disabled={busy !== "" || backup.state !== "ready"} onclick={() => { rollbackCandidate = backup; deleteCandidate = null; }}>{t("backups.rollback")}</button>
                    {#if deleteCandidate?.id === backup.id}
                      <button class="btn small danger-soft" disabled={busy !== ""} onclick={() => void confirmDelete()}>{t("backups.deleteConfirm")}</button>
                      <button class="btn small ghost" disabled={busy !== ""} onclick={() => { deleteCandidate = null; }}>{t("common.cancel")}</button>
                    {:else}
                      <button class="btn small ghost" aria-label={t("backups.deleteAria")} disabled={busy !== ""} onclick={() => { deleteCandidate = backup; rollbackCandidate = null; }}>{t("backups.delete")}</button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {/each}
    {/if}
  </main>

  {#if rollbackCandidate}
    <div class="modal-mask" role="presentation">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rollback-dialog-title"
        tabindex="-1"
        onkeydown={(event) => { if (event.key === "Escape" && busy === "") rollbackCandidate = null; }}
      >
        <h3 id="rollback-dialog-title">{t("backups.rollbackTitle")}</h3>
        <div class="m-body">
          <p>{t("backups.rollbackBody").replace("{time}", formatTime(rollbackCandidate.createdAtUnixSeconds)).replace("{name}", rollbackCandidate.instanceName)}</p>
          <p class="dim" style="margin-top:8px">{t("backups.rollbackNote")}</p>
        </div>
        <div class="m-acts">
          <button class="btn secondary" data-dialog-autofocus disabled={busy !== ""} onclick={() => { rollbackCandidate = null; }}>{t("common.cancel")}</button>
          <button class="btn primary" disabled={busy !== ""} onclick={() => void confirmRollback()}>{busy !== "" ? t("backups.rollbackRunning") : t("backups.rollbackConfirm")}</button>
        </div>
      </div>
    </div>
  {/if}
</AppShell>

<style>
  .page-title {
    font-size: 17px;
  }
  .empty-stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 80px 0;
  }
  .tl-item {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .tl-item:first-of-type {
    border-top: none;
  }
  .tl-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    flex: none;
  }
  .tl-dot.failed {
    background: var(--danger);
  }
</style>
