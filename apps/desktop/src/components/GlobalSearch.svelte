<script lang="ts">
  import { tick } from "svelte";

  import { requestSettingsPage } from "../accounts.svelte";
  import { t } from "../i18n.svelte";
  import { closeGlobalSearch } from "../search.svelte";
  import type { CrashReport, ManagedInstance, NavigationKey } from "../runtime";
  import Icon from "./Icon.svelte";

  interface Props {
    instances: ManagedInstance[];
    crashReports: CrashReport[];
    onOpenInstance: (instance: ManagedInstance) => void;
    onOpenCrash: (report: CrashReport) => void;
    onNavigate: (target: NavigationKey) => void;
  }

  let { instances, crashReports, onOpenInstance, onOpenCrash, onNavigate }: Props = $props();

  interface PageEntry {
    label: string;
    keywords: string;
    run: () => void;
  }

  let query = $state("");
  let inputEl = $state<HTMLInputElement>();
  let activeIndex = $state(0);

  const pageEntries: PageEntry[] = [
    { label: "首页", keywords: "home shouye", run: () => go("home") },
    { label: "实例", keywords: "instances shili", run: () => go("instances") },
    { label: "资源", keywords: "resources ziyuan mod", run: () => go("resources") },
    { label: "联机", keywords: "netplay lianji easytier", run: () => go("netplay") },
    { label: "任务", keywords: "tasks renwu", run: () => go("tasks") },
    { label: "数据", keywords: "data shuju recycle", run: () => go("data") },
    { label: "账户", keywords: "accounts zhanghu login", run: () => go("accounts") },
    { label: "设置", keywords: "settings shezhi", run: () => go("settings") },
  ];

  const settingsEntries: PageEntry[] = [
    { label: "通用", keywords: "general language close tray", run: () => goSettings("general") },
    { label: "外观与主题", keywords: "appearance theme background", run: () => goSettings("appearance") },
    { label: "下载", keywords: "download speed source", run: () => goSettings("download") },
    { label: "内存", keywords: "memory java xmx", run: () => goSettings("memory") },
    { label: "Java 环境", keywords: "java runtime jdk", run: () => goSettings("java") },
    { label: "备份", keywords: "backup world", run: () => goSettings("backups") },
    { label: "更新", keywords: "update upgrade", run: () => goSettings("updates") },
    { label: "开发者", keywords: "dev developer cli", run: () => goSettings("dev") },
    { label: "关于", keywords: "about version", run: () => goSettings("about") },
  ];

  interface ResultGroup {
    titleKey: string;
    items: { label: string; sub: string; run: () => void }[];
  }

  const groups = $derived.by((): ResultGroup[] => {
    const needle = query.trim().toLowerCase();
    const match = (text: string, keywords = "") =>
      !needle || text.toLowerCase().includes(needle) || keywords.toLowerCase().includes(needle);
    const result: ResultGroup[] = [];
    const pages = pageEntries
      .filter((entry) => match(entry.label, entry.keywords))
      .map((entry) => ({ label: entry.label, sub: "", run: entry.run }));
    if (pages.length > 0) result.push({ titleKey: "search.group.pages", items: pages });
    const settings = settingsEntries
      .filter((entry) => match(entry.label, entry.keywords))
      .map((entry) => ({ label: entry.label, sub: t("search.sub.settings"), run: entry.run }));
    if (settings.length > 0) result.push({ titleKey: "search.group.settings", items: settings });
    const instanceItems = instances
      .filter((instance) => match(instance.name, `${instance.gameVersion} ${instance.loaderKind}`))
      .slice(0, 8)
      .map((instance) => ({
        label: instance.name,
        sub: `${instance.gameVersion} · ${instance.loaderKind}`,
        run: () => {
          onOpenInstance(instance);
          closeGlobalSearch();
        },
      }));
    if (instanceItems.length > 0) result.push({ titleKey: "search.group.instances", items: instanceItems });
    const crashItems = crashReports
      .filter((report) => match(report.title, instances.find((instance) => instance.id === report.instanceId)?.name ?? ""))
      .slice(0, 5)
      .map((report) => ({
        label: report.title,
        sub: instances.find((instance) => instance.id === report.instanceId)?.name ?? "",
        run: () => {
          onOpenCrash(report);
          closeGlobalSearch();
        },
      }));
    if (crashItems.length > 0) result.push({ titleKey: "search.group.crashes", items: crashItems });
    return result;
  });

  const flatItems = $derived(groups.flatMap((group) => group.items));

  function go(target: NavigationKey): void {
    onNavigate(target);
    closeGlobalSearch();
  }

  function goSettings(page: string): void {
    requestSettingsPage(page);
    onNavigate("settings");
    closeGlobalSearch();
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.stopPropagation();
      closeGlobalSearch();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      activeIndex = Math.min(flatItems.length - 1, Math.max(0, activeIndex + delta));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const item = flatItems[activeIndex] ?? flatItems[0];
      item?.run();
    }
  }

  $effect(() => {
    query = "";
    activeIndex = 0;
    void tick().then(() => inputEl?.focus());
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="modal-mask search-mask" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) closeGlobalSearch(); }}>
  <div class="search-panel" role="dialog" aria-modal="true" aria-label={t("shell.search.aria")}>
    <div class="search-input-row">
      <Icon name="search" size={15} />
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder={t("shell.search.placeholder")}
        aria-label={t("shell.search.aria")}
        oninput={() => { activeIndex = 0; }}
      />
      <kbd>Ctrl K</kbd>
    </div>
    <div class="search-results">
      {#if flatItems.length === 0}
        <p class="dim" style="padding:14px 4px">{t("search.noResults")}</p>
      {/if}
      {#each groups as group}
        <div class="search-group">{t(group.titleKey)}</div>
        {#each group.items as item}
          {@const index = flatItems.indexOf(item)}
          <button class="search-item" class:active={index === activeIndex} onclick={item.run}>
            <span class="si-label">{item.label}</span>
            {#if item.sub}<span class="si-sub">{item.sub}</span>{/if}
          </button>
        {/each}
      {/each}
    </div>
  </div>
</div>

<style>
  .search-mask {
    align-items: start;
    display: flex;
    justify-content: center;
    padding-top: 96px;
  }
  .search-panel {
    width: 560px;
    max-width: calc(100% - 48px);
    background: rgba(18, 36, 46, 0.97);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    box-shadow: var(--shadow-2);
    overflow: hidden;
  }
  .search-input-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    color: var(--text-3);
  }
  .search-input-row input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--text-1);
    font-size: 14px;
    outline: none;
    font-family: var(--font);
  }
  .search-input-row kbd {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-3);
    border: 1px solid var(--glass-border);
    border-radius: 6px;
    padding: 1px 6px;
  }
  .search-results {
    max-height: 380px;
    overflow: hidden auto;
    padding: 8px;
  }
  .search-group {
    font-size: 11px;
    color: var(--text-3);
    letter-spacing: 0.06em;
    padding: 8px 10px 4px;
  }
  .search-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-radius: var(--r);
    background: transparent;
    color: var(--text-1);
    font-family: var(--font);
    font-size: 13px;
    cursor: pointer;
  }
  .search-item:hover,
  .search-item.active {
    background: var(--accent-soft);
  }
  .search-item .si-label {
    font-weight: 600;
  }
  .search-item .si-sub {
    font-size: 11.5px;
    color: var(--text-3);
    margin-left: auto;
  }
</style>
