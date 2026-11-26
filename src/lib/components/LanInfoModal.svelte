<script lang="ts">
  import { Wifi, X, CheckCircle2, Circle } from "lucide-svelte";
  import type { LanStatusInfo } from "../utils/tauri-ipc";

  export let deviceName: string;
  export let deviceId: string;
  export let info: LanStatusInfo;
  export let onClose: () => void;
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div
  class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
  on:click|self={onClose}
>
  <div
    class="w-full max-w-sm max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-700/80 rounded-2xl p-5 shadow-2xl space-y-4"
  >
    <div class="flex items-start justify-between gap-3">
      <div class="flex items-center gap-2.5 min-w-0">
        <div
          class="w-9 h-9 rounded-xl flex items-center justify-center shrink-0 {info.active
            ? 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/30'
            : 'bg-slate-800 text-slate-400 border border-slate-700'}"
        >
          <Wifi class="w-4 h-4" />
        </div>
        <div class="min-w-0">
          <h3 class="text-sm font-bold text-white truncate">{deviceName}</h3>
          <p class="text-[11px] font-mono text-slate-400 truncate">{deviceId}</p>
        </div>
      </div>
      <button
        type="button"
        on:click={onClose}
        class="p-1 text-slate-400 hover:text-white rounded-lg hover:bg-slate-800 cursor-pointer"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div
      class="p-3 rounded-xl border text-xs {info.active
        ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-200'
        : 'bg-slate-950/70 border-slate-800 text-slate-300'}"
    >
      <div class="font-semibold mb-0.5">
        Status Aktif: {info.active ? `LAN (${info.label})` : "WAN / Belum LAN"}
      </div>
      <p class="text-[11px] opacity-90 leading-relaxed">{info.detail}</p>
    </div>

    <div class="space-y-2">
      <p class="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
        Jalur Koneksi LAN
      </p>

      <div class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950/70 border border-slate-800 text-xs">
        <div class="flex items-center gap-2">
          {#if info.local_active}
            <CheckCircle2 class="w-4 h-4 text-emerald-400 shrink-0" />
          {:else}
            <Circle class="w-4 h-4 text-slate-600 shrink-0" />
          {/if}
          <div>
            <div class="font-semibold text-slate-200">Local LAN (Wi-Fi / Ethernet)</div>
            <div class="text-[10px] text-slate-400">
              {info.local_active && info.ip ? `IP: ${info.ip}` : "Tidak berada di subnet lokal yang sama"}
            </div>
          </div>
        </div>
        <span class="text-[10px] font-bold px-2 py-0.5 rounded-full {info.local_active ? 'bg-emerald-500/15 text-emerald-300' : 'bg-slate-800 text-slate-500'}">
          {info.local_active ? "Aktif" : "Off"}
        </span>
      </div>

      <div class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950/70 border border-slate-800 text-xs">
        <div class="flex items-center gap-2">
          {#if info.tailscale_active}
            <CheckCircle2 class="w-4 h-4 text-indigo-400 shrink-0" />
          {:else}
            <Circle class="w-4 h-4 text-slate-600 shrink-0" />
          {/if}
          <div>
            <div class="font-semibold text-slate-200">Tailscale Virtual LAN</div>
            <div class="text-[10px] text-slate-400">
              {info.tailscale_active && info.ip ? `IP: ${info.ip}` : "Tailscale tidak aktif / belum terdeteksi"}
            </div>
          </div>
        </div>
        <span class="text-[10px] font-bold px-2 py-0.5 rounded-full {info.tailscale_active ? 'bg-indigo-500/15 text-indigo-300' : 'bg-slate-800 text-slate-500'}">
          {info.tailscale_active ? "Aktif" : "Off"}
        </span>
      </div>

      <div class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950/70 border border-slate-800 text-xs">
        <div class="flex items-center gap-2">
          {#if info.mesh_active}
            <CheckCircle2 class="w-4 h-4 text-sky-400 shrink-0" />
          {:else}
            <Circle class="w-4 h-4 text-slate-600 shrink-0" />
          {/if}
          <div>
            <div class="font-semibold text-slate-200">Raydrop Direct Mesh</div>
            <div class="text-[10px] text-slate-400">
              {info.mesh_active ? "Tunnel QUIC P2P hangat aktif" : "Belum ada tunnel Direct Mesh aktif"}
            </div>
          </div>
        </div>
        <span class="text-[10px] font-bold px-2 py-0.5 rounded-full {info.mesh_active ? 'bg-sky-500/15 text-sky-300' : 'bg-slate-800 text-slate-500'}">
          {info.mesh_active ? "Aktif" : "Off"}
        </span>
      </div>
    </div>

    <button
      type="button"
      on:click={onClose}
      class="w-full py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white transition cursor-pointer"
    >
      Tutup
    </button>
  </div>
</div>
