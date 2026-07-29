<script lang="ts">
  import { onMount } from "svelte";

  import { t } from "../i18n.svelte";
  import { netplayRoom, refreshNetplayRoom, setNetplayRoom } from "../netplay.svelte";
  import type {
    MoyuRuntime,
    NatReportView,
    NavigationKey,
    NetplayPeerView,
    OnboardingSelection,
  } from "../runtime";
  import { pushToast } from "../toast.svelte";
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
  // 房间状态以全局 store 为准：AppShell 每 5s 轮询收敛(DHCP IP、端口侦测、转发状态)。
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
  let peers = $state<NetplayPeerView[]>([]);

  onMount(() => {
    void refreshNetplayRoom(runtime);
    return runtime.onNetplayDownloadProgress((event) => {
      downloadProgress = event.total > 0 ? event : null;
    });
  });

  // 成员列表由本页自己轮询(5s):在房间中启动,离开房间或离开页面即清理。
  const inRoom = $derived(room !== null);
  $effect(() => {
    if (!inRoom) {
      peers = [];
      return;
    }
    void refreshPeers();
    const timer = setInterval(() => void refreshPeers(), 5000);
    return () => clearInterval(timer);
  });

  async function refreshPeers(): Promise<void> {
    try {
      peers = await runtime.listNetplayPeers();
    } catch {
      // 成员读取失败静默,下一轮轮询重试。
    }
  }

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
    downloadProgress = null;
    try {
      const view = await runtime.startNetplayRoom(roomName.trim(), roomSecret, isHost);
      setNetplayRoom(view);
      pushToast({
        tone: "ok",
        title: isHost ? t("settings.network.room.created") : t("settings.network.room.joined"),
      });
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
      pushToast({ tone: "info", title: t("settings.network.room.left") });
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
      pushToast({ tone: "ok", title: t("settings.network.room.forwardReady") });
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
  activeNavigation="netplay"
  online={Boolean(room)}
  connectionStatus={room ? t("netplay.status.inRoom").replace("{name}", room.networkName) : t("netplay.status.idle")}
  {onNavigate}
  {runtime}
  {onMinimize}
  {onToggleMaximize}
  {onClose}
>
  <main class="content settings-main">
    <div class="tabs">
      {#each NETPLAY_TABS as item}
        <button class:on={tab === item.key} aria-current={tab === item.key ? "page" : undefined} onclick={() => { tab = item.key; }}>
          {t(item.labelKey)}
        </button>
      {/each}
    </div>

    {#if errorMessage}
      <div class="banner danger" role="alert" style="margin-bottom:16px">
        <div><strong>{t("settings.network.errorTitle")}</strong><div>{errorMessage}</div></div>
      </div>
    {/if}

    {#if tab === "room"}
      {#if room}
        <section class="panel pad" aria-labelledby="netplay-title">
          <div class="row spread">
            <div class="row">
              <h2 class="panel-title netplay-room-name" id="netplay-title">{room.networkName}</h2>
              <span class="tag accent">{room.isHost ? t("settings.network.room.hostBadge") : t("settings.network.room.memberBadge")}</span>
            </div>
            <div class="row">
              <button class="btn small secondary" onclick={() => void copyRoomInfo()}>{roomCopied ? t("settings.network.room.copied") : t("settings.network.room.copy")}</button>
              <button class="btn small danger-soft" disabled={roomBusy} onclick={() => void leaveRoom()}>{roomBusy ? t("settings.network.room.leaving") : t("settings.network.room.leave")}</button>
            </div>
          </div>
          <div class="kv-list">
            <div class="kv-row">
              <span class="muted">{t("settings.network.room.virtualIp")}</span>
              <code class="mono">{room.virtualIp}</code>
            </div>
            {#if room.isHost}
              <div class="kv-row">
                <span class="muted">{t("settings.network.room.lanPortLabel")}</span>
                <span>
                  {#if room.mcLanPort}
                    {t("settings.network.room.lanPortDetected").replace("{port}", String(room.mcLanPort))}
                  {:else}
                    {t("settings.network.room.lanPortPending")}
                  {/if}
                </span>
              </div>
              <p class="dim" style="margin-top:8px">{t("settings.network.room.hostHint")}</p>
            {:else if room.forwardedLocalPort}
              <div class="kv-row">
                <span class="muted">{t("settings.network.room.forwardAddress")}</span>
                <span class="row" style="gap:8px">
                  <code class="mono">127.0.0.1:{room.forwardedLocalPort}</code>
                  <button class="btn small ghost" onclick={() => void copyForwardAddress()}>{forwardCopied ? t("settings.network.room.copied") : t("settings.network.room.forwardCopy")}</button>
                </span>
              </div>
              <p class="dim" style="margin-top:8px">{t("settings.network.room.forwardHint").replace("{address}", `127.0.0.1:${room.forwardedLocalPort}`)}</p>
            {:else}
              <p class="dim" style="margin-top:8px">{t("settings.network.room.guestHint")}</p>
              <div class="row" style="margin-top:10px;flex-wrap:wrap">
                <input
                  class="input"
                  style="width:200px"
                  bind:value={forwardPort}
                  type="text"
                  inputmode="numeric"
                  aria-label={t("settings.network.room.forwardAria")}
                  placeholder={t("settings.network.room.forwardPlaceholder")}
                />
                <button class="btn primary" disabled={forwardBusy || !forwardPort.trim()} onclick={() => void submitForward()}>{forwardBusy ? t("settings.network.room.forwardStarting") : t("settings.network.room.forwardStart")}</button>
              </div>
            {/if}
          </div>
        </section>

        <section class="panel pad netplay-members" style="margin-top:16px" aria-label={t("settings.network.room.membersTitle")}>
          <h2 class="panel-title">{t("settings.network.room.membersTitle")}</h2>
          {#if peers.length === 0}
            <p class="dim" style="padding:8px 0">{t("settings.network.room.membersEmpty")}</p>
          {:else}
            <div style="margin-top:6px">
              {#each peers as peer}
                <div class="list-row netplay-member-row" style="padding-left:0;padding-right:0">
                  <code class="mono">{peer.ipv4}</code>
                  <div class="lr-main"><div class="lr-name">{peer.hostname}</div></div>
                  {#if peer.latencyMs !== null}
                    <span class="dim">{t("settings.network.room.latency").replace("{ms}", String(Math.round(peer.latencyMs)))}</span>
                  {/if}
                  <span class="tag neutral netplay-badge">{peer.isHost ? t("settings.network.room.hostBadge") : t("settings.network.room.memberBadge")}</span>
                  <span class="tag netplay-badge" class:warn={peer.connection !== "p2p"} class:ok={peer.connection === "p2p"}>{peer.connection === "p2p" ? t("settings.network.room.connP2p") : t("settings.network.room.connRelay")}</span>
                </div>
              {/each}
            </div>
          {/if}
        </section>
      {:else}
        <section class="panel pad" style="max-width:min(560px, 100%)" aria-labelledby="netplay-title">
          <h2 class="panel-title" id="netplay-title">{t("settings.network.room.title")}</h2>
          <p class="panel-desc" style="margin:4px 0 16px">{t("settings.network.room.description")}</p>
          <div class="col" style="gap:14px">
            <div class="field">
              <label for="netplay-name">{t("settings.network.room.nameLabel")}</label>
              <div class="row" style="flex-wrap:wrap">
                <input id="netplay-name" class="input" style="flex:1;min-width:min(180px, 100%)" bind:value={roomName} type="text" aria-label={t("settings.network.room.nameAria")} placeholder={t("settings.network.room.namePlaceholder")} />
                <button class="btn ghost" onclick={generateRoomName}>{t("settings.network.room.generate")}</button>
              </div>
            </div>
            <div class="field">
              <label for="netplay-secret">{t("settings.network.room.secretLabel")}</label>
              <input id="netplay-secret" class="input" bind:value={roomSecret} type="password" aria-label={t("settings.network.room.secretAria")} autocomplete="off" />
            </div>
            <div class="row" style="flex-wrap:wrap">
              <button class="btn primary" disabled={roomBusy || !roomName.trim() || !roomSecret} onclick={() => void submitRoom(true)}>{t("settings.network.room.create")}</button>
              <button class="btn secondary" disabled={roomBusy || !roomName.trim() || !roomSecret} onclick={() => void submitRoom(false)}>{t("settings.network.room.join")}</button>
            </div>
            {#if roomBusy && downloadProgress}
              <div role="status" aria-label={t("settings.network.room.downloading")}>
                <div class="progress"><i style={`width: ${Math.round((downloadProgress.current / downloadProgress.total) * 100)}%`}></i></div>
                <div class="dim" style="margin-top:6px">{t("settings.network.room.downloading")} {formatProgress(downloadProgress)}</div>
              </div>
            {:else if roomBusy}
              <div class="progress indet" role="status" aria-label={t("settings.network.room.preparing")}><i></i></div>
            {/if}
            <p class="dim">{t("settings.network.room.note")}</p>
          </div>
        </section>
      {/if}
    {:else}
      <section class="panel pad" style="max-width:min(640px, 100%)" aria-labelledby="nat-title">
        <div class="row spread">
          <div>
            <h2 class="panel-title" id="nat-title">{t("settings.network.nat.title")}</h2>
            <p class="panel-desc" style="margin-top:4px">{t("settings.network.nat.description")}</p>
          </div>
          <button class="btn secondary" disabled={natBusy} onclick={() => void runNatDetect()}>{natBusy ? t("settings.network.nat.detecting") : t("settings.network.nat.detect")}</button>
        </div>
        {#if natReport}
          <div class="kv-list" style="margin-top:14px">
            <div class="kv-row">
              <span class="muted">{t("settings.network.nat.mapped")}</span>
              <code class="mono">{natReport.mappedAddress}</code>
            </div>
            <div class="kv-row">
              <span class="muted">{t("settings.network.nat.behindNat")}</span>
              <span class="tag {natReport.behindNat ? "warn" : "ok"}">{natReport.behindNat ? t("settings.network.nat.behindNatYes") : t("settings.network.nat.behindNatNo")}</span>
            </div>
            <p class="muted" style="margin-top:10px">{natReport.impact}</p>
          </div>
        {/if}
      </section>
    {/if}
  </main>
</AppShell>

<style>
  .kv-list {
    margin-top: 12px;
  }
  .kv-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 9px 0;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .kv-row:first-child {
    border-top: none;
  }
</style>
