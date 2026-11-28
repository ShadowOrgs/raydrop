<script lang="ts">
  import {
    incomingInvite,
    joinMeetingRoom,
    toggleCamera,
  } from '../stores/meeting';
  import { stopIncomingCallRingtone } from '../stores/presence';
  import { activeMainTab } from '../stores/state';
  import { Phone, PhoneOff, Video, Users } from 'lucide-svelte';

  async function handleAccept(withVideo = true) {
    if (!$incomingInvite) return;
    const inv = $incomingInvite;
    stopIncomingCallRingtone();
    incomingInvite.set(null);
    activeMainTab.set('meeting');
    await joinMeetingRoom(inv.roomId, inv.roomTitle, inv.fromDeviceId);
    if (withVideo && inv.callType === 'video') {
      await toggleCamera();
    }
  }

  function handleDecline() {
    stopIncomingCallRingtone();
    incomingInvite.set(null);
  }
</script>

{#if $incomingInvite}
  {@const isVideo = $incomingInvite.callType === 'video'}
  <div
    class="fixed inset-0 z-[90] bg-slate-950/85 backdrop-blur-md flex items-center justify-center p-4 animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
  >
    <div
      class="w-full max-w-sm bg-slate-900 border border-emerald-500/40 rounded-3xl p-6 shadow-2xl shadow-emerald-500/10 flex flex-col items-center text-center gap-4 animate-in zoom-in-95 duration-200"
    >
      <!-- Discord-style Pulsing Caller Avatar -->
      <div class="relative flex items-center justify-center my-1">
        <span class="absolute w-24 h-24 rounded-full bg-emerald-500/20 animate-ping"></span>
        <span class="absolute w-22 h-22 rounded-full border border-emerald-400/40 animate-pulse"></span>
        <div class="relative w-20 h-20 rounded-full bg-gradient-to-tr from-indigo-500/30 to-emerald-500/30 border-2 border-emerald-400 flex items-center justify-center text-2xl font-bold text-white shadow-xl overflow-hidden">
          {#if $incomingInvite.fromAvatar}
            <img src={$incomingInvite.fromAvatar} alt={$incomingInvite.fromAlias} class="w-full h-full object-cover" />
          {:else}
            {$incomingInvite.fromAlias.slice(0, 2).toUpperCase()}
          {/if}
        </div>
        <div class="absolute -bottom-1 -right-1 p-1.5 rounded-full bg-emerald-500 text-slate-950 font-bold shadow-lg">
          {#if isVideo}
            <Video class="w-4 h-4 animate-pulse" />
          {:else}
            <Phone class="w-4 h-4 animate-pulse" />
          {/if}
        </div>
      </div>

      <!-- Caller Details -->
      <div class="space-y-1.5">
        <div class="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-emerald-500/15 border border-emerald-500/30 text-[10px] font-bold uppercase tracking-wider text-emerald-300">
          {#if $incomingInvite.isGroupCall}
            <Users class="w-3 h-3" />
            <span>Incoming Group {isVideo ? 'Video' : 'Voice'} Call</span>
          {:else}
            <span>Incoming {isVideo ? 'Video' : 'Voice'} Call</span>
          {/if}
        </div>
        <h3 class="text-lg font-extrabold text-white tracking-tight">{$incomingInvite.fromAlias}</h3>
        <p class="text-xs text-slate-400">
          {$incomingInvite.roomTitle}
        </p>
      </div>

      <!-- Actions: Decline / Accept -->
      <div class="flex items-center justify-center gap-4 w-full pt-2">
        <button
          type="button"
          on:click={handleDecline}
          class="flex-1 py-2.5 rounded-2xl bg-rose-500 hover:bg-rose-400 text-white font-bold text-xs shadow-lg shadow-rose-500/25 transition flex items-center justify-center gap-1.5 cursor-pointer active:scale-95"
        >
          <PhoneOff class="w-4 h-4" />
          <span>Decline</span>
        </button>

        <button
          type="button"
          on:click={() => handleAccept(true)}
          class="flex-1 py-2.5 rounded-2xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold text-xs shadow-lg shadow-emerald-500/25 transition flex items-center justify-center gap-1.5 cursor-pointer active:scale-95"
        >
          {#if isVideo}
            <Video class="w-4 h-4" />
            <span>Join Video</span>
          {:else}
            <Phone class="w-4 h-4" />
            <span>Answer</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
