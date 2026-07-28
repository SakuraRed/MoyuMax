<script lang="ts">
  interface Props {
    message?: string;
    /** 加载完成：鱼跳出水面并淡出,由调用方在动画结束后切换到完成页。 */
    leaping?: boolean;
  }

  let { message = "", leaping = false }: Props = $props();
</script>

<div class="fishtank-loader" class:leaping role="status" aria-live="polite">
  <div class="tank fishtank-loader-svg" aria-hidden="true">
    <div class="fish loader-fish">
      <svg class="fish-svg" viewBox="0 0 72 44" fill="none">
        <g class="tail"><path d="M46 22 L60 10 L57 22 L60 34 Z" fill="#2ba89a" /></g>
        <g class="fin"><path d="M28 16 C26 8 34 6 38 12 L36 18 Z" fill="#2ba89a" /></g>
        <g class="body">
          <ellipse cx="28" cy="22" rx="20" ry="13" fill="#3fd8c2" />
          <ellipse cx="24" cy="26" rx="12" ry="7" fill="#7fe8d8" opacity="0.7" />
          <circle cx="16" cy="19" r="3" fill="#06231f" />
          <circle cx="17" cy="18" r="1" fill="#ffffff" />
        </g>
      </svg>
    </div>
    <span class="bubble b1 loader-bubble"></span>
    <span class="bubble b2 loader-bubble"></span>
    <span class="bubble b3 loader-bubble"></span>
  </div>
  {#if message}<span class="fishtank-loader-text load-dots">{message}</span>{/if}
</div>
