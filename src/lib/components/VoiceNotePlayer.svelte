<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Play, Pause, Mic, Download, Volume2 } from 'lucide-svelte';

  export let src: string;
  export let isOutgoing: boolean = false;

  let audioElement: HTMLAudioElement;
  let isPlaying = false;
  let currentTime = 0;
  let duration = 0;
  let progressPct = 0;
  const SPEEDS = [1, 1.25, 1.5, 2];
  let speedIndex = 0;

  function cyclePlaybackSpeed() {
    speedIndex = (speedIndex + 1) % SPEEDS.length;
    if (audioElement) {
      audioElement.playbackRate = SPEEDS[speedIndex];
    }
  }

  function togglePlay() {
    if (!audioElement) return;
    if (isPlaying) {
      audioElement.pause();
    } else {
      audioElement.play().catch((err) => console.warn('Audio play failed:', err));
    }
  }

  function handleTimeUpdate() {
    if (!audioElement) return;
    currentTime = audioElement.currentTime;
    if (duration > 0) {
      progressPct = Math.min(100, (currentTime / duration) * 100);
    }
  }

  function handleLoadedMetadata() {
    if (!audioElement) return;
    duration = audioElement.duration;
  }

  function handleEnded() {
    isPlaying = false;
    currentTime = 0;
    progressPct = 0;
  }

  function handleSeek(e: MouseEvent) {
    if (!audioElement || duration <= 0) return;
    const target = e.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const pos = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    audioElement.currentTime = pos * duration;
  }

  function formatDuration(sec: number): string {
    if (!sec || !isFinite(sec) || sec < 0) return '0:00';
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    return `${m}:${s < 10 ? '0' : ''}${s}`;
  }

  function downloadAudio() {
    if (!src) return;
    const a = document.createElement('a');
    a.href = src;
    a.download = `voice-note-${Date.now()}.webm`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  }

  onDestroy(() => {
    if (audioElement) {
      audioElement.pause();
    }
  });
</script>

<div
  class="flex items-center gap-3 p-2.5 sm:p-3 rounded-2xl min-w-[220px] sm:min-w-[280px] {isOutgoing
    ? 'bg-gradient-to-r from-cyan-600/25 to-blue-600/25 border border-cyan-500/30'
    : 'bg-slate-900/90 border border-slate-800'}"
>
  <audio
    bind:this={audioElement}
    {src}
    on:play={() => (isPlaying = true)}
    on:pause={() => (isPlaying = false)}
    on:timeupdate={handleTimeUpdate}
    on:loadedmetadata={handleLoadedMetadata}
    on:ended={handleEnded}
    preload="metadata"
  ></audio>

  <!-- Play/Pause Button -->
  <button
    type="button"
    on:click={togglePlay}
    class="w-10 h-10 rounded-full flex items-center justify-center shrink-0 transition-transform active:scale-95 cursor-pointer shadow-md {isOutgoing
      ? 'bg-cyan-500 hover:bg-cyan-400 text-slate-950 shadow-cyan-500/30'
      : 'bg-slate-800 hover:bg-slate-700 text-cyan-400 border border-slate-700'}"
    title={isPlaying ? 'Pause' : 'Play voice note'}
  >
    {#if isPlaying}
      <Pause class="w-5 h-5 fill-current" />
    {:else}
      <Play class="w-5 h-5 fill-current ml-0.5" />
    {/if}
  </button>

  <!-- Waveform / Progress Slider -->
  <div class="flex-1 min-w-0 space-y-1.5">
    <div class="flex items-center justify-between text-[11px] font-mono">
      <span class="flex items-center gap-1 text-slate-300 font-medium">
        <Mic class="w-3 h-3 text-cyan-400" />
        <span>Voice Note</span>
      </span>
      <span class="text-slate-400 font-mono">
        {formatDuration(isPlaying ? currentTime : duration || currentTime)}
      </span>
    </div>

    <!-- Clickable Track -->
    <button
      type="button"
      class="w-full h-2 rounded-full bg-slate-950/70 border border-slate-800/80 cursor-pointer relative overflow-hidden group block text-left p-0 border-0"
      on:click={handleSeek}
      title="Seek audio track"
    >
      <div
        class="h-full bg-gradient-to-r from-cyan-400 to-indigo-500 rounded-full transition-all duration-75 pointer-events-none"
        style="width: {progressPct}%"
      ></div>
    </button>
  </div>

  <!-- Playback Speed Toggle -->
  <button
    type="button"
    on:click={cyclePlaybackSpeed}
    class="px-1.5 py-0.5 rounded-md bg-slate-800/90 hover:bg-slate-700 text-[10px] font-mono font-bold text-cyan-300 border border-slate-700 transition cursor-pointer shrink-0"
    title="Change Playback Speed (1x, 1.25x, 1.5x, 2x)"
  >
    {SPEEDS[speedIndex]}x
  </button>

  <!-- Download Voice Note -->
  <button
    type="button"
    on:click={downloadAudio}
    class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 transition cursor-pointer"
    title="Save voice note"
  >
    <Download class="w-4 h-4" />
  </button>
</div>
