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

<div class="modal-mask" role="presentation">
  <div
    bind:this={dialogRoot}
    class="modal close-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="close-dialog-title"
  >
    {#if mode === "choice"}
      <h3 id="close-dialog-title">{t("close.choice.title")}</h3>
      <div class="m-body" style="margin-bottom:14px">{t("close.choice.description")}</div>

      <div class="close-options" role="radiogroup" aria-label={t("close.choice.groupAria")}>
        <label class="close-opt" class:selected={selected === "minimize"}>
          <input type="radio" name="close-action" value="minimize" bind:group={selected} />
          <div>
            <div class="co-name">{t("close.choice.minimize.title")} <em class="close-default-tag">{t("close.choice.minimize.defaultTag")}</em></div>
            <div class="co-desc">{t("close.choice.minimize.description")}</div>
          </div>
        </label>
        <label class="close-opt" class:selected={selected === "exit"}>
          <input type="radio" name="close-action" value="exit" bind:group={selected} />
          <div>
            <div class="co-name">{t("close.choice.exit.title")}</div>
            <div class="co-desc">{t("close.choice.exit.description")}</div>
          </div>
        </label>
      </div>

      <label class="remember-row">
        <input type="checkbox" bind:checked={remember} />
        {t("close.choice.remember")}
      </label>

      {#if hasRunningGame}
        <div class="banner danger" role="note" style="margin-top:14px">
          <div>
            <strong>{t("close.choice.warningTitle")}</strong>
            {#each impactLines.filter((line) => line.danger) as line}
              <div>{line.text}</div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="m-acts">
        <button class="btn ghost" onclick={onCancel}>{t("common.cancel")}</button>
        <button
          class="btn primary close-dialog-primary"
          onclick={() => onConfirm(selected, remember)}
        >{t("close.choice.confirm")}</button>
      </div>
    {:else}
      <h3 id="close-dialog-title">{t("close.impact.title")}</h3>
      <div class="m-body" style="margin-bottom:12px">{t("close.impact.description")}</div>

      <div class="banner danger" role="note">
        <div>
          <strong>{t("close.impact.listTitle")}</strong>
          {#each impactLines as line}
            <div>{line.text}</div>
          {/each}
        </div>
      </div>

      {#if busy}
        <p class="dim" role="status" style="margin-top:10px">{t("close.impact.busy")}</p>
      {/if}
      {#if errorMessage}
        <div class="banner danger" role="alert" style="margin-top:10px">
          <div>
            <strong>{t("close.impact.errorTitle")}</strong>
            <div>{errorMessage}</div>
          </div>
        </div>
      {/if}

      <div class="m-acts">
        <button class="btn ghost" disabled={busy} onclick={onCancel}>{t("common.cancel")}</button>
        {#if errorMessage}
          <button class="btn danger-soft" onclick={onForceExit}>{t("close.impact.forceExit")}</button>
        {/if}
        <button class="btn primary close-dialog-primary" disabled={busy} onclick={onConfirmExit}>
          {t("close.impact.confirmExit")}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .close-options {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .close-opt {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.15);
    cursor: pointer;
  }
  .close-opt.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .close-opt input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  .close-opt .co-name {
    font-size: 13.5px;
    font-weight: 600;
  }
  .close-opt .co-desc {
    font-size: 12px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .close-default-tag {
    font-style: normal;
    font-size: 11px;
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 999px;
    padding: 1px 7px;
    margin-left: 6px;
  }  .remember-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
    font-size: 12.5px;
    color: var(--text-2);
    cursor: pointer;
  }
  .remember-row input {
    accent-color: var(--accent);
    width: 15px;
    height: 15px;
  }
</style>
