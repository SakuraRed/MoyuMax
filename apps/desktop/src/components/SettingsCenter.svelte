<script lang="ts">
  import { onMount, tick } from "svelte";

  import { consumeSettingsPage } from "../accounts.svelte";
  import {
    applyUiPreferences,
    refreshBackgroundImage,
    t,
    UI_CONTRASTS,
    UI_LANGUAGES,
    UI_MOTIONS,
    UI_THEMES,
    uiContrast,
    uiLanguage,
    uiMotion,
    uiTheme,
    type UiContrast,
    type UiLanguage,
    type UiMotion,
    type UiTheme,
  } from "../i18n.svelte";
  import { formatBytes } from "../installation";
  import type {
    JavaDeleteOutcome,
    JavaEnvironment,
    LaunchOptions,
    ManagedInstance,
    MoyuRuntime,
    NavigationKey,
    OnboardingSelection,
    ReferencingInstance,
    ReleaseInfo,
    SourcePolicy,
    UiBackground,
    WindowCloseBehavior,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";

  type SettingsPage =
    | "general"
    | "download"
    | "source"
    | "java"
    | "game"
    | "storage"
    | "appearance"
    | "accessibility"
    | "network"
    | "updates"
    | "privacy"
    | "dev"
    | "about";

  const SETTINGS_NAV: { key: SettingsPage; labelKey: string }[] = [
    { key: "general", labelKey: "settings.nav.general" },
    { key: "download", labelKey: "settings.nav.download" },
    { key: "source", labelKey: "settings.nav.source" },
    { key: "java", labelKey: "settings.nav.java" },
    { key: "game", labelKey: "settings.nav.game" },
    { key: "storage", labelKey: "settings.nav.storage" },
    { key: "appearance", labelKey: "settings.nav.appearance" },
    { key: "accessibility", labelKey: "settings.nav.accessibility" },
    { key: "network", labelKey: "settings.nav.network" },
    { key: "updates", labelKey: "settings.nav.updates" },
    { key: "privacy", labelKey: "settings.nav.privacy" },
    { key: "dev", labelKey: "settings.nav.dev" },
    { key: "about", labelKey: "settings.nav.about" },
  ];

  // 历史子页请求（如旧的 accounts/backups 直达）映射到新分区。
  const LEGACY_PAGE_MAP: Record<string, SettingsPage> = {
    accounts: "general",
    memory: "game",
    backups: "storage",
  };

  // 设置搜索索引：名称与说明经 t() 解析，跟随界面语言。
  interface SearchEntry {
    id: string;
    page: SettingsPage;
    nameKey: string;
    descKey?: string;
  }

  const SEARCH_ENTRIES: SearchEntry[] = [
    { id: "language", page: "general", nameKey: "settings.general.language.name", descKey: "settings.general.language.desc" },
    { id: "closeBehavior", page: "general", nameKey: "settings.general.closeBehavior.name", descKey: "settings.general.closeBehavior.desc" },
    { id: "accounts", page: "general", nameKey: "settings.general.accounts.name", descKey: "settings.general.accounts.desc" },
    { id: "concurrency", page: "download", nameKey: "settings.download.concurrencyLabel", descKey: "settings.download.concurrencyDesc" },
    { id: "speedLimit", page: "download", nameKey: "settings.download.speedLimit.name", descKey: "settings.download.speedLimit.desc" },
    { id: "sourcePolicy", page: "source", nameKey: "settings.source.groupAria" },
    { id: "javaEnvironments", page: "java", nameKey: "settings.java.heading", descKey: "settings.java.description" },
    { id: "memory", page: "game", nameKey: "settings.memory.title", descKey: "settings.memory.description" },
    { id: "backups", page: "storage", nameKey: "settings.backup.title", descKey: "settings.backup.description" },
    { id: "theme", page: "appearance", nameKey: "appearance.themeLabel" },
    { id: "background", page: "appearance", nameKey: "appearance.background.label" },
    { id: "motion", page: "accessibility", nameKey: "appearance.motionLabel", descKey: "appearance.motionDesc" },
    { id: "contrast", page: "accessibility", nameKey: "appearance.contrastLabel", descKey: "appearance.contrastDesc" },
    { id: "proxy", page: "network", nameKey: "settings.download.proxyLabel", descKey: "settings.download.proxyHint" },
    { id: "updateCheck", page: "updates", nameKey: "settings.update.title", descKey: "settings.update.description" },
    { id: "telemetry", page: "privacy", nameKey: "settings.general.telemetry" },
    { id: "cli", page: "dev", nameKey: "settings.dev.cliLabel", descKey: "settings.dev.cliHint" },
    { id: "version", page: "about", nameKey: "settings.about.version" },
    { id: "dataDirectory", page: "about", nameKey: "settings.general.dataDirectory" },
  ];

  const SOURCE_CARDS: {
    kind: "mirrorFirst" | "officialFirst" | "custom";
    nameKey: string;
    descKey: string;
  }[] = [
    { kind: "mirrorFirst", nameKey: "settings.source.mirrorFirst.name", descKey: "settings.source.mirrorFirst.desc" },
    { kind: "officialFirst", nameKey: "settings.source.officialFirst.name", descKey: "settings.source.officialFirst.desc" },
    { kind: "custom", nameKey: "settings.source.custom.name", descKey: "settings.source.custom.desc" },
  ];

  // 全局限速档位：0 表示不限速，与任务中心同一能力。
  const SPEED_PRESETS: ({ bytes: number; labelKey: string } | { bytes: number; label: string })[] = [
    { bytes: 0, labelKey: "settings.download.speedLimit.unlimited" },
    { bytes: 5 * 1024 * 1024, label: "5 MB/s" },
    { bytes: 10 * 1024 * 1024, label: "10 MB/s" },
  ];

  interface Props {
    runtime: MoyuRuntime;
    settings: OnboardingSelection;
    onNavigate: (target: NavigationKey) => void;
    onMinimize: () => Promise<void>;
    onToggleMaximize: () => Promise<void>;
    onClose: () => Promise<void>;
  }

  let {
    runtime,
    settings,
    onNavigate,
    onMinimize,
    onToggleMaximize,
    onClose,
  }: Props = $props();

  const requestedPage = consumeSettingsPage();

  let environments = $state<JavaEnvironment[]>([]);
  let deletedEnvironments = $state<JavaEnvironment[]>([]);
  let instances = $state<ManagedInstance[]>([]);
  let errorMessage = $state("");
  let notice = $state("");
  let busy = $state("");
  let deleteTarget = $state<JavaEnvironment | null>(null);
  let deleteAffected = $state<ReferencingInstance[]>([]);
  let deleteDialog = $state<HTMLElement | null>(null);
  let assignTarget = $state("");
  let assignInstance = $state("");
  let backupInterval = $state(30);
  let backupKeep = $state(20);
  let staticSettingsLoaded = $state(false);
  let subPage = $state<SettingsPage>(
    SETTINGS_NAV.some((item) => item.key === requestedPage)
      ? (requestedPage as SettingsPage)
      : (LEGACY_PAGE_MAP[requestedPage ?? ""] ?? "general"),
  );
  let cliEnabled = $state(false);
  let updateChecks = $state(true);
  let checkingUpdates = $state(false);
  let updateResult = $state<"none" | ReleaseInfo | null>("none");
  let downloading = $state(false);
  let downloadedPath = $state("");
  let backgroundType = $state<"default" | "color" | "image" | "themePack">("default");
  let backgroundColor = $state("#1b1b1f");
  let backgroundPackName = $state("");
  let backgroundBusy = $state(false);
  let uiPreferencesLoaded = $state(false);
  let memoryMode = $state<"auto" | "custom">("auto");
  let memoryMin = $state("");
  let memoryMax = $state("");
  let autoMemory = $state<LaunchOptions | null>(null);
  let memoryLoaded = $state(false);
  let savingMemory = $state(false);
  let downloadConcurrency = $state("24");
  let speedLimitBytes = $state(0);
  let closeBehavior = $state<WindowCloseBehavior>("ask");
  let sourcePolicyKind = $state<"mirrorFirst" | "officialFirst" | "custom">("mirrorFirst");
  let customMinecraftBase = $state("");
  let customModrinthBase = $state("");
  let proxyMode = $state<"system" | "direct" | "custom">("system");
  let proxyUrl = $state("");
  let searchQuery = $state("");
  let highlightId = $state("");
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  let mainEl = $state<HTMLElement | undefined>();

  onMount(() => {
    void refresh();
  });

  $effect(() => {
    if (deleteTarget) {
      void tick().then(() => {
        deleteDialog?.querySelector<HTMLElement>("[data-dialog-autofocus]")?.focus();
      });
    }
  });

  const searchMatches = $derived.by(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return [];
    return SEARCH_ENTRIES.filter((entry) => {
      const text = `${t(entry.nameKey)} ${entry.descKey ? t(entry.descKey) : ""}`.toLowerCase();
      return text.includes(query);
    }).slice(0, 8);
  });

  const speedOptions = $derived.by(() => {
    const options: { bytes: number; label: string }[] = SPEED_PRESETS.map((preset) => ({
      bytes: preset.bytes,
      label: "labelKey" in preset ? t(preset.labelKey) : preset.label,
    }));
    if (speedLimitBytes > 0 && !options.some((option) => option.bytes === speedLimitBytes)) {
      options.push({
        bytes: speedLimitBytes,
        label: `${Math.round(speedLimitBytes / 1024 / 1024)} MB/s`,
      });
    }
    return options;
  });

  async function refresh(): Promise<void> {
    errorMessage = "";
    try {
      [environments, deletedEnvironments, instances] = await Promise.all([
        runtime.listJavaEnvironments(),
        runtime.listDeletedJavaEnvironments(),
        runtime.listInstances(),
      ]);
      if (!staticSettingsLoaded) {
        const [backupSettings, cliState, updateChecksState, storedBackground, concurrency, speedLimit, closeBehaviorState, sourcePolicy, proxyPreference] = await Promise.all([
          runtime.getWorldBackupSettings(),
          runtime.getCliEnabled(),
          runtime.getUpdateChecksEnabled(),
          runtime.getUiBackground(),
          runtime.getDownloadConcurrency(),
          runtime.getDownloadSpeedLimit(),
          runtime.getWindowCloseBehavior(),
          runtime.getDownloadSourcePolicy(),
          runtime.getProxyPreference(),
        ]);
        backupInterval = backupSettings.intervalMinutes;
        backupKeep = backupSettings.keepCount;
        cliEnabled = cliState;
        updateChecks = updateChecksState;
        backgroundType = storedBackground.type;
        downloadConcurrency = String(concurrency);
        speedLimitBytes = speedLimit;
        closeBehavior = closeBehaviorState;
        sourcePolicyKind = sourcePolicy.kind;
        if (sourcePolicy.kind === "custom") {
          customMinecraftBase = sourcePolicy.minecraftBase ?? "";
          customModrinthBase = sourcePolicy.modrinthBase ?? "";
        }
        proxyMode = proxyPreference.mode;
        if (proxyPreference.mode === "custom") {
          proxyUrl = proxyPreference.url;
        }
        if (storedBackground.type === "color") backgroundColor = storedBackground.color;
        if (storedBackground.type === "themePack") {
          backgroundPackName = `${storedBackground.pack.name}（${storedBackground.pack.author}）`;
        }
        staticSettingsLoaded = true;
      }
      if (!memoryLoaded) {
        const [preference, auto] = await Promise.all([
          runtime.getGlobalLaunchPreference(),
          runtime.getAutoLaunchOptions(),
        ]);
        autoMemory = auto;
        if (preference.mode === "custom") {
          memoryMode = "custom";
          memoryMin = String(preference.minMib);
          memoryMax = String(preference.maxMib);
        }
        memoryLoaded = true;
      }
      if (!uiPreferencesLoaded) {
        const preferences = await runtime.getUiPreferences();
        applyStoredUiPreferences(preferences);
        uiPreferencesLoaded = true;
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  // 仅接受受支持的偏好值，非法存储值保持当前默认不变。
  function applyStoredUiPreferences(stored: {
    theme: string;
    language: string;
    motion: string;
    contrast: string;
  }): void {
    const nextTheme = UI_THEMES.find((entry) => entry.value === stored.theme)?.value;
    const nextLanguage = UI_LANGUAGES.find((entry) => entry.value === stored.language)?.value;
    const nextMotion = UI_MOTIONS.find((entry) => entry.value === stored.motion)?.value;
    const nextContrast = UI_CONTRASTS.find((entry) => entry.value === stored.contrast)?.value;
    applyUiPreferences({
      theme: nextTheme,
      language: nextLanguage,
      motion: nextMotion,
      contrast: nextContrast,
    });
  }

  async function selectTheme(value: UiTheme): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiTheme(value);
      applyUiPreferences({ theme: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectLanguage(value: UiLanguage): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiLanguage(value);
      applyUiPreferences({ language: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectMotion(value: UiMotion): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiMotion(value);
      applyUiPreferences({ motion: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectContrast(value: UiContrast): Promise<void> {
    errorMessage = "";
    try {
      await runtime.setUiContrast(value);
      applyUiPreferences({ contrast: value });
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectCloseBehavior(value: WindowCloseBehavior): Promise<void> {
    if (value === closeBehavior) return;
    const previous = closeBehavior;
    closeBehavior = value;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setWindowCloseBehavior(value);
      notice = t("settings.general.closeBehavior.saved");
    } catch (error) {
      closeBehavior = previous;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function applySpeedLimit(bytes: number): Promise<void> {
    if (bytes === speedLimitBytes) return;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setDownloadSpeedLimit(bytes);
      speedLimitBytes = bytes;
      notice = t("settings.download.speedLimit.saved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function toggleCli(checked: boolean): Promise<void> {
    const previous = cliEnabled;
    cliEnabled = checked;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setCliEnabled(checked);
      notice = checked ? t("settings.dev.cliEnabled") : t("settings.dev.cliDisabled");
    } catch (error) {
      cliEnabled = previous;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function toggleUpdateChecks(checked: boolean): Promise<void> {
    const previous = updateChecks;
    updateChecks = checked;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setUpdateChecksEnabled(checked);
    } catch (error) {
      updateChecks = previous;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function checkUpdates(): Promise<void> {
    checkingUpdates = true;
    errorMessage = "";
    notice = "";
    updateResult = "none";
    downloadedPath = "";
    try {
      updateResult = await runtime.checkForUpdates();
      if (updateResult === null) {
        notice = t("settings.update.upToDate");
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      checkingUpdates = false;
    }
  }

  async function downloadUpdate(): Promise<void> {
    if (updateResult === "none" || updateResult === null) return;
    downloading = true;
    errorMessage = "";
    notice = "";
    downloadedPath = "";
    try {
      downloadedPath = await runtime.downloadUpdateInstaller(updateResult);
      notice = t("settings.update.downloadDone");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      downloading = false;
    }
  }

  async function openDownloaded(): Promise<void> {
    if (!downloadedPath) return;
    errorMessage = "";
    try {
      await runtime.openUpdateLocation(downloadedPath);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function applyBackground(value: UiBackground): Promise<void> {
    backgroundBusy = true;
    errorMessage = "";
    notice = "";
    try {
      await runtime.setUiBackground(value);
      applyUiPreferences({ background: value });
      await refreshBackgroundImage(runtime);
      backgroundType = value.type;
      backgroundPackName =
        value.type === "themePack" ? `${value.pack.name}（${value.pack.author}）` : "";
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      backgroundBusy = false;
    }
  }

  async function selectBackgroundType(event: Event): Promise<void> {
    const next = (event.currentTarget as HTMLSelectElement).value as typeof backgroundType;
    if (next === "default") {
      await applyBackground({ type: "default" });
    } else {
      backgroundType = next;
    }
  }

  async function pickAndImportImage(): Promise<void> {
    errorMessage = "";
    try {
      const path = await runtime.pickBackgroundImage();
      if (!path) return;
      const background = await runtime.importBackgroundImage(path);
      await applyBackground(background);
      notice = t("appearance.background.imageApplied");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function pickAndImportThemePack(): Promise<void> {
    errorMessage = "";
    try {
      const path = await runtime.pickThemePackFile();
      if (!path) return;
      const pack = await runtime.importThemePack(path);
      await applyBackground({ type: "themePack", pack });
      notice = t("appearance.background.packApplied").replace("{name}", pack.name);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveBackupInterval(): Promise<void> {
    errorMessage = "";
    notice = "";
    if (!Number.isFinite(backupInterval) || backupInterval < 0 || backupInterval > 1440) {
      errorMessage = t("settings.backup.intervalInvalid");
      return;
    }
    try {
      await runtime.setWorldBackupIntervalMinutes(Math.floor(backupInterval));
      notice = backupInterval === 0 ? t("settings.backup.intervalDisabled") : t("settings.backup.intervalSaved").replace("{minutes}", String(Math.floor(backupInterval)));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveDownloadConcurrency(): Promise<void> {
    errorMessage = "";
    notice = "";
    const value = Number(downloadConcurrency);
    if (!Number.isInteger(value) || value < 1 || value > 32) {
      errorMessage = t("settings.download.concurrencyInvalid");
      return;
    }
    try {
      await runtime.setDownloadConcurrency(value);
      notice = t("settings.download.concurrencySaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveSourcePolicy(kind: "mirrorFirst" | "officialFirst" | "custom"): Promise<void> {
    errorMessage = "";
    notice = "";
    sourcePolicyKind = kind;
    if (kind === "custom") return; // 自定义需填基址后点保存
    const policy: SourcePolicy = { kind };
    try {
      await runtime.setDownloadSourcePolicy(policy);
      notice = t("settings.download.sourceSaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveCustomSourcePolicy(): Promise<void> {
    errorMessage = "";
    notice = "";
    const minecraftBase = customMinecraftBase.trim();
    const modrinthBase = customModrinthBase.trim();
    if (!minecraftBase && !modrinthBase) {
      errorMessage = t("settings.download.sourceCustomRequired");
      return;
    }
    for (const base of [minecraftBase, modrinthBase]) {
      if (base && !base.startsWith("https://")) {
        errorMessage = t("settings.download.sourceCustomHttps");
        return;
      }
    }
    const policy: SourcePolicy = {
      kind: "custom",
      minecraftBase: minecraftBase || null,
      modrinthBase: modrinthBase || null,
    };
    try {
      await runtime.setDownloadSourcePolicy(policy);
      notice = t("settings.download.sourceSaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveProxyMode(mode: "system" | "direct" | "custom"): Promise<void> {
    errorMessage = "";
    notice = "";
    proxyMode = mode;
    if (mode === "custom") return; // 自定义需填地址后点保存
    try {
      await runtime.setProxyPreference({ mode });
      notice = t("settings.download.proxySaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveCustomProxy(): Promise<void> {
    errorMessage = "";
    notice = "";
    const url = proxyUrl.trim();
    if (!url) {
      errorMessage = t("settings.download.proxyInvalid");
      return;
    }
    try {
      await runtime.setProxyPreference({ mode: "custom", url });
      notice = t("settings.download.proxySaved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveBackupKeep(): Promise<void> {
    errorMessage = "";
    notice = "";
    if (!Number.isFinite(backupKeep) || backupKeep < 1 || backupKeep > 100) {
      errorMessage = t("settings.backup.keepInvalid");
      return;
    }
    try {
      await runtime.setWorldBackupKeepCount(Math.floor(backupKeep));
      notice = t("settings.backup.keepSaved").replace("{count}", String(Math.floor(backupKeep)));
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function selectMemoryMode(mode: "auto" | "custom"): Promise<void> {
    if (mode === memoryMode) return;
    errorMessage = "";
    notice = "";
    if (mode === "auto") {
      savingMemory = true;
      try {
        await runtime.setGlobalLaunchPreference({ mode: "auto" });
        memoryMode = "auto";
        notice = t("settings.memory.savedAuto");
      } catch (error) {
        errorMessage = error instanceof Error ? error.message : String(error);
      } finally {
        savingMemory = false;
      }
      return;
    }
    // 切到自定义:用当前自动分配值预填,保存后才写入全局自定义。
    memoryMode = "custom";
    if (!memoryMin || !memoryMax) {
      memoryMin = String(autoMemory?.minimumMemoryMib ?? 512);
      memoryMax = String(autoMemory?.maximumMemoryMib ?? 4096);
    }
  }

  async function saveMemory(): Promise<void> {
    errorMessage = "";
    notice = "";
    const minimum = Number(memoryMin.trim());
    const maximum = Number(memoryMax.trim());
    if (
      !Number.isInteger(minimum) ||
      !Number.isInteger(maximum) ||
      minimum < 256 ||
      maximum < minimum ||
      maximum > 65536
    ) {
      errorMessage = t("settings.memory.invalid");
      return;
    }
    savingMemory = true;
    try {
      await runtime.setGlobalLaunchPreference({ mode: "custom", minMib: minimum, maxMib: maximum });
      notice = t("settings.memory.saved");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      savingMemory = false;
    }
  }

  function distributionName(environment: JavaEnvironment): string {
    return environment.distribution === "azulZulu" ? "Azul Zulu" : environment.distribution;
  }

  function statusLabel(environment: JavaEnvironment): string {
    if (!environment.healthy && environment.status === "ready") return t("settings.java.status.missingFiles");
    const keys: Record<string, string> = {
      planned: "settings.java.status.planned",
      installing: "settings.java.status.installing",
      ready: "settings.java.status.ready",
      missing: "settings.java.status.missing",
      failed: "settings.java.status.failed",
      deleted: "settings.java.status.deleted",
    };
    const key = keys[environment.status];
    return key ? t(key) : environment.status;
  }

  function environmentMeta(environment: JavaEnvironment): string {
    const refs = environment.referencingInstances.length > 0
      ? t("settings.java.refsNamed").replace(
          "{names}",
          environment.referencingInstances.map((entry) => entry.name).join(t("settings.java.namesSeparator")),
        )
      : t("settings.java.noRefs");
    return t("settings.java.lineMeta")
      .replace("{size}", formatBytes(environment.sizeBytes))
      .replace("{refs}", refs);
  }

  async function verify(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const healthy = await runtime.verifyJavaEnvironment(environment.id);
      notice = healthy
        ? t("settings.java.verifyOk").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion)
        : t("settings.java.verifyMissing").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion);
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function openLocation(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      await runtime.openJavaLocation(environment.id);
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function requestDelete(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const outcome: JavaDeleteOutcome = await runtime.deleteJavaEnvironment(
        environment.id,
        false,
      );
      if (outcome.kind === "requiresConfirmation") {
        deleteTarget = environment;
        deleteAffected = outcome.instances;
      } else {
        notice = t("settings.java.deleted").replace("{distribution}", distributionName(environment)).replace("{version}", environment.fullVersion);
        await refresh();
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    busy = deleteTarget.id;
    errorMessage = "";
    try {
      await runtime.deleteJavaEnvironment(deleteTarget.id, true);
      notice = t("settings.java.deletedWithRefs")
        .replace("{distribution}", distributionName(deleteTarget))
        .replace("{version}", deleteTarget.fullVersion)
        .replace("{count}", String(deleteAffected.length));
      deleteTarget = null;
      deleteAffected = [];
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function restore(environment: JavaEnvironment): Promise<void> {
    busy = environment.id;
    errorMessage = "";
    try {
      const restored = await runtime.restoreJavaEnvironment(environment.id);
      notice = t("settings.java.restored").replace("{distribution}", distributionName(restored)).replace("{version}", restored.fullVersion);
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  async function applyAssignment(environment: JavaEnvironment): Promise<void> {
    if (assignTarget !== environment.id) return;
    if (!assignInstance) return;
    busy = environment.id;
    errorMessage = "";
    try {
      await runtime.setInstanceJavaEnvironment(assignInstance, environment.id);
      const instance = instances.find((entry) => entry.id === assignInstance);
      notice = t("settings.java.assigned")
        .replace("{name}", instance?.name ?? assignInstance)
        .replace("{distribution}", distributionName(environment))
        .replace("{version}", environment.fullVersion);
      assignTarget = "";
      assignInstance = "";
      await refresh();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      busy = "";
    }
  }

  function navTo(page: SettingsPage): void {
    subPage = page;
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightId = "";
  }

  function onSearchKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" && searchMatches.length > 0) {
      event.preventDefault();
      void jumpToSetting(searchMatches[0]!);
    } else if (event.key === "Escape") {
      searchQuery = "";
    }
  }

  async function jumpToSetting(entry: SearchEntry): Promise<void> {
    subPage = entry.page;
    searchQuery = "";
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightId = entry.id;
    await tick();
    mainEl
      ?.querySelector<HTMLElement>(`[data-setting-id="${entry.id}"]`)
      ?.scrollIntoView({ block: "center" });
    highlightTimer = setTimeout(() => {
      highlightId = "";
    }, 2400);
  }
</script>

<AppShell
  pageTitle={t("settings.pageTitle")}
  activeNavigation="settings"
  {onNavigate}
  connectionStatus={t("settings.connectionStatus")}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content" data-scroll-region="main" bind:this={mainEl}>
    <div class="set-search">
      <svg width="15" height="15" viewBox="0 0 15 15" fill="none" aria-hidden="true"><circle cx="6.5" cy="6.5" r="4.8" stroke="currentColor" stroke-width="1.5"/><path d="M10.3 10.3 L14 14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      <input
        class="input"
        placeholder={t("settings.search.placeholder")}
        aria-label={t("settings.search.aria")}
        bind:value={searchQuery}
        onkeydown={onSearchKeydown}
      />
      {#if searchQuery.trim()}
        <div class="set-search-results panel">
          {#if searchMatches.length === 0}
            <div class="dim" style="padding:8px 10px">{t("settings.search.empty")}</div>
          {:else}
            {#each searchMatches as match}
              <button type="button" class="set-search-hit" onclick={() => void jumpToSetting(match)}>
                <span>{t(match.nameKey)}</span>
                <span class="dim">{t(SETTINGS_NAV.find((item) => item.key === match.page)?.labelKey ?? "")}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <div class="set-layout">
      <nav class="set-nav" aria-label={t("settings.nav.aria")}>
        {#each SETTINGS_NAV as item}
          <button
            type="button"
            class="sn-item"
            class:on={subPage === item.key}
            aria-current={subPage === item.key ? "page" : undefined}
            onclick={() => navTo(item.key)}
          >{t(item.labelKey)}</button>
        {/each}
      </nav>

      <div class="set-main java-content">
        {#if errorMessage}
          <div class="banner danger" role="alert" style="margin-bottom:12px"><strong>{t("settings.error.title")}</strong><span>{errorMessage}</span></div>
        {/if}
        {#if notice}
          <div class="banner info java-notice" role="status" style="margin-bottom:12px"><span>{notice}</span></div>
        {/if}

        {#if subPage === "general"}
          <section aria-labelledby="general-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="general-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.general")}</h2>
            </div>
            <div class="panel pad">
              <div class="set-row" class:hl={highlightId === "language"} data-setting-id="language">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.general.language.name")}</div>
                  <div class="sr-desc">{t("settings.general.language.desc")}</div>
                </div>
                <select
                  class="input"
                  style="width:180px"
                  aria-label={t("appearance.languageAria")}
                  value={uiLanguage()}
                  onchange={(event) => void selectLanguage(event.currentTarget.value as UiLanguage)}
                >
                  {#each UI_LANGUAGES as languageOption}
                    <option value={languageOption.value}>{languageOption.label}</option>
                  {/each}
                </select>
              </div>
              <div class="set-row" class:hl={highlightId === "closeBehavior"} data-setting-id="closeBehavior">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.general.closeBehavior.name")}</div>
                  <div class="sr-desc">{t("settings.general.closeBehavior.desc")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("settings.general.closeBehavior.name")}>
                  <button type="button" class:on={closeBehavior === "ask"} onclick={() => void selectCloseBehavior("ask")}>{t("settings.general.closeBehavior.ask")}</button>
                  <button type="button" class:on={closeBehavior === "minimizeToTray"} onclick={() => void selectCloseBehavior("minimizeToTray")}>{t("settings.general.closeBehavior.tray")}</button>
                  <button type="button" class:on={closeBehavior === "exit"} onclick={() => void selectCloseBehavior("exit")}>{t("settings.general.closeBehavior.exit")}</button>
                </div>
              </div>
              <div class="set-row" class:hl={highlightId === "accounts"} data-setting-id="accounts">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.general.accounts.name")}</div>
                  <div class="sr-desc">{t("settings.general.accounts.desc")}</div>
                </div>
                <button type="button" class="btn small secondary" onclick={() => onNavigate("accounts")}>{t("settings.general.accounts.open")}</button>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "download"}
          <section aria-labelledby="download-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="download-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.download")}</h2>
            </div>
            <div class="panel pad">
              <div class="set-row" class:hl={highlightId === "concurrency"} data-setting-id="concurrency">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.download.concurrencyLabel")}</div>
                  <div class="sr-desc">{t("settings.download.concurrencyDesc")}</div>
                </div>
                <input
                  class="input"
                  style="width:110px"
                  type="number"
                  min="1"
                  max="32"
                  aria-label={t("settings.download.concurrencyAria")}
                  bind:value={downloadConcurrency}
                  onchange={() => void saveDownloadConcurrency()}
                />
              </div>
              <div class="set-row" class:hl={highlightId === "speedLimit"} data-setting-id="speedLimit">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.download.speedLimit.name")}</div>
                  <div class="sr-desc">{t("settings.download.speedLimit.desc")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("settings.download.speedLimit.aria")}>
                  {#each speedOptions as option}
                    <button type="button" class:on={speedLimitBytes === option.bytes} onclick={() => void applySpeedLimit(option.bytes)}>{option.label}</button>
                  {/each}
                </div>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "source"}
          <section aria-labelledby="source-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="source-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.source")}</h2>
            </div>
            <div role="radiogroup" aria-label={t("settings.source.groupAria")}>
              {#each SOURCE_CARDS as card}
                <div class="panel src-card" class:sel={sourcePolicyKind === card.kind}>
                  <button
                    type="button"
                    class="src-pick"
                    role="radio"
                    aria-checked={sourcePolicyKind === card.kind}
                    onclick={() => void saveSourcePolicy(card.kind)}
                  >
                    <span class="radio-dot" class:on={sourcePolicyKind === card.kind} aria-hidden="true"></span>
                    <span class="lr-main">
                      <span class="lr-name">
                        {t(card.nameKey)}{#if card.kind === "mirrorFirst"}<span class="tag accent" style="height:18px;padding:0 7px;font-size:10.5px;margin-left:4px">{t("settings.source.defaultTag")}</span>{/if}
                      </span>
                      <span class="lr-sub" style="white-space:normal">{t(card.descKey)}</span>
                    </span>
                  </button>
                  {#if card.kind === "custom"}
                    <div class="row" style="margin-top:10px">
                      <span class="tag warn">{t("settings.source.custom.limitTag")}</span>
                      <span class="dim">{t("settings.source.custom.limitDesc")}</span>
                    </div>
                    {#if sourcePolicyKind === "custom"}
                      <div class="col" style="gap:10px;margin-top:12px">
                        <div class="row" style="gap:10px;flex-wrap:wrap">
                          <input class="input" style="flex:1;min-width:200px" type="text" aria-label={t("settings.download.sourceCustomMinecraftAria")} placeholder="https://bmclapi2.bangbang93.com" bind:value={customMinecraftBase} />
                          <input class="input" style="flex:1;min-width:200px" type="text" aria-label={t("settings.download.sourceCustomModrinthAria")} placeholder="https://mod.mcimirror.top" bind:value={customModrinthBase} />
                        </div>
                        <div class="row">
                          <button type="button" class="btn small primary" onclick={() => void saveCustomSourcePolicy()}>{t("settings.download.sourceCustomSave")}</button>
                        </div>
                      </div>
                    {/if}
                  {/if}
                </div>
              {/each}
            </div>
            <div class="panel pad" style="margin-top:16px">
              <div class="panel-title" style="font-size:13.5px">{t("settings.source.platformTitle")}</div>
              <div class="row" style="margin-top:8px;flex-wrap:wrap">
                <span style="font-size:13px;font-weight:600">CurseForge</span>
                <span class="tag neutral">{t("settings.source.curseforge.tag")}</span>
                <span class="dim">{t("settings.source.curseforge.desc")}</span>
              </div>
              <div class="row" style="margin-top:8px;flex-wrap:wrap">
                <span style="font-size:13px;font-weight:600">Modrinth</span>
                <span class="tag ok">{t("settings.source.modrinth.tag")}</span>
                <span class="dim">{t("settings.source.modrinth.desc")}</span>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "java"}
          <section aria-labelledby="java-title">
            <div class="banner info" style="margin-bottom:14px">
              <span>{t("settings.java.banner")}</span>
            </div>
            <div class="panel">
              <div class="row spread" style="padding:16px 18px 10px">
                <h2 id="java-title" class="panel-title" style="margin:0">{t("settings.java.heading")}</h2>
              </div>
              {#if environments.length === 0 && deletedEnvironments.length === 0}
                <div class="dim" style="padding:0 18px 16px">{t("settings.java.emptyTitle")} — {t("settings.java.emptyDescription")}</div>
              {/if}
              {#each environments as environment}
                <div class="list-row" style="padding:14px 18px">
                  <div class="lr-main">
                    <div class="lr-name">{distributionName(environment)} <span class="mono" style="color:var(--text-2);font-weight:400">{environment.fullVersion}</span> <span class="dim">{environment.architecture}</span></div>
                    <div class="lr-sub mono" style="white-space:normal">{environment.homeDirectory}</div>
                    <div class="dim" style="margin-top:3px">{environmentMeta(environment)}</div>
                  </div>
                  <span class="tag" class:ok={environment.healthy && environment.status === "ready"} class:warn={!(environment.healthy && environment.status === "ready")}><span class="cdot"></span>{statusLabel(environment)}</span>
                  <div class="java-acts">
                    <button type="button" class="btn small ghost" disabled={busy === environment.id} onclick={() => void verify(environment)}>{t("settings.java.verify")}</button>
                    <button
                      type="button"
                      class="btn small ghost"
                      disabled={busy === environment.id || instances.length === 0}
                      onclick={() => {
                        assignTarget = assignTarget === environment.id ? "" : environment.id;
                        assignInstance = instances[0]?.id ?? "";
                      }}
                    >{t("settings.java.assign")}</button>
                    <button type="button" class="btn small ghost" disabled={busy === environment.id} onclick={() => void openLocation(environment)}>{t("settings.java.openLocation")}</button>
                    <button type="button" class="btn small danger-soft" disabled={busy === environment.id} onclick={() => void requestDelete(environment)}>{t("settings.java.delete")}</button>
                  </div>
                </div>
                {#if assignTarget === environment.id}
                  <div class="java-assign" role="group" aria-label={t("settings.java.assignAria")}>
                    <div class="row" style="gap:10px;flex-wrap:wrap">
                      <label class="dim" for="assign-instance-{environment.id}">{t("settings.java.assignTarget")}</label>
                      <select id="assign-instance-{environment.id}" class="input" bind:value={assignInstance}>
                        {#each instances as instance}
                          <option value={instance.id}>{t("settings.java.instanceOption").replace("{name}", instance.name).replace("{version}", instance.gameVersion).replace("{loader}", instance.loaderKind)}</option>
                        {/each}
                      </select>
                      <button type="button" class="btn small primary" disabled={busy === environment.id || !assignInstance} onclick={() => void applyAssignment(environment)}>{t("settings.java.assignConfirm")}</button>
                    </div>
                    <div class="dim" style="margin-top:6px">{t("settings.java.assignHint")}</div>
                  </div>
                {/if}
              {/each}
            </div>
            <p class="dim" style="margin-top:12px">{t("settings.java.footerNote")}</p>
            {#if deletedEnvironments.length > 0}
              <div class="panel" style="margin-top:14px">
                <div class="row spread" style="padding:16px 18px 6px">
                  <div class="panel-title">{t("settings.java.deletedSectionTitle")}</div>
                </div>
                <div class="dim" style="padding:0 18px 10px">{t("settings.java.deletedSectionDescription")}</div>
                {#each deletedEnvironments as environment}
                  <div class="list-row" style="padding:12px 18px">
                    <div class="lr-main">
                      <div class="lr-name">{distributionName(environment)} <span class="mono" style="color:var(--text-2);font-weight:400">{environment.fullVersion}</span></div>
                      {#if environment.referencingInstances.length > 0}
                        <div class="lr-sub" style="white-space:normal">{t("settings.java.stillReferenced").replace("{names}", environment.referencingInstances.map((entry) => entry.name).join(t("settings.java.namesSeparator")))}</div>
                      {/if}
                    </div>
                    <span class="tag neutral">{t("settings.java.status.deleted")}</span>
                    <button type="button" class="btn small primary" disabled={busy === environment.id} onclick={() => void restore(environment)}>{t("settings.java.restore")}</button>
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

        {#if subPage === "game"}
          <section aria-labelledby="memory-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="memory-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.memory.title")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.memory.description")}</p>
              <div class="set-row" class:hl={highlightId === "memory"} data-setting-id="memory">
                <div class="sr-main">
                  <div class="sr-name">{memoryMode === "auto" ? t("settings.memory.modeAuto") : t("settings.memory.modeCustom")}</div>
                  <div class="sr-desc">{memoryMode === "auto" ? t("settings.memory.autoSummary").replace("{min}", String(autoMemory?.minimumMemoryMib ?? "…")).replace("{max}", String(autoMemory?.maximumMemoryMib ?? "…")) : t("settings.memory.customHint")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("settings.memory.title")}>
                  <button type="button" class:on={memoryMode === "auto"} disabled={savingMemory} onclick={() => void selectMemoryMode("auto")}>{t("settings.memory.modeAuto")}</button>
                  <button type="button" class:on={memoryMode === "custom"} disabled={savingMemory} onclick={() => void selectMemoryMode("custom")}>{t("settings.memory.modeCustom")}</button>
                </div>
              </div>
              {#if memoryMode === "custom"}
                <div class="set-row">
                  <div class="sr-main">
                    <div class="row" style="gap:12px;flex-wrap:wrap">
                      <label class="field" style="gap:4px">
                        <span class="dim">{t("settings.memory.minLabel")}</span>
                        <input class="input" style="width:120px" bind:value={memoryMin} type="text" inputmode="numeric" aria-label={t("settings.memory.minAria")} />
                      </label>
                      <label class="field" style="gap:4px">
                        <span class="dim">{t("settings.memory.maxLabel")}</span>
                        <input class="input" style="width:120px" bind:value={memoryMax} type="text" inputmode="numeric" aria-label={t("settings.memory.maxAria")} />
                      </label>
                    </div>
                  </div>
                  <button type="button" class="btn small primary" disabled={savingMemory} onclick={() => void saveMemory()}>{savingMemory ? t("settings.memory.saving") : t("settings.memory.save")}</button>
                </div>
              {/if}
            </div>
          </section>
        {/if}

        {#if subPage === "storage"}
          <section aria-labelledby="backup-settings-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="backup-settings-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.backup.title")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.backup.description")}</p>
              <div class="set-row" class:hl={highlightId === "backups"} data-setting-id="backups">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.backup.intervalLabel")}</div>
                </div>
                <input
                  class="input"
                  style="width:110px"
                  type="number"
                  min="0"
                  max="1440"
                  aria-label={t("settings.backup.intervalAria")}
                  bind:value={backupInterval}
                  onchange={() => void saveBackupInterval()}
                />
              </div>
              <div class="set-row">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.backup.keepLabel")}</div>
                </div>
                <input
                  class="input"
                  style="width:110px"
                  type="number"
                  min="1"
                  max="100"
                  aria-label={t("settings.backup.keepAria")}
                  bind:value={backupKeep}
                  onchange={() => void saveBackupKeep()}
                />
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "appearance"}
          <section aria-labelledby="appearance-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="appearance-title" class="panel-title" style="font-size:16px;margin:0">{t("appearance.title")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("appearance.description")}</p>
              <div class="set-row" class:hl={highlightId === "theme"} data-setting-id="theme">
                <div class="sr-main">
                  <div class="sr-name">{t("appearance.themeLabel")}</div>
                  <div class="sr-desc">{t("appearance.themeDesc")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("appearance.themeAria")}>
                  {#each UI_THEMES as themeOption}
                    <button type="button" class:on={uiTheme() === themeOption.value} onclick={() => void selectTheme(themeOption.value)}>{t(themeOption.labelKey)}</button>
                  {/each}
                </div>
              </div>
              <div class="set-row" class:hl={highlightId === "background"} data-setting-id="background">
                <div class="sr-main">
                  <div class="sr-name">{t("appearance.background.label")}</div>
                  <div class="sr-desc">{t("appearance.background.desc")}</div>
                  {#if backgroundPackName && backgroundType === "themePack"}
                    <div class="dim" style="margin-top:4px">{t("appearance.background.packActive")} {backgroundPackName}</div>
                  {/if}
                </div>
                <div class="bg-controls">
                  <select
                    class="input"
                    aria-label={t("appearance.background.label")}
                    value={backgroundType}
                    disabled={backgroundBusy}
                    onchange={(event) => void selectBackgroundType(event)}
                  >
                    <option value="default">{t("appearance.background.type.default")}</option>
                    <option value="color">{t("appearance.background.type.color")}</option>
                    <option value="image">{t("appearance.background.type.image")}</option>
                    <option value="themePack">{t("appearance.background.type.themePack")}</option>
                  </select>
                  {#if backgroundType === "color"}
                    <input
                      type="color"
                      aria-label={t("appearance.background.colorLabel")}
                      bind:value={backgroundColor}
                    />
                    <button type="button" class="btn small secondary" disabled={backgroundBusy} onclick={() => void applyBackground({ type: "color", color: backgroundColor })}>{t("appearance.background.applyColor")}</button>
                  {/if}
                  {#if backgroundType === "image"}
                    <button type="button" class="btn small ghost" disabled={backgroundBusy} onclick={() => void pickAndImportImage()}>{t("appearance.background.pickImage")}</button>
                    <button type="button" class="btn small ghost" disabled={backgroundBusy} onclick={() => void applyBackground({ type: "default" })}>{t("appearance.background.clearImage")}</button>
                  {/if}
                  {#if backgroundType === "themePack"}
                    <button type="button" class="btn small ghost" disabled={backgroundBusy} onclick={() => void pickAndImportThemePack()}>{t("appearance.background.importPack")}</button>
                    <button type="button" class="btn small ghost" disabled={backgroundBusy} onclick={() => void applyBackground({ type: "default" })}>{t("appearance.background.removePack")}</button>
                  {/if}
                </div>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "accessibility"}
          <section aria-labelledby="accessibility-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="accessibility-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.accessibility")}</h2>
            </div>
            <div class="panel pad">
              <div class="set-row" class:hl={highlightId === "motion"} data-setting-id="motion">
                <div class="sr-main">
                  <div class="sr-name">{t("appearance.motionLabel")}</div>
                  <div class="sr-desc">{t("appearance.motionDesc")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("appearance.motionAria")}>
                  {#each UI_MOTIONS as motionOption}
                    <button type="button" class:on={uiMotion() === motionOption.value} onclick={() => void selectMotion(motionOption.value)}>{t(motionOption.labelKey)}</button>
                  {/each}
                </div>
              </div>
              <div class="set-row" class:hl={highlightId === "contrast"} data-setting-id="contrast">
                <div class="sr-main">
                  <div class="sr-name">{t("appearance.contrastLabel")}</div>
                  <div class="sr-desc">{t("appearance.contrastDesc")}</div>
                </div>
                <div class="seg" role="group" aria-label={t("appearance.contrastAria")}>
                  {#each UI_CONTRASTS as contrastOption}
                    <button type="button" class:on={uiContrast() === contrastOption.value} onclick={() => void selectContrast(contrastOption.value)}>{t(contrastOption.labelKey)}</button>
                  {/each}
                </div>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "network"}
          <section aria-labelledby="network-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="network-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.network")}</h2>
            </div>
            <div class="panel pad">
              <div class="set-row" class:hl={highlightId === "proxy"} data-setting-id="proxy">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.download.proxyLabel")}</div>
                  <div class="sr-desc">{t("settings.download.proxyHint")}</div>
                </div>
                <select
                  class="input"
                  style="width:200px"
                  aria-label={t("settings.download.proxyAria")}
                  value={proxyMode}
                  onchange={(event) => void saveProxyMode(event.currentTarget.value as "system" | "direct" | "custom")}
                >
                  <option value="system">{t("settings.download.proxySystem")}</option>
                  <option value="direct">{t("settings.download.proxyDirect")}</option>
                  <option value="custom">{t("settings.download.proxyCustom")}</option>
                </select>
              </div>
              {#if proxyMode === "custom"}
                <div class="set-row">
                  <div class="sr-main">
                    <div class="sr-name">{t("settings.download.proxyUrlLabel")}</div>
                  </div>
                  <div class="row" style="flex:none;gap:8px">
                    <input class="input" style="width:230px" type="text" aria-label={t("settings.download.proxyUrlAria")} placeholder="http://127.0.0.1:10808" bind:value={proxyUrl} />
                    <button type="button" class="btn small primary" onclick={() => void saveCustomProxy()}>{t("settings.download.proxyCustomSave")}</button>
                  </div>
                </div>
              {/if}
            </div>
          </section>
        {/if}

        {#if subPage === "updates"}
          <section aria-labelledby="update-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="update-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.update.title")}</h2>
              <button type="button" class="btn small secondary" disabled={checkingUpdates || !updateChecks} onclick={() => void checkUpdates()}>
                {checkingUpdates ? t("settings.update.checking") : t("settings.update.check")}
              </button>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.update.description")}</p>
              <div class="set-row">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.update.currentVersion")}</div>
                </div>
                <strong>0.1.0</strong>
              </div>
              <div class="set-row" class:hl={highlightId === "updateCheck"} data-setting-id="updateCheck">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.update.promptLabel")}</div>
                  <div class="sr-desc">{t("settings.update.promptHint")}</div>
                </div>
                <button
                  type="button"
                  class="switch"
                  class:on={updateChecks}
                  role="switch"
                  aria-checked={updateChecks}
                  aria-label={t("settings.update.promptLabel")}
                  onclick={() => void toggleUpdateChecks(!updateChecks)}
                ></button>
              </div>
            </div>
            {#if updateResult !== "none" && updateResult !== null}
              <div class="panel pad" style="margin-top:12px" role="status">
                <div class="row">
                  <span class="tag accent">{t("settings.update.available")}</span>
                  <strong>{updateResult.tag}</strong>
                </div>
                {#if updateResult.notes}
                  <p class="muted" style="margin:8px 0 0">{updateResult.notes}</p>
                {/if}
                {#if updateResult.minAppVersion}
                  <p class="dim" style="margin:6px 0 0">{t("settings.update.minVersion").replace("{version}", updateResult.minAppVersion)}</p>
                {/if}
                <div class="row" style="margin-top:12px">
                  <button
                    type="button"
                    class="btn small primary"
                    disabled={downloading || !updateResult.installer}
                    onclick={() => void downloadUpdate()}
                  >{downloading ? t("settings.update.downloading") : t("settings.update.download")}</button>
                  {#if downloadedPath}
                    <button type="button" class="btn small ghost" onclick={() => void openDownloaded()}>{t("settings.update.openLocation")}</button>
                  {/if}
                </div>
                {#if downloadedPath}
                  <div class="dim mono" style="margin-top:8px;word-break:break-all">{downloadedPath}</div>
                {/if}
              </div>
            {/if}
          </section>
        {/if}

        {#if subPage === "privacy"}
          <section aria-labelledby="privacy-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="privacy-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.nav.privacy")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.privacy.description")}</p>
              <div class="set-row" class:hl={highlightId === "telemetry"} data-setting-id="telemetry">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.general.telemetry")}</div>
                  <div class="sr-desc">{t("settings.privacy.telemetryDesc")}</div>
                </div>
                <span class="tag neutral">{t("settings.general.telemetryOff")}</span>
              </div>
              <div class="set-row">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.general.isolation")}</div>
                  <div class="sr-desc">{t("settings.privacy.isolationDesc")}</div>
                </div>
                <span class="tag neutral">{settings.instanceIsolationEnabled ? t("settings.general.isolationOn") : t("settings.general.isolationOff")}</span>
              </div>
            </div>
          </section>
        {/if}

        {#if subPage === "dev"}
          <section aria-labelledby="dev-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="dev-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.dev.title")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.dev.description")}</p>
              <div class="set-row" class:hl={highlightId === "cli"} data-setting-id="cli">
                <div class="sr-main">
                  <div class="sr-name">{t("settings.dev.cliLabel")}</div>
                  <div class="sr-desc">{t("settings.dev.cliHint")}</div>
                  <div class="sr-desc" style="color:var(--warn)">{t("settings.dev.riskWarning")}</div>
                </div>
                <button
                  type="button"
                  class="switch"
                  class:on={cliEnabled}
                  role="switch"
                  aria-checked={cliEnabled}
                  aria-label={t("settings.dev.cliLabel")}
                  onclick={() => void toggleCli(!cliEnabled)}
                ></button>
              </div>
              {#if cliEnabled}
                <div style="margin-top:10px">
                  <code class="mono">moyumax-desktop.exe --cli instances list</code>
                  <div class="dim" style="margin-top:4px">{t("settings.dev.usageHint")}</div>
                </div>
              {/if}
            </div>
          </section>
        {/if}

        {#if subPage === "about"}
          <section aria-labelledby="about-title">
            <div class="row spread" style="margin-bottom:12px">
              <h2 id="about-title" class="panel-title" style="font-size:16px;margin:0">{t("settings.about.title")}</h2>
            </div>
            <div class="panel pad">
              <p class="panel-desc" style="margin:0 0 6px">{t("settings.about.description")}</p>
              <div class="set-row" class:hl={highlightId === "version"} data-setting-id="version">
                <div class="sr-main"><div class="sr-name">{t("settings.about.version")}</div></div>
                <span class="mono">0.1.0</span>
              </div>
              <div class="set-row" class:hl={highlightId === "dataDirectory"} data-setting-id="dataDirectory">
                <div class="sr-main"><div class="sr-name">{t("settings.general.dataDirectory")}</div></div>
                <span class="mono" style="word-break:break-all;text-align:right">{settings.dataDirectory}</span>
              </div>
              <div class="set-row">
                <div class="sr-main"><div class="sr-name">{t("settings.about.license")}</div></div>
                <span>GPL-3.0-only</span>
              </div>
              <div class="set-row">
                <div class="sr-main"><div class="sr-name">{t("settings.about.repository")}</div></div>
                <span class="mono" style="word-break:break-all;text-align:right">github.com/SakuraRed/MoyuMax</span>
              </div>
              <div class="set-row">
                <div class="sr-main"><div class="sr-name">{t("settings.about.sbom")}</div></div>
                <span class="mono" style="word-break:break-all;text-align:right">docs/SBOM.json · docs/THIRD-PARTY-LICENSES.md</span>
              </div>
            </div>
            <div class="banner warn" style="margin-top:12px">
              <span><strong>{t("settings.about.unsignedTitle")}</strong> — {t("settings.about.unsignedBody")}</span>
            </div>
          </section>
        {/if}
      </div>
    </div>
  </main>

  {#if deleteTarget}
    <div class="modal-mask" role="presentation">
      <div class="modal java-delete-modal" role="dialog" aria-modal="true" aria-labelledby="delete-java-title" bind:this={deleteDialog}>
        <h3 id="delete-java-title">{t("settings.java.deleteDialogTitle")}</h3>
        <div class="m-body">
          <p>{t("settings.java.deleteDialogInUse")}{t("settings.java.deleteDialogImpact")}{t("settings.java.deleteDialogNote")}</p>
          <div style="margin-top:12px;display:flex;flex-direction:column;gap:6px">
            {#each deleteAffected as instance}
              <div class="row" style="gap:8px">
                <span class="tag warn">{t("settings.java.deleteDialogAffected")}</span>
                <span style="font-size:13px">{t("settings.java.deleteDialogInstance").replace("{name}", instance.name)}</span>
              </div>
            {/each}
          </div>
        </div>
        <div class="m-acts">
          <button type="button" class="btn danger-soft" disabled={busy === deleteTarget.id} onclick={() => void confirmDelete()}>
            {t("settings.java.deleteDialogForce")}
          </button>
          <button type="button" class="btn primary" data-dialog-autofocus onclick={() => { deleteTarget = null; deleteAffected = []; }}>
            {t("common.cancel")}
          </button>
        </div>
        <div class="dim" style="margin-top:10px;text-align:right">{t("settings.java.deleteDialogFocusNote")}</div>
      </div>
    </div>
  {/if}
</AppShell>

<style>
  .set-search {
    position: relative;
    margin-bottom: 16px;
  }
  .set-search .input {
    width: 100%;
    padding-left: 34px;
  }
  .set-search > svg {
    position: absolute;
    left: 11px;
    top: 10px;
    color: var(--text-3);
    pointer-events: none;
  }
  .set-search-results {
    position: absolute;
    top: 42px;
    left: 0;
    right: 0;
    z-index: 30;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .set-search-hit {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    border-radius: var(--r);
    background: transparent;
    color: var(--text-1);
    font-family: var(--font);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }
  .set-search-hit:hover {
    background: var(--glass-strong);
  }

  .set-layout {
    display: flex;
    gap: 20px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .set-nav {
    width: 150px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 1px;
    position: sticky;
    top: 0;
  }
  .set-nav button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 7px 12px;
    border: none;
    border-radius: var(--r);
    background: transparent;
    color: var(--text-2);
    font-family: var(--font);
    font-size: 13px;
    cursor: pointer;
  }
  .set-nav button:hover {
    background: var(--glass);
    color: var(--text-1);
  }
  .set-nav button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .set-main {
    flex: 1 1 300px;
    min-width: 0;
  }
  /* 极窄(含 200% 放大):控件排不下时换行,别把说明文字压没 */
  .set-main .set-row {
    flex-wrap: wrap;
  }
  .set-main .set-row .sr-main {
    min-width: 160px;
  }

  /* 窄窗口:导航横排置顶 */
  @media (max-width: 900px) {
    .set-nav {
      width: 100%;
      flex-direction: row;
      flex-wrap: wrap;
      position: static;
    }
    .set-nav button {
      width: auto;
    }
  }

  /* 删除确认弹窗:小窗口(含 200% 放大)下收窄,保证落在视口内 */
  .modal.java-delete-modal {
    width: 440px;
  }

  /* 来源策略选项卡 */
  .src-card {
    padding: 16px 18px;
  }
  .src-card + .src-card {
    margin-top: 12px;
  }
  .src-card.sel {
    border-color: rgba(63, 216, 194, 0.45);
  }
  .src-pick {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    font-family: var(--font);
    cursor: pointer;
    padding: 0;
    text-align: left;
  }
  .src-pick .lr-main {
    flex: 1;
    min-width: 0;
    display: block;
  }
  .src-pick .lr-name {
    display: block;
    font-size: 13.5px;
    font-weight: 600;
  }
  .src-pick .lr-sub {
    display: block;
    font-size: 12px;
    color: var(--text-2);
    margin-top: 2px;
  }
  .radio-dot {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    flex: none;
    border: 1.5px solid var(--text-3);
    position: relative;
    display: inline-block;
    margin-top: 2px;
  }
  .radio-dot.on {
    border-color: var(--accent);
  }
  .radio-dot.on::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--accent);
  }

  /* Java 环境行内操作 */
  .java-acts {
    display: flex;
    gap: 2px;
    flex: none;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .java-assign {
    padding: 0 18px 14px;
  }

  .bg-controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }
  .bg-controls input[type="color"] {
    width: 48px;
    height: 36px;
    padding: 2px;
    border: 1px solid var(--glass-border);
    border-radius: var(--r);
    background: rgba(0, 0, 0, 0.22);
  }

  /* 搜索跳转高亮 */
  .set-row.hl {
    border: 1px solid var(--accent);
    border-radius: var(--r);
    background: var(--accent-soft);
    padding: 14px 12px;
    margin: 0 -12px;
    animation: hlpulse 1.2s ease-in-out 2;
  }
  @keyframes hlpulse {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgba(63, 216, 194, 0);
    }
    50% {
      box-shadow: 0 0 0 3px rgba(63, 216, 194, 0.12);
    }
  }
</style>
