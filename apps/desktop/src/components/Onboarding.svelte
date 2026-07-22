<script lang="ts">
  import { tick, untrack } from "svelte";

  import {
    buildSelection,
    createOnboardingState,
    goBack,
    goForward,
    setOnboardingError,
    setSubmitting,
    updateOnboardingDraft,
  } from "../onboarding";
  import type { OnboardingState } from "../onboarding";
  import type { BootstrapState, Language, OnboardingSelection } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    bootstrap: BootstrapState;
    onPersist: (selection: OnboardingSelection) => Promise<void>;
    onSkip: () => Promise<void>;
    onStart: (selection: OnboardingSelection) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    bootstrap,
    onPersist,
    onSkip,
    onStart,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  let flow: OnboardingState = $state(
    untrack(() => createOnboardingState(bootstrap.defaults)),
  );
  let usesCustomDataDirectory = $state(false);
  let wizardRoot: HTMLElement | undefined = $state();

  const languages: ReadonlyArray<{
    value: Language;
    title: string;
    subtitle: string;
  }> = [
    { value: "zh-CN", title: "简体中文", subtitle: "检测到系统语言：中文（简体，中国）" },
    { value: "zh-TW", title: "繁體中文", subtitle: "介面語言：繁體中文" },
    { value: "en", title: "English", subtitle: "Interface language: English" },
  ];

  const stepNames = ["语言", "数据位置", "隐私"] as const;

  $effect(() => {
    flow.step;
    void tick().then(() => {
      wizardRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });
  });

  function chooseLanguage(language: Language): void {
    flow = updateOnboardingDraft(flow, { language });
  }

  function useDefaultDataDirectory(): void {
    usesCustomDataDirectory = false;
    flow = updateOnboardingDraft(flow, {
      dataDirectory: bootstrap.defaultDataDirectory,
    });
  }

  function useCustomDataDirectory(): void {
    usesCustomDataDirectory = true;
    if (flow.draft.dataDirectory === bootstrap.defaultDataDirectory) {
      flow = updateOnboardingDraft(flow, { dataDirectory: "" });
    }
  }

  function updateDataDirectory(value: string): void {
    flow = updateOnboardingDraft(flow, { dataDirectory: value });
  }

  function next(): void {
    flow = goForward(flow);
  }

  function previous(): void {
    flow = goBack(flow);
  }

  async function complete(): Promise<void> {
    flow = setSubmitting(flow, true);
    try {
      await onPersist(buildSelection(flow));
      flow = goForward(setSubmitting(flow, false));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      flow = { ...setOnboardingError(flow, message), step: "data" };
      usesCustomDataDirectory = true;
    }
  }

  async function skip(): Promise<void> {
    flow = setSubmitting(flow, true);
    try {
      await onSkip();
      flow = {
        ...flow,
        draft: { ...bootstrap.defaults },
        step: "done",
        isSubmitting: false,
      };
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      flow = setOnboardingError(flow, message);
    }
  }

  function languageLabel(language: Language): string {
    return languages.find((candidate) => candidate.value === language)?.title ?? language;
  }
</script>

<AppShell
  pageTitle="首次运行"
  titleSuffix="首次运行引导"
  dataDirectory={flow.draft.dataDirectory || bootstrap.defaultDataDirectory}
  navigationDisabled
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content wizard-content" bind:this={wizardRoot}>
    <section class="wizard-card" aria-label="首次运行设置">
      <ol class="steps" aria-label="首次运行进度">
        {#each stepNames as name, index}
          {@const stepNumber = index + 1}
          {@const completeStep = stepNumber < (["language", "data", "privacy", "done"].indexOf(flow.step) + 1)}
          {@const currentStep = stepNumber === (["language", "data", "privacy"].indexOf(flow.step) + 1)}
          <li class:done={completeStep} class:current={currentStep} aria-current={currentStep ? "step" : undefined}>
            <span>{#if completeStep}<Icon name="check" size={12} />{:else}{stepNumber}{/if}</span>{name}
          </li>
          {#if index < stepNames.length - 1}<i aria-hidden="true"></i>{/if}
        {/each}
      </ol>

      {#if flow.step === "language"}
        <div class="wizard-body">
          <h1>欢迎使用 MoyuMax</h1>
          <p class="muted intro">只需三步完成初始设置，其余能力会在你首次使用时按需介绍，不会一次性铺开。</p>
          <fieldset class="choice-section">
            <legend>选择界面语言</legend>
            <div class="choice-group">
              {#each languages as language, index}
                <label class:selected={flow.draft.language === language.value} class="choice">
                  <input
                    type="radio"
                    name="language"
                    value={language.value}
                    checked={flow.draft.language === language.value}
                    data-autofocus={index === 0 ? "true" : undefined}
                    onchange={() => chooseLanguage(language.value)}
                  />
                  <span class="radio-mark"></span>
                  <span class="choice-copy">
                    <strong>
                      {language.title}{#if language.value === "zh-CN"}<em>跟随系统 · 默认</em>{/if}
                    </strong>
                    <small>{language.subtitle}</small>
                  </span>
                </label>
              {/each}
            </div>
          </fieldset>
          <p class="hint">无法匹配系统语言时使用简体中文。安全关键文案与风险提示始终由内置受信资源提供，不受社区语言包影响。</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>跳过引导，使用默认</button></span>
          <span><button class="button primary" onclick={next}>下一步 <Icon name="arrow-right" size={14} /></button></span>
        </footer>
      {:else if flow.step === "data"}
        <div class="wizard-body">
          <h1>数据位置</h1>
          <p class="muted intro">实例、共享游戏文件、Java 环境和索引数据库都保存在这里。之后可以随时一键分离或迁移，不必现在纠结。</p>
          {#if flow.errorMessage}
            <div class="error-block" role="alert">
              <strong>数据位置未保存</strong>
              <span>首次运行状态没有被标记为完成，原有本地状态未被修改。</span>
              <span>请选择本地绝对路径后重试。</span>
              <details><summary>技术详情</summary><code>{flow.errorMessage}</code></details>
            </div>
          {/if}
          <div class="choice-group" role="radiogroup" aria-label="数据位置">
            <button
              class:selected={!usesCustomDataDirectory}
              class="choice"
              role="radio"
              aria-checked={!usesCustomDataDirectory}
              data-autofocus={!usesCustomDataDirectory ? "true" : undefined}
              onclick={useDefaultDataDirectory}
            >
              <span class="radio-mark"></span>
              <span class="choice-copy">
                <strong>默认位置（推荐）</strong>
                <small>应用目录下的 data 子目录，之后可一键迁移</small>
                <code>{bootstrap.defaultDataDirectory}</code>
              </span>
            </button>
            <button
              class:selected={usesCustomDataDirectory}
              class="choice"
              role="radio"
              aria-checked={usesCustomDataDirectory}
              onclick={useCustomDataDirectory}
            >
              <span class="radio-mark"></span>
              <span class="choice-copy">
                <strong>自定义位置…</strong>
                <small>支持本地磁盘与外接 USB 存储，断开时实例保留并标记为存储离线</small>
                <span class="warning-copy">不支持 SMB、NAS 或 UNC 网络位置：无法保证数据库锁定与原子迁移</span>
              </span>
            </button>
          </div>
          {#if usesCustomDataDirectory}
            <label class="path-field">
              本地绝对路径
              <input
                name="data-directory"
                value={flow.draft.dataDirectory}
                placeholder="E:\Games\MoyuMax"
                autocomplete="off"
                data-autofocus="true"
                oninput={(event) => updateDataDirectory(event.currentTarget.value)}
              />
            </label>
          {/if}
          <p class="hint">迁移采用原子事务：新位置校验失败时索引继续指向原位置，原文件不会被删除。</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>跳过引导，使用默认</button></span>
          <span>
            <button class="button ghost" onclick={previous}>上一步</button>
            <button class="button primary" onclick={next}>下一步 <Icon name="arrow-right" size={14} /></button>
          </span>
        </footer>
      {:else if flow.step === "privacy"}
        <div class="wizard-body">
          <h1>隐私选择</h1>
          <p class="muted intro">这三项之后都可以在“设置 → 隐私”中随时更改。MoyuMax 不会替你开启敏感联网能力。</p>
          <div class="settings-panel">
            <label class="setting-row">
              <span><strong>诊断与遥测<em>默认关闭</em></strong><small>不上报任何使用数据与崩溃信息。</small></span>
              <input
                type="checkbox"
                role="switch"
                name="telemetry"
                checked={flow.draft.telemetryEnabled}
                data-autofocus="true"
                onchange={(event) => flow = updateOnboardingDraft(flow, { telemetryEnabled: event.currentTarget.checked })}
              />
              <span class="switch" aria-hidden="true"></span>
            </label>
            <label class="setting-row">
              <span><strong>自动检查并提示更新<em>默认开启</em></strong><small>只检查并提示；下载与安装始终由你手动触发。</small></span>
              <input
                type="checkbox"
                role="switch"
                name="updates"
                checked={flow.draft.updateChecksEnabled}
                onchange={(event) => flow = updateOnboardingDraft(flow, { updateChecksEnabled: event.currentTarget.checked })}
              />
              <span class="switch" aria-hidden="true"></span>
            </label>
            <label class="setting-row">
              <span><strong>NAT 检测<em>默认关闭</em></strong><small>需要时再手动访问外部 STUN 服务并暴露公网 IP。</small></span>
              <input
                type="checkbox"
                role="switch"
                name="nat"
                checked={flow.draft.natDetectionEnabled}
                onchange={(event) => flow = updateOnboardingDraft(flow, { natDetectionEnabled: event.currentTarget.checked })}
              />
              <span class="switch" aria-hidden="true"></span>
            </label>
          </div>
          <p class="hint">公网监听同样默认关闭，首次开启时会单独请求风险授权，此处不会预先同意。</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>跳过引导，使用默认</button></span>
          <span>
            <button class="button ghost" onclick={previous}>上一步</button>
            <button class="button primary" disabled={flow.isSubmitting} onclick={() => void complete()}>
              {flow.isSubmitting ? "正在保存…" : "完成设置"}
            </button>
          </span>
        </footer>
      {:else}
        <div class="done-heading">
          <span class="done-mark"><Icon name="check" size={18} /></span>
          <div><h1>一切就绪</h1><p class="muted">以下选择已保存，之后均可在设置中更改。</p></div>
        </div>
        <dl class="summary-list">
          <div><dt>界面语言</dt><dd>{languageLabel(flow.draft.language)}</dd><span>已保存</span></div>
          <div><dt>数据位置</dt><dd>{flow.draft.dataDirectory}</dd><span>本地</span></div>
          <div><dt>诊断遥测</dt><dd>{flow.draft.telemetryEnabled ? "允许匿名诊断" : "不上报"}</dd><span>{flow.draft.telemetryEnabled ? "已开启" : "已关闭"}</span></div>
          <div><dt>更新检查</dt><dd>{flow.draft.updateChecksEnabled ? "仅提示，手动下载安装" : "不自动检查"}</dd><span>{flow.draft.updateChecksEnabled ? "已开启" : "已关闭"}</span></div>
          <div><dt>NAT 检测</dt><dd>{flow.draft.natDetectionEnabled ? "允许手动检测" : "不访问外部 STUN"}</dd><span>{flow.draft.natDetectionEnabled ? "已开启" : "已关闭"}</span></div>
          <div><dt>实例隔离</dt><dd>开启，实例间世界、模组与配置完全隔离</dd><span>推荐</span></div>
        </dl>
        <div class="done-actions">
          <button class="button primary large" data-autofocus="true" onclick={() => onStart(buildSelection(flow))}>
            <Icon name="play" size={14} />开始使用
          </button>
        </div>
        <p class="tiny centered">可随时从“设置 → 关于 → 重新查看引导”回顾这些选择。</p>
      {/if}
    </section>
  </main>
</AppShell>
