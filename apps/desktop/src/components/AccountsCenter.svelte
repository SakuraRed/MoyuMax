<script lang="ts">
  import { t } from "../i18n.svelte";
  import type {
    ContentInstallTask,
    InstallTask,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    tasks: InstallTask[];
    contentTasks: ContentInstallTask[];
    instances: ManagedInstance[];
    notice: string;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    tasks,
    contentTasks,
    instances,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  const activeTaskCount = $derived(
    tasks.filter((task) => !["completed", "cancelled"].includes(task.state)).length +
      contentTasks.filter((task) => !["completed", "cancelled"].includes(task.state)).length,
  );
</script>

<AppShell
  pageTitle={t("shell.account.pageTitle")}
  activeNavigation="accounts"
  taskCount={activeTaskCount}
  instanceCount={instances.length}
  {runtime}
  {onNavigate}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content">
    <p class="muted">{t("shell.account.pageTitle")}</p>
  </main>
</AppShell>
