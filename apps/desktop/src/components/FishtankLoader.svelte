<script lang="ts">
  interface Props {
    message?: string;
    /** 加载完成：鱼跳出水面并淡出,由调用方在动画结束后切换到完成页。 */
    leaping?: boolean;
  }

  let { message = "", leaping = false }: Props = $props();
</script>

<div class="fishtank-loader" class:leaping role="status" aria-live="polite">
  <svg class="fishtank-loader-svg" viewBox="0 0 160 110" aria-hidden="true">
    <rect x="20" y="18" width="120" height="76" rx="10" fill="var(--surface)" stroke="var(--border-strong)" stroke-width="2.5" />
    <path d="M24 44 Q48 36 72 44 T120 44 T136 44 V84 Q136 92 128 92 H32 Q24 92 24 84 Z" fill="var(--accent-dim)" />
    <path d="M24 44 Q48 36 72 44 T120 44 T136 44" fill="none" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" />
    <path d="M44 86 Q42 74 47 66 M54 86 Q57 76 52 68" fill="none" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" opacity="0.7" />
    <g class="loader-fish">
      <ellipse cx="84" cy="62" rx="15" ry="9" fill="var(--accent)" />
      <path d="M69 62 L57 54 V70 Z" fill="var(--accent)" />
      <circle cx="92" cy="59.5" r="1.8" fill="var(--on-accent)" />
    </g>
    <circle class="loader-bubble bubble-a" cx="106" cy="74" r="3" fill="none" stroke="var(--accent)" stroke-width="2" />
    <circle class="loader-bubble bubble-b" cx="114" cy="79" r="2.2" fill="none" stroke="var(--accent)" stroke-width="1.8" />
  </svg>
  {#if message}<span class="fishtank-loader-text">{message}</span>{/if}
</div>
