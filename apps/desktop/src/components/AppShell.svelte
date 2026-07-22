<script lang="ts">
  import type { Snippet } from "svelte";

  import Icon from "./Icon.svelte";

  interface Props {
    pageTitle: string;
    dataDirectory: string;
    children: Snippet;
    titleSuffix?: string;
    searchVisible?: boolean;
    navigationDisabled?: boolean;
    activeNavigation?: "home" | "instances" | "resources" | "tasks" | "data" | "settings";
    connectionStatus?: string;
    taskStatus?: string;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    pageTitle,
    dataDirectory,
    children,
    titleSuffix,
    searchVisible = false,
    navigationDisabled = false,
    activeNavigation = "home",
    connectionStatus = "本地模式 · 未进行联网检查",
    taskStatus = "无活动任务",
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  const navigation = [
    { key: "home" as const, name: "home" as const, label: "首页" },
    { key: "instances" as const, name: "box" as const, label: "实例" },
    { key: "resources" as const, name: "compass" as const, label: "资源" },
    { key: "tasks" as const, name: "task" as const, label: "任务" },
    { key: "data" as const, name: "database" as const, label: "数据" },
    { key: "settings" as const, name: "settings" as const, label: "设置" },
  ];
</script>

<div class="window" data-theme="system">
  <header class="titlebar" data-tauri-drag-region>
    <span class="brand-mark">M</span>
    <span class="titlebar-name">
      <strong>MoyuMax</strong>{#if titleSuffix} — {titleSuffix}{/if}
    </span>
    <span class="titlebar-spacer" data-tauri-drag-region></span>
    <div class="window-controls">
      <button aria-label="最小化" onclick={() => void onMinimize()}><span class="minimize-glyph"></span></button>
      <button aria-label="最大化或还原" onclick={() => void onToggleMaximize()}><span class="maximize-glyph"></span></button>
      <button class="close" aria-label="关闭" onclick={() => void onClose()}><span class="close-glyph"></span></button>
    </div>
  </header>

  <div class="app-body">
    <nav class:nav-disabled={navigationDisabled} class="navrail" aria-label="主导航">
      {#each navigation as item}
        {@const active = item.key === activeNavigation}
        <button
          class:active
          class="nav-item"
          aria-current={active ? "page" : undefined}
          disabled={navigationDisabled || !active}
        >
          <Icon name={item.name} />
          <span>{item.label}</span>
        </button>
      {/each}
      <span class="nav-spacer"></span>
      <button class="account" aria-label="添加账户" disabled>
        <span class="avatar">?</span>
        <span><strong>未登录</strong><small>点击添加账户</small></span>
      </button>
    </nav>

    <section class="main-area">
      <header class="topbar">
        <strong>{pageTitle}</strong>
        {#if searchVisible}
          <button class="searchbox" aria-label="全局搜索" disabled>
            <Icon name="search" size={14} />
            <span>搜索实例、内容、设置…</span>
            <kbd>Ctrl K</kbd>
          </button>
        {/if}
      </header>

      {@render children()}

      <footer class="statusbar">
        <span><Icon name="wifi" size={12} /> {connectionStatus}</span>
        <span>{taskStatus}</span>
        <span class="status-right">
          <Icon name="disk" size={12} /> 数据 {dataDirectory}<b>v0.1.0-preview.1</b>
        </span>
      </footer>
    </section>
  </div>
</div>
