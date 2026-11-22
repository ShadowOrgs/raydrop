<script lang="ts">
  import { Zap, X, Globe, Wifi, Save, CheckCircle2, AlertCircle } from "lucide-svelte";
  import { contacts } from "../stores/contacts";
  import {
    wolModalTarget,
    getWolConfig,
    saveWolConfig,
    executeWakeOnLan,
  } from "../stores/wol";

  let mac = "";
  let wanHost = "";
  let port = 9;
  let sending = false;
  let statusMsg = "";
  let errorMsg = "";
  let autoDetectedMac = false;

  $: if ($wolModalTarget) {
    const cfg = getWolConfig($wolModalTarget.deviceId);
    const contact = $contacts.find((c) => c.device_id === $wolModalTarget?.deviceId);
    const detected = contact?.is_own_device && contact?.mac_address ? contact.mac_address : "";
    mac = cfg.mac || detected;
    autoDetectedMac = !cfg.mac && !!detected;
    wanHost = cfg.wanHost;
    port = cfg.port || 9;
    statusMsg = "";
    errorMsg = "";
  }

  function close() {
    wolModalTarget.set(null);
  }

  function handleSaveOnly() {
    if (!$wolModalTarget) return;
    errorMsg = "";
    saveWolConfig($wolModalTarget.deviceId, { mac, wanHost, port });
    statusMsg = "Wake-on-LAN configuration saved!";
  }

  async function handleWake() {
    if (!$wolModalTarget || sending) return;
    if (!mac.trim()) {
      errorMsg = "Please enter the target device's MAC address.";
      return;
    }
    sending = true;
    errorMsg = "";
    statusMsg = "";
    try {
      await executeWakeOnLan($wolModalTarget.deviceId, $wolModalTarget.deviceName, {
        mac,
        wanHost,
        port,
      });
      statusMsg = wanHost.trim()
        ? `⚡ WAN Magic Packet sent to ${wanHost.trim()}:${port || 9} (${mac.trim()})!`
        : `⚡ LAN Magic Packet broadcast sent to ${mac.trim()} (port ${port || 9})!`;
    } catch (err: any) {
      errorMsg = err?.message || String(err);
    } finally {
      sending = false;
    }
  }
</script>

{#if $wolModalTarget}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
    on:click={close}
  >
    <div
      class="bg-slate-900 border border-slate-700/80 rounded-2xl max-w-md w-full p-5 space-y-4 shadow-2xl max-h-[90vh] overflow-y-auto"
      on:click|stopPropagation
    >
      <div class="flex items-center justify-between border-b border-slate-800 pb-3">
        <div class="flex items-center gap-2.5">
          <div class="w-9 h-9 rounded-xl bg-amber-500/15 border border-amber-500/30 flex items-center justify-center text-amber-400">
            <Zap class="w-4 h-4" />
          </div>
          <div>
            <h3 class="text-sm font-bold text-white">Wake-on-LAN (LAN & WAN)</h3>
            <p class="text-[11px] text-slate-400 truncate max-w-[260px]">
              Target: <span class="text-slate-200 font-semibold">{$wolModalTarget.deviceName}</span>
            </p>
          </div>
        </div>
        <button
          type="button"
          on:click={close}
          class="p-1.5 rounded-lg bg-slate-800 text-slate-400 hover:text-white cursor-pointer"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="space-y-3">
        <div>
          <div class="flex items-center justify-between mb-1">
            <label for="wol-mac" class="block text-xs font-semibold text-slate-300">
              MAC Address <span class="text-rose-400">*</span>
            </label>
            {#if autoDetectedMac}
              <span class="text-[10px] font-semibold text-emerald-300 bg-emerald-500/15 border border-emerald-500/30 px-2 py-0.5 rounded-full">
                Auto-detected (Same User)
              </span>
            {/if}
          </div>
          <input
            id="wol-mac"
            type="text"
            bind:value={mac}
            placeholder="AA:BB:CC:DD:EE:FF"
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
          />
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
          <div class="sm:col-span-2">
            <label for="wol-host" class="flex items-center gap-1 text-xs font-semibold text-slate-300 mb-1">
              <Globe class="w-3.5 h-3.5 text-cyan-400" />
              <span>WAN Host / DDNS (Optional)</span>
            </label>
            <input
              id="wol-host"
              type="text"
              bind:value={wanHost}
              placeholder="rumah.duckdns.org or Public IP"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
            />
          </div>

          <div>
            <label for="wol-port" class="block text-xs font-semibold text-slate-300 mb-1">
              UDP Port
            </label>
            <input
              id="wol-port"
              type="number"
              min="1"
              max="65535"
              bind:value={port}
              placeholder="9"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        <div class="rounded-xl bg-slate-950/70 border border-slate-800/80 p-3 text-[11px] text-slate-400 space-y-1">
          <div class="flex items-center gap-1.5 font-semibold text-slate-300">
            <Wifi class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
            <span>LAN vs WAN Mode</span>
          </div>
          <p>
            Kosongkan <strong>WAN Host / DDNS</strong> untuk broadcast LAN lokal (port 9 & 7). Isi dengan domain DDNS / IP Publik + port custom jika router target sudah diset Port Forwarding UDP + Static ARP.
          </p>
        </div>

        {#if errorMsg}
          <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
            <AlertCircle class="w-4 h-4 shrink-0" />
            <span>{errorMsg}</span>
          </div>
        {/if}

        {#if statusMsg}
          <div class="flex items-center gap-2 text-xs text-emerald-300 bg-emerald-500/10 border border-emerald-500/30 px-3 py-2 rounded-xl">
            <CheckCircle2 class="w-4 h-4 shrink-0" />
            <span>{statusMsg}</span>
          </div>
        {/if}
      </div>

      <div class="flex items-center justify-end gap-2 pt-2 border-t border-slate-800">
        <button
          type="button"
          on:click={handleSaveOnly}
          class="flex items-center gap-1.5 px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 cursor-pointer"
        >
          <Save class="w-3.5 h-3.5" />
          <span>Save Config</span>
        </button>
        <button
          type="button"
          on:click={handleWake}
          disabled={sending}
          class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-amber-500 hover:bg-amber-400 text-slate-950 text-xs font-bold shadow-md shadow-amber-500/20 disabled:opacity-50 cursor-pointer"
        >
          <Zap class="w-3.5 h-3.5" />
          <span>{sending ? "Sending..." : "Save & Wake PC"}</span>
        </button>
      </div>
    </div>
  </div>
{/if}
