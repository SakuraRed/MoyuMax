<script lang="ts">
  import { t } from "../i18n.svelte";
  import type {
    CrashEvidenceKind,
    CrashReport,
    DiagnosticExportPreview,
    DiagnosticExportResult,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import { pushToast } from "../toast.svelte";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    report: CrashReport;
    instance: ManagedInstance | null;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    report,
    instance,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let preview = $state<DiagnosticExportPreview | null>(null);
  let exportResult = $state<DiagnosticExportResult | null>(null);
  let previewing = $state(false);
  let exporting = $state(false);
  let retrying = $state(false);

  function evidenceLabel(kind: CrashEvidenceKind): string {
    const keys: Record<CrashEvidenceKind, string> = {
      gameOutput: "crash.evidence.kind.gameOutput",
      gameLog: "crash.evidence.kind.gameLog",
      gameCrashReport: "crash.evidence.kind.gameCrashReport",
      nativeCrash: "crash.evidence.kind.nativeCrash",
      launcherLog: "crash.evidence.kind.launcherLog",
      launchScript: "crash.evidence.kind.launchScript",
      environment: "crash.evidence.kind.environment",
    };
    return t(keys[kind]);
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  }

  async function loadPreview(): Promise<void> {
    previewing = true;
    exportResult = null;
    try {
      preview = await runtime.previewDiagnosticExport(report.id);
      pushToast({ tone: "info", title: t("crash.msg.previewReady") });
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
    } finally {
      previewing = false;
    }
  }

  async function exportBundle(): Promise<void> {
    if (!preview) return;
    exporting = true;
    try {
      exportResult = await runtime.confirmDiagnosticExport(preview.id);
      preview = null;
      pushToast({ tone: "ok", title: t("crash.export.resultTitle") });
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
    } finally {
      exporting = false;
    }
  }

  async function retryInstance(): Promise<void> {
    if (!instance) return;
    retrying = true;
    try {
      await runtime.startInstance(instance.id);
      pushToast({ tone: "ok", title: t("crash.msg.restarting").replace("{name}", instance.name) });
    } catch (error) {
      pushToast({ tone: "danger", title: error instanceof Error ? error.message : String(error) });
    } finally {
      retrying = false;
    }
  }
</script>

<AppShell
  pageTitle={t("crash.title")}
  activeNavigation="home"
  onBack={() => onNavigate("home")}
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content">
    <h1 style="font-size:17px;margin-bottom:6px">{t("crash.title")}</h1>
    <p class="muted" style="margin-bottom:16px">{instance ? t("crash.heading.instanceExit").replace("{name}", instance.name) : t("crash.heading.localExit")}</p>

    <section class="panel pad" style="margin-bottom:16px" aria-labelledby="crash-summary-title">
      <div class="row" style="gap:12px;align-items:flex-start">
        <span class="tag danger" style="flex:none;margin-top:2px"><span class="cdot"></span>{t("crash.kicker")}</span>
        <div>
          <h2 class="panel-title" id="crash-summary-title">{report.title}</h2>
          <div class="panel-desc" style="margin-top:4px">{report.summary}</div>
        </div>
      </div>
    </section>

    <div class="crash-grid">
      <section class="panel pad" aria-labelledby="crash-actions-title">
        <h2 class="panel-title" id="crash-actions-title">{t("crash.actions.title")}</h2>
        <ol class="crash-recommendations">
          {#each report.recommendations as recommendation}
            <li>{recommendation}</li>
          {/each}
        </ol>
        <div class="row" style="margin-top:14px">
          <button class="btn secondary" disabled={!instance || retrying} onclick={() => void retryInstance()}>
            {retrying ? t("crash.actions.retrying") : t("crash.actions.retry")}
          </button>
          <span class="dim">{t("crash.actions.noTouch")}</span>
        </div>
      </section>

      <section class="panel pad" aria-labelledby="crash-evidence-title">
        <h2 class="panel-title" id="crash-evidence-title">{t("crash.evidence.title")}</h2>
        <div style="margin-top:8px">
          {#each report.evidence as evidence}
            <div class="evidence-row">
              <div class="lr-main">
                <div class="lr-name">{evidenceLabel(evidence.kind)}</div>
                <div class="lr-sub mono">{evidence.bundleName}</div>
              </div>
              <span class="dim">{formatBytes(evidence.includedBytes)}{evidence.truncated ? t("crash.evidence.truncatedSuffix") : ""}</span>
            </div>
          {/each}
        </div>
      </section>
    </div>

    <section class="panel pad" style="margin-top:16px" aria-labelledby="diagnostic-export-title">
      <div class="row spread">
        <div>
          <h2 class="panel-title" id="diagnostic-export-title">{t("crash.export.title")}</h2>
          <div class="panel-desc" style="margin-top:4px">{t("crash.export.description")}</div>
        </div>
        {#if !preview && !exportResult}
          <button class="btn primary" disabled={previewing} onclick={() => void loadPreview()}>
            {previewing ? t("crash.export.previewing") : t("crash.export.preview")}
          </button>
        {/if}
      </div>

      {#if preview}
        <div class="preview-block">
          <h2 class="panel-title" style="font-size:13.5px">{t("crash.export.privacyTitle")}</h2>
          <p class="muted" style="margin-top:6px">{t("crash.export.summary").replace("{count}", String(preview.files.length)).replace("{size}", formatBytes(preview.totalBytes)).replace("{max}", formatBytes(preview.maximumEvidenceBytes))}</p>
          <div class="preview-grid">
            <div>
              <h3 class="preview-sub">{t("crash.export.filesTitle")}</h3>
              <ul class="preview-list">
                {#each preview.files as file}
                  <li><code>{file.bundleName}</code><span class="dim">{formatBytes(file.includedBytes)}</span></li>
                {/each}
              </ul>
            </div>
            <div>
              <h3 class="preview-sub">{t("crash.export.redactionsTitle")}</h3>
              <ul class="preview-list">
                {#each preview.redactions as redaction}
                  <li>{redaction}</li>
                {/each}
              </ul>
            </div>
          </div>
          <div class="m-acts">
            <button class="btn secondary" disabled={exporting} onclick={() => preview = null}>{t("common.cancel")}</button>
            <button class="btn primary" disabled={exporting} onclick={() => void exportBundle()}>
              {exporting ? t("crash.export.writing") : t("crash.export.confirm")}
            </button>
          </div>
        </div>
      {:else if exportResult}
        <div class="banner info" role="status" style="margin-top:14px">
          <Icon name="check" size={16} />
          <div>
            <strong>{t("crash.export.resultTitle")}</strong>
            <div class="mono" style="margin-top:2px">{exportResult.archivePath}</div>
            <div class="dim" style="margin-top:2px">{t("crash.export.resultLine").replace("{count}", String(exportResult.fileCount)).replace("{size}", formatBytes(exportResult.archiveBytes))}</div>
          </div>
        </div>
      {/if}
    </section>
  </main>
</AppShell>

<style>
  .crash-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(320px, 100%), 1fr));
    gap: 16px;
  }
  .crash-recommendations {
    margin: 10px 0 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text-2);
    font-size: 13px;
  }
  .evidence-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .evidence-row:first-child {
    border-top: none;
  }
  .preview-block {
    margin-top: 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.07);
    padding-top: 14px;
  }
  .preview-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(280px, 100%), 1fr));
    gap: 16px;
    margin-top: 12px;
  }
  .preview-sub {
    font-size: 12.5px;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  .preview-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .preview-list code {
    font-family: var(--mono);
    font-size: 11.5px;
    margin-right: 8px;
    overflow-wrap: anywhere;
  }
  .lr-sub.mono {
    overflow-wrap: anywhere;
    white-space: normal;
  }
</style>
