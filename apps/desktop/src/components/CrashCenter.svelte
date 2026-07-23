<script lang="ts">
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
    const labels: Record<CrashEvidenceKind, string> = {
      gameOutput: "游戏最后输出",
      gameLog: "游戏日志",
      gameCrashReport: "Minecraft 崩溃报告",
      nativeCrash: "原生崩溃文本",
      launcherLog: "MoyuMax 会话日志",
      launchScript: "脱敏启动脚本",
      environment: "环境摘要",
    };
    return labels[kind];
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
      statusMessage = "已生成本地导出清单，尚未写出 ZIP。";
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
      statusMessage = "诊断包已保存在本地";
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
      statusMessage = `正在重新启动「${instance.name}」`;
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      retrying = false;
    }
  }
</script>

<AppShell
  pageTitle="崩溃诊断"
  dataDirectory={settings.dataDirectory}
  activeNavigation="home"
  navigationTargets={["home", "resources", "tasks"]}
  onNavigate={(target) => target === "home" ? onBack() : target === "resources" ? onOpenResources() : target === "tasks" ? onOpenTasks() : undefined}
  connectionStatus="完全本地诊断 · 未上传任何内容"
  taskStatus="诊断报告已持久化"
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content crash-content">
    <div class="crash-scroll" data-scroll-region="main">
      <header class="crash-heading">
        <button class="back-link" onclick={onBack}>返回首页</button>
        <div>
          <h1>崩溃诊断</h1>
          <p>{instance ? `实例「${instance.name}」异常退出` : "本地游戏会话异常退出"}。以下结论只来自本机证据。</p>
        </div>
      </header>

      <section class="crash-summary-panel" aria-labelledby="crash-summary-title">
        <div class="crash-summary-icon" aria-hidden="true"><Icon name="info" size={20} /></div>
        <div>
          <span class="crash-kicker">可能原因</span>
          <h2 id="crash-summary-title">{report.title}</h2>
          <p>{report.summary}</p>
        </div>
      </section>

      <div class="crash-grid">
        <section class="crash-panel" aria-labelledby="crash-actions-title">
          <header>
            <h2 id="crash-actions-title">建议操作</h2>
            <p>这些建议不会自动修改实例。</p>
          </header>
          <ol class="crash-recommendations">
            {#each report.recommendations as recommendation}
              <li>{recommendation}</li>
            {/each}
          </ol>
          <div class="crash-action-row">
            <button class="button" disabled={!instance || retrying} onclick={() => void retryInstance()}>
              {retrying ? "正在重新启动" : "再次启动实例"}
            </button>
            <span>未经授权，MoyuMax 不会修改配置、删除文件或安装模组。</span>
          </div>
        </section>

        <section class="crash-panel" aria-labelledby="crash-evidence-title">
          <header>
            <h2 id="crash-evidence-title">已收集证据</h2>
            <p>这里只展示包内名称，不展示原始本地路径。</p>
          </header>
          <div class="crash-evidence-list">
            {#each report.evidence as evidence}
              <div class="crash-evidence-row">
                <div>
                  <strong>{evidenceLabel(evidence.kind)}</strong>
                  <code>{evidence.bundleName}</code>
                </div>
                <span>{formatBytes(evidence.includedBytes)}{evidence.truncated ? " · 已截取末尾" : ""}</span>
              </div>
            {/each}
          </div>
        </section>
      </div>

      <section class="diagnostic-export-panel" aria-labelledby="diagnostic-export-title">
        <header>
          <div>
            <h2 id="diagnostic-export-title">导出脱敏诊断包</h2>
            <p>先预览文件清单和脱敏范围，再由你确认写入本地 ZIP。MoyuMax 不会自动上传。</p>
          </div>
          {#if !preview && !exportResult}
            <button class="button primary" disabled={previewing} onclick={() => void loadPreview()}>
              {previewing ? "正在生成清单" : "预览诊断包"}
            </button>
          {/if}
        </header>

        {#if preview}
          <div class="diagnostic-preview">
            <h2>导出前隐私检查</h2>
            <p>预计包含 {preview.files.length} 个文件，共 {formatBytes(preview.totalBytes)}；单项文本最多保留 {formatBytes(preview.maximumEvidenceBytes)}。</p>
            <div class="diagnostic-preview-grid">
              <div>
                <h3>文件清单</h3>
                <ul class="diagnostic-file-list">
                  {#each preview.files as file}
                    <li><code>{file.bundleName}</code><span>{formatBytes(file.includedBytes)}</span></li>
                  {/each}
                </ul>
              </div>
              <div>
                <h3>脱敏摘要</h3>
                <ul class="diagnostic-redaction-list">
                  {#each preview.redactions as redaction}
                    <li>{redaction}</li>
                  {/each}
                </ul>
              </div>
            </div>
            <div class="diagnostic-confirm-row">
              <button class="button" disabled={exporting} onclick={() => preview = null}>取消</button>
              <button class="button primary" disabled={exporting} onclick={() => void exportBundle()}>
                {exporting ? "正在写入本地 ZIP" : "确认并导出到本地"}
              </button>
            </div>
          </div>
        {:else if exportResult}
          <div class="diagnostic-export-result" role="status">
            <Icon name="check" size={18} />
            <div>
              <strong>诊断包已保存在本地</strong>
              <code>{exportResult.archivePath}</code>
              <span>{exportResult.fileCount} 个文件 · {formatBytes(exportResult.archiveBytes)} · 未上传</span>
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
