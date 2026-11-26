<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { X, Send, Loader2, CheckCircle2, AlertTriangle, Laptop, Users, Wifi } from "lucide-svelte";
  import { contacts, lanContacts } from "../../stores/contacts";
  import { cloudMyDevices, cloudContacts, appSettings } from "../../stores/state";
  import { raydropApi, skillsApi, type SkillRecord } from "../../utils/tauri-ipc";

  export let skill: SkillRecord;

  const dispatch = createEventDispatcher<{ close: void }>();

  interface TargetPeer {
    device_id: string;
    label: string;
    group: "My Devices" | "Contacts" | "LAN Radar";
    is_online: boolean;
  }

  $: allTargets = (() => {
    const map = new Map<string, TargetPeer>();
    const myId = $appSettings.device_id;

    for (const d of $cloudMyDevices) {
      if (d.device_id === myId) continue;
      map.set(d.device_id, {
        device_id: d.device_id,
        label: d.alias_name || d.device_id,
        group: "My Devices",
        is_online: d.is_online,
      });
    }
    for (const grp of $cloudContacts) {
      for (const d of grp.devices || []) {
        if (d.device_id === myId || map.has(d.device_id)) continue;
        map.set(d.device_id, {
          device_id: d.device_id,
          label: `${grp.user_name || grp.email} (${d.alias_name || d.device_id.slice(0, 8)})`,
          group: "Contacts",
          is_online: d.is_online,
        });
      }
    }
    for (const c of $contacts) {
      if (c.device_id === myId || map.has(c.device_id)) continue;
      map.set(c.device_id, {
        device_id: c.device_id,
        label: c.custom_name || c.alias_name || c.device_id,
        group: c.is_own_device ? "My Devices" : "Contacts",
        is_online: c.is_online,
      });
    }
    for (const l of $lanContacts) {
      if (l.device_id === myId || map.has(l.device_id)) continue;
      map.set(l.device_id, {
        device_id: l.device_id,
        label: l.custom_name || l.alias_name || l.device_id,
        group: "LAN Radar",
        is_online: true,
      });
    }
    return Array.from(map.values());
  })();

  let selectedIds: string[] = [];
  let sending = false;
  let errorMsg = "";
  let results: Array<{ device_id: string; label: string; ok: boolean; err?: string }> = [];

  function togglePeer(id: string) {
    if (selectedIds.includes(id)) {
      selectedIds = selectedIds.filter((x) => x !== id);
    } else {
      selectedIds = [...selectedIds, id];
    }
  }

  async function handleShare() {
    if (selectedIds.length === 0 || sending) return;
    sending = true;
    errorMsg = "";
    results = [];
    try {
      const zipPath = await skillsApi.exportZip(skill.id);
      for (const pid of selectedIds) {
        const peer = allTargets.find((t) => t.device_id === pid);
        const label = peer?.label || pid;
        try {
          await raydropApi.initiateTransfer(pid, [zipPath]);
          results = [...results, { device_id: pid, label, ok: true }];
        } catch (err: any) {
          results = [
            ...results,
            { device_id: pid, label, ok: false, err: err?.message || String(err) },
          ];
        }
      }
    } catch (e: any) {
      errorMsg = e?.message || String(e);
    } finally {
      sending = false;
    }
  }
</script>

<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-3 sm:p-6">
  <div class="w-full max-w-lg max-h-[85vh] rounded-2xl bg-slate-900 border border-slate-700/80 shadow-2xl flex flex-col overflow-hidden">
    <div class="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
      <div>
        <h3 class="text-base font-bold text-white">Share Skill via P2P</h3>
        <p class="text-xs text-slate-400">
          {skill.name} — dikirim sebagai arsip <span class="font-mono text-cyan-300">.zip</span> (siap Import ZIP di device penerima)
        </p>
      </div>
      <button
        type="button"
        on:click={() => dispatch("close")}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-5 space-y-3">
      {#if allTargets.length === 0}
        <div class="p-6 rounded-xl bg-slate-950/70 border border-slate-800 text-center text-xs text-slate-400">
          Belum ada device atau kontak lain yang terdeteksi. Pastikan device tujuan ada di Contacts / My Devices / LAN Radar.
        </div>
      {:else}
        <div class="text-xs font-bold text-slate-200">Pilih Device / Kontak Tujuan ({selectedIds.length} dipilih)</div>
        <div class="space-y-1.5">
          {#each allTargets as peer}
            {@const checked = selectedIds.includes(peer.device_id)}
            <button
              type="button"
              on:click={() => togglePeer(peer.device_id)}
              class="w-full p-2.5 rounded-xl border text-left text-xs flex items-center justify-between gap-2 cursor-pointer transition {checked ? 'bg-cyan-500/15 border-cyan-500/50 text-white' : 'bg-slate-950/60 border-slate-800 text-slate-300 hover:border-slate-700'}"
            >
              <div class="flex items-center gap-2 min-w-0">
                {#if peer.group === "My Devices"}
                  <Laptop class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                {:else if peer.group === "LAN Radar"}
                  <Wifi class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                {:else}
                  <Users class="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                {/if}
                <span class="font-semibold truncate">{peer.label}</span>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <span class="text-[10px] text-slate-400">{peer.group}</span>
                <span class="w-2 h-2 rounded-full {peer.is_online ? 'bg-emerald-400' : 'bg-slate-600'}"></span>
              </div>
            </button>
          {/each}
        </div>
      {/if}

      {#if errorMsg}
        <div class="p-3 rounded-xl bg-rose-500/15 border border-rose-500/40 text-xs text-rose-200">
          {errorMsg}
        </div>
      {/if}

      {#if results.length > 0}
        <div class="space-y-1.5">
          <div class="text-xs font-bold text-emerald-300">Status Pengiriman</div>
          {#each results as r}
            <div class="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs flex items-center justify-between gap-2">
              <span class="flex items-center gap-1.5 font-semibold text-white truncate">
                {#if r.ok}
                  <CheckCircle2 class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                {:else}
                  <AlertTriangle class="w-3.5 h-3.5 text-rose-400 shrink-0" />
                {/if}
                {r.label}
              </span>
              <span class="text-[11px] {r.ok ? 'text-emerald-300' : 'text-rose-300'}">
                {r.ok ? "Transfer dimulai" : r.err}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="px-5 py-3.5 border-t border-slate-800 flex items-center justify-end gap-2">
      <button
        type="button"
        on:click={() => dispatch("close")}
        class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 cursor-pointer"
      >
        Close
      </button>
      <button
        type="button"
        disabled={sending || selectedIds.length === 0}
        on:click={handleShare}
        class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-40 text-slate-950 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
      >
        {#if sending}
          <Loader2 class="w-3.5 h-3.5 animate-spin" />
          <span>Sending...</span>
        {:else}
          <Send class="w-3.5 h-3.5" />
          <span>Send Skill</span>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  button {
    transition: transform 0.12s ease, background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  button:active:not(:disabled) {
    transform: scale(0.94);
  }
</style>
