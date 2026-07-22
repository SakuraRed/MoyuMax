<script lang="ts">
  type IconName =
    | "home"
    | "box"
    | "compass"
    | "task"
    | "database"
    | "settings"
    | "search"
    | "wifi"
    | "disk"
    | "arrow-right"
    | "check"
    | "play"
    | "info";

  let { name, size = 17 } = $props<{ name: IconName; size?: number }>();
</script>

<svg
  aria-hidden="true"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  {#if name === "home"}
    <path d="M3 10.8 12 3l9 7.8v8.7a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 19.5Z" />
    <path d="M9 21v-7h6v7" />
  {:else if name === "box"}
    <path d="m21 8-9 5-9-5 9-5Z" />
    <path d="m3 8 9 5 9-5v8l-9 5-9-5Z" />
    <path d="M12 13v8" />
  {:else if name === "compass"}
    <circle cx="12" cy="12" r="9" />
    <path d="m15.5 8.5-2.2 4.8-4.8 2.2 2.2-4.8Z" />
  {:else if name === "task"}
    <path d="M4 6h10M4 12h16M4 18h12" />
    <path d="m17 5 1.5 1.5L21 4" />
  {:else if name === "database"}
    <ellipse cx="12" cy="5" rx="8" ry="3" />
    <path d="M4 5v7c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12v7c0 1.7 3.6 3 8 3s8-1.3 8-3v-7" />
  {:else if name === "settings"}
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1A1.7 1.7 0 0 0 9 4.6 1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z" />
  {:else if name === "search"}
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-4-4" />
  {:else if name === "wifi"}
    <path d="M4 9a12 12 0 0 1 16 0M7 13a7.5 7.5 0 0 1 10 0M10 17a3 3 0 0 1 4 0" />
    <circle cx="12" cy="20" r=".7" fill="currentColor" stroke="none" />
  {:else if name === "disk"}
    <path d="M4 5h16v14H4z" />
    <path d="M8 9h8M8 14h4" />
  {:else if name === "arrow-right"}
    <path d="M5 12h14M14 7l5 5-5 5" />
  {:else if name === "check"}
    <path d="m5 12 4 4L19 6" />
  {:else if name === "play"}
    <path d="m8 5 11 7-11 7Z" />
  {:else if name === "info"}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v6M12 7h.01" />
  {/if}
</svg>
