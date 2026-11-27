<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    Activity,
    Cpu,
    MemoryStick,
    Wifi,
    Laptop,
    Users,
    Zap,
    MessageSquare,
    Video,
    Monitor,
    StickyNote,
    RefreshCw,
    Plus,
    Minus,
    Globe,
    ShieldCheck,
  } from "lucide-svelte";
  import {
    raydropApi,
    formatBytes,
    type SystemTelemetry,
    type TransferAnalytics,
  } from "../utils/tauri-ipc";
  import {
    appSettings,
    networkInfo,
    myRankExp,
    activeMainTab,
  } from "../stores/state";
  import { lanContacts } from "../stores/contacts";
  import { activeTransfers } from "../stores/transfers";
  import { totalUnreadCount } from "../stores/chat";
  import { getRankInfo } from "../utils/rank";

  export let active: boolean = false;

  const isTauriRuntime =
    typeof window !== "undefined" && Boolean((window as any).__TAURI_INTERNALS__);

  let telemetry: SystemTelemetry | null = null;
  let analytics: TransferAnalytics | null = null;
  let loading = false;
  let liveMonitorEnabled = false;
  let nowTime = "";
  let dialMode: "reliability" | "cpu" | "ram" = "reliability";

  let telemetryTimer: any = null;
  let analyticsTimer: any = null;
  let clockTimer: any = null;

  function updateClock() {
    nowTime = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  onMount(() => {
    updateClock();
    clockTimer = setInterval(updateClock, 15000);
  });

  async function fetchTelemetry() {
    if (!isTauriRuntime) return;
    loading = true;
    try {
      telemetry = await raydropApi.getSystemTelemetry();
    } catch (_) {
    } finally {
      loading = false;
    }
  }

  async function fetchAnalytics() {
    if (!isTauriRuntime) return;
    try {
      analytics = await raydropApi.getTransferAnalytics();
    } catch (_) {}
  }

  function startTelemetryPolling() {
    if (!isTauriRuntime || telemetryTimer) return;
    fetchTelemetry();
    telemetryTimer = setInterval(fetchTelemetry, 2000);
  }

  function stopTelemetryPolling() {
    if (telemetryTimer) {
      clearInterval(telemetryTimer);
      telemetryTimer = null;
    }
  }

  function toggleLiveMonitor() {
    liveMonitorEnabled = !liveMonitorEnabled;
    if (liveMonitorEnabled) {
      startTelemetryPolling();
    } else {
      stopTelemetryPolling();
    }
  }

  function stopPolling() {
    stopTelemetryPolling();
    if (analyticsTimer) {
      clearInterval(analyticsTimer);
      analyticsTimer = null;
    }
  }

  $: if (active) {
    if (!analytics && isTauriRuntime) {
      fetchAnalytics();
    }
    if (liveMonitorEnabled) {
      startTelemetryPolling();
    }
  } else {
    stopPolling();
  }

  onDestroy(() => {
    stopPolling();
    if (clockTimer) clearInterval(clockTimer);
  });

  function formatUptime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}H ${m}M`;
  }

  function formatDayShort(epochSecs: number): string {
    return new Date(epochSecs * 1000).toLocaleDateString(undefined, { weekday: "short" });
  }

  $: totalLifetimeBytes =
    (analytics?.total_received_bytes || 0) + (analytics?.total_sent_bytes || 0);
  $: totalSessions =
    (analytics?.sent_sessions || 0) +
    (analytics?.received_sessions || 0) +
    (analytics?.failed_sessions || 0);
  $: successRate =
    totalSessions > 0
      ? Math.round(
          (((analytics?.sent_sessions || 0) + (analytics?.received_sessions || 0)) /
            totalSessions) *
            100,
        )
      : 100;
  $: maxDailyBytes = Math.max(
    1,
    ...(analytics?.daily || []).map((d) => Math.max(d.sent_bytes, d.received_bytes)),
  );

  // Circular dial metric
  $: dialValue =
    dialMode === "reliability"
      ? successRate
      : dialMode === "cpu"
        ? Math.round(telemetry?.cpu_usage_percent || 0)
        : Math.round(telemetry?.ram_usage_percent || 0);
  $: dialLabel =
    dialMode === "reliability"
      ? "Mesh Reliability"
      : dialMode === "cpu"
        ? "CPU Load"
        : "Memory Load";
  $: dialCircumference = 2 * Math.PI * 52;
  $: dialOffset = dialCircumference - (Math.min(100, Math.max(0, dialValue)) / 100) * dialCircumference;

  function cycleDial(dir: 1 | -1) {
    const order: ("reliability" | "cpu" | "ram")[] = ["reliability", "cpu", "ram"];
    const next = (order.indexOf(dialMode) + dir + order.length) % order.length;
    dialMode = order[next];
  }
</script>

<div class="h-full w-full flex flex-col xl:flex-row gap-3.5 overflow-y-auto xl:overflow-hidden">
  <!-- LEFT / CENTER SPATIAL GLASS GRID -->
  <div class="flex-1 min-w-0 xl:overflow-y-auto space-y-3.5 pr-0.5">
    <!-- Top Row: Node Hero Card + Rounded Bar Chart -->
    <div class="grid grid-cols-1 lg:grid-cols-12 gap-3.5">
      <!-- Device / Node Hero Card (7 cols) -->
      <div class="lg:col-span-7 spatial-card rounded-[22px] bg-slate-900/80 border border-white/[0.14] p-4 sm:p-5 flex flex-col justify-between gap-4">
        <div class="flex items-start justify-between gap-3">
          <div class="flex items-center gap-3.5 min-w-0">
            <div class="w-14 h-14 rounded-2xl bg-white/[0.08] border border-white/15 flex items-center justify-center shrink-0 shadow-inner">
              <img src="/icon.png" alt="Raydrop" class="w-11 h-11 object-contain rounded-xl" />
            </div>
            <div class="min-w-0">
              <div class="text-[11px] font-medium text-white/55 uppercase tracking-wider">Raydrop Node</div>
              <h2 class="text-base sm:text-lg font-bold text-white truncate">
                {$appSettings.alias_name || "Raydrop Workstation"}
              </h2>
            </div>
          </div>
          <div class="flex items-center gap-1.5 shrink-0">
            <button
              type="button"
              on:click={toggleLiveMonitor}
              class="px-2.5 py-1.5 rounded-full text-[11px] font-semibold border transition cursor-pointer flex items-center gap-1.5 {liveMonitorEnabled
                ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                : 'bg-white/[0.08] hover:bg-white/15 text-white/80 border-white/10'}"
              title={liveMonitorEnabled ? "Pause Live System Monitor" : "Start Live System Monitor"}
            >
              <span class="w-2 h-2 rounded-full {liveMonitorEnabled ? 'bg-emerald-400 animate-pulse' : 'bg-slate-400'}"></span>
              <span>{liveMonitorEnabled ? "Live ON" : "Monitor"}</span>
            </button>
            <button
              type="button"
              disabled={loading}
              on:click={() => {
                fetchTelemetry();
                fetchAnalytics();
              }}
              class="w-8 h-8 rounded-full bg-white/[0.08] hover:bg-white/15 border border-white/10 flex items-center justify-center text-white/80 transition cursor-pointer shrink-0"
              title="Refresh Telemetry"
            >
              <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
            </button>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3 bg-black/20 rounded-2xl p-3 border border-white/[0.07]">
          <div>
            <div class="text-lg sm:text-xl font-extrabold font-mono text-white">
              {telemetry ? formatUptime(telemetry.uptime_secs) : "0H 0M"}
            </div>
            <div class="text-[10px] text-white/55">System Uptime</div>
          </div>
          <div class="border-l border-white/10 pl-3">
            <div class="text-lg sm:text-xl font-extrabold font-mono text-white">
              {formatBytes(totalLifetimeBytes)}
            </div>
            <div class="text-[10px] text-white/55">Lifetime P2P Volume</div>
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2.5 text-xs">
          <div class="rounded-xl bg-black/20 border border-white/[0.08] px-3 py-2 flex items-center justify-between">
            <span class="text-white/55 text-[10px]">Local IP</span>
            <span class="font-mono font-semibold text-white text-[11px]">{$networkInfo.localIp}</span>
          </div>
          <div class="rounded-xl bg-black/20 border border-white/[0.08] px-3 py-2 flex items-center justify-between">
            <span class="text-white/55 text-[10px]">Port</span>
            <span class="font-mono font-semibold text-white text-[11px]">{$networkInfo.listenPort}</span>
          </div>
        </div>

        <!-- Brightness/Progress-style Rank Bar -->
        {#if true}
          {@const myRank = getRankInfo($myRankExp)}
          <div class="space-y-1">
            <div class="flex items-center justify-between text-[10px] font-mono text-white/70">
              <span>{myRank.icon} {myRank.tierName} • Lv.{myRank.level}</span>
              <span>{myRank.progressPercent}%</span>
            </div>
            <div class="w-full h-2.5 rounded-full bg-black/30 overflow-hidden p-0.5 border border-white/10">
              <div
                class="h-full rounded-full bg-white transition-all duration-300"
                style="width: {Math.max(8, myRank.progressPercent)}%"
              ></div>
            </div>
          </div>
        {/if}
      </div>

      <!-- 7-Day Rounded Pill Bar Chart Card (5 cols) -->
      <div class="lg:col-span-5 spatial-card rounded-[22px] bg-white/[0.08] backdrop-blur-xl border border-white/[0.14] p-4 sm:p-5 flex flex-col justify-between">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold text-white">Transfer Volume (7 Days)</h3>
          <span class="text-[10px] font-mono text-white/60">RX + TX</span>
        </div>

        <div class="grid grid-cols-7 gap-2.5 h-36 items-end pt-4 pb-1">
          {#if analytics && analytics.daily.length > 0}
            {#each analytics.daily as day, i}
              {@const total = day.received_bytes + day.sent_bytes}
              {@const pct = Math.max(18, Math.round((total / (maxDailyBytes * 2)) * 100))}
              {@const isPeak = total > 0 && total >= maxDailyBytes * 0.7}
              <div class="flex flex-col items-center gap-2 h-full justify-end">
                <div
                  class="w-3.5 sm:w-4 rounded-full transition-all duration-300 {isPeak
                    ? 'bg-sky-400 shadow-[0_0_16px_rgba(56,189,248,0.65)]'
                    : 'bg-white/25 hover:bg-white/40'}"
                  style="height: {total > 0 ? pct : 22}%"
                  title="{formatDayShort(day.day_epoch_secs)}: {formatBytes(total)}"
                ></div>
                <span class="text-[10px] font-mono text-white/60">{formatDayShort(day.day_epoch_secs)}</span>
              </div>
            {/each}
          {:else}
            {#each ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] as dayLabel, idx}
              <div class="flex flex-col items-center gap-2 h-full justify-end">
                <div
                  class="w-3.5 sm:w-4 rounded-full {idx === 2 ? 'bg-sky-400/70' : 'bg-white/20'}"
                  style="height: {[48, 35, 82, 55, 68, 42, 30][idx]}%"
                ></div>
                <span class="text-[10px] font-mono text-white/55">{dayLabel}</span>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    </div>

    <!-- Middle Row: 3 Spatial Stat Cards -->
    <div class="grid grid-cols-1 sm:grid-cols-3 gap-3.5">
      <div class="spatial-card rounded-[20px] bg-white/[0.08] backdrop-blur-xl border border-white/[0.14] p-4 space-y-1">
        <div class="text-[11px] text-white/60 font-medium">Downloaded</div>
        <div class="text-2xl font-extrabold font-mono text-white">
          {formatBytes(analytics?.total_received_bytes || 0)}
        </div>
      </div>
      <div class="spatial-card rounded-[20px] bg-white/[0.08] backdrop-blur-xl border border-white/[0.14] p-4 space-y-1">
        <div class="text-[11px] text-white/60 font-medium">Memory Usage</div>
        <div class="text-2xl font-extrabold font-mono text-white">
          {telemetry ? `${telemetry.ram_usage_percent.toFixed(1)} %` : "0.0 %"}
        </div>
      </div>
      <div class="spatial-card rounded-[20px] bg-white/[0.08] backdrop-blur-xl border border-white/[0.14] p-4 space-y-1">
        <div class="text-[11px] text-white/60 font-medium">CPU Load</div>
        <div class="text-2xl font-extrabold font-mono text-white">
          {telemetry ? `${telemetry.cpu_usage_percent.toFixed(1)} %` : "0.0 %"}
        </div>
      </div>
    </div>

    <!-- Bottom Row: 4 Interactive Spatial Quick-Tiles (with visionOS pill switches) -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3.5 pb-2">
      <button
        type="button"
        on:click={() => activeMainTab.set("transfers")}
        class="spatial-card group rounded-[22px] bg-white/[0.11] hover:bg-white/[0.16] backdrop-blur-xl border border-white/[0.16] p-4 text-left flex flex-col justify-between min-h-[120px] transition cursor-pointer"
      >
        <div class="flex items-center justify-between w-full">
          <div class="w-8 h-8 rounded-full bg-white/15 flex items-center justify-center text-white">
            <Zap class="w-4 h-4" />
          </div>
          <span class="w-9 h-5 rounded-full bg-sky-400 p-0.5 flex items-center justify-end">
            <span class="w-4 h-4 rounded-full bg-white shadow"></span>
          </span>
        </div>
        <div class="mt-4">
          <div class="text-[10px] text-white/60">{$activeTransfers.length} Active</div>
          <div class="text-sm font-bold text-white">File Transfers</div>
        </div>
      </button>

      <button
        type="button"
        on:click={() => activeMainTab.set("chat")}
        class="spatial-card group rounded-[22px] bg-white/[0.08] hover:bg-white/[0.14] backdrop-blur-xl border border-white/[0.14] p-4 text-left flex flex-col justify-between min-h-[120px] transition cursor-pointer"
      >
        <div class="flex items-center justify-between w-full">
          <div class="w-8 h-8 rounded-full bg-white/10 flex items-center justify-center text-white">
            <MessageSquare class="w-4 h-4" />
          </div>
          <span class="w-9 h-5 rounded-full p-0.5 flex items-center {$totalUnreadCount > 0 ? 'bg-sky-400 justify-end' : 'bg-white/20 justify-start'}">
            <span class="w-4 h-4 rounded-full bg-white shadow"></span>
          </span>
        </div>
        <div class="mt-4">
          <div class="text-[10px] text-white/60">{$totalUnreadCount} Unread</div>
          <div class="text-sm font-bold text-white">Encrypted Chat</div>
        </div>
      </button>

      <button
        type="button"
        on:click={() => activeMainTab.set("meeting")}
        class="spatial-card group rounded-[22px] bg-white/[0.11] hover:bg-white/[0.16] backdrop-blur-xl border border-white/[0.16] p-4 text-left flex flex-col justify-between min-h-[120px] transition cursor-pointer"
      >
        <div class="flex items-center justify-between w-full">
          <div class="w-8 h-8 rounded-full bg-white/15 flex items-center justify-center text-white">
            <Video class="w-4 h-4" />
          </div>
          <span class="w-9 h-5 rounded-full bg-sky-400 p-0.5 flex items-center justify-end">
            <span class="w-4 h-4 rounded-full bg-white shadow"></span>
          </span>
        </div>
        <div class="mt-4">
          <div class="text-[10px] text-white/60">P2P Room</div>
          <div class="text-sm font-bold text-white">Online Meeting</div>
        </div>
      </button>

      <button
        type="button"
        disabled={!$appSettings.discoverable}
        on:click={() => {
          if ($appSettings.discoverable) activeMainTab.set("remote");
        }}
        class="spatial-card group rounded-[22px] bg-white/[0.08] hover:bg-white/[0.14] backdrop-blur-xl border border-white/[0.14] p-4 text-left flex flex-col justify-between min-h-[120px] transition {!$appSettings.discoverable
          ? 'opacity-45 cursor-not-allowed'
          : 'cursor-pointer'}"
      >
        <div class="flex items-center justify-between w-full">
          <div class="w-8 h-8 rounded-full bg-white/10 flex items-center justify-center text-white">
            <Monitor class="w-4 h-4" />
          </div>
          <span class="w-9 h-5 rounded-full p-0.5 flex items-center {$appSettings.discoverable ? 'bg-sky-400 justify-end' : 'bg-white/20 justify-start'}">
            <span class="w-4 h-4 rounded-full bg-white shadow"></span>
          </span>
        </div>
        <div class="mt-4">
          <div class="text-[10px] text-white/60">{$appSettings.discoverable ? "Ready" : "Hidden"}</div>
          <div class="text-sm font-bold text-white">Remote Desktop</div>
        </div>
      </button>
    </div>
  </div>

  <!-- RIGHT SPATIAL SIDEBAR (Clock + Circular Gauge + Quick Mode Tiles + Mesh Status) -->
  <aside class="w-full xl:w-72 shrink-0 spatial-card rounded-[24px] bg-white/[0.07] backdrop-blur-2xl border border-white/[0.14] p-4 flex flex-col justify-between gap-4">
    <!-- Live Clock -->
    <div class="text-center pt-1">
      <div class="text-xl font-bold tracking-tight text-white font-display">{nowTime || "20:54"}</div>
      <div class="flex items-center justify-between mt-2 text-[11px] text-white/70 px-1">
        <span>{dialLabel}</span>
        <span class="inline-flex items-center gap-1 text-[10px] font-mono">
          <Globe class="w-3 h-3 text-sky-400" />
          {$networkInfo.signalingConnected ? "WAN" : "LAN"}
        </span>
      </div>
    </div>

    <!-- Circular Spatial Gauge (Thermostat-inspired) -->
    <div class="flex flex-col items-center justify-center relative py-1">
      <div class="relative w-36 h-36 flex items-center justify-center">
        <svg class="w-full h-full -rotate-90" viewBox="0 0 120 120">
          <circle
            cx="60"
            cy="60"
            r="52"
            fill="none"
            stroke="rgba(255,255,255,0.14)"
            stroke-width="6"
          />
          <circle
            cx="60"
            cy="60"
            r="52"
            fill="none"
            stroke="#38bdf8"
            stroke-width="6"
            stroke-linecap="round"
            stroke-dasharray={dialCircumference}
            stroke-dashoffset={dialOffset}
            class="transition-all duration-500"
          />
        </svg>
        <div class="absolute inset-0 flex flex-col items-center justify-center text-center">
          <span class="text-3xl font-extrabold font-mono text-white">{dialValue}%</span>
          <span class="text-[10px] text-white/60 uppercase tracking-wider mt-0.5">{dialMode}</span>
        </div>
      </div>

      <div class="inline-flex items-center gap-2 px-2.5 py-1 rounded-full bg-white/[0.1] border border-white/15 -mt-2">
        <button
          type="button"
          on:click={() => cycleDial(-1)}
          class="w-5 h-5 rounded-full hover:bg-white/15 flex items-center justify-center text-white cursor-pointer"
          title="Previous Metric"
        >
          <Minus class="w-3 h-3" />
        </button>
        <span class="text-[10px] font-mono text-white/75 px-1">Metric</span>
        <button
          type="button"
          on:click={() => cycleDial(1)}
          class="w-5 h-5 rounded-full hover:bg-white/15 flex items-center justify-center text-white cursor-pointer"
          title="Next Metric"
        >
          <Plus class="w-3 h-3" />
        </button>
      </div>
    </div>

    <!-- 4 Quick Mode Tiles (Hot/Eco/Fan/Cold equivalent) -->
    <div class="grid grid-cols-4 gap-2">
      <button
        type="button"
        on:click={() => activeMainTab.set("lan")}
        class="rounded-2xl bg-white/[0.08] hover:bg-white/[0.16] border border-white/10 py-2.5 flex flex-col items-center gap-1 text-white transition cursor-pointer"
      >
        <Wifi class="w-4 h-4" />
        <span class="text-[9px] text-white/75">LAN ({$lanContacts.length})</span>
      </button>
      <button
        type="button"
        on:click={() => activeMainTab.set("devices")}
        class="rounded-2xl bg-white/[0.08] hover:bg-white/[0.16] border border-white/10 py-2.5 flex flex-col items-center gap-1 text-white transition cursor-pointer"
      >
        <Laptop class="w-4 h-4" />
        <span class="text-[9px] text-white/75">Devices</span>
      </button>
      <button
        type="button"
        on:click={() => activeMainTab.set("contacts")}
        class="rounded-2xl bg-white/[0.08] hover:bg-white/[0.16] border border-white/10 py-2.5 flex flex-col items-center gap-1 text-white transition cursor-pointer"
      >
        <Users class="w-4 h-4" />
        <span class="text-[9px] text-white/75">Peers</span>
      </button>
      <button
        type="button"
        on:click={() => activeMainTab.set("notes")}
        class="rounded-2xl bg-white/[0.08] hover:bg-white/[0.16] border border-white/10 py-2.5 flex flex-col items-center gap-1 text-white transition cursor-pointer"
      >
        <StickyNote class="w-4 h-4" />
        <span class="text-[9px] text-white/75">Notes</span>
      </button>
    </div>

    <!-- Bottom Spatial Mesh Player Card -->
    <div class="rounded-2xl bg-white/[0.09] border border-white/15 p-3.5 space-y-2.5">
      <div class="flex items-center justify-between gap-2">
        <div class="min-w-0">
          <div class="text-[10px] text-white/55 truncate">ChaCha20-Poly1305 • P2P Mesh</div>
          <div class="text-xs font-bold text-white truncate">
            {telemetry ? `↓ ${formatBytes(telemetry.net_rx_bytes_per_sec)}/s • ↑ ${formatBytes(telemetry.net_tx_bytes_per_sec)}/s` : "Standby • Zero Cloud"}
          </div>
        </div>
        <ShieldCheck class="w-4 h-4 text-sky-400 shrink-0" />
      </div>
      <div class="w-full h-1 rounded-full bg-white/15 overflow-hidden">
        <div class="h-full w-2/3 rounded-full bg-white"></div>
      </div>
    </div>
  </aside>
</div>
