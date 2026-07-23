<script lang="ts">
  import { onMount } from "svelte";

  import { describeExitImpact } from "../close-flow";
  import { t } from "../i18n.svelte";
  import type { ExitImpact, WindowCloseAction } from "../runtime";

  interface Props {
    mode: "choice" | "impact";
    impact: ExitImpact;
    busy?: boolean;
    errorMessage?: string;
    onCancel: () => void;
    onConfirm: (choice: WindowCloseAction, remember: boolean) => void;
    onConfirmExit: () => void;
    onForceExit: () => void;
  }

  let {
    mode,
    impact,
    busy = false,
    errorMessage = "",
    onCancel,
    onConfirm,
    onConfirmExit,
    onForceExit,
  }: Props = $props();

  let selected = $state<WindowCloseAction>("minimize");
  let remember = $state(false);
  let dialogRoot = $state<HTMLDivElement>();

  const impactLines = $derived(describeExitImpact(impact));
  const hasRunningGame = $derived(impact.runningSessions.length > 0);

  onMount(() => {
    const initial = dialogRoot?.querySelector<HTMLElement>(
      mode === "choice" ? `input[value="${selected}"]` : ".close-dialog-primary",
    );
    initial?.focus();
  });

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !busy) {
      event.stopPropagation();
      onCancel();
      return;
    }
    if (event.key !== "Tab" || !dialogRoot) return;
    const focusable = Array.from(
      dialogRoot.querySelectorAll<HTMLElement>(
        "button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex='-1'])",
      ),
    );
    if (focusable.length === 0) return;
    const first = focusable[0]!;
    const last = focusable[focusable.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="modal-backdrop" role="presentation">
  <div
    bind:this={dialogRoot}
    class="confirmation-dialog close-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="close-dialog-title"
  >
    {#if mode === "choice"}
      <header>
        <h2 id="close-dialog-title">{t("close.choice.title")}</h2>
        <p>{t("close.choice.description")}</p>
      </header>

      <div class="close-options" role="radiogroup" aria-label={t("close.choice.groupAria")}>
        <label class:selected={selected === "minimize"} class="close-option">
          <input type="radio" name="close-action" value="minimize" bind:group={selected} />
          <span class="close-option-copy">
            <strong>{t("close.choice.minimize.title")} <em class="close-default-tag">{t("close.choice.minimize.defaultTag")}</em></strong>
            <small>{t("close.choice.minimize.description")}</small>
          </span>
        </label>
        <label class:selected={selected === "exit"} class="close-option">
          <input type="radio" name="close-action" value="exit" bind:group={selected} />
          <span class="close-option-copy">
            <strong>{t("close.choice.exit.title")}</strong>
            <small>{t("close.choice.exit.description")}</small>
          </span>
        </label>
      </div>

      <label class="close-remember">
        <input type="checkbox" bind:checked={remember} />
        <span>{t("close.choice.remember")}</span>
      </label>

      {#if hasRunningGame}
        <div class="confirmation-impact danger-impact" role="note">
          <strong>{t("close.choice.warningTitle")}</strong>
          {#each impactLines.filter((line) => line.danger) as line}
            <span>{line.text}</span>
          {/each}
        </div>
      {/if}

      <div class="confirmation-actions">
        <button class="button ghost" onclick={onCancel}>{t("common.cancel")}</button>
        <button
          class="button primary close-dialog-primary"
          onclick={() => onConfirm(selected, remember)}
        >{t("close.choice.confirm")}</button>
      </div>
    {:else}
      <header>
        <h2 id="close-dialog-title">{t("close.impact.title")}</h2>
        <p>{t("close.impact.description")}</p>
      </header>

      <div class="confirmation-impact danger-impact" role="note">
        <strong>{t("close.impact.listTitle")}</strong>
        {#each impactLines as line}
          <span>{line.text}</span>
        {/each}
      </div>

      {#if busy}
        <p class="close-busy" role="status">{t("close.impact.busy")}</p>
      {/if}
      {#if errorMessage}
        <div class="error-block" role="alert">
          <strong>{t("close.impact.errorTitle")}</strong>
          <span>{errorMessage}</span>
        </div>
      {/if}

      <div class="confirmation-actions">
        <button class="button ghost" disabled={busy} onclick={onCancel}>{t("common.cancel")}</button>
        {#if errorMessage}
          <button class="button danger-subtle" onclick={onForceExit}>{t("close.impact.forceExit")}</button>
        {/if}
        <button class="button primary close-dialog-primary" disabled={busy} onclick={onConfirmExit}>
          {t("close.impact.confirmExit")}
        </button>
      </div>
    {/if}
  </div>
</div>
