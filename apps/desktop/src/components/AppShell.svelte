<script lang="ts">
  import type { Snippet } from "svelte";

  import { markAvatarFailed, shellAccount, skinAvatarUrl } from "../accounts.svelte";
  import { netplayRoom, refreshNetplayRoom, setNetplayRoom } from "../netplay.svelte";
  import { t, uiBackground, uiBackgroundImageUrl, uiContrast, uiMotion, uiTheme } from "../i18n.svelte";
  import type { MoyuRuntime, NavigationKey } from "../runtime";
  import Fish from "./Fish.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    pageTitle: string;
    dataDirectory?: string;
    children: Snippet;
    titleSuffix?: string;
    /** 二级页返回按钮 */
    onBack?: (() => void) | undefined;
    searchVisible?: boolean;
    onSearch?: (() => void) | undefined;
    navigationDisabled?: boolean;
    activeNavigation?: NavigationKey;
    /** 在线状态:false 时标题栏网络点变灰黄 */
    online?: boolean;
    connectionStatus?: string;
    /** 活动任务数:导航与标题栏任务入口的角标 */
    taskCount?: number;
    /** 实例数:实例导航项角标 */
    instanceCount?: number;
    taskStatus?: string;
    runtime?: MoyuRuntime | undefined;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
    onNavigate?: (target: NavigationKey) => void;
  }

  let {
    pageTitle,
    children,
    titleSuffix,
    onBack = undefined,
    onSearch = undefined,
    navigationDisabled = false,
    activeNavigation = "home",
    online = true,
    connectionStatus = t("shell.status.defaultConnection"),
    taskCount = 0,
    instanceCount = 0,
    runtime = undefined,
    onMinimize,
    onToggleMaximize,
    onClose,
    onNavigate,
  }: Props = $props();

  $effect(() => {
    if (runtime) {
      void refreshNetplayRoom(runtime);
      const timer = setInterval(() => void refreshNetplayRoom(runtime), 5000);
      return () => clearInterval(timer);
    }
  });

  async function leaveRoom(): Promise<void> {
    if (!runtime) return;
    try {
      await runtime.stopNetplayRoom();
      setNetplayRoom(null);
    } catch {
      // 离开失败时由下一次轮询收敛
    }
  }

  const navigation = [
    { key: "home" as const, labelKey: "nav.home" },
    { key: "instances" as const, labelKey: "nav.instances" },
    { key: "resources" as const, labelKey: "nav.resources" },
    { key: "netplay" as const, labelKey: "nav.netplay" },
    { key: "tasks" as const, labelKey: "nav.tasks" },
    { key: "data" as const, labelKey: "nav.data" },
  ];

  // 自定义背景:纯色改变量,图片压暗铺底(减少动画时降级),主题包叠加配色(高对比忽略)。
  const backgroundStyle = $derived.by(() => {
    const value = uiBackground();
    if (value.type === "color") {
      return `--bg-window: ${value.color}; --bg-grad: ${value.color}`;
    }
    if (value.type === "image" && uiMotion() !== "reduce") {
      const url = uiBackgroundImageUrl();
      if (url) {
        return `background-image: linear-gradient(rgba(8, 19, 28, 0.82), rgba(8, 19, 28, 0.82)), url(${url}); background-size: cover; background-position: center`;
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

  function openAccounts(): void {
    onNavigate?.("accounts");
  }
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
    {#if onBack}
      <button class="tb-back" aria-label={t("shell.back")} onclick={onBack}>
        <Icon name="arrow-left" size={15} />
      </button>
    {/if}
    <span class="tb-title">{pageTitle}</span>
    {#if titleSuffix}<span class="tb-sub">{titleSuffix}</span>{/if}
    <span class="tb-spacer" data-tauri-drag-region></span>
    <button class="tb-tool" aria-label={t("shell.search.aria")} disabled={!onSearch} onclick={() => onSearch?.()}>
      <Icon name="search" size={13} />
      {t("shell.search.label")}
    </button>
    <button class="tb-tool" aria-label={t("shell.tasks.aria")} disabled={navigationDisabled} onclick={() => onNavigate?.("tasks")}>
      {t("nav.tasks")}
      {#if taskCount > 0}<span class="tag accent" style="height:18px;padding:0 7px;font-size:10.5px">{taskCount}</span>{/if}
    </button>
    <span class="tb-tool" role="status" aria-label={t("shell.status.connectionAria")}>
      <span class="dot" class:off={!online}></span>{connectionStatus}
    </span>
    <button class="tb-win" aria-label={t("shell.window.minimize")} onclick={() => void onMinimize()}><i class="min-line"></i></button>
    <button class="tb-win" aria-label={t("shell.window.maximize")} onclick={() => void onToggleMaximize()}>▢</button>
    <button class="tb-win close" aria-label={t("shell.window.close")} onclick={() => void onClose()}>✕</button>
  </header>

  <div class="shell">
    <nav class:nav-disabled={navigationDisabled} class="navrail" aria-label={t("shell.navAria")}>
      <div class="nav-brand">
        <Fish variant="logo" />
        <span class="name">MoyuMax</span>
      </div>
      {#each navigation as item}
        {@const active = item.key === activeNavigation}
        <button
          class:active
          class="nav-item"
          aria-current={active ? "page" : undefined}
          disabled={navigationDisabled}
          onclick={() => onNavigate?.(item.key)}
        >
          <span>{t(item.labelKey)}</span>
          {#if item.key === "instances" && instanceCount > 0}
            <span class="badge" aria-hidden="true">{instanceCount}</span>
          {:else if item.key === "tasks" && taskCount > 0}
            <span class="badge" aria-hidden="true">{taskCount}</span>
          {/if}
        </button>
      {/each}
      <div class="nav-foot">
        {#if shellAccount().loaded && shellAccount().kind !== null}
          {@const account = shellAccount()}
          {@const avatarUrl = account.avatarFailed ? "" : skinAvatarUrl(account.playerUuid, account.kind)}
          <button
            class="nav-account"
            class:active={activeNavigation === "accounts"}
            aria-label={t("shell.account.aria")}
            aria-current={activeNavigation === "accounts" ? "page" : undefined}
            disabled={navigationDisabled}
            onclick={openAccounts}
          >
            <span class="avatar">
              {#if avatarUrl}
                <img src={avatarUrl} alt="" onerror={() => markAvatarFailed()} />
              {:else}
                {account.name.slice(0, 1) || "?"}
              {/if}
            </span>
            <div>
              <div class="a-name">{account.name}</div>
              <div class="a-type">{account.kind === "microsoft" ? t("shell.account.microsoft") : account.kind === "authlib" ? t("shell.account.authlib") : t("shell.account.offline")}</div>
            </div>
          </button>
        {:else}
          <button
            class="nav-account"
            class:active={activeNavigation === "accounts"}
            aria-label={t("shell.account.aria")}
            aria-current={activeNavigation === "accounts" ? "page" : undefined}
            disabled={navigationDisabled}
            onclick={openAccounts}
          >
            <span class="avatar">?</span>
            <div>
              <div class="a-name">{t("shell.account.notLoggedIn")}</div>
              <div class="a-type">{t("shell.account.addHint")}</div>
            </div>
          </button>
        {/if}
        <button
          class="nav-item"
          class:active={activeNavigation === "settings"}
          aria-current={activeNavigation === "settings" ? "page" : undefined}
          disabled={navigationDisabled}
          onclick={() => onNavigate?.("settings")}
        >
          <span>{t("nav.settings")}</span>
        </button>
      </div>
    </nav>

    {@render children()}
  </div>

  {#if netplayRoom() && activeNavigation !== "netplay"}
    <div class="netplay-float" role="status" aria-label={t("netplay.float.aria")}>
      <span class="netplay-float-dot" aria-hidden="true"></span>
      <span class="netplay-float-info">
        <strong>{netplayRoom()?.networkName}</strong>
        <small>{netplayRoom()?.virtualIp}</small>
      </span>
      <button class="netplay-float-btn" onclick={() => onNavigate?.("netplay")}>{t("netplay.float.open")}</button>
      {#if runtime}
        <button class="netplay-float-btn danger" onclick={() => void leaveRoom()}>{t("netplay.float.leave")}</button>
      {/if}
    </div>
  {/if}
</div>
