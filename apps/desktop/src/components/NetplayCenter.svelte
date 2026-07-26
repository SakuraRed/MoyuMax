<script lang="ts">
  import { onMount } from "svelte";

  import { t } from "../i18n.svelte";
  import { netplayRoom, refreshNetplayRoom, setNetplayRoom } from "../netplay.svelte";
  import type {
    MoyuRuntime,
    NatReportView,
    NavigationKey,
    OnboardingSelection,
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

  type NetplayTab = "room" | "nat";

  const NETPLAY_TABS: { key: NetplayTab; labelKey: string }[] = [
    { key: "room", labelKey: "settings.network.room.title" },
    { key: "nat", labelKey: "settings.network.nat.title" },
  ];

  let tab = $state<NetplayTab>("room");
  // 房间状态以全局 store 为准：AppShell 每 5s 轮询收敛（DHCP IP、端口侦测、转发状态）。
  const room = $derived(netplayRoom());
  let roomName = $state("");
  let roomSecret = $state("");
  let roomBusy = $state(false);
  let roomCopied = $state(false);
  let forwardPort = $state("");
  let forwardBusy = $state(false);
  let forwardCopied = $state(false);
  let downloadProgress = $state<{ current: number; total: number } | null>(null);
  let natReport = $state<NatReportView | null>(null);
  let natBusy = $state(false);
  let errorMessage = $state("");
  let notice = $state("");

  onMount(() => {
    void refreshNetplayRoom(runtime);
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
      const view = await runtime.startNetplayRoom(roomName.trim(), roomSecret, isHost);
      setNetplayRoom(view);
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
      setNetplayRoom(null);
      notice = t("settings.network.room.left");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      roomBusy = false;
    }
  }

  async function submitForward(): Promise<void> {
    const port = Number(forwardPort.trim());
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      errorMessage = t("settings.network.room.forwardInvalid");
      return;
    }
    forwardBusy = true;
    errorMessage = "";
    try {
      await runtime.setNetplayForward(port);
      await refreshNetplayRoom(runtime);
      notice = t("settings.network.room.forwardReady");
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      forwardBusy = false;
    }
  }

  async function copyRoomInfo(): Promise<void> {
    if (!room) return;
    const template =
      room.isHost && room.mcLanPort
        ? t("settings.network.room.copyPayloadHost")
            .replace("{name}", room.networkName)
            .replace("{port}", String(room.mcLanPort))
        : t("settings.network.room.copyPayload").replace("{name}", room.networkName);
    try {
      await navigator.clipboard.writeText(template);
      roomCopied = true;
      setTimeout(() => { roomCopied = false; }, 1600);
    } catch {
      errorMessage = t("settings.network.room.copyFailed");
    }
  }

  async function copyForwardAddress(): Promise<void> {
    if (!room?.forwardedLocalPort) return;
    try {
      await navigator.clipboard.writeText(`127.0.0.1:${room.forwardedLocalPort}`);
      forwardCopied = true;
      setTimeout(() => { forwardCopied = false; }, 1600);
    } catch {
      errorMessage = t("settings.network.room.copyFailed");
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
  {runtime}
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
                  {#if room.isHost}
                    {#if room.mcLanPort}
                      <small>{t("settings.network.room.lanPortDetected").replace("{port}", String(room.mcLanPort))}</small>
                    {:else}
                      <small>{t("settings.network.room.lanPortPending")}</small>
                    {/if}
                    <small>{t("settings.network.room.hostHint")}</small>
                  {:else if room.forwardedLocalPort}
                    <small class="netplay-forward-ready">
                      {t("settings.network.room.forwardAddress")}: <code>127.0.0.1:{room.forwardedLocalPort}</code>
                    </small>
                    <small>{t("settings.network.room.forwardHint").replace("{address}", `127.0.0.1:${room.forwardedLocalPort}`)}</small>
                  {:else}
                    <small>{t("settings.network.room.guestHint")}</small>
                  {/if}
                </div>
                <div class="task-buttons">
                  <button class="button ghost compact" onclick={() => void copyRoomInfo()}>{roomCopied ? t("settings.network.room.copied") : t("settings.network.room.copy")}</button>
                  {#if !room.isHost && room.forwardedLocalPort}
                    <button class="button ghost compact" onclick={() => void copyForwardAddress()}>{forwardCopied ? t("settings.network.room.copied") : t("settings.network.room.forwardCopy")}</button>
                  {/if}
                  <button class="button danger-subtle compact" disabled={roomBusy} onclick={() => void leaveRoom()}>{roomBusy ? t("settings.network.room.leaving") : t("settings.network.room.leave")}</button>
                </div>
                {#if !room.isHost && !room.forwardedLocalPort}
                  <div class="netplay-forward-form">
                    <label>
                      <span>{t("settings.network.room.forwardLabel")}</span>
                      <input bind:value={forwardPort} type="text" inputmode="numeric" aria-label={t("settings.network.room.forwardAria")} placeholder={t("settings.network.room.forwardPlaceholder")} />
                    </label>
                    <button class="button primary compact" disabled={forwardBusy || !forwardPort.trim()} onclick={() => void submitForward()}>{forwardBusy ? t("settings.network.room.forwardStarting") : t("settings.network.room.forwardStart")}</button>
                  </div>
                {/if}
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
