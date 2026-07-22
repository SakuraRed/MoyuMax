<script lang="ts">
  import { onMount, tick } from "svelte";

  import {
    defaultInstanceName,
    formatBytes,
    installStageLabel,
    recommendedFabricLoader,
    recommendedVersion,
  } from "../installation";
  import type {
    FabricLoaderSummary,
    GameVersionSummary,
    InstallPreview,
    InstallTask,
    LoaderChoice,
    MoyuRuntime,
    OnboardingSelection,
    VersionCatalog,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onBack: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  type InstallView = "loading" | "configure" | "previewing" | "confirm" | "queueing" | "queued";

  let {
    runtime,
    settings,
    onBack,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let view = $state<InstallView>("loading");
  let catalog = $state<VersionCatalog | null>(null);
  let selectedVersion = $state<GameVersionSummary | null>(null);
  let fabricLoaders = $state<FabricLoaderSummary[]>([]);
  let loader = $state<LoaderChoice>({ kind: "vanilla" });
  let instanceName = $state("");
  let nameEdited = $state(false);
  let preview = $state<InstallPreview | null>(null);
  let task = $state<InstallTask | null>(null);
  let errorMessage = $state("");
  let loaderMessage = $state("");
  let showOlderVersions = $state(false);
  let pageRoot: HTMLElement | undefined = $state();
  let loaderRequestSequence = 0;

  const visibleVersions = $derived(
    (catalog?.versions ?? [])
      .filter((version) => version.releaseType === "release")
      .slice(0, showOlderVersions ? 24 : 3),
  );

  $effect(() => {
    view;
    void tick().then(() => {
      pageRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });
  });

  onMount(() => {
    void loadCatalog();
  });

  async function loadCatalog(): Promise<void> {
    view = "loading";
    errorMessage = "";
    try {
      catalog = await runtime.getGameVersionCatalog();
      const recommended = recommendedVersion(catalog.versions);
      if (!recommended) throw new Error("官方版本目录没有可安装版本");
      selectedVersion = recommended;
      await loadFabric(recommended, true);
      view = "configure";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function selectVersion(version: GameVersionSummary): Promise<void> {
    selectedVersion = version;
    loader = { kind: "vanilla" };
    fabricLoaders = [];
    updateGeneratedName();
    await loadFabric(version, false);
  }

  async function loadFabric(version: GameVersionSummary, selectRecommended: boolean): Promise<void> {
    const requestSequence = ++loaderRequestSequence;
    loaderMessage = "正在查询兼容的 Fabric Loader…";
    try {
      const compatibleLoaders = await runtime.getFabricLoaders(version.id);
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      fabricLoaders = compatibleLoaders;
      const recommended = recommendedFabricLoader(fabricLoaders);
      if (recommended && selectRecommended) {
        loader = { kind: "fabric", version: recommended.version };
      }
      loaderMessage = fabricLoaders.length === 0 ? "该版本没有可用的 Fabric Loader" : "";
      updateGeneratedName();
    } catch (error) {
      if (requestSequence !== loaderRequestSequence || selectedVersion?.id !== version.id) return;
      loader = { kind: "vanilla" };
      fabricLoaders = [];
      loaderMessage = `Fabric 元数据暂不可用，仍可安装原版：${error instanceof Error ? error.message : String(error)}`;
      updateGeneratedName();
    }
  }

  function selectVanilla(): void {
    loader = { kind: "vanilla" };
    updateGeneratedName();
  }

  function selectFabric(): void {
    const recommended = recommendedFabricLoader(fabricLoaders);
    if (!recommended) return;
    loader = { kind: "fabric", version: recommended.version };
    updateGeneratedName();
  }

  function selectFabricVersion(version: string): void {
    loader = { kind: "fabric", version };
    updateGeneratedName();
  }

  function updateGeneratedName(): void {
    if (!nameEdited && selectedVersion) {
      instanceName = defaultInstanceName(selectedVersion.id, loader);
    }
  }

  async function createPreview(): Promise<void> {
    if (!selectedVersion || instanceName.trim() === "") return;
    view = "previewing";
    errorMessage = "";
    try {
      preview = await runtime.previewInstall({
        instanceName: instanceName.trim(),
        gameVersion: selectedVersion,
        loader,
        isolation: "full",
      });
      view = "confirm";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "configure";
    }
  }

  async function confirmInstall(): Promise<void> {
    if (!preview) return;
    view = "queueing";
    errorMessage = "";
    try {
      task = await runtime.confirmInstallPreview(preview.id);
      view = "queued";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
      view = "confirm";
    }
  }

  function returnToConfiguration(): void {
    preview = null;
    view = "configure";
  }

  function releaseDescription(version: GameVersionSummary): string {
    const date = new Date(version.releaseTime);
    const releaseDate = Number.isNaN(date.valueOf())
      ? "发布日期未知"
      : new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium" }).format(date);
    return `${releaseDate} 发布${version.recommended ? " · 当前官方稳定版" : ""}`;
  }
</script>

<AppShell
  pageTitle={view === "confirm" || view === "queueing" ? "确认安装" : view === "queued" ? "安装任务" : "新建实例"}
  dataDirectory={settings.dataDirectory}
  activeNavigation={view === "queued" ? "tasks" : "instances"}
  connectionStatus={catalog?.source === "cache" ? "离线模式 · 使用版本目录缓存" : "官方元数据 · 按需连接"}
  taskStatus={task ? "1 个安装任务已排队" : "无活动任务"}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content install-content" bind:this={pageRoot}>
    {#if view === "loading"}
      <section class="install-loading" aria-live="polite">
        <div class="loading-line wide"></div>
        <div class="loading-line"></div>
        <strong>正在读取官方版本目录…</strong>
        <span>只在进入安装页面时联网，不影响 MoyuMax 首屏与本地实例。</span>
      </section>
    {:else if view === "configure"}
      <div class="install-scroll">
        <header class="install-heading">
          <button class="button ghost compact" aria-label="返回首页" onclick={onBack}>返回</button>
          <div>
            <h1>安装第一个游戏</h1>
            <p>推荐项已经自动选择；不需要自行安装 Java 或修改环境变量。</p>
          </div>
        </header>

        {#if catalog?.source === "cache"}
          <div class="info-banner" role="status">
            <Icon name="info" size={16} />
            <span>官方版本服务当前不可用，正在使用最近一次成功缓存。创建任务前仍会验证所选版本详情。</span>
          </div>
        {/if}
        {#if errorMessage}
          <div class="error-block" role="alert">
            <strong>无法准备安装信息</strong>
            <span>尚未创建任务，也没有修改实例或共享文件。</span>
            <span>{errorMessage}</span>
            <button class="button ghost compact" onclick={() => void loadCatalog()}>重新加载</button>
          </div>
        {/if}

        {#if selectedVersion}
          <section class="install-section" aria-labelledby="game-version-heading">
            <div class="section-number">1</div>
            <div class="section-content">
              <h2 id="game-version-heading">Minecraft 版本</h2>
              <div class="install-choice-list" role="radiogroup" aria-label="Minecraft 版本">
                {#each visibleVersions as version, index}
                  <button
                    class:selected={selectedVersion.id === version.id}
                    class="install-choice-row"
                    role="radio"
                    aria-checked={selectedVersion.id === version.id}
                    data-autofocus={index === 0 ? "true" : undefined}
                    onclick={() => void selectVersion(version)}
                  >
                    <span class="radio-mark"></span>
                    <span class="choice-copy">
                      <strong>{version.id}{#if version.recommended}<em>推荐稳定版</em>{/if}</strong>
                      <small>{releaseDescription(version)}</small>
                    </span>
                  </button>
                {/each}
              </div>
              <button class="inline-link version-toggle" onclick={() => showOlderVersions = !showOlderVersions}>
                {showOlderVersions ? "收起旧版本" : "展开更多稳定版"}
              </button>
            </div>
          </section>

          <section class="install-section" aria-labelledby="loader-heading">
            <div class="section-number">2</div>
            <div class="section-content">
              <h2 id="loader-heading">加载器（可选）</h2>
              <div class="loader-grid" role="radiogroup" aria-label="加载器">
                <button class:selected={loader.kind === "vanilla"} class="loader-card" role="radio" aria-checked={loader.kind === "vanilla"} onclick={selectVanilla}>
                  <span class="radio-mark"></span><strong>不安装</strong><small>原版</small>
                </button>
                <button class:selected={loader.kind === "fabric"} class="loader-card" role="radio" aria-checked={loader.kind === "fabric"} disabled={fabricLoaders.length === 0} onclick={selectFabric}>
                  <span class="radio-mark"></span><strong>Fabric</strong><small>{recommendedFabricLoader(fabricLoaders)?.version ?? "不可用"} · 推荐</small>
                </button>
                {#each ["Forge", "NeoForge", "Quilt"] as pendingLoader}
                  <button class="loader-card pending" disabled><strong>{pendingLoader}</strong><small>接口接入中</small></button>
                {/each}
              </div>
              {#if loader.kind === "fabric"}
                <label class="loader-version-field">
                  Fabric Loader 版本
                  <select value={loader.version} onchange={(event) => selectFabricVersion(event.currentTarget.value)}>
                    {#each fabricLoaders as candidate}
                      <option value={candidate.version}>{candidate.version}{candidate.recommended ? "（推荐）" : ""}</option>
                    {/each}
                  </select>
                  <small>列表仅包含 Fabric 元数据服务为 Minecraft {selectedVersion.id} 返回的兼容版本。</small>
                </label>
              {/if}
              {#if loaderMessage}<p class="hint">{loaderMessage}</p>{/if}
            </div>
          </section>

          <section class="install-section" aria-labelledby="instance-name-heading">
            <div class="section-number">3</div>
            <div class="section-content">
              <h2 id="instance-name-heading">名称与位置</h2>
              <div class="install-form-card">
                <label>
                  实例名称
                  <input
                    value={instanceName}
                    maxlength="120"
                    oninput={(event) => {
                      nameEdited = true;
                      instanceName = event.currentTarget.value;
                    }}
                  />
                  <small>已根据版本与加载器自动填写，可以修改。</small>
                </label>
                <div class="managed-location">
                  <span>托管位置</span>
                  <code>{settings.dataDirectory}\instances\&lt;实例 ID&gt;</code>
                  <small>实例可变内容完全隔离；可校验重建的基础文件由全局存储共享。</small>
                </div>
              </div>
            </div>
          </section>

          <footer class="install-actions">
            <button class="button primary large" disabled={instanceName.trim() === ""} onclick={() => void createPreview()}>
              查看安装信息 <Icon name="arrow-right" size={14} />
            </button>
            <p>将解析准确空间和 Azul Zulu 完整构建；确认前不会创建任务或写入实例。</p>
          </footer>
        {/if}
      </div>
    {:else if view === "previewing"}
      <section class="install-loading" aria-live="polite">
        <div class="loading-line wide"></div>
        <div class="loading-line"></div>
        <strong>正在生成可校验安装计划…</strong>
        <span>正在解析官方游戏文件、兼容加载器与托管 Java 完整构建。</span>
      </section>
    {:else if (view === "confirm" || view === "queueing") && preview}
      <div class="install-scroll confirm-layout">
        <header class="install-heading">
          <button class="button ghost compact" disabled={view === "queueing"} onclick={returnToConfiguration}>返回修改</button>
          <div><h1>确认安装信息</h1><p>以下内容会作为版本化计划快照写入持久化任务队列。</p></div>
        </header>
        {#if errorMessage}
          <div class="error-block" role="alert"><strong>任务尚未创建</strong><span>{errorMessage}</span></div>
        {/if}
        <dl class="install-summary">
          <div><dt>Minecraft 版本</dt><dd>{preview.gameVersion}</dd><span>官方元数据</span></div>
          <div><dt>加载器</dt><dd>{preview.loaderName}{preview.loaderVersion ? ` ${preview.loaderVersion}` : ""}</dd><span>兼容项</span></div>
          <div><dt>Java</dt><dd>Azul Zulu {preview.javaVersion} · {preview.javaArchitecture}</dd><span>托管，不影响系统 Java</span></div>
          <div><dt>实例隔离</dt><dd>完全隔离（推荐）</dd><span>共享可重建基础文件</span></div>
          <div><dt>预计下载</dt><dd>{formatBytes(preview.estimatedDownloadBytes)}</dd><span>以解析清单为准</span></div>
        </dl>
        <div class="stage-preview">
          <h2>任务阶段</h2>
          <ol>
            {#each ["prepare", "downloadGameFiles", "verifyFiles", "installGameEnvironment", "applyLoader", "commitChanges", "createRollbackPoint"] as stage}
              <li>{installStageLabel(stage as import("../runtime").InstallStage)}</li>
            {/each}
          </ol>
        </div>
        <footer class="install-actions confirm-actions">
          <button class="button primary large" data-autofocus="true" disabled={view === "queueing"} onclick={() => void confirmInstall()}>
            {view === "queueing" ? "正在创建任务…" : "开始安装"}
          </button>
          <p>任务会先写入独立暂存区；校验与提交完成前不会出现可启动实例。</p>
        </footer>
      </div>
    {:else if view === "queued" && task}
      <section class="queued-result" aria-live="polite">
        <span class="done-mark"><Icon name="check" size={18} /></span>
        <h1>安装任务已进入队列</h1>
        <p>计划已经持久化。当前构建尚未接入文件执行器，因此不会显示虚假下载进度。</p>
        <div class="queued-task-card">
          <div><strong>{task.plan.instanceName}</strong><span>等待执行器</span></div>
          <ol>
            {#each task.plan.stages as stage, index}
              <li class:current={index === 0}><span>{index + 1}</span><b>{installStageLabel(stage)}</b></li>
            {/each}
          </ol>
          <small>暂存区：<code>{task.stagingDirectory}</code></small>
        </div>
        <button class="button primary" data-autofocus="true" onclick={onBack}>返回首页</button>
      </section>
    {/if}
  </main>
</AppShell>
