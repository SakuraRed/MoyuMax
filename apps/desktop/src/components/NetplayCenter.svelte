<script lang="ts">
  import { onMount } from "svelte";

  import { t } from "../i18n.svelte";
  import type {
    LanBroadcastView,
    MoyuRuntime,
    NatReportView,
    NavigationKey,
    NetplayRoomView,
    OnboardingSelection,
    PortForwardView,
  } from "../runtime";
  import AppShell from "./AppShell.svelte";

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

  type NetplayTab = "room" | "forward" | "nat";

  const NETPLAY_TABS: { key: NetplayTab; labelKey: string }[] = [
    { key: "room", labelKey: "settings.network.room.title" },
    { key: "forward", labelKey: "settings.network.forward.title" },
    { key: "nat", labelKey: "settings.network.nat.title" },
  ];

  let tab = $state<NetplayTab>("room");
  let room = $state<NetplayRoomView | null>(null);
  let roomName = $state("");
  let roomSecret = $state("");
  let roomBusy = $state(false);
  let roomCopied = $state(false);
  let downloadProgress = $state<{ current: number; total: number } | null>(null);
  let forward = $state<PortForwardView | null>(null);
  let lanGame = $state<LanBroadcastView | null>(null);
  let detectingGame = $state(false);
  let forwardLocalPort = $state("25565");
  let forwardPublic = $state(true);
  let forwardConfirmOpen = $state(false);
  let forwardBusy = $state(false);
  let natReport = $state<NatReportView | null>(null);
  let natBusy = $state(false);
  let errorMessage = $state("");
  let notice = $state("");

  onMount(() => {
    void (async () => {
      try {
        [room, forward] = await Promise.all([
          runtime.getNetplayStatus(),
          runtime.getPortForward(),
        ]);
      } catch {
        // 状态读取失败不阻塞页面
      }
    })();
    return runtime.onNetplayDownloadProgress((event) => {
      downloadProgress = event.total > 0 ? event : null;
    });
  });

  function generateRoomName(): void {
    const alphabet = "abcdefghjkmnpqrstuvwxyz23456789";
    roomName = Array.from(
      { length: 8 },
      () => alphabet[Math.floor(Math.random() * alphabet.length)],
    ).join("");
  }

  async function submitRoom(isHost: boolean): Promise<void> {
    roomBusy = true;
    errorMessage = "";
    notice = "";
    downloadProgress = null;
    try {
      room = await runtime.startNetplayRoom(roomName.trim(), roomSecret, isHost);
      notice = isHost
        ? t("settings.network.room.created")
        : t("settings.network.room.joined");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      roomBusy = false;
      downloadProgress = null;
    }
  }

  async function leaveRoom(): Promise<void> {
    roomBusy = true;
    errorMessage = "";
    try {
      await runtime.stopNetplayRoom();
      room = null;
      notice = t("settings.network.room.left");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      roomBusy = false;
    }
  }

  async function copyRoomInfo(): Promise<void> {
    if (!room) return;
    try {
      await navigator.clipboard.writeText(
        t("settings.network.room.copyPayload")
          .replace("{name}", room.networkName),
      );
      roomCopied = true;
      setTimeout(() => { roomCopied = false; }, 1600);
    } catch {
      errorMessage = t("settings.network.room.copyFailed");
    }
  }

  async function detectGame(): Promise<void> {
    detectingGame = true;
    errorMessage = "";
    notice = "";
    try {
      lanGame = await runtime.detectLanGame();
      if (lanGame) {
        forwardLocalPort = String(lanGame.port);
        notice = t("settings.network.forward.detected").replace("{port}", String(lanGame.port));
      } else {
        errorMessage = t("settings.network.forward.notDetected");
      }
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      detectingGame = false;
    }
  }

  async function submitForward(): Promise<void> {
    if (forwardPublic && !forwardConfirmOpen) {
      forwardConfirmOpen = true;
      return;
    }
    if (!lanGame) {
      errorMessage = t("settings.network.forward.needDetect");
      return;
    }
    forwardBusy = true;
    errorMessage = "";
    notice = "";
    try {
      forward = await runtime.startPortForward(
        `0.0.0.0:${forwardLocalPort.trim()}`,
        `127.0.0.1:${lanGame.port}`,
        forwardPublic,
      );
      forwardConfirmOpen = false;
      notice = t("settings.network.forward.started");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      forwardBusy = false;
    }
  }

  async function stopForward(): Promise<void> {
    forwardBusy = true;
    errorMessage = "";
    try {
      await runtime.stopPortForward();
      forward = null;
      notice = t("settings.network.forward.stopped");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      forwardBusy = false;
    }
  }

  async function runNatDetect(): Promise<void> {
    natBusy = true;
    errorMessage = "";
    try {
      natReport = await runtime.detectNatType();
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      natBusy = false;
    }
  }

  function formatProgress(event: { current: number; total: number }): string {
    const mib = (value: number) => (value / 1024 / 1024).toFixed(1);
    return `${mib(event.current)} / ${mib(event.total)} MiB`;
  }
</script>

<AppShell
  pageTitle={t("nav.netplay")}
  dataDirectory={settings.dataDirectory}
  activeNavigation="netplay"
  {onNavigate}
  connectionStatus={room ? t("netplay.status.inRoom").replace("{name}", room.networkName) : t("netplay.status.idle")}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content settings-content">
    <div class="settings-layout">
      <nav class="settings-nav" aria-label={t("netplay.nav.aria")}>
        <div class="sn-group">EasyTier</div>
        {#each NETPLAY_TABS as item}
          <button
            class="sn-item"
            class:active={tab === item.key}
            aria-current={tab === item.key ? "page" : undefined}
            onclick={() => { tab = item.key; }}
          >{t(item.labelKey)}</button>
        {/each}
      </nav>

      <div class="settings-main" data-scroll-region="main">
        {#if errorMessage}
          <div class="error-block" role="alert"><strong>{t("settings.network.errorTitle")}</strong><span>{errorMessage}</span></div>
        {/if}
        {#if notice}
          <div class="java-notice" role="status">{notice}</div>
        {/if}

        {#if tab === "room"}
          <section class="backup-settings" aria-labelledby="netplay-title">
            <header>
              <div>
                <h2 id="netplay-title">{t("settings.network.room.title")}</h2>
                <p>{t("settings.network.room.description")}</p>
              </div>
            </header>
            {#if room}
              <article class="netplay-room-card">
                <div class="netplay-room-info">
                  <div class="netplay-room-line">
                    <strong class="netplay-room-name">{room.networkName}</strong>
                    <span class="netplay-badge">{room.isHost ? t("settings.network.room.hostBadge") : t("settings.network.room.memberBadge")}</span>
                  </div>
                  <small>{t("settings.network.room.virtualIp")}: <code>{room.virtualIp}</code></small>
                  <small>{t("settings.network.room.hint")}</small>
                </div>
                <div class="task-buttons">
                  <button class="button ghost compact" onclick={() => void copyRoomInfo()}>{roomCopied ? t("settings.network.room.copied") : t("settings.network.room.copy")}</button>
                  <button class="button danger-subtle compact" disabled={roomBusy} onclick={() => void leaveRoom()}>{roomBusy ? t("settings.network.room.leaving") : t("settings.network.room.leave")}</button>
                </div>
              </article>
            {:else}
              <div class="account-form" role="group" aria-label={t("settings.network.room.title")}>
                <label>
                  <span>{t("settings.network.room.nameLabel")}</span>
                  <div class="netplay-name-row">
                    <input bind:value={roomName} type="text" aria-label={t("settings.network.room.nameAria")} placeholder={t("settings.network.room.namePlaceholder")} />
                    <button class="button ghost compact" onclick={generateRoomName}>{t("settings.network.room.generate")}</button>
                  </div>
                </label>
                <label>
                  <span>{t("settings.network.room.secretLabel")}</span>
                  <input bind:value={roomSecret} type="password" aria-label={t("settings.network.room.secretAria")} autocomplete="off" />
                </label>
                <div class="local-content-actions">
                  <button class="button primary compact" disabled={roomBusy || !roomName.trim() || !roomSecret} onclick={() => void submitRoom(true)}>{t("settings.network.room.create")}</button>
                  <button class="button ghost compact" disabled={roomBusy || !roomName.trim() || !roomSecret} onclick={() => void submitRoom(false)}>{t("settings.network.room.join")}</button>
                </div>
                {#if roomBusy && downloadProgress}
                  <div class="netplay-progress" role="status" aria-label={t("settings.network.room.downloading")}>
                    <div class="netplay-progress-bar" style={`width: ${Math.round((downloadProgress.current / downloadProgress.total) * 100)}%`}></div>
                    <span>{t("settings.network.room.downloading")} {formatProgress(downloadProgress)}</span>
                  </div>
                {:else if roomBusy}
                  <small class="netplay-note" role="status">{t("settings.network.room.preparing")}</small>
                {/if}
                <small class="netplay-note">{t("settings.network.room.note")}</small>
              </div>
            {/if}
          </section>
        {:else if tab === "forward"}
          <section class="backup-settings" aria-labelledby="forward-title">
            <header>
              <div>
                <h2 id="forward-title">{t("settings.network.forward.title")}</h2>
                <p>{t("settings.network.forward.description")}</p>
              </div>
            </header>
            {#if forward}
              <article class="netplay-room-card">
                <div class="netplay-room-info">
                  <div class="netplay-room-line">
                    <strong><code>{forward.listen}</code> → <code>{forward.target}</code></strong>
                    {#if forward.publicBind}<span class="netplay-badge warn">{t("settings.network.forward.publicBadge")}</span>{/if}
                  </div>
                  <small>{t("settings.network.forward.running")}</small>
                </div>
                <div class="task-buttons">
                  <button class="button danger-subtle compact" disabled={forwardBusy} onclick={() => void stopForward()}>{t("settings.network.forward.stop")}</button>
                </div>
              </article>
            {:else}
              <div class="account-form" role="group" aria-label={t("settings.network.forward.title")}>
                <div class="netplay-detect-row">
                  <button class="button ghost compact" disabled={detectingGame} onclick={() => void detectGame()}>{detectingGame ? t("settings.network.forward.detecting") : t("settings.network.forward.detect")}</button>
                  {#if lanGame}
                    <span class="netplay-detect-result">{t("settings.network.forward.detectResult").replace("{port}", String(lanGame.port)).replace("{motd}", lanGame.motd)}</span>
                  {:else}
                    <span class="netplay-note">{t("settings.network.forward.detectHint")}</span>
                  {/if}
                </div>
                <label>
                  <span>{t("settings.network.forward.localPortLabel")}</span>
                  <input bind:value={forwardLocalPort} type="text" inputmode="numeric" aria-label={t("settings.network.forward.localPortAria")} />
                </label>
                <label class="auto-update-toggle">
                  <input type="checkbox" checked={forwardPublic} aria-label={t("settings.network.forward.publicLabel")} onchange={(event) => { forwardPublic = (event.currentTarget as HTMLInputElement).checked; }} />
                  <span>
                    <strong>{t("settings.network.forward.publicLabel")}</strong>
                    <small>{t("settings.network.forward.publicHint")}</small>
                  </span>
                </label>
                {#if forwardConfirmOpen}
                  <div class="warning-panel" role="alert">
                    <strong>{t("settings.network.forward.confirmTitle")}</strong>
                    <span>{t("settings.network.forward.confirmBody")}</span>
                  </div>
                {/if}
                <div class="local-content-actions">
                  <button class="button primary compact" disabled={forwardBusy || !lanGame || !forwardLocalPort.trim()} onclick={() => void submitForward()}>
                    {forwardConfirmOpen ? t("settings.network.forward.confirmStart") : t("settings.network.forward.start")}
                  </button>
                  {#if forwardConfirmOpen}
                    <button class="button ghost compact" onclick={() => { forwardConfirmOpen = false; }}>{t("common.cancel")}</button>
                  {/if}
                </div>
              </div>
            {/if}
          </section>
        {:else}
          <section class="backup-settings" aria-labelledby="nat-title">
            <header>
              <div>
                <h2 id="nat-title">{t("settings.network.nat.title")}</h2>
                <p>{t("settings.network.nat.description")}</p>
              </div>
              <div class="local-content-actions">
                <button class="button ghost compact" disabled={natBusy} onclick={() => void runNatDetect()}>{natBusy ? t("settings.network.nat.detecting") : t("settings.network.nat.detect")}</button>
              </div>
            </header>
            {#if natReport}
              <div class="nat-report">
                <div class="nat-row"><span>{t("settings.network.nat.mapped")}</span><code>{natReport.mappedAddress}</code></div>
                <div class="nat-row"><span>{t("settings.network.nat.behindNat")}</span><strong>{natReport.behindNat ? t("settings.network.nat.behindNatYes") : t("settings.network.nat.behindNatNo")}</strong></div>
                <p class="nat-impact">{natReport.impact}</p>
              </div>
            {/if}
          </section>
        {/if}
      </div>
    </div>
  </main>
</AppShell>
