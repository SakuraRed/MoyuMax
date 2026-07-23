<script lang="ts">
  import { t } from "../i18n.svelte";
  import type {
    CrashEvidenceKind,
    CrashReport,
    DiagnosticExportPreview,
    DiagnosticExportResult,
    ManagedInstance,
    MoyuRuntime,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    report: CrashReport;
    instance: ManagedInstance | null;
    onBack: () => void;
    onOpenResources: () => void;
    onOpenTasks: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    report,
    instance,
    onBack,
    onOpenResources,
    onOpenTasks,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let preview = $state<DiagnosticExportPreview | null>(null);
  let exportResult = $state<DiagnosticExportResult | null>(null);
  let previewing = $state(false);
  let exporting = $state(false);
  let retrying = $state(false);
  let errorMessage = $state("");
  let statusMessage = $state("");

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
    errorMessage = "";
    statusMessage = "";
    exportResult = null;
    try {
      preview = await runtime.previewDiagnosticExport(report.id);
      statusMessage = t("crash.msg.previewReady");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      previewing = false;
    }
  }

  async function exportBundle(): Promise<void> {
    if (!preview) return;
    exporting = true;
    errorMessage = "";
    statusMessage = "";
    try {
      exportResult = await runtime.confirmDiagnosticExport(preview.id);
      preview = null;
      statusMessage = t("crash.export.resultTitle");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      exporting = false;
    }
  }

  async function retryInstance(): Promise<void> {
    if (!instance) return;
    retrying = true;
    errorMessage = "";
    statusMessage = "";
    try {
      await runtime.startInstance(instance.id);
      statusMessage = t("crash.msg.restarting").replace("{name}", instance.name);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      retrying = false;
    }
  }
</script>

<AppShell
  pageTitle={t("crash.title")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="home"
  navigationTargets={["home", "resources", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus={t("crash.connectionStatus")}
  taskStatus={t("crash.taskStatus")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content crash-content">
    <div class="crash-scroll" data-scroll-region="main">
      <header class="crash-heading">
        <button class="back-link" onclick={onBack}>{t("settings.back")}</button>
        <div>
          <h1>{t("crash.title")}</h1>
          <p>{instance ? t("crash.heading.instanceExit").replace("{name}", instance.name) : t("crash.heading.localExit")}{t("crash.heading.evidenceNote")}</p>
        </div>
      </header>

      <section class="crash-summary-panel" aria-labelledby="crash-summary-title">
        <div class="crash-summary-icon" aria-hidden="true"><Icon name="info" size={20} /></div>
        <div>
          <span class="crash-kicker">{t("crash.kicker")}</span>
          <h2 id="crash-summary-title">{report.title}</h2>
          <p>{report.summary}</p>
        </div>
      </section>

      <div class="crash-grid">
        <section class="crash-panel" aria-labelledby="crash-actions-title">
          <header>
            <h2 id="crash-actions-title">{t("crash.actions.title")}</h2>
            <p>{t("crash.actions.description")}</p>
          </header>
          <ol class="crash-recommendations">
            {#each report.recommendations as recommendation}
              <li>{recommendation}</li>
            {/each}
          </ol>
          <div class="crash-action-row">
            <button class="button" disabled={!instance || retrying} onclick={() => void retryInstance()}>
              {retrying ? t("crash.actions.retrying") : t("crash.actions.retry")}
            </button>
            <span>{t("crash.actions.noTouch")}</span>
          </div>
        </section>

        <section class="crash-panel" aria-labelledby="crash-evidence-title">
          <header>
            <h2 id="crash-evidence-title">{t("crash.evidence.title")}</h2>
            <p>{t("crash.evidence.description")}</p>
          </header>
          <div class="crash-evidence-list">
            {#each report.evidence as evidence}
              <div class="crash-evidence-row">
                <div>
                  <strong>{evidenceLabel(evidence.kind)}</strong>
                  <code>{evidence.bundleName}</code>
                </div>
                <span>{formatBytes(evidence.includedBytes)}{evidence.truncated ? t("crash.evidence.truncatedSuffix") : ""}</span>
              </div>
            {/each}
          </div>
        </section>
      </div>

      <section class="diagnostic-export-panel" aria-labelledby="diagnostic-export-title">
        <header>
          <div>
            <h2 id="diagnostic-export-title">{t("crash.export.title")}</h2>
            <p>{t("crash.export.description")}</p>
          </div>
          {#if !preview && !exportResult}
            <button class="button primary" disabled={previewing} onclick={() => void loadPreview()}>
              {previewing ? t("crash.export.previewing") : t("crash.export.preview")}
            </button>
          {/if}
        </header>

        {#if preview}
          <div class="diagnostic-preview">
            <h2>{t("crash.export.privacyTitle")}</h2>
            <p>{t("crash.export.summary").replace("{count}", String(preview.files.length)).replace("{size}", formatBytes(preview.totalBytes)).replace("{max}", formatBytes(preview.maximumEvidenceBytes))}</p>
            <div class="diagnostic-preview-grid">
              <div>
                <h3>{t("crash.export.filesTitle")}</h3>
                <ul class="diagnostic-file-list">
                  {#each preview.files as file}
                    <li><code>{file.bundleName}</code><span>{formatBytes(file.includedBytes)}</span></li>
                  {/each}
                </ul>
              </div>
              <div>
                <h3>{t("crash.export.redactionsTitle")}</h3>
                <ul class="diagnostic-redaction-list">
                  {#each preview.redactions as redaction}
                    <li>{redaction}</li>
                  {/each}
                </ul>
              </div>
            </div>
            <div class="diagnostic-confirm-row">
              <button class="button" disabled={exporting} onclick={() => preview = null}>{t("common.cancel")}</button>
              <button class="button primary" disabled={exporting} onclick={() => void exportBundle()}>
                {exporting ? t("crash.export.writing") : t("crash.export.confirm")}
              </button>
            </div>
          </div>
        {:else if exportResult}
          <div class="diagnostic-export-result" role="status">
            <Icon name="check" size={18} />
            <div>
              <strong>{t("crash.export.resultTitle")}</strong>
              <code>{exportResult.archivePath}</code>
              <span>{t("crash.export.resultLine").replace("{count}", String(exportResult.fileCount)).replace("{size}", formatBytes(exportResult.archiveBytes))}</span>
            </div>
          </div>
        {/if}
      </section>
    </div>
  </main>

  {#if errorMessage}
    <div class="toast danger-toast" role="alert"><Icon name="info" size={16} /><span>{errorMessage}</span></div>
  {:else if statusMessage}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{statusMessage}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{errorMessage || statusMessage}</div>
</AppShell>
