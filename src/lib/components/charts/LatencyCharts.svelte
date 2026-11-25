<script lang="ts">
  import type { LiveNetworkEvent, WebAuditReport } from "../../stores/webDebugger";

  export let audit: WebAuditReport | null = null;
  export let liveRequests: LiveNetworkEvent[] = [];

  $: nav = audit?.navigation;
  $: timingSegments = nav
    ? [
        { label: "DNS", ms: nav.dns, color: "#38bdf8" },
        { label: "TCP", ms: nav.tcp, color: "#818cf8" },
        { label: "TLS", ms: nav.tls, color: "#a78bfa" },
        { label: "TTFB", ms: nav.ttfb, color: "#06b6d4" },
        { label: "Download", ms: nav.download, color: "#10b981" },
      ]
    : [];
  $: timingSum = timingSegments.reduce((acc, s) => acc + s.ms, 0) || 1;

  $: byType = audit?.resources?.byType || {};
  $: payloadBuckets = [
    { label: "JS", size: byType.script?.size || 0, color: "#f59e0b" },
    { label: "CSS", size: byType.css?.size || 0, color: "#38bdf8" },
    { label: "Images", size: byType.img?.size || 0, color: "#10b981" },
    { label: "Fonts", size: byType.font?.size || 0, color: "#a78bfa" },
    { label: "API", size: byType.api?.size || 0, color: "#06b6d4" },
    { label: "Other", size: byType.other?.size || 0, color: "#64748b" },
  ].filter((b) => b.size > 0);
  $: totalPayload = payloadBuckets.reduce((acc, b) => acc + b.size, 0) || 1;

  // Sparkline points from live requests (reversed to chronological) or ping samples
  $: sparkValues =
    liveRequests.length > 1
      ? liveRequests
          .slice(0, 30)
          .reverse()
          .map((r) => r.duration)
      : (audit?.ping?.samples || []).filter((v): v is number => typeof v === "number");

  $: sparkMax = Math.max(50, ...sparkValues);
  $: sparkPoints =
    sparkValues.length > 1
      ? sparkValues
          .map((v, i) => {
            const x = (i / (sparkValues.length - 1)) * 280;
            const y = 64 - (v / sparkMax) * 54;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(" ")
      : "";

  $: areaPoints = sparkPoints ? `0,68 ${sparkPoints} 280,68` : "";

  function fmtBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }
</script>

<div class="grid grid-cols-1 lg:grid-cols-3 gap-3">
  <!-- 1. Navigation Timing Breakdown -->
  <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 flex flex-col justify-between">
    <div class="flex items-center justify-between mb-2">
      <h4 class="text-xs font-bold text-white uppercase tracking-wider">Timing Breakdown</h4>
      {#if nav}
        <span class="text-[11px] font-mono text-cyan-400">{nav.loadTotal} ms total</span>
      {/if}
    </div>

    {#if nav}
      <div class="h-4 w-full rounded-full overflow-hidden flex bg-slate-800 my-2">
        {#each timingSegments as seg}
          {#if seg.ms > 0}
            <div
              style="width: {Math.max(3, (seg.ms / timingSum) * 100)}%; background-color: {seg.color};"
              title="{seg.label}: {seg.ms} ms"
            ></div>
          {/if}
        {/each}
      </div>
      <div class="grid grid-cols-3 gap-y-1.5 gap-x-2 mt-2 text-[11px]">
        {#each timingSegments as seg}
          <div class="flex items-center gap-1.5">
            <span class="w-2 h-2 rounded-full shrink-0" style="background-color: {seg.color};"></span>
            <span class="text-slate-400">{seg.label}:</span>
            <span class="font-mono text-white font-semibold">{seg.ms}ms</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="text-xs text-slate-500 py-4 text-center">No timing data yet</div>
    {/if}
  </div>

  <!-- 2. Latency Sparkline -->
  <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 flex flex-col justify-between">
    <div class="flex items-center justify-between mb-1">
      <h4 class="text-xs font-bold text-white uppercase tracking-wider">
        {liveRequests.length > 1 ? "Live Request Latency" : "Endpoint Ping Latency"}
      </h4>
      <span class="text-[11px] font-mono text-emerald-400">
        {sparkValues.length > 0 ? `Peak ${Math.max(...sparkValues)} ms` : "--"}
      </span>
    </div>

    {#if sparkValues.length > 1}
      <svg viewBox="0 0 280 70" class="w-full h-16 overflow-visible">
        <defs>
          <linearGradient id="raySparkGrad" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stop-color="#06b6d4" stop-opacity="0.38" />
            <stop offset="100%" stop-color="#06b6d4" stop-opacity="0.0" />
          </linearGradient>
        </defs>
        <polygon points={areaPoints} fill="url(#raySparkGrad)" />
        <polyline
          fill="none"
          stroke="#22d3ee"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          points={sparkPoints}
        />
      </svg>
      <div class="flex justify-between text-[10px] font-mono text-slate-400 mt-1">
        <span>{sparkValues.length} samples</span>
        <span>Avg {Math.round(sparkValues.reduce((a, b) => a + b, 0) / sparkValues.length)} ms</span>
      </div>
    {:else}
      <div class="text-xs text-slate-500 py-4 text-center">Run audit or start Live Debug</div>
    {/if}
  </div>

  <!-- 3. Payload Composition -->
  <div class="rounded-xl bg-slate-900/70 border border-white/10 p-3.5 flex flex-col justify-between">
    <div class="flex items-center justify-between mb-2">
      <h4 class="text-xs font-bold text-white uppercase tracking-wider">Payload Composition</h4>
      <span class="text-[11px] font-mono text-indigo-300">
        {audit ? fmtBytes(audit.resources.totalTransfer) : "--"}
      </span>
    </div>

    {#if payloadBuckets.length > 0}
      <div class="h-4 w-full rounded-full overflow-hidden flex bg-slate-800 my-2">
        {#each payloadBuckets as b}
          <div
            style="width: {Math.max(3, (b.size / totalPayload) * 100)}%; background-color: {b.color};"
            title="{b.label}: {fmtBytes(b.size)}"
          ></div>
        {/each}
      </div>
      <div class="grid grid-cols-3 gap-y-1.5 gap-x-2 mt-2 text-[11px]">
        {#each payloadBuckets as b}
          <div class="flex items-center gap-1.5">
            <span class="w-2 h-2 rounded-full shrink-0" style="background-color: {b.color};"></span>
            <span class="text-slate-400">{b.label}:</span>
            <span class="font-mono text-white font-semibold">{fmtBytes(b.size)}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="text-xs text-slate-500 py-4 text-center">No resource payload data</div>
    {/if}
  </div>
</div>
