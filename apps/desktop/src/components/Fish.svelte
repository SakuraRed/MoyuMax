<script lang="ts">
  interface Props {
    /** logo:导航品牌小图;tank:缸中游鱼(加载/等待);dryland:扑通鱼(首次运行) */
    variant?: "logo" | "tank" | "dryland";
    /** tank/dryland 可选加载文案(带节奏点) */
    message?: string;
  }

  let { variant = "tank", message = "" }: Props = $props();
</script>

{#snippet fishSvg()}
  <svg class="fish-svg" viewBox="0 0 72 44" fill="none" aria-hidden="true">
    <g class="tail"><path d="M46 22 L60 10 L57 22 L60 34 Z" fill="#2ba89a" /></g>
    <g class="fin"><path d="M28 16 C26 8 34 6 38 12 L36 18 Z" fill="#2ba89a" /></g>
    <g class="body">
      <ellipse cx="28" cy="22" rx="20" ry="13" fill="#3fd8c2" />
      <ellipse cx="24" cy="26" rx="12" ry="7" fill="#7fe8d8" opacity="0.7" />
      <circle cx="16" cy="19" r="3" fill="#06231f" />
      <circle cx="17" cy="18" r="1" fill="#ffffff" />
    </g>
  </svg>
{/snippet}

{#if variant === "logo"}
  <svg class="logo" viewBox="0 0 72 44" fill="none" aria-hidden="true">
    <g><path d="M46 22 L60 10 L57 22 L60 34 Z" fill="#2ba89a" /></g>
    <ellipse cx="28" cy="22" rx="20" ry="13" fill="#3fd8c2" />
    <circle cx="16" cy="19" r="3" fill="#06231f" />
  </svg>
{:else if variant === "dryland"}
  <div class="col" style="align-items:center;gap:14px">
    <div class="dryland" role="status" aria-label={message || undefined}>
      <div class="ground"></div>
      <div class="puddle"></div>
      <div class="puff p1"></div>
      <div class="puff p2"></div>
      <div class="fish">{@render fishSvg()}</div>
    </div>
    {#if message}<span class="muted load-dots">{message}</span>{/if}
  </div>
{:else}
  <div class="col" style="align-items:center;gap:14px">
    <div class="tank" role="status" aria-live="polite" aria-label={message || undefined}>
      <div class="fish">{@render fishSvg()}</div>
      <span class="bubble b1"></span>
      <span class="bubble b2"></span>
      <span class="bubble b3"></span>
    </div>
    {#if message}<span class="muted load-dots">{message}</span>{/if}
  </div>
{/if}
