<script lang="ts">
  import { tick } from "svelte";

  import { t } from "../i18n.svelte";
  import type { ModrinthVersionSummary } from "../runtime";
  import {
    buildVersionGroups,
    SNAPSHOT_GROUP_KEY,
    UNKNOWN_GROUP_KEY,
    type VersionGroupKind,
    type VersionGroupTarget,
    versionOptionLabel,
  } from "../version-groups";

  interface Props {
    versions: ModrinthVersionSummary[];
    kind: VersionGroupKind;
    target?: VersionGroupTarget | null;
    value: string;
    /** 提供「自动(最新兼容)」选项(value="")。 */
    showAuto?: boolean;
    disabled?: boolean;
    ariaLabel: string;
    onSelect: (versionId: string) => void;
  }

  let {
    versions,
    kind,
    target = null,
    value,
    showAuto = false,
    disabled = false,
    ariaLabel,
    onSelect,
  }: Props = $props();

  let open = $state(false);
  /** 二级视图:当前展开的游戏版本组;null 为一级(组列表)。 */
  let activeGroupKey = $state<string | null>(null);
  let root = $state<HTMLElement | null>(null);

  const groups = $derived(buildVersionGroups(versions, { kind, target }));
  /** 单一游戏版本时跳过一级,直接展开版本列表。 */
  const singleGroup = $derived(groups.length === 1 ? (groups[0] ?? null) : null);
  const activeGroup = $derived(groups.find((group) => group.key === activeGroupKey) ?? null);
  const currentVersion = $derived(versions.find((version) => version.id === value) ?? null);
  const triggerLabel = $derived(
    value === "" && showAuto
      ? t("resources.versions.auto")
      : currentVersion
        ? versionOptionLabel(currentVersion)
        : t("resources.versions.pickPlaceholder"),
  );

  function groupLabel(key: string): string {
    if (key === SNAPSHOT_GROUP_KEY) return t("resources.versions.snapshotGroup");
    if (key === UNKNOWN_GROUP_KEY) return t("resources.versions.otherGroup");
    return key;
  }

  async function toggleOpen(): Promise<void> {
    if (disabled) return;
    open = !open;
    activeGroupKey = singleGroup?.key ?? null;
    if (open) {
      await tick();
      root?.querySelector<HTMLElement>(".vp-row.current, .vp-row")?.focus();
    }
  }

  function close(): void {
    open = false;
    activeGroupKey = null;
  }

  function choose(versionId: string): void {
    onSelect(versionId);
    close();
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.stopPropagation();
      close();
    }
  }

  function onWindowPointerdown(event: PointerEvent): void {
    if (!open || !root) return;
    if (event.target instanceof Node && !root.contains(event.target)) close();
  }
</script>

<svelte:window onkeydown={onWindowKeydown} onpointerdown={onWindowPointerdown} />

<div class="vp" bind:this={root}>
  <button
    type="button"
    class="input vp-trigger"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={ariaLabel}
    {disabled}
    onclick={toggleOpen}
  >
    <span class="vp-current">{triggerLabel}</span>
    <span class="vp-caret" aria-hidden="true">{open ? "▴" : "▾"}</span>
  </button>

  {#if open}
    <div class="vp-panel" role="listbox" aria-label={ariaLabel} tabindex="-1">
      {#if activeGroup === null}
        {#if showAuto}
          <button
            type="button"
            class="vp-row"
            class:current={value === ""}
            role="option"
            aria-selected={value === ""}
            onclick={() => choose("")}
          >
            <span class="vp-name">{t("resources.versions.auto")}</span>
          </button>
        {/if}
        {#each groups as group}
          <button
            type="button"
            class="vp-row"
            role="option"
            aria-selected={false}
            onclick={() => { activeGroupKey = group.key; }}
          >
            <span class="vp-name">{groupLabel(group.key)}</span>
            {#if group.recommended}<span class="tag accent">{t("resources.versions.recommended")}</span>{/if}
            <span class="vp-meta">{t("resources.versions.groupCount").replace("{count}", String(group.versions.length))}</span>
            <span class="vp-chevron" aria-hidden="true">›</span>
          </button>
        {/each}
      {:else}
        <button type="button" class="vp-row vp-back" onclick={() => { activeGroupKey = singleGroup ? activeGroupKey : null; }}>
          <span aria-hidden="true">‹</span>
          <span class="vp-name">{groupLabel(activeGroup.key)}</span>
          {#if activeGroup.recommended}<span class="tag accent">{t("resources.versions.recommended")}</span>{/if}
        </button>
        {#each activeGroup.versions as version}
          <button
            type="button"
            class="vp-row"
            class:current={version.id === value}
            role="option"
            aria-selected={version.id === value}
            onclick={() => choose(version.id)}
          >
            <span class="vp-name">{versionOptionLabel(version)}</span>
            <span class="vp-meta">{version.datePublished.slice(0, 10)}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .vp {
    position: relative;
  }
  .vp-trigger {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    cursor: pointer;
    text-align: left;
  }
  .vp-trigger .vp-current {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vp-caret {
    color: var(--text-3);
    flex: none;
  }
  .vp-panel {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 6px);
    z-index: 45;
    max-height: 300px;
    overflow: hidden auto;
    background: rgba(18, 36, 46, 0.97);
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    box-shadow: var(--shadow-2);
    padding: 6px;
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
  }
  .vp-row {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .vp-row:hover {
    background: var(--glass-strong);
  }
  .vp-row.current {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .vp-row .vp-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .vp-row .vp-meta {
    margin-left: auto;
    color: var(--text-3);
    font-size: 11.5px;
    flex: none;
  }
  .vp-row .vp-chevron {
    color: var(--text-3);
    flex: none;
  }
  .vp-back {
    color: var(--text-2);
    border-bottom: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: var(--r) var(--r) 0 0;
    margin-bottom: 4px;
  }
</style>
