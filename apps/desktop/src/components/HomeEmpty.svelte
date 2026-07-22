<script lang="ts">
  import type { OnboardingSelection } from "../runtime";
  import AppShell from "./AppShell.svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    settings: OnboardingSelection;
    notice: string;
    onInstall: () => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    settings,
    notice,
    onInstall,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();
</script>

<AppShell
  pageTitle="首页"
  dataDirectory={settings.dataDirectory}
  searchVisible
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content home-empty">
    <div class="empty-graphic" aria-hidden="true"></div>
    <h1>从安装第一个游戏开始</h1>
    <p>推荐稳定版会自动配好 Java、加载器和隔离环境。你不需要打开文件资源管理器、命令行或 Java 官网。</p>
    <button class="button primary large" onclick={onInstall}>安装第一个游戏</button>
    <small>
      也可以 <button class="inline-link" disabled>导入整合包</button> 或
      <button class="inline-link" disabled>从其他启动器迁移</button>（第二个公开版本提供）
    </small>
  </main>

  {#if notice}
    <div class="toast" role="status"><Icon name="info" size={16} /><span>{notice}</span></div>
  {/if}
  <div class="sr-live" aria-live="polite">{notice}</div>
</AppShell>
