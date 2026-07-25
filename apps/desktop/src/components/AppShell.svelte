<script lang="ts">
  import type { Snippet } from "svelte";

  import { markAvatarFailed, requestSettingsPage, shellAccount, skinAvatarUrl } from "../accounts.svelte";
  import { t, uiBackground, uiBackgroundImageUrl, uiContrast, uiMotion, uiTheme } from "../i18n.svelte";
  import type { NavigationKey } from "../runtime";
  import Icon from "./Icon.svelte";

  interface Props {
    pageTitle: string;
    dataDirectory: string;
    children: Snippet;
    titleSuffix?: string;
    searchVisible?: boolean;
    navigationDisabled?: boolean;
    activeNavigation?: NavigationKey;
    connectionStatus?: string;
    taskStatus?: string;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
    onNavigate?: (target: NavigationKey) => void;
  }

  let {
    pageTitle,
    dataDirectory,
    children,
    titleSuffix,
    searchVisible = false,
    navigationDisabled = false,
    activeNavigation = "home",
    connectionStatus = t("shell.status.defaultConnection"),
    taskStatus = t("shell.status.noTasks"),
    onMinimize,
    onToggleMaximize,
    onClose,
    onNavigate,
  }: Props = $props();

  const navigation = [
    { key: "home" as const, name: "home" as const, labelKey: "nav.home" },
    { key: "instances" as const, name: "box" as const, labelKey: "nav.instances" },
    { key: "resources" as const, name: "compass" as const, labelKey: "nav.resources" },
    { key: "netplay" as const, name: "wifi" as const, labelKey: "nav.netplay" },
    { key: "tasks" as const, name: "task" as const, labelKey: "nav.tasks" },
    { key: "data" as const, name: "database" as const, labelKey: "nav.data" },
    { key: "settings" as const, name: "settings" as const, labelKey: "nav.settings" },
  ];

  // 自定义背景:纯色改变量,图片压暗铺底(减少动画时降级),主题包叠加配色(高对比忽略)。
  const backgroundStyle = $derived.by(() => {
    const value = uiBackground();
    if (value.type === "color") {
      return `--bg-window: ${value.color}; --bg-app: ${value.color}`;
    }
    if (value.type === "image" && uiMotion() !== "reduce") {
      const url = uiBackgroundImageUrl();
      if (url) {
        return `background-image: linear-gradient(rgba(14, 14, 18, 0.8), rgba(14, 14, 18, 0.8)), url(${url}); background-size: cover; background-position: center`;
      }
      return "";
    }
    if (value.type === "themePack" && uiContrast() !== "high") {
      return Object.entries(value.pack.colors)
        .map(([token, color]) => `--${token}: ${color}`)
        .join("; ");
    }
    return "";
  });
</script>

<div
  class="window"
  data-theme={uiTheme()}
  data-motion={uiMotion()}
  data-contrast={uiContrast()}
  data-background={uiBackground().type}
  style={backgroundStyle}
>
  <header class="titlebar" data-tauri-drag-region="deep">
    <span class="brand-mark">M</span>
    <span class="titlebar-name">
      <strong>MoyuMax</strong>{#if titleSuffix} — {titleSuffix}{/if}
    </span>
    <span class="titlebar-spacer" data-tauri-drag-region></span>
    <div class="window-controls">
      <button aria-label={t("shell.window.minimize")} onclick={() => void onMinimize()}><span class="minimize-glyph"></span></button>
      <button aria-label={t("shell.window.maximize")} onclick={() => void onToggleMaximize()}><span class="maximize-glyph"></span></button>
      <button class="close" aria-label={t("shell.window.close")} onclick={() => void onClose()}><span class="close-glyph"></span></button>
    </div>
  </header>

  <div class="app-body">
    <nav class:nav-disabled={navigationDisabled} class="navrail" aria-label={t("shell.navAria")}>
      {#each navigation as item}
        {@const active = item.key === activeNavigation}
        <button
          class:active
          class="nav-item"
          aria-label={t(item.labelKey)}
          aria-current={active ? "page" : undefined}
          disabled={navigationDisabled}
          onclick={() => onNavigate?.(item.key)}
        >
          <Icon name={item.name} />
          <span>{t(item.labelKey)}</span>
        </button>
      {/each}
      <span class="nav-spacer"></span>
      {#if shellAccount().loaded && shellAccount().kind !== null}
        {@const account = shellAccount()}
        {@const avatarUrl = account.avatarFailed ? "" : skinAvatarUrl(account.playerUuid, account.kind)}
        <button
          class="account"
          aria-label={t("shell.account.aria")}
          disabled={navigationDisabled}
          onclick={() => { requestSettingsPage("accounts"); onNavigate?.("settings"); }}
        >
          {#if avatarUrl}
            <img class="avatar avatar-img" src={avatarUrl} alt="" onerror={() => markAvatarFailed()} />
          {:else}
            <span class="avatar">{account.name.slice(0, 1) || "?"}</span>
          {/if}
          <span>
            <strong>{account.name}</strong>
            <small>{account.kind === "microsoft" ? t("shell.account.microsoft") : account.kind === "authlib" ? t("shell.account.authlib") : t("shell.account.offline")}</small>
          </span>
        </button>
      {:else}
        <button
          class="account"
          aria-label={t("shell.account.aria")}
          disabled={navigationDisabled}
          onclick={() => { requestSettingsPage("accounts"); onNavigate?.("settings"); }}
        >
          <span class="avatar">?</span>
          <span><strong>{t("shell.account.notLoggedIn")}</strong><small>{t("shell.account.addHint")}</small></span>
        </button>
      {/if}
    </nav>

    <section class="main-area">
      <header class="topbar">
        <strong>{pageTitle}</strong>
        {#if searchVisible}
          <button class="searchbox" aria-label={t("shell.search.aria")} disabled>
            <Icon name="search" size={14} />
            <span>{t("shell.search.placeholder")}</span>
            <kbd>Ctrl K</kbd>
          </button>
        {/if}
      </header>

      {@render children()}

      <footer class="statusbar">
        <span><Icon name="wifi" size={12} /> {connectionStatus}</span>
        <span>{taskStatus}</span>
        <span class="status-right">
          <Icon name="disk" size={12} /> {t("shell.statusbar.data")} {dataDirectory}<b>v0.1.0-preview.1</b>
        </span>
      </footer>
    </section>
  </div>
</div>
