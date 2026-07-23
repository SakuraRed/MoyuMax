<script lang="ts">
  import { onMount } from "svelte";

  import { describeExitImpact } from "../close-flow";
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
        <h2 id="close-dialog-title">关闭 MoyuMax</h2>
        <p>这是你第一次关闭主窗口，选择默认行为：</p>
      </header>

      <div class="close-options" role="radiogroup" aria-label="关闭窗口行为">
        <label class:selected={selected === "minimize"} class="close-option">
          <input type="radio" name="close-action" value="minimize" bind:group={selected} />
          <span class="close-option-copy">
            <strong>最小化到系统托盘 <em class="close-default-tag">默认</em></strong>
            <small>后台任务继续运行，双击托盘图标即刻恢复窗口</small>
          </span>
        </label>
        <label class:selected={selected === "exit"} class="close-option">
          <input type="radio" name="close-action" value="exit" bind:group={selected} />
          <span class="close-option-copy">
            <strong>退出 MoyuMax</strong>
            <small>进行中的下载暂停并可在下次启动时恢复；正在运行的游戏先安全终止并完成退出备份</small>
          </span>
        </label>
      </div>

      <label class="close-remember">
        <input type="checkbox" bind:checked={remember} />
        <span>记住本次选择，之后关闭窗口不再询问</span>
      </label>

      {#if hasRunningGame}
        <div class="confirmation-impact danger-impact" role="note">
          <strong>退出前请注意</strong>
          {#each impactLines.filter((line) => line.danger) as line}
            <span>{line.text}</span>
          {/each}
        </div>
      {/if}

      <div class="confirmation-actions">
        <button class="button ghost" onclick={onCancel}>取消</button>
        <button
          class="button primary close-dialog-primary"
          onclick={() => onConfirm(selected, remember)}
        >确定</button>
      </div>
    {:else}
      <header>
        <h2 id="close-dialog-title">退出 MoyuMax</h2>
        <p>退出前请确认以下影响：</p>
      </header>

      <div class="confirmation-impact danger-impact" role="note">
        <strong>退出将产生以下影响</strong>
        {#each impactLines as line}
          <span>{line.text}</span>
        {/each}
      </div>

      {#if busy}
        <p class="close-busy" role="status">正在安全终止游戏并完成退出备份…</p>
      {/if}
      {#if errorMessage}
        <div class="error-block" role="alert">
          <strong>无法在规定时间内安全退出</strong>
          <span>{errorMessage}</span>
        </div>
      {/if}

      <div class="confirmation-actions">
        <button class="button ghost" disabled={busy} onclick={onCancel}>取消</button>
        {#if errorMessage}
          <button class="button danger-subtle" onclick={onForceExit}>仍然退出(强制)</button>
        {/if}
        <button class="button primary close-dialog-primary" disabled={busy} onclick={onConfirmExit}>
          确认退出
        </button>
      </div>
    {/if}
  </div>
</div>
