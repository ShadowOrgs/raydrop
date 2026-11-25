<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    Activity,
    AlertTriangle,
    CheckCircle2,
    Download,
    Gauge,
    Globe,
    Layout,
    Network,
    Shield,
    Terminal,
    Zap,
  } from "lucide-svelte";
  import {
    auditHistory,
    buildMarkdownReport,
    currentAudit,
    debugSession,
    liveCls,
    liveErrors,
    liveRequests,
    speedtestResult,
    type SpeedtestResult,
  } from "../stores/webDebugger";
  import ScoreGauge from "./charts/ScoreGauge.svelte";
  import LatencyCharts from "./charts/LatencyCharts.svelte";

  type SubTab = "overview" | "network" | "ui" | "console";
  let activeSubTab: SubTab = "overview";
  let severityFilter: "all" | "critical" | "warning" | "info" = "all";

  let speedtestUrl = "";
  let lastAutoUrl = "";
  let speedtestLoading = false;
  let speedtestError = "";

  $: targetUrl = $currentAudit?.url || $debugSession.url || speedtestUrl;
  $: if ($currentAudit?.url && $currentAudit.url !== lastAutoUrl) {
    lastAutoUrl = $currentAudit.url;
    speedtestUrl = $currentAudit.url;
  }

  $: filteredFixes = ($currentAudit?.fixes || []).filter(
    (f) => severityFilter === "all" || f.severity === severityFilter,
  );

  async function runRustSpeedtest() {
    const url = (speedtestUrl || targetUrl).trim();
    if (!url) return;
    speedtestLoading = true;
    speedtestError = "";
    try {
      const res = await invoke<SpeedtestResult>("run_web_speedtest", { url });
      speedtestResult.set(res);
    } catch (err) {
      speedtestError = String(err);
    } finally {
      speedtestLoading = false;
    }
  }

  function downloadFile(filename: string, content: string, mime: string) {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  }

  function exportJson() {
    if (!$currentAudit) return;
    downloadFile(
      `raydrop-audit-${Date.now()}.json`,
      JSON.stringify($currentAudit, null, 2),
      "application/json",
    );
  }

  function exportMarkdown() {
    if (!$currentAudit) return;
    downloadFile(
      `raydrop-audit-${Date.now()}.md`,
      buildMarkdownReport($currentAudit),
      "text/markdown",
    );
  }

  function sevBadgeClass(sev: string): string {
    if (sev === "critical") return "bg-rose-500/20 text-rose-300 border-rose-500/40";
    if (sev === "warning") return "bg-amber-500/20 text-amber-300 border-amber-500/40";
    return "bg-sky-500/20 text-sky-300 border-sky-500/40";
  }

  function statusClass(code: number): string {
    if (code >= 500 || code === 0) return "text-rose-400";
    if (code >= 400) return "text-amber-400";
    return "text-emerald-400";
  }
</script>

<div class="h-full w-full flex flex-col gap-4 p-4 sm:p-5 overflow-y-auto text-slate-100">
  <!-- Top Header -->
  <div class="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-white/10">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-500/25 to-indigo-500/25 border border-cyan-400/30 flex items-center justify-center">
        <Globe class="w-5 h-5 text-cyan-400" />
      </div>
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-base font-extrabold tracking-tight text-white">Web Debugger &amp; Analyzer</h2>
          {#if $debugSession.active}
            <span class="px-2 py-0.5 rounded-full text-[10px] font-bold bg-amber-500/20 text-amber-300 border border-amber-500/40 animate-pulse">
              LIVE RECORDING
            </span>
          {:else}
            <span class="px-2 py-0.5 rounded-full text-[10px] font-bold bg-slate-800 text-slate-400 border border-white/10">
              IDLE (0% OVERHEAD)
            </span>
          {/if}
        </div>
        <p class="text-xs text-slate-400 font-mono truncate max-w-md">
          {targetUrl || "Start audit or live debug from Raydrop Chrome Extension"}
        </p>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div class="flex items-center gap-1.5 bg-slate-900/80 border border-white/10 rounded-lg px-2 py-1">
        <input
          type="text"
          bind:value={speedtestUrl}
          placeholder="https://example.com"
          class="bg-transparent text-xs text-white outline-none w-44 font-mono"
        />
        <button
          type="button"
          on:click={runRustSpeedtest}
          disabled={speedtestLoading || !speedtestUrl.trim()}
          class="px-2.5 py-1 rounded-md bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-400/30 text-[11px] font-semibold flex items-center gap-1 disabled:opacity-40 cursor-pointer"
        >
          <Zap class="w-3.5 h-3.5" />
          {speedtestLoading ? "Testing..." : "Speedtest"}
        </button>
      </div>

      {#if $currentAudit}
        <button
          type="button"
          on:click={exportMarkdown}
          class="px-2.5 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
        >
          <Download class="w-3.5 h-3.5 text-cyan-400" />
          MD
        </button>
        <button
          type="button"
          on:click={exportJson}
          class="px-2.5 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
        >
          <Download class="w-3.5 h-3.5 text-indigo-400" />
          JSON
        </button>
      {/if}
    </div>
  </div>

  {#if speedtestError}
    <div class="rounded-lg bg-rose-500/15 border border-rose-500/30 px-3 py-2 text-xs text-rose-300">
      Speedtest error: {speedtestError}
    </div>
  {/if}

  {#if $speedtestResult}
    <div class="rounded-xl bg-slate-900/75 border border-cyan-500/25 p-3 flex flex-wrap items-center justify-between gap-3">
      <div class="flex items-center gap-2">
        <Gauge class="w-4 h-4 text-cyan-400" />
        <span class="text-xs font-bold text-white">Rust Engine Speedtest:</span>
        <span class="text-xs font-mono text-slate-300 truncate max-w-xs">{$speedtestResult.url}</span>
      </div>
      <div class="flex flex-wrap items-center gap-4 text-xs font-mono">
        <span>Status: <strong class="text-emerald-400">{$speedtestResult.statusCode}</strong></span>
        <span>Min: <strong class="text-white">{$speedtestResult.minMs}ms</strong></span>
        <span>Avg: <strong class="text-cyan-300">{$speedtestResult.avgMs}ms</strong></span>
        <span>Jitter: <strong class="text-amber-300">{$speedtestResult.jitterMs}ms</strong></span>
        <span>Throughput: <strong class="text-emerald-300">{$speedtestResult.mbps} Mbps</strong></span>
      </div>
    </div>
  {/if}

  {#if !$currentAudit && $liveRequests.length === 0 && $liveErrors.length === 0}
    <div class="flex-1 flex flex-col items-center justify-center text-center py-12 px-4 rounded-2xl bg-slate-900/40 border border-dashed border-white/10">
      <Activity class="w-10 h-10 text-cyan-400/80 mb-3" />
      <h3 class="text-sm font-bold text-white mb-1">Belum Ada Sesi Analisa Aktif</h3>
      <p class="text-xs text-slate-400 max-w-md leading-relaxed">
        Buka <strong>Raydrop Chrome Extension</strong> → tab <strong>Web Debugger</strong>, lalu klik
        <strong>Run Full Audit</strong> (cek UI, Responsive, Speed, Latency &amp; Score) atau
        <strong>Start Live Debug</strong> untuk memantau request &amp; JS error secara real-time.
      </p>
    </div>
  {:else}
    <!-- Sub-Navigation Tabs -->
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="flex items-center gap-1.5 bg-slate-900/70 border border-white/10 p-1 rounded-xl">
        <button
          type="button"
          on:click={() => (activeSubTab = "overview")}
          class="px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 cursor-pointer {activeSubTab === 'overview'
            ? 'bg-gradient-to-r from-cyan-500 to-indigo-500 text-white'
            : 'text-slate-400 hover:text-white'}"
        >
          <Gauge class="w-3.5 h-3.5" /> Overview &amp; Fixes
        </button>
        <button
          type="button"
          on:click={() => (activeSubTab = "network")}
          class="px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 cursor-pointer {activeSubTab === 'network'
            ? 'bg-gradient-to-r from-cyan-500 to-indigo-500 text-white'
            : 'text-slate-400 hover:text-white'}"
        >
          <Network class="w-3.5 h-3.5" /> Network ({$liveRequests.length || $currentAudit?.resources.totalRequests || 0})
        </button>
        <button
          type="button"
          on:click={() => (activeSubTab = "ui")}
          class="px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 cursor-pointer {activeSubTab === 'ui'
            ? 'bg-gradient-to-r from-cyan-500 to-indigo-500 text-white'
            : 'text-slate-400 hover:text-white'}"
        >
          <Layout class="w-3.5 h-3.5" /> UI &amp; Responsive
        </button>
        <button
          type="button"
          on:click={() => (activeSubTab = "console")}
          class="px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 cursor-pointer {activeSubTab === 'console'
            ? 'bg-gradient-to-r from-cyan-500 to-indigo-500 text-white'
            : 'text-slate-400 hover:text-white'}"
        >
          <Terminal class="w-3.5 h-3.5" /> Console &amp; Headers ({$liveErrors.length})
        </button>
      </div>

      {#if $auditHistory.length > 1}
        <select
          class="bg-slate-900 border border-white/10 rounded-lg px-2.5 py-1.5 text-xs text-slate-300 outline-none"
          on:change={(e) => {
            const idx = Number(e.currentTarget.value);
            if ($auditHistory[idx]) currentAudit.set($auditHistory[idx]);
          }}
        >
          {#each $auditHistory as item, idx}
            <option value={idx}>
              {new Date(item.timestamp).toLocaleTimeString()} — {item.scores.overall}/100 ({item.title.slice(0, 24)})
            </option>
          {/each}
        </select>
      {/if}
    </div>

    <!-- SUBTAB 1: OVERVIEW & FIXES -->
    {#if activeSubTab === "overview"}
      {#if $currentAudit}
        <div class="rounded-2xl bg-slate-900/75 border border-white/10 p-4 flex flex-wrap items-center justify-around gap-6">
          <ScoreGauge score={$currentAudit.scores.overall} label="Overall Score" size={112} strokeWidth={9} />
          <div class="h-16 w-px bg-white/10 hidden sm:block"></div>
          <ScoreGauge score={$currentAudit.scores.network} label="Network & Latency" size={82} />
          <ScoreGauge score={$currentAudit.scores.speed} label="Performance & Speed" size={82} />
          <ScoreGauge score={$currentAudit.scores.ui} label="UI & Responsive" size={82} />
          <ScoreGauge score={$currentAudit.scores.security} label="Console & Security" size={82} />
        </div>
      {/if}

      <LatencyCharts audit={$currentAudit} liveRequests={$liveRequests} />

      {#if $currentAudit}
        <div class="rounded-xl bg-slate-900/70 border border-white/10 p-4">
          <div class="flex flex-wrap items-center justify-between gap-2 mb-3">
            <div class="flex items-center gap-2">
              <AlertTriangle class="w-4 h-4 text-amber-400" />
              <h3 class="text-xs font-bold uppercase tracking-wider text-white">
                Yang Harus Diperbaiki ({filteredFixes.length})
              </h3>
            </div>
            <div class="flex items-center gap-1 text-[11px]">
              {#each ["all", "critical", "warning", "info"] as sev}
                <button
                  type="button"
                  on:click={() => (severityFilter = sev as any)}
                  class="px-2.5 py-1 rounded-md capitalize cursor-pointer {severityFilter === sev
                    ? 'bg-white/15 text-white font-semibold'
                    : 'text-slate-400 hover:text-white'}"
                >
                  {sev}
                </button>
              {/each}
            </div>
          </div>

          {#if filteredFixes.length === 0}
            <div class="flex items-center gap-2 text-xs text-emerald-400 py-3">
              <CheckCircle2 class="w-4 h-4" />
              <span>Tidak ada masalah pada filter ini.</span>
            </div>
          {:else}
            <div class="space-y-2.5">
              {#each filteredFixes as fix}
                <div class="rounded-lg bg-slate-950/60 border border-white/10 p-3 flex flex-col gap-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="px-2 py-0.5 rounded text-[10px] font-bold uppercase border {sevBadgeClass(fix.severity)}">
                      {fix.severity}
                    </span>
                    <span class="px-2 py-0.5 rounded text-[10px] font-semibold bg-white/5 text-slate-300">
                      {fix.category}
                    </span>
                    <span class="text-xs font-bold text-white">{fix.title}</span>
                  </div>
                  <div class="text-[11px] font-mono text-cyan-300/90 truncate">Target: {fix.target}</div>
                  <div class="text-xs text-slate-300 leading-relaxed">{fix.suggestion}</div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    <!-- SUBTAB 2: NETWORK & SPEEDTEST -->
    {#if activeSubTab === "network"}
      {#if $currentAudit?.ping}
        <div class="grid grid-cols-2 sm:grid-cols-5 gap-2.5">
          <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3">
            <div class="text-[11px] text-slate-400">Ping Min</div>
            <div class="text-base font-extrabold font-mono text-emerald-400">{$currentAudit.ping.min ?? "--"} ms</div>
          </div>
          <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3">
            <div class="text-[11px] text-slate-400">Ping Avg</div>
            <div class="text-base font-extrabold font-mono text-cyan-400">{$currentAudit.ping.avg ?? "--"} ms</div>
          </div>
          <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3">
            <div class="text-[11px] text-slate-400">Ping Max</div>
            <div class="text-base font-extrabold font-mono text-amber-400">{$currentAudit.ping.max ?? "--"} ms</div>
          </div>
          <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3">
            <div class="text-[11px] text-slate-400">Jitter</div>
            <div class="text-base font-extrabold font-mono text-indigo-300">{$currentAudit.ping.jitter ?? "--"} ms</div>
          </div>
          <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3">
            <div class="text-[11px] text-slate-400">Protocol</div>
            <div class="text-base font-extrabold font-mono text-white">{$currentAudit.navigation?.protocol || "--"}</div>
          </div>
        </div>
      {/if}

      <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 overflow-x-auto">
        <h3 class="text-xs font-bold uppercase tracking-wider text-white mb-2.5">
          {$liveRequests.length > 0 ? "Live Network Stream" : "Slowest Page Resources"}
        </h3>
        <table class="w-full text-left text-xs">
          <thead class="text-[11px] text-slate-400 border-b border-white/10">
            <tr>
              <th class="py-2 pr-3">Status</th>
              <th class="py-2 pr-3">Type</th>
              <th class="py-2 pr-3">Duration</th>
              <th class="py-2 pr-3">Cache / IP</th>
              <th class="py-2">URL</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-white/5 font-mono text-[11px]">
            {#if $liveRequests.length > 0}
              {#each $liveRequests as req}
                <tr class="hover:bg-white/5">
                  <td class="py-1.5 pr-3 font-bold {statusClass(req.statusCode)}">{req.statusCode || "ERR"}</td>
                  <td class="py-1.5 pr-3 text-slate-300">{req.type}</td>
                  <td class="py-1.5 pr-3 {req.duration > 800 ? 'text-rose-400 font-bold' : 'text-cyan-300'}">{req.duration} ms</td>
                  <td class="py-1.5 pr-3 text-slate-400">{req.fromCache ? "CACHE" : req.ip || "net"}</td>
                  <td class="py-1.5 text-slate-200 truncate max-w-md" title={req.url}>{req.url}</td>
                </tr>
              {/each}
            {:else if $currentAudit?.resources?.slowest}
              {#each $currentAudit.resources.slowest as res}
                <tr class="hover:bg-white/5">
                  <td class="py-1.5 pr-3 text-emerald-400">200</td>
                  <td class="py-1.5 pr-3 text-slate-300">{res.type}</td>
                  <td class="py-1.5 pr-3 {res.duration > 800 ? 'text-rose-400 font-bold' : 'text-cyan-300'}">{res.duration} ms</td>
                  <td class="py-1.5 pr-3 text-slate-400">{res.cached ? "CACHE" : "net"}</td>
                  <td class="py-1.5 text-slate-200 truncate max-w-md" title={res.url}>{res.url}</td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {/if}

    <!-- SUBTAB 3: UI & RESPONSIVE -->
    {#if activeSubTab === "ui" && $currentAudit}
      <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
        {#each $currentAudit.responsiveMatrix as vp}
          <div class="rounded-xl bg-slate-900/70 border {vp.hasOverflow ? 'border-rose-500/40' : 'border-emerald-500/30'} p-3.5">
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs font-bold text-white">{vp.label} ({vp.width}×{vp.height})</span>
              <span class="px-2 py-0.5 rounded text-[10px] font-bold {vp.hasOverflow ? 'bg-rose-500/20 text-rose-300' : 'bg-emerald-500/20 text-emerald-300'}">
                {vp.hasOverflow ? `OVERFLOW +${vp.overflowPx}px` : "PASS"}
              </span>
            </div>
            <div class="text-[11px] text-slate-400 font-mono">
              ScrollWidth: {vp.scrollWidth ?? "--"}px • Offenders: {vp.offenderCount ?? 0}
            </div>
          </div>
        {/each}
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 space-y-2 text-xs">
          <h4 class="font-bold text-white uppercase tracking-wider">UI &amp; Web Vitals Summary</h4>
          <div class="grid grid-cols-2 gap-2 font-mono text-[11px]">
            <div class="p-2 rounded bg-slate-950/60">LCP: <strong class="text-cyan-300">{$currentAudit.vitals.lcp ?? "--"} ms</strong></div>
            <div class="p-2 rounded bg-slate-950/60">FCP: <strong class="text-cyan-300">{$currentAudit.vitals.fcp ?? "--"} ms</strong></div>
            <div class="p-2 rounded bg-slate-950/60">CLS: <strong class="text-emerald-300">{$debugSession.active ? $liveCls : $currentAudit.vitals.cls}</strong></div>
            <div class="p-2 rounded bg-slate-950/60">DOM Nodes: <strong class="text-white">{$currentAudit.ui.domNodes}</strong></div>
            <div class="p-2 rounded bg-slate-950/60">Small Text (&lt;12px): <strong class="text-amber-300">{$currentAudit.ui.smallTextCount}</strong></div>
            <div class="p-2 rounded bg-slate-950/60">Missing Alt Img: <strong class="text-amber-300">{$currentAudit.ui.missingAltCount}</strong></div>
          </div>
        </div>

        <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 space-y-2 text-xs">
          <h4 class="font-bold text-white uppercase tracking-wider">Overflow &amp; Asset Issues</h4>
          {#if $currentAudit.ui.overflowElements.length === 0 && $currentAudit.ui.brokenImages.length === 0 && $currentAudit.ui.oversizedImages.length === 0}
            <div class="text-emerald-400 py-2">Semua elemen UI dan gambar terlihat rapi!</div>
          {:else}
            <div class="space-y-1.5 max-h-40 overflow-y-auto font-mono text-[11px]">
              {#each $currentAudit.ui.overflowElements as el}
                <div class="text-rose-300">Overflow +{el.overflowPx}px: {el.selector}</div>
              {/each}
              {#each $currentAudit.ui.brokenImages as img}
                <div class="text-rose-400">Broken img: {img.selector}</div>
              {/each}
              {#each $currentAudit.ui.oversizedImages as img}
                <div class="text-amber-300">Oversized {img.natural} → {img.rendered}: {img.selector}</div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <!-- SUBTAB 4: CONSOLE & HEADERS -->
    {#if activeSubTab === "console"}
      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5">
          <div class="flex items-center gap-2 mb-2">
            <Terminal class="w-4 h-4 text-rose-400" />
            <h4 class="text-xs font-bold uppercase tracking-wider text-white">Live JS Errors ({$liveErrors.length})</h4>
          </div>
          {#if $liveErrors.length === 0}
            <div class="text-xs text-slate-400 py-3">Tidak ada runtime error yang tertangkap.</div>
          {:else}
            <div class="space-y-2 max-h-64 overflow-y-auto font-mono text-[11px]">
              {#each $liveErrors as err}
                <div class="p-2 rounded bg-rose-500/10 border border-rose-500/25 text-rose-200">
                  <div class="font-bold">{err.message}</div>
                  <div class="text-[10px] text-rose-300/70">{err.source}</div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5">
          <div class="flex items-center gap-2 mb-2">
            <Shield class="w-4 h-4 text-cyan-400" />
            <h4 class="text-xs font-bold uppercase tracking-wider text-white">Response &amp; Security Headers</h4>
          </div>
          {#if $currentAudit?.ping?.headers}
            <div class="space-y-1.5 font-mono text-[11px]">
              {#each Object.entries($currentAudit.ping.headers) as [k, v]}
                <div class="p-2 rounded bg-slate-950/60 border border-white/5">
                  <span class="text-cyan-300">{k}:</span>
                  <span class="text-slate-300 break-all ml-1">{v}</span>
                </div>
              {/each}
            </div>
          {:else}
            <div class="text-xs text-slate-400 py-3">Jalankan Full Audit untuk melihat headers.</div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
