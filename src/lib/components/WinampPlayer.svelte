<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    Play,
    Pause,
    Square,
    SkipBack,
    SkipForward,
    Volume2,
    VolumeX,
    Shuffle,
    Repeat,
    Repeat1,
    Video,
    Music,
    ListMusic,
    Search,
    Plus,
    Trash2,
    X,
    Minimize2,
    Maximize2,
    FolderPlus,
    Loader2,
    Disc3,
    AlertCircle,
    Download,
    FolderOpen,
    CheckCircle2,
  } from "lucide-svelte";
  import {
    raydropApi,
    type MediaTrack,
    type YtMusicSearchItem,
    type MediaPlayerState,
  } from "../utils/tauri-ipc";
  import {
    winampPlayerOpen,
    mediaPlayerState,
    mediaPlaylists,
    selectedPlaylistId,
    selectedPlaylistItems,
    refreshMediaPlaylists,
    loadPlaylistItems,
    playlistItemToTrack,
  } from "../stores/player";

  let mediaEl: HTMLVideoElement;
  let videoBoxEl: HTMLDivElement;
  let canvasEl: HTMLCanvasElement;
  let audioCtx: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let sourceNode: MediaElementAudioSourceNode | null = null;
  let animFrameId: number | null = null;

  let activeTab: "queue" | "playlists" | "ytmusic" = "queue";
  let minimizedBar = false;
  let resolvingStream = false;
  let resolvingMessage = "Resolving stream...";
  let playerError = "";

  // Dragging state for floating Winamp window
  let posX = 48;
  let posY = 76;
  let dragging = false;
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  // Playlist creation
  let newPlaylistName = "";
  let addTargetPlaylistId = "";

  // YTMusic search & download
  let ytQuery = "";
  let ytFilter: "all" | "songs" | "videos" = "all";
  let ytResults: YtMusicSearchItem[] = [];
  let ytSearching = false;
  let ytDownloadingKey = "";
  let ytDownloadPercent = 0;
  let ytDownloadStatusText = "";
  let ytLastDownloaded: { title: string; output_path: string; format: string } | null = null;

  let unlistenRemote: (() => void) | null = null;
  let unlistenYtdlp: (() => void) | null = null;
  let unlistenYtDlProg: (() => void) | null = null;
  let unlistenFfmpeg: (() => void) | null = null;
  let lastSyncTs = 0;

  onMount(async () => {
    await refreshMediaPlaylists();
    const list = get(mediaPlaylists);
    if (list.length > 0) {
      addTargetPlaylistId = list[0].id;
      await loadPlaylistItems(list[0].id);
    }

    unlistenRemote = await raydropApi.listenEvent<{ action: string; payload: any }>(
      "media_player_remote_action",
      async (evt) => {
        await handleRemoteAction(evt.action, evt.payload);
      },
    );

    unlistenYtdlp = await raydropApi.listenEvent<{ status: string; message: string }>(
      "media_ytdlp_status",
      (evt) => {
        if (evt.status === "downloading" && evt.message) {
          resolvingMessage = evt.message;
          if (ytDownloadingKey) {
            ytDownloadStatusText = evt.message;
          }
        } else {
          resolvingMessage = "Resolving stream...";
        }
      },
    );

    unlistenYtDlProg = await raydropApi.listenEvent<{
      video_id: string;
      format: string;
      percent: number;
      status_text: string;
    }>("media_ytdlp_download_progress", (p) => {
      ytDownloadPercent = Math.round(p.percent ?? 0);
      ytDownloadStatusText = p.status_text || "";
    });

    unlistenFfmpeg = await raydropApi.listenEvent<{
      status: string;
      percent?: number;
      message: string;
    }>("media_ffmpeg_status", (s) => {
      if (ytDownloadingKey && (s.status === "downloading" || s.status === "extracting")) {
        ytDownloadStatusText = s.message || "Preparing FFmpeg...";
        if (typeof s.percent === "number") {
          ytDownloadPercent = Math.round(s.percent);
        }
      }
    });

    startVisualizerLoop();
  });

  onDestroy(() => {
    if (unlistenRemote) unlistenRemote();
    if (unlistenYtdlp) unlistenYtdlp();
    if (unlistenYtDlProg) unlistenYtDlProg();
    if (unlistenFfmpeg) unlistenFfmpeg();
    if (animFrameId !== null) cancelAnimationFrame(animFrameId);
    if (audioCtx) {
      audioCtx.close().catch(() => {});
    }
  });

  function formatTime(secs: number): string {
    if (!isFinite(secs) || secs < 0) return "00:00";
    const s = Math.floor(secs);
    const hrs = Math.floor(s / 3600);
    const mins = Math.floor((s % 3600) / 60);
    const rem = s % 60;
    const mm = String(mins).padStart(2, "0");
    const ss = String(rem).padStart(2, "0");
    return hrs > 0 ? `${hrs}:${mm}:${ss}` : `${mm}:${ss}`;
  }

  function syncBackend(force = false) {
    const now = Date.now();
    if (!force && now - lastSyncTs < 750) return;
    lastSyncTs = now;
    const st = get(mediaPlayerState);
    raydropApi.mediaSyncFrontendState(st).catch(() => {});
  }

  function ensureAudioAnalyser(isLocalSource: boolean) {
    if (!mediaEl || !isLocalSource) return;
    try {
      if (!audioCtx) {
        audioCtx = new AudioContext();
      }
      if (audioCtx.state === "suspended") {
        audioCtx.resume().catch(() => {});
      }
      if (!sourceNode) {
        sourceNode = audioCtx.createMediaElementSource(mediaEl);
        analyser = audioCtx.createAnalyser();
        analyser.fftSize = 64;
        sourceNode.connect(analyser);
        analyser.connect(audioCtx.destination);
      }
    } catch (_) {
      // Fallback to animated bars if Web Audio API cannot attach
    }
  }

  function startVisualizerLoop() {
    const render = () => {
      if (canvasEl) {
        const ctx = canvasEl.getContext("2d");
        if (ctx) {
          const w = canvasEl.width;
          const h = canvasEl.height;
          ctx.clearRect(0, 0, w, h);

          const st = get(mediaPlayerState);
          const barCount = 24;
          const gap = 2;
          const barW = Math.max(2, Math.floor((w - gap * (barCount - 1)) / barCount));

          let freqData: Uint8Array | null = null;
          if (analyser && st.is_playing && st.current_track?.source_type === "local") {
            freqData = new Uint8Array(analyser.frequencyBinCount);
            analyser.getByteFrequencyData(freqData);
          }

          const t = Date.now() / 180;
          for (let i = 0; i < barCount; i++) {
            let norm = 0.06;
            if (st.is_playing) {
              if (freqData && i < freqData.length) {
                norm = Math.max(0.08, freqData[i] / 255);
              } else {
                // Smooth fallback visualizer for remote streams
                norm =
                  0.18 +
                  0.62 *
                    Math.abs(
                      Math.sin(t + i * 0.45) * Math.cos(t * 0.7 - i * 0.25),
                    );
              }
            }
            const barH = Math.max(2, Math.round(norm * h));
            const x = i * (barW + gap);
            const y = h - barH;

            const grad = ctx.createLinearGradient(0, h, 0, 0);
            grad.addColorStop(0, "#06b6d4");
            grad.addColorStop(0.6, "#22d3ee");
            grad.addColorStop(1, "#f59e0b");
            ctx.fillStyle = grad;
            ctx.fillRect(x, y, barW, barH);
          }
        }
      }
      animFrameId = requestAnimationFrame(render);
    };
    animFrameId = requestAnimationFrame(render);
  }

  async function playTrack(track: MediaTrack, indexInQueue?: number) {
    playerError = "";
    resolvingStream = false;
    let srcUrl = "";
    const wantVideo = track.is_video || get(mediaPlayerState).video_mode;

    if (track.source_type === "local") {
      srcUrl = convertFileSrc(track.uri_or_id);
      ensureAudioAnalyser(true);
    } else {
      resolvingStream = true;
      try {
        const resolved = await raydropApi.mediaResolveStream(track.uri_or_id, wantVideo);
        srcUrl = resolved.stream_url;
        track = { ...track, stream_url: srcUrl };
      } catch (err: any) {
        playerError = err?.message || String(err);
        resolvingStream = false;
        return;
      } finally {
        resolvingStream = false;
      }
    }

    mediaPlayerState.update((s) => {
      let queue = s.queue;
      let idx = indexInQueue ?? queue.findIndex((q) => q.id === track.id);
      if (idx < 0) {
        queue = [...queue, track];
        idx = queue.length - 1;
      } else {
        queue = queue.map((q, i) => (i === idx ? track : q));
      }
      return {
        ...s,
        current_track: track,
        queue,
        current_index: idx,
        is_playing: true,
        position_secs: 0,
        video_mode: track.is_video ? true : s.video_mode,
      };
    });

    if (mediaEl) {
      mediaEl.src = srcUrl;
      mediaEl.volume = get(mediaPlayerState).volume;
      mediaEl.muted = get(mediaPlayerState).muted;
      try {
        await mediaEl.play();
      } catch (err: any) {
        playerError = `Playback error: ${err?.message || String(err)}`;
      }
    }
    syncBackend(true);
  }

  function togglePlayPause() {
    const st = get(mediaPlayerState);
    if (!mediaEl) return;
    if (!st.current_track) {
      if (st.queue.length > 0) {
        playTrack(st.queue[0], 0);
      }
      return;
    }
    if (mediaEl.paused) {
      mediaEl.play().catch(() => {});
      mediaPlayerState.update((s) => ({ ...s, is_playing: true }));
    } else {
      mediaEl.pause();
      mediaPlayerState.update((s) => ({ ...s, is_playing: false }));
    }
    syncBackend(true);
  }

  function stopPlayback() {
    if (mediaEl) {
      mediaEl.pause();
      mediaEl.currentTime = 0;
    }
    mediaPlayerState.update((s) => ({ ...s, is_playing: false, position_secs: 0 }));
    syncBackend(true);
  }

  function playNext(fromEnded = false) {
    const st = get(mediaPlayerState);
    if (st.queue.length === 0) return;

    if (fromEnded && st.repeat_mode === "one" && st.current_track) {
      if (mediaEl) {
        mediaEl.currentTime = 0;
        mediaEl.play().catch(() => {});
      }
      return;
    }

    let nextIdx = 0;
    if (st.shuffle && st.queue.length > 1) {
      do {
        nextIdx = Math.floor(Math.random() * st.queue.length);
      } while (nextIdx === st.current_index);
    } else {
      const cur = st.current_index ?? -1;
      if (cur + 1 < st.queue.length) {
        nextIdx = cur + 1;
      } else if (st.repeat_mode === "all" || !fromEnded) {
        nextIdx = 0;
      } else {
        stopPlayback();
        return;
      }
    }

    playTrack(st.queue[nextIdx], nextIdx);
  }

  function playPrev() {
    const st = get(mediaPlayerState);
    if (st.queue.length === 0) return;
    if (mediaEl && mediaEl.currentTime > 3) {
      mediaEl.currentTime = 0;
      return;
    }
    const cur = st.current_index ?? 0;
    const prevIdx = cur - 1 >= 0 ? cur - 1 : st.queue.length - 1;
    playTrack(st.queue[prevIdx], prevIdx);
  }

  function handleSeek(e: Event) {
    const val = Number((e.target as HTMLInputElement).value);
    if (mediaEl && isFinite(val)) {
      mediaEl.currentTime = val;
      mediaPlayerState.update((s) => ({ ...s, position_secs: val }));
    }
  }

  function handleVolumeChange(e: Event) {
    const vol = Number((e.target as HTMLInputElement).value);
    if (mediaEl) {
      mediaEl.volume = vol;
      mediaEl.muted = false;
    }
    mediaPlayerState.update((s) => ({ ...s, volume: vol, muted: false }));
    syncBackend(true);
  }

  function toggleMute() {
    const next = !get(mediaPlayerState).muted;
    if (mediaEl) mediaEl.muted = next;
    mediaPlayerState.update((s) => ({ ...s, muted: next }));
    syncBackend(true);
  }

  function cycleRepeat() {
    mediaPlayerState.update((s) => {
      const next =
        s.repeat_mode === "off" ? "all" : s.repeat_mode === "all" ? "one" : "off";
      return { ...s, repeat_mode: next };
    });
    syncBackend(true);
  }

  async function addLocalFiles(targetPlaylistId?: string) {
    const picked = await open({
      multiple: true,
      title: "Select Audio or Video Files",
      filters: [
        {
          name: "Audio & Video",
          extensions: [
            "mp3",
            "flac",
            "wav",
            "ogg",
            "m4a",
            "aac",
            "opus",
            "mp4",
            "mkv",
            "webm",
            "mov",
            "avi",
          ],
        },
      ],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    const videoExts = new Set(["mp4", "mkv", "webm", "mov", "avi"]);

    const newTracks: MediaTrack[] = [];
    for (const p of paths) {
      const parts = p.split(/[/\\]/);
      const filename = parts[parts.length - 1] || p;
      const dotIdx = filename.lastIndexOf(".");
      const ext = dotIdx >= 0 ? filename.slice(dotIdx + 1).toLowerCase() : "";
      const title = dotIdx > 0 ? filename.slice(0, dotIdx) : filename;
      const isVideo = videoExts.has(ext);

      if (targetPlaylistId) {
        const item = await raydropApi.mediaAddPlaylistItem({
          playlistId: targetPlaylistId,
          sourceType: "local",
          title,
          artist: "Local Media",
          durationSecs: 0,
          uriOrId: p,
          thumbnailUrl: null,
          isVideo,
        });
        newTracks.push(playlistItemToTrack(item));
      } else {
        newTracks.push({
          id: crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`,
          source_type: "local",
          title,
          artist: "Local Media",
          duration_secs: 0,
          uri_or_id: p,
          thumbnail_url: null,
          is_video: isVideo,
          stream_url: null,
        });
      }
    }

    if (targetPlaylistId) {
      await refreshMediaPlaylists();
      await loadPlaylistItems(targetPlaylistId);
    } else if (newTracks.length > 0) {
      const hadTrack = !!get(mediaPlayerState).current_track;
      mediaPlayerState.update((s) => ({ ...s, queue: [...s.queue, ...newTracks] }));
      if (!hadTrack) {
        await playTrack(newTracks[0]);
      } else {
        syncBackend(true);
      }
    }
  }

  async function handleYtSearch() {
    if (!ytQuery.trim() || ytSearching) return;
    activeTab = "ytmusic";
    ytSearching = true;
    playerError = "";
    try {
      ytResults = await raydropApi.mediaSearchYtmusic(ytQuery, ytFilter);
    } catch (err: any) {
      playerError = err?.message || String(err);
    } finally {
      ytSearching = false;
    }
  }

  async function handleYtDownload(item: YtMusicSearchItem, format: "mp3" | "mp4") {
    if (ytDownloadingKey) return;
    ytDownloadingKey = `${item.video_id}-${format}`;
    ytDownloadPercent = 0;
    ytDownloadStatusText = `Starting ${format.toUpperCase()} download...`;
    playerError = "";
    ytLastDownloaded = null;
    try {
      const res = await raydropApi.mediaDownloadYtmusic({
        videoId: item.video_id,
        title: item.title,
        format,
      });
      ytLastDownloaded = {
        title: res.title,
        output_path: res.output_path,
        format: res.format,
      };
    } catch (err: any) {
      playerError = err?.message || String(err);
    } finally {
      ytDownloadingKey = "";
      ytDownloadPercent = 0;
      ytDownloadStatusText = "";
    }
  }

  function ytItemToTrack(item: YtMusicSearchItem): MediaTrack {
    return {
      id: `yt-${item.video_id}-${Date.now()}`,
      source_type: "ytmusic",
      title: item.title,
      artist: item.artist,
      duration_secs: item.duration_secs,
      uri_or_id: item.video_id,
      thumbnail_url: item.thumbnail_url ?? null,
      is_video: item.is_video,
      stream_url: null,
    };
  }

  async function handleAddYtToPlaylist(item: YtMusicSearchItem) {
    let pid = addTargetPlaylistId || get(selectedPlaylistId);
    if (!pid) {
      const created = await raydropApi.mediaCreatePlaylist("My Favorites");
      await refreshMediaPlaylists();
      pid = created.id;
      addTargetPlaylistId = pid;
    }
    await raydropApi.mediaAddPlaylistItem({
      playlistId: pid,
      sourceType: "ytmusic",
      title: item.title,
      artist: item.artist,
      durationSecs: item.duration_secs,
      uriOrId: item.video_id,
      thumbnailUrl: item.thumbnail_url ?? null,
      isVideo: item.is_video,
    });
    await refreshMediaPlaylists();
    if (get(selectedPlaylistId) === pid) {
      await loadPlaylistItems(pid);
    }
  }

  async function handleCreatePlaylist() {
    if (!newPlaylistName.trim()) return;
    const pl = await raydropApi.mediaCreatePlaylist(newPlaylistName.trim());
    newPlaylistName = "";
    await refreshMediaPlaylists();
    addTargetPlaylistId = pl.id;
    await loadPlaylistItems(pl.id);
  }

  async function handlePlayPlaylist(playlistId: string) {
    const items = await raydropApi.mediaListPlaylistItems(playlistId);
    if (items.length === 0) return;
    const tracks = items.map(playlistItemToTrack);
    mediaPlayerState.update((s) => ({
      ...s,
      queue: tracks,
      active_playlist_id: playlistId,
    }));
    await playTrack(tracks[0], 0);
  }

  async function handleRemoteAction(action: string, payload: any) {
    switch (action) {
      case "toggle":
        togglePlayPause();
        break;
      case "pause":
        if (mediaEl && !mediaEl.paused) togglePlayPause();
        break;
      case "resume":
        if (mediaEl && mediaEl.paused) togglePlayPause();
        break;
      case "stop":
        stopPlayback();
        break;
      case "next":
        playNext(false);
        break;
      case "prev":
        playPrev();
        break;
      case "set_volume": {
        const v = Math.max(0, Math.min(1, Number(payload?.volume ?? 0.85)));
        if (mediaEl) {
          mediaEl.volume = v;
          mediaEl.muted = false;
        }
        mediaPlayerState.update((s) => ({ ...s, volume: v, muted: false }));
        syncBackend(true);
        break;
      }
      case "set_video_mode": {
        const vm = Boolean(payload?.video_mode);
        mediaPlayerState.update((s) => ({ ...s, video_mode: vm }));
        syncBackend(true);
        break;
      }
      case "play_track": {
        if (payload?.track) {
          winampPlayerOpen.set(true);
          await playTrack(payload.track as MediaTrack);
        }
        break;
      }
      case "play_playlist": {
        if (payload?.playlist_id) {
          winampPlayerOpen.set(true);
          await handlePlayPlaylist(payload.playlist_id);
        }
        break;
      }
    }
  }

  async function toggleVideoMode() {
    const st = get(mediaPlayerState);
    const nextVideoMode = !st.video_mode;
    mediaPlayerState.update((s) => ({ ...s, video_mode: nextVideoMode }));
    syncBackend(true);

    if (
      nextVideoMode &&
      st.current_track &&
      st.current_track.source_type === "ytmusic" &&
      mediaEl &&
      mediaEl.videoWidth === 0
    ) {
      const savedTime = mediaEl.currentTime || 0;
      const wasPlaying = !mediaEl.paused;
      resolvingStream = true;
      resolvingMessage = "Loading video stream...";
      playerError = "";
      try {
        const resolved = await raydropApi.mediaResolveStream(
          st.current_track.uri_or_id,
          true,
        );
        mediaEl.src = resolved.stream_url;
        mediaEl.currentTime = savedTime;
        if (wasPlaying) {
          await mediaEl.play().catch(() => {});
        }
      } catch (err: any) {
        playerError = err?.message || String(err);
      } finally {
        resolvingStream = false;
        resolvingMessage = "Resolving stream...";
      }
    }
  }

  function toggleVideoFullscreen() {
    if (!mediaEl) return;
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      mediaEl.requestFullscreen?.().catch(() => {
        videoBoxEl?.requestFullscreen?.().catch(() => {});
      });
    }
  }

  function startDrag(e: MouseEvent) {
    dragging = true;
    dragOffsetX = e.clientX - posX;
    dragOffsetY = e.clientY - posY;
    window.addEventListener("mousemove", onDrag);
    window.addEventListener("mouseup", stopDrag);
  }

  function onDrag(e: MouseEvent) {
    if (!dragging) return;
    posX = Math.max(8, Math.min(window.innerWidth - 340, e.clientX - dragOffsetX));
    posY = Math.max(8, Math.min(window.innerHeight - 120, e.clientY - dragOffsetY));
  }

  function stopDrag() {
    dragging = false;
    window.removeEventListener("mousemove", onDrag);
    window.removeEventListener("mouseup", stopDrag);
  }
</script>

<div
  style="left: {posX}px; top: {posY}px;"
  class:hidden={!$winampPlayerOpen}
  class="fixed z-[75] w-[420px] max-w-[calc(100vw-16px)] rounded-2xl bg-slate-950/95 backdrop-blur-xl border border-cyan-500/35 shadow-2xl shadow-black/70 overflow-hidden select-none text-slate-100"
>
    <!-- Winamp Title Bar (Draggable) -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div
      on:mousedown={startDrag}
      class="px-3 py-2 bg-gradient-to-r from-slate-900 via-slate-800 to-slate-900 border-b border-slate-800 flex items-center justify-between cursor-move"
    >
      <div class="flex items-center gap-2">
        <Disc3 class="w-4 h-4 text-amber-400 {$mediaPlayerState.is_playing ? 'animate-spin' : ''}" />
        <span class="text-[11px] font-bold tracking-wider uppercase text-cyan-300">
          RAYAMP <span class="text-slate-500 font-normal">Audio • Video</span>
        </span>
      </div>
      <div class="flex items-center gap-1" on:mousedown|stopPropagation>
        <button
          type="button"
          on:click={toggleVideoMode}
          class="px-2 py-1 rounded-md text-[10px] font-semibold flex items-center gap-1 transition cursor-pointer {$mediaPlayerState.video_mode
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
            : 'text-slate-300 bg-slate-800/80 hover:text-white hover:bg-slate-700'}"
          title={$mediaPlayerState.video_mode ? "Hide Video" : "Show Video"}
        >
          <Video class="w-3 h-3" />
          <span>{$mediaPlayerState.video_mode ? "Hide Video" : "Show Video"}</span>
        </button>
        <button
          type="button"
          on:click={() => (minimizedBar = !minimizedBar)}
          class="p-1 rounded-md text-slate-400 hover:text-white hover:bg-slate-800 cursor-pointer"
          title={minimizedBar ? "Expand Player" : "Minimize to Deck"}
        >
          {#if minimizedBar}
            <Maximize2 class="w-3.5 h-3.5" />
          {:else}
            <Minimize2 class="w-3.5 h-3.5" />
          {/if}
        </button>
        <button
          type="button"
          on:click={() => winampPlayerOpen.set(false)}
          class="p-1 rounded-md text-slate-400 hover:text-rose-300 hover:bg-slate-800 cursor-pointer"
          title="Hide Window (keeps playing in background)"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>

    <!-- Video Display Frame when Video Mode is ON -->
    <div
      bind:this={videoBoxEl}
      class="{$mediaPlayerState.video_mode && !minimizedBar ? 'block' : 'hidden'} relative bg-black border-b border-slate-800 group"
    >
      <video
        bind:this={mediaEl}
        class="w-full aspect-video bg-black cursor-pointer"
        playsinline
        controls={$mediaPlayerState.video_mode}
        on:dblclick={toggleVideoFullscreen}
        on:timeupdate={() => {
          if (!mediaEl) return;
          mediaPlayerState.update((s) => ({
            ...s,
            position_secs: mediaEl.currentTime || 0,
            duration_secs: isFinite(mediaEl.duration) ? mediaEl.duration : s.duration_secs,
          }));
          syncBackend(false);
        }}
        on:loadedmetadata={() => {
          if (!mediaEl) return;
          mediaPlayerState.update((s) => ({
            ...s,
            duration_secs: isFinite(mediaEl.duration) ? mediaEl.duration : s.duration_secs,
          }));
          syncBackend(true);
        }}
        on:ended={() => playNext(true)}
        on:play={() => {
          mediaPlayerState.update((s) => ({ ...s, is_playing: true }));
          syncBackend(true);
        }}
        on:pause={() => {
          mediaPlayerState.update((s) => ({ ...s, is_playing: false }));
          syncBackend(true);
        }}
      ></video>
      <div class="absolute top-2 right-2 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          type="button"
          on:click={toggleVideoFullscreen}
          class="px-2 py-1 rounded-lg bg-black/75 hover:bg-black text-[10px] font-semibold text-white border border-white/20 flex items-center gap-1 cursor-pointer"
          title="Fullscreen Video"
        >
          <Maximize2 class="w-3 h-3" />
          <span>Fullscreen</span>
        </button>
        <button
          type="button"
          on:click={toggleVideoMode}
          class="px-2 py-1 rounded-lg bg-black/75 hover:bg-black text-[10px] font-semibold text-slate-200 border border-white/20 cursor-pointer"
          title="Hide Video"
        >
          Hide
        </button>
      </div>
    </div>

    <!-- Winamp LCD + Spectrum Visualizer -->
    <div class="p-3 space-y-2.5 bg-slate-950">
      <div class="rounded-xl bg-black/90 border border-slate-800 p-2.5 flex items-center gap-3">
        <div class="w-24 shrink-0 flex flex-col justify-between">
          <div class="font-mono text-lg font-bold text-emerald-400 leading-none tracking-tight">
            {formatTime($mediaPlayerState.position_secs)}
          </div>
          <div class="text-[10px] font-mono text-slate-500 mt-1">
            / {formatTime($mediaPlayerState.duration_secs || $mediaPlayerState.current_track?.duration_secs || 0)}
          </div>
        </div>
        <div class="flex-1 min-w-0 space-y-1">
          <div class="flex items-center justify-between gap-2">
            <p class="text-xs font-semibold text-cyan-200 truncate">
              {#if resolvingStream}
                {resolvingMessage}
              {:else if $mediaPlayerState.current_track}
                {$mediaPlayerState.current_track.title}
              {:else}
                RayAmp Ready — Add Audio/Video
              {/if}
            </p>
            {#if $mediaPlayerState.current_track}
              <span class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-amber-300 uppercase shrink-0">
                {$mediaPlayerState.current_track.source_type}
              </span>
            {/if}
          </div>
          <p class="text-[10px] text-slate-400 truncate">
            {$mediaPlayerState.current_track?.artist || "Drop files, pick playlist, or search YTMusic"}
          </p>
          <canvas bind:this={canvasEl} width="230" height="24" class="w-full h-6 rounded"></canvas>
        </div>
      </div>

      <!-- Seek Bar -->
      <input
        type="range"
        min="0"
        max={Math.max(1, $mediaPlayerState.duration_secs || $mediaPlayerState.current_track?.duration_secs || 1)}
        step="0.5"
        value={$mediaPlayerState.position_secs}
        on:input={handleSeek}
        class="w-full h-1.5 accent-cyan-400 cursor-pointer"
      />

      <!-- Transport Controls -->
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-1">
          <button
            type="button"
            on:click={playPrev}
            class="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 cursor-pointer"
            title="Previous"
          >
            <SkipBack class="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            on:click={togglePlayPause}
            class="p-2 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold shadow-md shadow-cyan-500/20 cursor-pointer"
            title={$mediaPlayerState.is_playing ? "Pause" : "Play"}
          >
            {#if resolvingStream}
              <Loader2 class="w-4 h-4 animate-spin" />
            {:else if $mediaPlayerState.is_playing}
              <Pause class="w-4 h-4" />
            {:else}
              <Play class="w-4 h-4" />
            {/if}
          </button>
          <button
            type="button"
            on:click={stopPlayback}
            class="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 cursor-pointer"
            title="Stop"
          >
            <Square class="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            on:click={() => playNext(false)}
            class="p-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-300 cursor-pointer"
            title="Next"
          >
            <SkipForward class="w-3.5 h-3.5" />
          </button>
        </div>

        <!-- Volume & Modes -->
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            on:click={() => mediaPlayerState.update((s) => ({ ...s, shuffle: !s.shuffle }))}
            class="p-1.5 rounded-lg border text-xs cursor-pointer {$mediaPlayerState.shuffle
              ? 'bg-cyan-500/20 border-cyan-500/40 text-cyan-300'
              : 'bg-slate-900 border-slate-800 text-slate-400 hover:text-white'}"
            title="Shuffle"
          >
            <Shuffle class="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            on:click={cycleRepeat}
            class="p-1.5 rounded-lg border text-xs cursor-pointer {$mediaPlayerState.repeat_mode !== 'off'
              ? 'bg-amber-500/20 border-amber-500/40 text-amber-300'
              : 'bg-slate-900 border-slate-800 text-slate-400 hover:text-white'}"
            title="Repeat: {$mediaPlayerState.repeat_mode}"
          >
            {#if $mediaPlayerState.repeat_mode === "one"}
              <Repeat1 class="w-3.5 h-3.5" />
            {:else}
              <Repeat class="w-3.5 h-3.5" />
            {/if}
          </button>
          <button
            type="button"
            on:click={toggleMute}
            class="p-1.5 rounded-lg bg-slate-900 border border-slate-800 text-slate-300 cursor-pointer"
            title="Mute"
          >
            {#if $mediaPlayerState.muted || $mediaPlayerState.volume === 0}
              <VolumeX class="w-3.5 h-3.5 text-rose-400" />
            {:else}
              <Volume2 class="w-3.5 h-3.5 text-cyan-400" />
            {/if}
          </button>
          <input
            type="range"
            min="0"
            max="1"
            step="0.02"
            value={$mediaPlayerState.volume}
            on:input={handleVolumeChange}
            class="w-16 h-1 accent-amber-400 cursor-pointer"
          />
        </div>
      </div>

      {#if playerError}
        <div class="flex items-start gap-2 text-[11px] text-rose-300 bg-rose-500/10 border border-rose-500/30 rounded-xl px-2.5 py-2">
          <AlertCircle class="w-3.5 h-3.5 shrink-0 mt-0.5" />
          <span class="break-words">{playerError}</span>
        </div>
      {/if}
    </div>

    <!-- Drawer Tabs: Queue | Playlists | YouTube Music -->
    {#if !minimizedBar}
      <div class="border-t border-slate-800 bg-slate-900/60">
        <!-- Quick YTMusic Search + Add Local always accessible at the top -->
        <div class="px-3 pt-2.5 pb-2 border-b border-slate-800/80 flex items-center gap-1.5">
          <div class="flex-1 flex items-center gap-1 bg-slate-950 border border-slate-700/90 rounded-lg px-2 py-1 focus-within:border-amber-500">
            <Search class="w-3.5 h-3.5 text-amber-400 shrink-0" />
            <input
              type="text"
              bind:value={ytQuery}
              placeholder="Search YTMusic / YouTube..."
              on:keydown={(e) => e.key === "Enter" && handleYtSearch()}
              class="w-full bg-transparent text-xs text-white placeholder-slate-500 focus:outline-none"
            />
          </div>
          <select
            bind:value={ytFilter}
            class="px-1.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-[10px] text-slate-200"
          >
            <option value="all">All</option>
            <option value="songs">Songs</option>
            <option value="videos">Videos</option>
          </select>
          <button
            type="button"
            disabled={ytSearching}
            on:click={handleYtSearch}
            class="px-2 py-1.5 rounded-lg bg-amber-500 hover:bg-amber-400 text-slate-950 text-[11px] font-bold cursor-pointer disabled:opacity-50 shrink-0"
          >
            {ytSearching ? "..." : "Go"}
          </button>
          <button
            type="button"
            on:click={() => addLocalFiles()}
            class="px-2 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700 text-[11px] font-semibold text-cyan-300 flex items-center gap-1 cursor-pointer shrink-0"
            title="Add Local Audio / Video"
          >
            <FolderPlus class="w-3.5 h-3.5" />
            <span>Local</span>
          </button>
        </div>

        <div class="grid grid-cols-3 border-b border-slate-800 text-[11px] font-semibold">
          <button
            type="button"
            on:click={() => (activeTab = "queue")}
            class="py-2 flex items-center justify-center gap-1.5 cursor-pointer {activeTab === 'queue'
              ? 'text-cyan-300 border-b-2 border-cyan-400 bg-slate-900'
              : 'text-slate-400 hover:text-white'}"
          >
            <Music class="w-3.5 h-3.5" />
            <span>Queue ({$mediaPlayerState.queue.length})</span>
          </button>
          <button
            type="button"
            on:click={() => (activeTab = "playlists")}
            class="py-2 flex items-center justify-center gap-1.5 cursor-pointer {activeTab === 'playlists'
              ? 'text-cyan-300 border-b-2 border-cyan-400 bg-slate-900'
              : 'text-slate-400 hover:text-white'}"
          >
            <ListMusic class="w-3.5 h-3.5" />
            <span>Playlists ({$mediaPlaylists.length})</span>
          </button>
          <button
            type="button"
            on:click={() => (activeTab = "ytmusic")}
            class="py-2 flex items-center justify-center gap-1.5 cursor-pointer {activeTab === 'ytmusic'
              ? 'text-amber-300 border-b-2 border-amber-400 bg-slate-900'
              : 'text-slate-400 hover:text-white'}"
          >
            <Search class="w-3.5 h-3.5" />
            <span>YTMusic</span>
          </button>
        </div>

        <div class="p-3 max-h-60 overflow-y-auto space-y-2">
          {#if activeTab === "queue"}
            <div class="flex items-center justify-between gap-2">
              <button
                type="button"
                on:click={() => addLocalFiles()}
                class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700 text-[11px] font-semibold text-cyan-300 flex items-center gap-1.5 cursor-pointer"
              >
                <FolderPlus class="w-3.5 h-3.5" />
                <span>Add Local Audio / Video</span>
              </button>
              {#if $mediaPlayerState.queue.length > 0}
                <button
                  type="button"
                  on:click={() => {
                    stopPlayback();
                    mediaPlayerState.update((s) => ({
                      ...s,
                      queue: [],
                      current_track: null,
                      current_index: null,
                    }));
                    syncBackend(true);
                  }}
                  class="text-[11px] text-slate-400 hover:text-rose-300 cursor-pointer"
                >
                  Clear Queue
                </button>
              {/if}
            </div>

            {#if $mediaPlayerState.queue.length === 0}
              <p class="text-[11px] text-slate-500 italic py-4 text-center">
                Queue kosong. Tambahkan file lokal atau cari lagu/video di atas.
              </p>
            {:else}
              <div class="space-y-1">
                {#each $mediaPlayerState.queue as track, idx (track.id)}
                  <div
                    class="flex items-center justify-between gap-2 px-2 py-1.5 rounded-lg border text-xs {$mediaPlayerState.current_index === idx
                      ? 'bg-cyan-500/15 border-cyan-500/40 text-white'
                      : 'bg-slate-900/70 border-slate-800/80 text-slate-300 hover:bg-slate-800/70'}"
                  >
                    <button
                      type="button"
                      on:click={() => playTrack(track, idx)}
                      class="flex-1 min-w-0 text-left flex items-center gap-2 cursor-pointer"
                    >
                      <span class="text-[10px] font-mono text-slate-500 w-4 shrink-0">{idx + 1}.</span>
                      {#if track.thumbnail_url}
                        <img
                          src={track.thumbnail_url}
                          alt={track.title}
                          class="w-8 h-8 rounded object-cover shrink-0 border border-slate-700/60"
                        />
                      {:else}
                        <div class="w-8 h-8 rounded bg-slate-800/90 border border-slate-700/60 flex items-center justify-center shrink-0">
                          {#if track.is_video}
                            <Video class="w-3.5 h-3.5 text-amber-400" />
                          {:else}
                            <Music class="w-3.5 h-3.5 text-cyan-400" />
                          {/if}
                        </div>
                      {/if}
                      <div class="min-w-0">
                        <p class="truncate font-medium text-xs">{track.title}</p>
                        <p class="truncate text-[10px] text-slate-400">{track.artist}</p>
                      </div>
                    </button>
                    <button
                      type="button"
                      on:click={() => {
                        mediaPlayerState.update((s) => ({
                          ...s,
                          queue: s.queue.filter((_, i) => i !== idx),
                        }));
                        syncBackend(true);
                      }}
                      class="p-1 text-slate-500 hover:text-rose-400 cursor-pointer"
                    >
                      <Trash2 class="w-3 h-3" />
                    </button>
                  </div>
                {/each}
              </div>
            {/if}

          {:else if activeTab === "playlists"}
            <div class="flex items-center gap-1.5">
              <input
                type="text"
                bind:value={newPlaylistName}
                placeholder="New playlist name..."
                on:keydown={(e) => e.key === "Enter" && handleCreatePlaylist()}
                class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white focus:outline-none focus:border-cyan-500"
              />
              <button
                type="button"
                on:click={handleCreatePlaylist}
                class="px-2.5 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center gap-1 cursor-pointer"
              >
                <Plus class="w-3.5 h-3.5" />
                <span>Create</span>
              </button>
            </div>

            {#if $mediaPlaylists.length > 0}
              <div class="flex items-center gap-1.5 overflow-x-auto pb-1">
                {#each $mediaPlaylists as pl (pl.id)}
                  <button
                    type="button"
                    on:click={() => loadPlaylistItems(pl.id)}
                    class="px-2.5 py-1 rounded-lg text-[11px] font-semibold shrink-0 border cursor-pointer {$selectedPlaylistId === pl.id
                      ? 'bg-cyan-500/20 border-cyan-500/50 text-cyan-200'
                      : 'bg-slate-900 border-slate-800 text-slate-400 hover:text-white'}"
                  >
                    {pl.name} ({pl.item_count})
                  </button>
                {/each}
              </div>

              {#if $selectedPlaylistId}
                <div class="flex items-center justify-between gap-2 pt-1">
                  <div class="flex items-center gap-1.5">
                    <button
                      type="button"
                      on:click={() => $selectedPlaylistId && handlePlayPlaylist($selectedPlaylistId)}
                      class="px-2.5 py-1 rounded-lg bg-emerald-500/20 hover:bg-emerald-500/30 border border-emerald-500/40 text-emerald-300 text-[11px] font-semibold flex items-center gap-1 cursor-pointer"
                    >
                      <Play class="w-3 h-3" />
                      <span>Play All</span>
                    </button>
                    <button
                      type="button"
                      on:click={() => $selectedPlaylistId && addLocalFiles($selectedPlaylistId)}
                      class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-[11px] flex items-center gap-1 cursor-pointer"
                    >
                      <FolderPlus class="w-3 h-3" />
                      <span>+ Local</span>
                    </button>
                  </div>
                  <button
                    type="button"
                    on:click={async () => {
                      if (!$selectedPlaylistId) return;
                      await raydropApi.mediaDeletePlaylist($selectedPlaylistId);
                      selectedPlaylistId.set(null);
                      selectedPlaylistItems.set([]);
                      await refreshMediaPlaylists();
                    }}
                    class="text-[11px] text-rose-400 hover:text-rose-300 cursor-pointer"
                  >
                    Delete Playlist
                  </button>
                </div>

                <div class="space-y-1">
                  {#each $selectedPlaylistItems as item (item.id)}
                    <div class="flex items-center justify-between gap-2 px-2 py-1.5 rounded-lg bg-slate-900/80 border border-slate-800 text-xs">
                      <button
                        type="button"
                        on:click={() => playTrack(playlistItemToTrack(item))}
                        class="flex-1 min-w-0 text-left flex items-center gap-2 text-slate-200 hover:text-cyan-300 cursor-pointer"
                      >
                        {#if item.thumbnail_url}
                          <img
                            src={item.thumbnail_url}
                            alt={item.title}
                            class="w-8 h-8 rounded object-cover shrink-0 border border-slate-700/60"
                          />
                        {:else}
                          <div class="w-8 h-8 rounded bg-slate-800/90 border border-slate-700/60 flex items-center justify-center shrink-0">
                            {#if item.is_video}
                              <Video class="w-3.5 h-3.5 text-amber-400" />
                            {:else}
                              <Music class="w-3.5 h-3.5 text-cyan-400" />
                            {/if}
                          </div>
                        {/if}
                        <div class="min-w-0">
                          <p class="truncate font-medium text-xs">{item.title}</p>
                          <p class="truncate text-[10px] text-slate-400">{item.artist}</p>
                        </div>
                      </button>
                      <button
                        type="button"
                        on:click={async () => {
                          await raydropApi.mediaRemovePlaylistItem(item.id);
                          await refreshMediaPlaylists();
                          if ($selectedPlaylistId) await loadPlaylistItems($selectedPlaylistId);
                        }}
                        class="p-1 text-slate-500 hover:text-rose-400 cursor-pointer"
                      >
                        <Trash2 class="w-3 h-3" />
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            {/if}

          {:else if activeTab === "ytmusic"}
            {#if $mediaPlaylists.length > 0}
              <div class="flex items-center gap-1.5 text-[10px] text-slate-400">
                <span>Save to:</span>
                <select
                  bind:value={addTargetPlaylistId}
                  class="px-1.5 py-0.5 rounded bg-slate-950 border border-slate-800 text-slate-200"
                >
                  {#each $mediaPlaylists as pl (pl.id)}
                    <option value={pl.id}>{pl.name}</option>
                  {/each}
                </select>
              </div>
            {/if}

            {#if ytDownloadingKey}
              <div class="p-2 rounded-lg bg-slate-900/90 border border-cyan-500/30 space-y-1.5 text-[11px]">
                <div class="flex items-center justify-between gap-2 text-cyan-300 font-medium">
                  <span class="truncate">{ytDownloadStatusText || "Downloading..."}</span>
                  <span class="shrink-0 font-mono">{ytDownloadPercent}%</span>
                </div>
                <div class="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden">
                  <div
                    class="h-full bg-cyan-400 transition-all duration-200"
                    style="width: {ytDownloadPercent}%"
                  ></div>
                </div>
              </div>
            {/if}

            {#if ytLastDownloaded}
              <div class="flex items-center justify-between gap-2 p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/30 text-[11px]">
                <div class="flex items-center gap-1.5 min-w-0 text-emerald-300">
                  <CheckCircle2 class="w-3.5 h-3.5 shrink-0" />
                  <span class="truncate">
                    Saved {ytLastDownloaded.format.toUpperCase()}: {ytLastDownloaded.title}
                  </span>
                </div>
                <button
                  type="button"
                  on:click={() => ytLastDownloaded && raydropApi.openFolderPath(ytLastDownloaded.output_path)}
                  class="px-2 py-0.5 rounded bg-slate-900 hover:bg-slate-800 border border-slate-700 text-white flex items-center gap-1 shrink-0 cursor-pointer"
                >
                  <FolderOpen class="w-3 h-3" />
                  <span>Open</span>
                </button>
              </div>
            {/if}

            <div class="space-y-1.5">
              {#each ytResults as item (item.video_id)}
                <div class="flex items-center justify-between gap-2 p-2 rounded-lg bg-slate-900/80 border border-slate-800">
                  <div class="flex items-center gap-2 min-w-0">
                    {#if item.thumbnail_url}
                      <img
                        src={item.thumbnail_url}
                        alt={item.title}
                        class="w-9 h-9 rounded object-cover shrink-0"
                      />
                    {/if}
                    <div class="min-w-0">
                      <p class="text-xs font-semibold text-white truncate">{item.title}</p>
                      <p class="text-[10px] text-slate-400 truncate">
                        {item.artist} {item.duration_text ? `• ${item.duration_text}` : ""} {item.is_video ? "• Video" : ""}
                      </p>
                    </div>
                  </div>
                  <div class="flex items-center gap-1 shrink-0">
                    <button
                      type="button"
                      on:click={() => playTrack(ytItemToTrack(item))}
                      class="p-1.5 rounded-lg bg-cyan-500/20 hover:bg-cyan-500 text-cyan-300 hover:text-slate-950 cursor-pointer"
                      title="Play Now"
                    >
                      <Play class="w-3 h-3" />
                    </button>
                    <button
                      type="button"
                      disabled={!!ytDownloadingKey}
                      on:click={() => handleYtDownload(item, "mp3")}
                      class="px-1.5 py-1 rounded-lg bg-emerald-500/15 hover:bg-emerald-500/30 border border-emerald-500/30 text-emerald-300 text-[10px] font-bold flex items-center gap-0.5 cursor-pointer disabled:opacity-40"
                      title="Download Audio (MP3)"
                    >
                      {#if ytDownloadingKey === `${item.video_id}-mp3`}
                        <Loader2 class="w-3 h-3 animate-spin" />
                      {:else}
                        <Download class="w-2.5 h-2.5" />
                      {/if}
                      <span>MP3</span>
                    </button>
                    <button
                      type="button"
                      disabled={!!ytDownloadingKey}
                      on:click={() => handleYtDownload(item, "mp4")}
                      class="px-1.5 py-1 rounded-lg bg-amber-500/15 hover:bg-amber-500/30 border border-amber-500/30 text-amber-300 text-[10px] font-bold flex items-center gap-0.5 cursor-pointer disabled:opacity-40"
                      title="Download Video (MP4)"
                    >
                      {#if ytDownloadingKey === `${item.video_id}-mp4`}
                        <Loader2 class="w-3 h-3 animate-spin" />
                      {:else}
                        <Download class="w-2.5 h-2.5" />
                      {/if}
                      <span>MP4</span>
                    </button>
                    <button
                      type="button"
                      on:click={() => handleAddYtToPlaylist(item)}
                      class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
                      title="Add to Playlist"
                    >
                      <Plus class="w-3 h-3" />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>
    {/if}
</div>
