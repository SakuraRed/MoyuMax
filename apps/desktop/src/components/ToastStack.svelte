<script lang="ts">
  import { dismissToast, toastItems } from "../toast.svelte";

  const TONE_COLOR: Record<string, string> = {
    ok: "var(--ok)",
    warn: "var(--warn)",
    danger: "var(--danger)",
    info: "var(--info)",
  };
</script>

{#if toastItems().length > 0}
  <div class="toast-stack" aria-live="polite">
    {#each toastItems() as item (item.id)}
      <div class="toast" role="status">
        <span class="t-icon" style:background={TONE_COLOR[item.tone]}></span>
        <div class="t-main">
          <div class="t-title">{item.title}</div>
          {#if item.sub}<div class="t-sub">{item.sub}</div>{/if}
        </div>
        {#if item.action}
          <button
            class="btn small secondary"
            onclick={() => {
              item.action?.run();
              dismissToast(item.id);
            }}
          >{item.action.label}</button>
        {/if}
        <button class="toast-close" aria-label="关闭通知" onclick={() => dismissToast(item.id)}>✕</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: absolute;
    top: 60px;
    right: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 30;
    width: 340px;
  }
  .toast .t-icon {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .toast .t-main {
    flex: 1;
    min-width: 0;
  }
  .toast .t-title {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast .t-sub {
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast-close {
    flex: none;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--text-3);
    border-radius: var(--r);
    cursor: pointer;
    font-size: 10px;
  }
  .toast-close:hover {
    background: var(--glass-strong);
    color: var(--text-1);
  }
</style>
