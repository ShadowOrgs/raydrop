<script lang="ts">
  import {
    incomingInvite,
    joinMeetingRoom,
  } from '../stores/meeting';
  import { activeMainTab } from '../stores/state';
  import { Phone, PhoneOff, Radio, Users } from 'lucide-svelte';

  function handleAccept() {
    if (!$incomingInvite) return;
    const inv = $incomingInvite;
    incomingInvite.set(null);
    activeMainTab.set('meeting');
    joinMeetingRoom(inv.roomId, inv.roomTitle);
  }

  function handleDecline() {
    incomingInvite.set(null);
  }
</script>

{#if $incomingInvite}
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4 animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
  >
    <div
      class="w-full max-w-sm bg-slate-900 border border-indigo-500/40 rounded-3xl p-6 shadow-2xl flex flex-col items-center text-center gap-4 animate-in zoom-in-95 duration-200"
    >
      <!-- Pulsing Caller Avatar -->
      <div class="relative">
        <div class="w-20 h-20 rounded-3xl bg-gradient-to-tr from-indigo-500/20 to-sky-500/20 border-2 border-indigo-500/60 flex items-center justify-center text-2xl font-bold text-indigo-300 shadow-xl overflow-hidden">
          {#if $incomingInvite.fromAvatar}
            <img src={$incomingInvite.fromAvatar} alt={$incomingInvite.fromAlias} class="w-full h-full object-cover" />
          {:else}
            {$incomingInvite.fromAlias.slice(0, 2).toUpperCase()}
          {/if}
        </div>
        <div class="absolute -bottom-1 -right-1 p-1.5 rounded-xl bg-indigo-500 text-slate-950 font-bold shadow-md">
          <Radio class="w-4 h-4 animate-pulse" />
        </div>
      </div>

      <!-- Caller Details -->
      <div class="space-y-1">
        <h3 class="text-base font-bold text-white">{$incomingInvite.fromAlias}</h3>
        <p class="text-xs text-indigo-300 font-medium">Inviting you to Voice & Online Meeting</p>
        <div class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full bg-slate-950 border border-slate-800 text-[11px] font-mono text-slate-400 mt-1">
          <span>Room: {$incomingInvite.roomTitle}</span>
        </div>
      </div>

      <!-- Actions: Accept / Decline -->
      <div class="flex items-center gap-3 w-full pt-2">
        <button
          type="button"
          on:click={handleDecline}
          class="flex-1 py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-rose-400 font-semibold text-xs border border-slate-700 transition flex items-center justify-center gap-1.5 cursor-pointer"
        >
          <PhoneOff class="w-4 h-4" />
          <span>Decline</span>
        </button>

        <button
          type="button"
          on:click={handleAccept}
          class="flex-1 py-2.5 rounded-xl bg-gradient-to-r from-emerald-500 to-teal-600 hover:from-emerald-400 hover:to-teal-500 text-slate-950 font-bold text-xs shadow-lg shadow-emerald-500/20 transition flex items-center justify-center gap-1.5 cursor-pointer"
        >
          <Phone class="w-4 h-4" />
          <span>Join Meeting</span>
        </button>
      </div>
    </div>
  </div>
{/if}
