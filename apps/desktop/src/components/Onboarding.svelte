<script lang="ts">
  import { tick, untrack } from "svelte";

  import { t } from "../i18n.svelte";
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

  // 语言选项的标题与说明刻意保留各语言自名/自描述，不随界面语言切换，不入字典。
  const languages: ReadonlyArray<{
    value: Language;
    title: string;
    subtitle: string;
  }> = [
    { value: "zh-CN", title: "简体中文", subtitle: "检测到系统语言：中文（简体，中国）" },
    { value: "zh-TW", title: "繁體中文", subtitle: "介面語言：繁體中文" },
    { value: "en", title: "English", subtitle: "Interface language: English" },
  ];

  const stepKeys = ["onboarding.step.language", "onboarding.step.data", "onboarding.step.privacy"] as const;

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
  pageTitle={t("onboarding.pageTitle")}
  titleSuffix={t("onboarding.titleSuffix")}
  dataDirectory={flow.draft.dataDirectory || bootstrap.defaultDataDirectory}
  navigationDisabled
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content wizard-content" bind:this={wizardRoot}>
    <section class="wizard-card" aria-label={t("onboarding.wizardAria")}>
      <ol class="steps" aria-label={t("onboarding.stepsAria")}>
        {#each stepKeys as name, index}
          {@const stepNumber = index + 1}
          {@const completeStep = stepNumber < (["language", "data", "privacy", "done"].indexOf(flow.step) + 1)}
          {@const currentStep = stepNumber === (["language", "data", "privacy"].indexOf(flow.step) + 1)}
          <li class:done={completeStep} class:current={currentStep} aria-current={currentStep ? "step" : undefined}>
            <span>{#if completeStep}<Icon name="check" size={12} />{:else}{stepNumber}{/if}</span>{t(name)}
          </li>
          {#if index < stepKeys.length - 1}<i aria-hidden="true"></i>{/if}
        {/each}
      </ol>

      {#if flow.step === "language"}
        <div class="wizard-body">
          <h1>{t("onboarding.language.title")}</h1>
          <p class="muted intro">{t("onboarding.language.intro")}</p>
          <fieldset class="choice-section">
            <legend>{t("onboarding.language.legend")}</legend>
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
                      {language.title}{#if language.value === "zh-CN"}<em>{t("onboarding.language.systemTag")}</em>{/if}
                    </strong>
                    <small>{language.subtitle}</small>
                  </span>
                </label>
              {/each}
            </div>
          </fieldset>
          <p class="hint">{t("onboarding.language.hint")}</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>{t("onboarding.skip")}</button></span>
          <span><button class="button primary" onclick={next}>{t("onboarding.next")} <Icon name="arrow-right" size={14} /></button></span>
        </footer>
      {:else if flow.step === "data"}
        <div class="wizard-body">
          <h1>{t("onboarding.step.data")}</h1>
          <p class="muted intro">{t("onboarding.data.intro")}</p>
          {#if flow.errorMessage}
            <div class="error-block" role="alert">
              <strong>{t("onboarding.data.errorTitle")}</strong>
              <span>{t("onboarding.data.errorBody")}</span>
              <span>{t("onboarding.data.errorHint")}</span>
              <details><summary>{t("onboarding.data.errorDetails")}</summary><code>{flow.errorMessage}</code></details>
            </div>
          {/if}
          <div class="choice-group" role="radiogroup" aria-label={t("onboarding.step.data")}>
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
                <strong>{t("onboarding.data.defaultTitle")}</strong>
                <small>{t("onboarding.data.defaultDescription")}</small>
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
                <strong>{t("onboarding.data.customTitle")}</strong>
                <small>{t("onboarding.data.customDescription")}</small>
                <span class="warning-copy">{t("onboarding.data.customWarning")}</span>
              </span>
            </button>
          </div>
          {#if usesCustomDataDirectory}
            <label class="path-field">
              {t("onboarding.data.pathLabel")}
              <input
                name="data-directory"
                value={flow.draft.dataDirectory}
                placeholder={t("onboarding.data.pathPlaceholder")}
                autocomplete="off"
                data-autofocus="true"
                oninput={(event) => updateDataDirectory(event.currentTarget.value)}
              />
            </label>
          {/if}
          <p class="hint">{t("onboarding.data.migrationHint")}</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>{t("onboarding.skip")}</button></span>
          <span>
            <button class="button ghost" onclick={previous}>{t("onboarding.previous")}</button>
            <button class="button primary" onclick={next}>{t("onboarding.next")} <Icon name="arrow-right" size={14} /></button>
          </span>
        </footer>
      {:else if flow.step === "privacy"}
        <div class="wizard-body">
          <h1>{t("onboarding.privacy.title")}</h1>
          <p class="muted intro">{t("onboarding.privacy.intro")}</p>
          <div class="settings-panel">
            <label class="setting-row">
              <span><strong>{t("onboarding.privacy.telemetry.title")}<em>{t("onboarding.privacy.tagOff")}</em></strong><small>{t("onboarding.privacy.telemetry.description")}</small></span>
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
              <span><strong>{t("onboarding.privacy.updates.title")}<em>{t("onboarding.privacy.tagOn")}</em></strong><small>{t("onboarding.privacy.updates.description")}</small></span>
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
              <span><strong>{t("onboarding.privacy.nat.title")}<em>{t("onboarding.privacy.tagOff")}</em></strong><small>{t("onboarding.privacy.nat.description")}</small></span>
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
          <p class="hint">{t("onboarding.privacy.hint")}</p>
        </div>
        <footer class="wizard-actions">
          <span><button class="inline-link" onclick={() => void skip()}>{t("onboarding.skip")}</button></span>
          <span>
            <button class="button ghost" onclick={previous}>{t("onboarding.previous")}</button>
            <button class="button primary" disabled={flow.isSubmitting} onclick={() => void complete()}>
              {flow.isSubmitting ? t("onboarding.privacy.saving") : t("onboarding.privacy.complete")}
            </button>
          </span>
        </footer>
      {:else}
        <div class="done-heading">
          <span class="done-mark"><Icon name="check" size={18} /></span>
          <div><h1>{t("onboarding.done.title")}</h1><p class="muted">{t("onboarding.done.description")}</p></div>
        </div>
        <dl class="summary-list">
          <div><dt>{t("onboarding.done.languageLabel")}</dt><dd>{languageLabel(flow.draft.language)}</dd><span>{t("onboarding.done.savedTag")}</span></div>
          <div><dt>{t("onboarding.step.data")}</dt><dd>{flow.draft.dataDirectory}</dd><span>{t("onboarding.done.localTag")}</span></div>
          <div><dt>{t("onboarding.done.telemetryLabel")}</dt><dd>{flow.draft.telemetryEnabled ? t("onboarding.done.telemetryAllowed") : t("onboarding.done.telemetryOff")}</dd><span>{flow.draft.telemetryEnabled ? t("onboarding.done.stateOn") : t("onboarding.done.stateOff")}</span></div>
          <div><dt>{t("onboarding.done.updatesLabel")}</dt><dd>{flow.draft.updateChecksEnabled ? t("onboarding.done.updatesOn") : t("onboarding.done.updatesOff")}</dd><span>{flow.draft.updateChecksEnabled ? t("onboarding.done.stateOn") : t("onboarding.done.stateOff")}</span></div>
          <div><dt>{t("onboarding.done.natLabel")}</dt><dd>{flow.draft.natDetectionEnabled ? t("onboarding.done.natOn") : t("onboarding.done.natOff")}</dd><span>{flow.draft.natDetectionEnabled ? t("onboarding.done.stateOn") : t("onboarding.done.stateOff")}</span></div>
          <div><dt>{t("onboarding.done.isolationLabel")}</dt><dd>{t("onboarding.done.isolationValue")}</dd><span>{t("onboarding.done.recommendedTag")}</span></div>
        </dl>
        <div class="done-actions">
          <button class="button primary large" data-autofocus="true" onclick={() => onStart(buildSelection(flow))}>
            <Icon name="play" size={14} />{t("onboarding.done.start")}
          </button>
        </div>
        <p class="tiny centered">{t("onboarding.done.reviewHint")}</p>
      {/if}
    </section>
  </main>
</AppShell>
