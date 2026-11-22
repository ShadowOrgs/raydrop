<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    Activity,
    Cpu,
    MemoryStick,
    MonitorPlay,
    Wifi,
    HardDrive,
    ArrowDownLeft,
    ArrowUpRight,
    Server,
    RefreshCw,
    Database,
    CheckCircle2,
  } from "lucide-svelte";
  import {
    raydropApi,
    formatBytes,
    type SystemTelemetry,
    type TransferAnalytics,
  } from "../utils/tauri-ipc";
  import { appSettings, networkInfo, myRankExp } from "../stores/state";
  import { getRankInfo } from "../utils/rank";

  export let active: boolean = false;

  const MAX_POINTS = 40;

  let telemetry: SystemTelemetry | null = null;
  let analytics: TransferAnalytics | null = null;
  let loading = false;
  let errorMsg: string | null = null;

  let cpuHistory: number[] = Array(MAX_POINTS).fill(0);
  let ramHistory: number[] = Array(MAX_POINTS).fill(0);
  let gpuHistory: number[] = Array(MAX_POINTS).fill(0);
  let netRxHistory: number[] = Array(MAX_POINTS).fill(0);
  let netTxHistory: number[] = Array(MAX_POINTS).fill(0);

  let telemetryTimer: any = null;
  let analyticsTimer: any = null;

  function pushSample(arr: number[], val: number): number[] {
    const next = [...arr.slice(1), val];
    return next;
  }

  async function fetchTelemetry() {
    try {
      const t = await raydropApi.getSystemTelemetry();
      telemetry = t;
      cpuHistory = pushSample(cpuHistory, t.cpu_usage_percent);
      ramHistory = pushSample(ramHistory, t.ram_usage_percent);
      gpuHistory = pushSample(gpuHistory, t.gpu_usage_percent);
      netRxHistory = pushSample(netRxHistory, t.net_rx_bytes_per_sec);
      netTxHistory = pushSample(netTxHistory, t.net_tx_bytes_per_sec);
      errorMsg = null;
    } catch (e: any) {
      errorMsg = e?.message || String(e);
    }
  }

  async function fetchAnalytics() {
    try {
      analytics = await raydropApi.getTransferAnalytics();
    } catch (_) {}
  }

  function startPolling() {
    if (telemetryTimer) return;
    loading = !telemetry;
    Promise.all([fetchTelemetry(), fetchAnalytics()]).finally(() => {
      loading = false;
    });
    telemetryTimer = setInterval(() => {
      fetchTelemetry();
    }, 1200);
    analyticsTimer = setInterval(() => {
      fetchAnalytics();
    }, 5000);
  }

  function stopPolling() {
    if (telemetryTimer) {
      clearInterval(telemetryTimer);
      telemetryTimer = null;
    }
    if (analyticsTimer) {
      clearInterval(analyticsTimer);
      analyticsTimer = null;
    }
  }

  $: if (active) {
    startPolling();
  } else {
    stopPolling();
  }

  onDestroy(() => {
    stopPolling();
  });

  function buildLinePath(values: number[], maxVal: number, w = 240, h = 64): string {
    if (!values.length) return "";
    const safeMax = Math.max(maxVal, 1);
    const stepX = w / Math.max(values.length - 1, 1);
    return values
      .map((v, i) => {
        const x = (i * stepX).toFixed(1);
        const clamped = Math.min(Math.max(v, 0), safeMax);
        const y = (h - (clamped / safeMax) * (h - 6) - 3).toFixed(1);
        return `${i === 0 ? "M" : "L"}${x},${y}`;
      })
      .join(" ");
  }

  function buildAreaPath(values: number[], maxVal: number, w = 240, h = 64): string {
    const line = buildLinePath(values, maxVal, w, h);
    if (!line) return "";
    return `${line} L${w},${h} L0,${h} Z`;
  }

  function formatUptime(secs: number): string {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }

  function formatDayShort(epochSecs: number): string {
    const dt = new Date(epochSecs * 1000);
    return dt.toLocaleDateString(undefined, { weekday: "short" });
  }

  $: netMaxScale = Math.max(
    1024 * 64,
    ...netRxHistory,
    ...netTxHistory,
  );

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
</script>

<div class="h-full w-full overflow-y-auto pr-1 space-y-4">
  <!-- Header Strip -->
  <div
    class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-slate-900/70 border border-slate-800/80 rounded-2xl p-4"
  >
    <div class="flex items-center gap-3">
      <div
        class="w-10 h-10 rounded-xl bg-cyan-500/15 flex items-center justify-center text-cyan-400 shrink-0"
      >
        <Activity class="w-5 h-5" />
      </div>
      <div>
        <h2 class="text-base sm:text-lg font-bold text-white flex items-center gap-2">
          <span>System Monitor & Transfer Analytics</span>
          <span
            class="px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 font-mono text-[10px] flex items-center gap-1"
          >
            <span class="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse"></span>
            Live
          </span>
        </h2>
        <p class="text-xs text-slate-400">
          Real-time CPU, RAM, GPU, Network & Disk telemetry (active only while viewing this page).
        </p>
      </div>
    </div>

    <button
      type="button"
      on:click={() => {
        fetchTelemetry();
        fetchAnalytics();
      }}
      class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 flex items-center gap-1.5 self-end sm:self-auto cursor-pointer transition"
    >
      <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
      <span>Refresh Now</span>
    </button>
  </div>

  {#if errorMsg}
    <div class="p-3 rounded-xl bg-rose-950/60 border border-rose-500/30 text-xs text-rose-300">
      {errorMsg}
    </div>
  {/if}

  <!-- Real-Time Task Manager Grid (CPU, RAM, GPU, Network) -->
  <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3.5">
    <!-- CPU Card -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Cpu class="w-4 h-4 text-cyan-400" />
          <span class="text-xs font-bold text-white">CPU</span>
        </div>
        <span class="text-lg font-extrabold font-mono text-white">
          {telemetry ? telemetry.cpu_usage_percent.toFixed(1) : "0.0"}%
        </span>
      </div>

      <div class="w-full h-16 rounded-xl bg-slate-950/70 overflow-hidden relative">
        <svg viewBox="0 0 240 64" preserveAspectRatio="none" class="w-full h-full">
          <line x1="0" y1="16" x2="240" y2="16" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="32" x2="240" y2="32" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="48" x2="240" y2="48" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <path d={buildAreaPath(cpuHistory, 100)} fill="rgba(0, 120, 232, 0.22)" />
          <path d={buildLinePath(cpuHistory, 100)} fill="none" stroke="#3898ff" stroke-width="2" />
        </svg>
      </div>

      <div class="text-[11px] text-slate-400 space-y-0.5">
        <div class="truncate text-slate-300 font-medium" title={telemetry?.cpu_brand}>
          {telemetry?.cpu_brand || "Detecting CPU..."}
        </div>
        <div class="flex items-center justify-between font-mono text-[10px] text-slate-500">
          <span>{telemetry ? `${telemetry.cpu_cores_physical}C / ${telemetry.cpu_cores_logical}T` : "--"}</span>
          <span>
            {telemetry?.cpu_freq_mhz ? `${(telemetry.cpu_freq_mhz / 1000).toFixed(2)} GHz` : "--"}
            {telemetry?.cpu_temp_celsius ? ` • ${telemetry.cpu_temp_celsius.toFixed(0)}°C` : ""}
          </span>
        </div>
      </div>
    </div>

    <!-- Memory (RAM) Card -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <MemoryStick class="w-4 h-4 text-cyan-400" />
          <span class="text-xs font-bold text-white">Memory (RAM)</span>
        </div>
        <span class="text-lg font-extrabold font-mono text-white">
          {telemetry ? telemetry.ram_usage_percent.toFixed(1) : "0.0"}%
        </span>
      </div>

      <div class="w-full h-16 rounded-xl bg-slate-950/70 overflow-hidden relative">
        <svg viewBox="0 0 240 64" preserveAspectRatio="none" class="w-full h-full">
          <line x1="0" y1="16" x2="240" y2="16" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="32" x2="240" y2="32" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="48" x2="240" y2="48" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <path d={buildAreaPath(ramHistory, 100)} fill="rgba(139, 92, 246, 0.22)" />
          <path d={buildLinePath(ramHistory, 100)} fill="none" stroke="#a78bfa" stroke-width="2" />
        </svg>
      </div>

      <div class="text-[11px] text-slate-400 space-y-0.5">
        <div class="text-slate-300 font-medium font-mono">
          {telemetry
            ? `${formatBytes(telemetry.ram_used_bytes)} / ${formatBytes(telemetry.ram_total_bytes)}`
            : "-- / --"}
        </div>
        <div class="flex items-center justify-between font-mono text-[10px] text-slate-500">
          <span>Swap Used</span>
          <span>
            {telemetry ? `${formatBytes(telemetry.swap_used_bytes)} / ${formatBytes(telemetry.swap_total_bytes)}` : "--"}
          </span>
        </div>
      </div>
    </div>

    <!-- GPU Card -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <MonitorPlay class="w-4 h-4 text-cyan-400" />
          <span class="text-xs font-bold text-white">GPU</span>
        </div>
        <span class="text-lg font-extrabold font-mono text-white">
          {telemetry ? telemetry.gpu_usage_percent.toFixed(1) : "0.0"}%
        </span>
      </div>

      <div class="w-full h-16 rounded-xl bg-slate-950/70 overflow-hidden relative">
        <svg viewBox="0 0 240 64" preserveAspectRatio="none" class="w-full h-full">
          <line x1="0" y1="16" x2="240" y2="16" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="32" x2="240" y2="32" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="48" x2="240" y2="48" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <path d={buildAreaPath(gpuHistory, 100)} fill="rgba(56, 189, 248, 0.2)" />
          <path d={buildLinePath(gpuHistory, 100)} fill="none" stroke="#38bdf8" stroke-width="2" />
        </svg>
      </div>

      <div class="text-[11px] text-slate-400 space-y-0.5">
        <div class="truncate text-slate-300 font-medium" title={telemetry?.gpu_name}>
          {telemetry?.gpu_name || "Detecting GPU..."}
        </div>
        <div class="flex items-center justify-between font-mono text-[10px] text-slate-500">
          <span>3D Engine</span>
          <span>Live PDH Sampler</span>
        </div>
      </div>
    </div>

    <!-- Network Throughput Card -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col gap-3">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Wifi class="w-4 h-4 text-cyan-400" />
          <span class="text-xs font-bold text-white">Network</span>
        </div>
        <span class="text-[11px] font-mono text-cyan-300 font-semibold">
          {telemetry ? `${formatBytes(telemetry.net_rx_bytes_per_sec)}/s` : "0 B/s"}
        </span>
      </div>

      <div class="w-full h-16 rounded-xl bg-slate-950/70 overflow-hidden relative">
        <svg viewBox="0 0 240 64" preserveAspectRatio="none" class="w-full h-full">
          <line x1="0" y1="16" x2="240" y2="16" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="32" x2="240" y2="32" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <line x1="0" y1="48" x2="240" y2="48" stroke="rgba(255,255,255,0.05)" stroke-width="1" />
          <path d={buildAreaPath(netRxHistory, netMaxScale)} fill="rgba(0, 120, 232, 0.22)" />
          <path d={buildLinePath(netRxHistory, netMaxScale)} fill="none" stroke="#3898ff" stroke-width="2" />
          <path d={buildLinePath(netTxHistory, netMaxScale)} fill="none" stroke="#a78bfa" stroke-width="1.6" stroke-dasharray="3 2" />
        </svg>
      </div>

      <div class="text-[11px] text-slate-400 space-y-0.5">
        <div class="flex items-center justify-between font-mono text-[11px]">
          <span class="text-cyan-300">↓ {telemetry ? `${formatBytes(telemetry.net_rx_bytes_per_sec)}/s` : "0 B/s"}</span>
          <span class="text-violet-300">↑ {telemetry ? `${formatBytes(telemetry.net_tx_bytes_per_sec)}/s` : "0 B/s"}</span>
        </div>
        <div class="flex items-center justify-between font-mono text-[10px] text-slate-500">
          <span>LAN: {$networkInfo.localIp}</span>
          <span>Port: {$networkInfo.listenPort}</span>
        </div>
      </div>
    </div>
  </div>

  <!-- My Raydrop Rank & EXP Progress -->
  {#if true}
    {@const myRank = getRankInfo($myRankExp)}
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
      <div class="flex items-center gap-3.5">
        <div
          class="w-12 h-12 rounded-2xl flex items-center justify-center text-2xl shrink-0"
          style={myRank.badgeStyle}
        >
          {myRank.icon}
        </div>
        <div>
          <div class="flex items-center gap-2 flex-wrap">
            <h3 class="text-sm font-bold text-white">
              {myRank.tierName} Tier — Level {myRank.level}
            </h3>
            <span
              class="text-[10px] font-bold px-2 py-0.5 rounded-full font-mono"
              style={myRank.badgeStyle}
            >
              {myRank.exp.toLocaleString()} EXP
            </span>
          </div>
          <p class="text-[11px] text-slate-400 mt-0.5">
            Earn EXP from File Transfers (+2 EXP/MB), Chat (+5 EXP/msg), Remote Desktop, Meeting & Tablet Extend (+10 EXP/min), and Sync/Share (+25 EXP).
          </p>
        </div>
      </div>

      <div class="w-full sm:w-64 space-y-1.5 shrink-0">
        <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
          <span>Lv.{myRank.level} Progress</span>
          <span>{myRank.progressPercent}% ({myRank.exp.toLocaleString()} / {myRank.nextLevelExp.toLocaleString()})</span>
        </div>
        <div class="w-full h-2 rounded-full bg-slate-800 overflow-hidden">
          <div
            class="h-full rounded-full transition-all"
            style="width: {myRank.progressPercent}%; background: {myRank.barColor}"
          ></div>
        </div>
      </div>
    </div>
  {/if}

  <!-- Persistent File Transfer Analytics -->
  <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-4">
    <div class="flex items-center justify-between flex-wrap gap-2">
      <div class="flex items-center gap-2">
        <Database class="w-4 h-4 text-cyan-400" />
        <h3 class="text-sm font-bold text-white">Persistent File Transfer Analytics</h3>
      </div>
      <span class="text-[11px] font-mono text-slate-400">
        Stored permanently in local SQLite
      </span>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
      <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800/70 space-y-1">
        <div class="flex items-center justify-between text-xs text-slate-400">
          <span>Total Downloaded (Received)</span>
          <ArrowDownLeft class="w-4 h-4 text-cyan-400" />
        </div>
        <div class="text-xl font-extrabold font-mono text-white">
          {formatBytes(analytics?.total_received_bytes || 0)}
        </div>
        <div class="text-[11px] text-slate-500 font-mono">
          {analytics?.total_received_files || 0} files • {analytics?.received_sessions || 0} sessions
        </div>
      </div>

      <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800/70 space-y-1">
        <div class="flex items-center justify-between text-xs text-slate-400">
          <span>Total Uploaded (Sent)</span>
          <ArrowUpRight class="w-4 h-4 text-violet-400" />
        </div>
        <div class="text-xl font-extrabold font-mono text-white">
          {formatBytes(analytics?.total_sent_bytes || 0)}
        </div>
        <div class="text-[11px] text-slate-500 font-mono">
          {analytics?.total_sent_files || 0} files • {analytics?.sent_sessions || 0} sessions
        </div>
      </div>

      <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800/70 space-y-1">
        <div class="flex items-center justify-between text-xs text-slate-400">
          <span>Lifetime Total Volume</span>
          <Activity class="w-4 h-4 text-cyan-400" />
        </div>
        <div class="text-xl font-extrabold font-mono text-white">
          {formatBytes(totalLifetimeBytes)}
        </div>
        <div class="text-[11px] text-slate-500 font-mono">
          LAN: {formatBytes(analytics?.lan_bytes || 0)} • WAN: {formatBytes(analytics?.wan_bytes || 0)}
        </div>
      </div>

      <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800/70 space-y-1">
        <div class="flex items-center justify-between text-xs text-slate-400">
          <span>Transfer Reliability</span>
          <CheckCircle2 class="w-4 h-4 text-cyan-400" />
        </div>
        <div class="text-xl font-extrabold font-mono text-white">
          {successRate}%
        </div>
        <div class="text-[11px] text-slate-500 font-mono">
          {totalSessions} total • {analytics?.failed_sessions || 0} failed
        </div>
      </div>
    </div>

    <!-- 7-Day Activity Chart -->
    {#if analytics && analytics.daily.length > 0}
      <div class="p-3.5 rounded-xl bg-slate-950/60 border border-slate-800/70 space-y-2.5">
        <div class="flex items-center justify-between text-xs">
          <span class="font-semibold text-slate-300">7-Day Transfer Volume</span>
          <div class="flex items-center gap-3 text-[11px] font-mono">
            <span class="flex items-center gap-1 text-cyan-300">
              <span class="w-2 h-2 rounded-sm bg-cyan-500"></span> Received
            </span>
            <span class="flex items-center gap-1 text-violet-300">
              <span class="w-2 h-2 rounded-sm bg-violet-400"></span> Sent
            </span>
          </div>
        </div>
        <div class="grid grid-cols-7 gap-2 h-24 items-end pt-2">
          {#each analytics.daily as day}
            {@const rxPct = Math.max(4, Math.round((day.received_bytes / maxDailyBytes) * 100))}
            {@const txPct = Math.max(4, Math.round((day.sent_bytes / maxDailyBytes) * 100))}
            <div class="flex flex-col items-center gap-1.5 h-full justify-end">
              <div class="flex items-end gap-1 w-full justify-center flex-1">
                <div
                  class="w-2.5 rounded-t bg-cyan-500 transition-all"
                  style="height: {day.received_bytes > 0 ? rxPct : 4}%; opacity: {day.received_bytes > 0 ? 1 : 0.25}"
                  title="Received: {formatBytes(day.received_bytes)}"
                ></div>
                <div
                  class="w-2.5 rounded-t bg-violet-400 transition-all"
                  style="height: {day.sent_bytes > 0 ? txPct : 4}%; opacity: {day.sent_bytes > 0 ? 1 : 0.25}"
                  title="Sent: {formatBytes(day.sent_bytes)}"
                ></div>
              </div>
              <span class="text-[10px] font-mono text-slate-500">
                {formatDayShort(day.day_epoch_secs)}
              </span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <!-- Bottom Row: System Information + Storage Drives -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-3.5 pb-4">
    <!-- System Information -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-3">
      <div class="flex items-center gap-2">
        <Server class="w-4 h-4 text-cyan-400" />
        <h3 class="text-sm font-bold text-white">System Information</h3>
      </div>

      <div class="grid grid-cols-2 gap-2.5 text-xs">
        <div class="p-2.5 rounded-xl bg-slate-950/60">
          <div class="text-[10px] text-slate-500 uppercase">Hostname</div>
          <div class="font-semibold text-slate-200 truncate">{telemetry?.hostname || "--"}</div>
        </div>
        <div class="p-2.5 rounded-xl bg-slate-950/60">
          <div class="text-[10px] text-slate-500 uppercase">Operating System</div>
          <div class="font-semibold text-slate-200 truncate">
            {telemetry ? `${telemetry.os_name} ${telemetry.os_version}` : "--"}
          </div>
        </div>
        <div class="p-2.5 rounded-xl bg-slate-950/60">
          <div class="text-[10px] text-slate-500 uppercase">Architecture & Kernel</div>
          <div class="font-mono text-slate-200 truncate">
            {telemetry ? `${telemetry.arch} • ${telemetry.kernel_version}` : "--"}
          </div>
        </div>
        <div class="p-2.5 rounded-xl bg-slate-950/60">
          <div class="text-[10px] text-slate-500 uppercase">System Uptime</div>
          <div class="font-mono text-slate-200">
            {telemetry ? formatUptime(telemetry.uptime_secs) : "--"}
          </div>
        </div>
        <div class="p-2.5 rounded-xl bg-slate-950/60 col-span-2">
          <div class="text-[10px] text-slate-500 uppercase">Raydrop Device ID & Network</div>
          <div class="font-mono text-slate-300 text-[11px] truncate">
            {$appSettings.device_id} • LAN {$networkInfo.localIp}:{$networkInfo.listenPort}
            {$networkInfo.wanReflexiveAddr ? ` • WAN ${$networkInfo.wanReflexiveAddr}` : ""}
          </div>
        </div>
      </div>
    </div>

    <!-- Disk / Storage Usage -->
    <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-3">
      <div class="flex items-center gap-2">
        <HardDrive class="w-4 h-4 text-cyan-400" />
        <h3 class="text-sm font-bold text-white">Storage & Disks</h3>
      </div>

      <div class="space-y-2.5 max-h-56 overflow-y-auto pr-1">
        {#if telemetry && telemetry.disks.length > 0}
          {#each telemetry.disks as d}
            <div class="p-2.5 rounded-xl bg-slate-950/60 space-y-1.5">
              <div class="flex items-center justify-between text-xs">
                <span class="font-semibold text-slate-200 truncate">
                  {d.mount_point} {d.name ? `(${d.name})` : ""}
                </span>
                <span class="font-mono text-[11px] text-slate-400 shrink-0">
                  {formatBytes(d.used_bytes)} / {formatBytes(d.total_bytes)} ({d.usage_percent.toFixed(0)}%)
                </span>
              </div>
              <div class="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden">
                <div
                  class="h-full bg-cyan-500 rounded-full transition-all"
                  style="width: {Math.min(100, d.usage_percent)}%"
                ></div>
              </div>
            </div>
          {/each}
        {:else}
          <div class="text-xs text-slate-500 py-4 text-center">
            Loading storage telemetry...
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
