<script lang="ts">
  import { tick, untrack } from "svelte";

  import { t, uiContrast, uiMotion, uiTheme } from "../i18n.svelte";
  import {
    buildSelection,
    createOnboardingState,
    setOnboardingError,
    setSubmitting,
    updateOnboardingDraft,
  } from "../onboarding";
  import type { OnboardingState } from "../onboarding";
  import type { BootstrapState, Language, OnboardingSelection } from "../runtime";
  import Fish from "./Fish.svelte";

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
  // welcome:扑通鱼欢迎页;wizard:两步引导(flow.step 复用 language/privacy 两个状态位)。
  let stage = $state<"welcome" | "wizard">("welcome");
  let editingDataDirectory = $state(false);
  let wizardRoot: HTMLElement | undefined = $state();

  // 语言选项的标题刻意保留各语言自名，不随界面语言切换，不入字典。
  const languages: ReadonlyArray<{ value: Language; title: string }> = [
    { value: "zh-CN", title: "简体中文" },
    { value: "zh-TW", title: "繁體中文" },
    { value: "en", title: "English" },
  ];

  const wizardStep = $derived(flow.step === "privacy" ? 2 : 1);

  // 仅在欢迎页/步骤切换时移动焦点到主按钮；草稿编辑不抢焦点。
  let lastFocusKey = "";
  $effect(() => {
    const key = `${stage}:${flow.step}`;
    if (key === lastFocusKey) return;
    lastFocusKey = key;
    void tick().then(() => {
      wizardRoot?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });
  });

  function beginSetup(): void {
    stage = "wizard";
  }

  function chooseLanguage(language: Language): void {
    flow = updateOnboardingDraft(flow, { language });
  }

  function updateDataDirectory(value: string): void {
    flow = updateOnboardingDraft(flow, { dataDirectory: value });
  }

  function toggleDataDirectoryEdit(): void {
    if (editingDataDirectory) {
      flow = updateOnboardingDraft(flow, { dataDirectory: bootstrap.defaultDataDirectory });
      editingDataDirectory = false;
    } else {
      editingDataDirectory = true;
      void tick().then(() => {
        wizardRoot?.querySelector<HTMLInputElement>("[data-data-directory]")?.focus();
      });
    }
  }

  function setTelemetry(enabled: boolean): void {
    flow = updateOnboardingDraft(flow, { telemetryEnabled: enabled });
  }

  function setUpdateChecks(enabled: boolean): void {
    flow = updateOnboardingDraft(flow, { updateChecksEnabled: enabled });
  }

  function next(): void {
    flow = { ...flow, step: "privacy", errorMessage: null };
  }

  function previous(): void {
    flow = { ...flow, step: "language", errorMessage: null };
  }

  async function complete(): Promise<void> {
    flow = setSubmitting(flow, true);
    try {
      const selection = buildSelection(flow);
      await onPersist(selection);
      onStart(selection);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      flow = { ...setOnboardingError(flow, message), step: "language" };
      editingDataDirectory = true;
    }
  }

  async function skip(): Promise<void> {
    flow = setSubmitting(flow, true);
    try {
      await onSkip();
      onStart({ ...bootstrap.defaults });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      flow = setOnboardingError(flow, message);
    }
  }
</script>

<div
  class="window"
  data-theme={uiTheme()}
  data-motion={uiMotion()}
  data-contrast={uiContrast()}
  bind:this={wizardRoot}
>
  <header class="titlebar" data-tauri-drag-region="deep">
    <span class="tb-title">{stage === "welcome" ? "MoyuMax" : t("onboarding.setupTitle")}</span>
    {#if stage === "wizard"}
      <span class="tb-sub">{t("onboarding.stepCounter").replace("{step}", String(wizardStep)).replace("{total}", "2")}</span>
    {/if}
    <span class="tb-spacer" data-tauri-drag-region></span>
    <button class="tb-win" aria-label={t("shell.window.minimize")} onclick={() => void onMinimize()}><i class="min-line"></i></button>
    <button class="tb-win" aria-label={t("shell.window.maximize")} onclick={() => void onToggleMaximize()}>▢</button>
    <button class="tb-win close" aria-label={t("shell.window.close")} onclick={() => void onClose()}>✕</button>
  </header>

  {#if stage === "welcome"}
    <main class="center-stage">
      <Fish variant="dryland" />
      <h1 class="brand-line"><span class="name">MoyuMax</span></h1>
      <p class="muted" style="text-align:center;max-width:46ch;margin:0">{t("onboarding.welcome.description")}</p>
      <div class="row" style="margin-top:6px">
        <button class="btn primary large" data-autofocus="true" disabled={flow.isSubmitting} onclick={beginSetup}>
          {t("onboarding.welcome.start")}
        </button>
        <button class="btn ghost" disabled={flow.isSubmitting} onclick={() => void skip()}>
          {t("onboarding.welcome.skip")}
        </button>
      </div>
      {#if flow.errorMessage}
        <div class="banner danger" role="alert" style="max-width:520px"><span>{flow.errorMessage}</span></div>
      {/if}
    </main>
  {:else}
    <main class="wizard">
      <div class="wizard-card panel">
        <div class="steps" aria-hidden="true"><i class:on={wizardStep >= 1}></i><i class:on={wizardStep >= 2}></i></div>

        {#if wizardStep === 1}
          <div class="wizard-body">
            <h1 class="panel-title" style="font-size:17px;margin:0">{t("onboarding.step1.title")}</h1>
            <p class="panel-desc" style="margin:4px 0 18px">{t("onboarding.step1.description")}</p>
            {#if flow.errorMessage}
              <div class="banner danger" role="alert" style="margin-bottom:14px">
                <div>
                  <strong>{t("onboarding.data.errorTitle")}</strong>
                  <div>{t("onboarding.data.errorBody")} {t("onboarding.data.errorHint")}</div>
                  <div class="mono" style="margin-top:4px;word-break:break-all">{flow.errorMessage}</div>
                </div>
              </div>
            {/if}
            <div class="col" style="gap:16px">
              <div class="field">
                <label for="onboarding-language">{t("onboarding.step1.languageLabel")}</label>
                <select
                  id="onboarding-language"
                  class="input"
                  value={flow.draft.language}
                  onchange={(event) => chooseLanguage(event.currentTarget.value as Language)}
                >
                  {#each languages as language}
                    <option value={language.value}>
                      {language.value === bootstrap.defaults.language
                        ? t("onboarding.step1.systemOption").replace("{name}", language.title)
                        : language.title}
                    </option>
                  {/each}
                </select>
              </div>
              <div class="field">
                <label for="onboarding-data-directory">{t("onboarding.step1.dataLabel")}</label>
                <div class="row">
                  <input
                    id="onboarding-data-directory"
                    class="input"
                    style="flex:1"
                    value={flow.draft.dataDirectory || bootstrap.defaultDataDirectory}
                    readonly={!editingDataDirectory}
                    data-data-directory="true"
                    placeholder={t("onboarding.data.pathPlaceholder")}
                    autocomplete="off"
                    oninput={(event) => updateDataDirectory(event.currentTarget.value)}
                  />
                  <button class="btn secondary" onclick={toggleDataDirectoryEdit}>
                    {editingDataDirectory ? t("onboarding.step1.useDefault") : t("onboarding.step1.change")}
                  </button>
                </div>
                <span class="help">{t("onboarding.step1.dataHelp")}</span>
              </div>
            </div>
          </div>
          <footer class="wizard-actions">
            <button class="btn ghost" disabled={flow.isSubmitting} onclick={() => void skip()}>{t("onboarding.skipWizard")}</button>
            <button class="btn primary" data-autofocus="true" onclick={next}>{t("onboarding.next")}</button>
          </footer>
        {:else}
          <div class="wizard-body">
            <h1 class="panel-title" style="font-size:17px;margin:0">{t("onboarding.step2.title")}</h1>
            <p class="panel-desc" style="margin:4px 0 18px">{t("onboarding.step2.description")}</p>
            <div class="col" style="gap:12px" role="radiogroup" aria-label={t("onboarding.step2.telemetryAria")}>
              <button
                type="button"
                class="opt"
                class:sel={!flow.draft.telemetryEnabled}
                role="radio"
                aria-checked={!flow.draft.telemetryEnabled}
                onclick={() => setTelemetry(false)}
              >
                <span class="radio" aria-hidden="true"></span>
                <div>
                  <b>{t("onboarding.step2.telemetryOff.title")}</b>
                  <span>{t("onboarding.step2.telemetryOff.description")}</span>
                </div>
              </button>
              <button
                type="button"
                class="opt"
                class:sel={flow.draft.telemetryEnabled}
                role="radio"
                aria-checked={flow.draft.telemetryEnabled}
                onclick={() => setTelemetry(true)}
              >
                <span class="radio" aria-hidden="true"></span>
                <div>
                  <b>{t("onboarding.step2.telemetryOn.title")}</b>
                  <span>{t("onboarding.step2.telemetryOn.description")}</span>
                </div>
              </button>
            </div>
            <div class="set-row" style="border:none;padding:12px 0 0">
              <div class="sr-main">
                <div class="sr-name">{t("onboarding.step2.updates.title")}</div>
                <div class="sr-desc">{t("onboarding.step2.updates.description")}</div>
              </div>
              <button
                type="button"
                class="switch"
                class:on={flow.draft.updateChecksEnabled}
                role="switch"
                aria-checked={flow.draft.updateChecksEnabled}
                aria-label={t("onboarding.step2.updates.title")}
                onclick={() => setUpdateChecks(!flow.draft.updateChecksEnabled)}
              ></button>
            </div>
          </div>
          <footer class="wizard-actions">
            <button class="btn ghost" onclick={previous}>{t("onboarding.previous")}</button>
            <button class="btn primary" data-autofocus="true" disabled={flow.isSubmitting} onclick={() => void complete()}>
              {flow.isSubmitting ? t("onboarding.privacy.saving") : t("onboarding.step2.complete")}
            </button>
          </footer>
        {/if}
      </div>
    </main>
  {/if}
</div>

<style>
  .center-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 20px;
    padding: 40px;
    overflow: auto;
  }
  .brand-line {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0;
  }
  .brand-line .name {
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .wizard {
    flex: 1;
    display: flex;
    padding: 32px;
    overflow: auto;
  }
  .wizard-card {
    width: 620px;
    max-width: 100%;
    padding: 28px 32px;
    margin: auto;
  }
  .steps {
    display: flex;
    gap: 8px;
    margin-bottom: 22px;
  }
  .steps i {
    height: 4px;
    width: 44px;
    border-radius: 999px;
    background: var(--glass-strong);
  }
  .steps i.on {
    background: var(--accent);
  }

  .wizard-body {
    margin-top: 0;
  }
  .wizard-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 26px;
    padding-top: 0;
    border-top: none;
  }

  .opt {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    width: 100%;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    padding: 14px 16px;
    cursor: pointer;
    background: rgba(0, 0, 0, 0.15);
    color: var(--text-1);
    font-family: var(--font);
    text-align: left;
  }
  .opt.sel {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .opt .radio {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--text-3);
    margin-top: 3px;
    flex: none;
  }
  .opt.sel .radio {
    border-color: var(--accent);
    background: radial-gradient(circle, var(--accent) 45%, transparent 50%);
  }
  .opt b {
    font-size: 13.5px;
    display: block;
  }
  .opt span {
    font-size: 12px;
    color: var(--text-2);
    display: block;
    margin-top: 2px;
  }
</style>
