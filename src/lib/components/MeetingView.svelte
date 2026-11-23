<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { openWithLastDir } from "../utils/filePicker";
  import { contacts } from "../stores/contacts";
  import { appSettings, myRankExp } from "../stores/state";
  import { getRankInfo } from "../utils/rank";
  import { peerAvatars, myAvatar, loadPeerAvatar } from "../stores/avatars";
  import { raydropApi } from "../utils/tauri-ipc";
  import {
    isInMeeting,
    currentRoomId,
    currentRoomTitle,
    isMicMuted,
    isCameraOn,
    isScreenSharing,
    isHandRaised,
    isPushToTalk,
    isRecording,
    recordingDuration,
    participants,
    meetingChatMessages,
    floatingReactions,
    localAudioStream,
    localVideoStream,
    createMeetingRoom,
    joinMeetingRoom,
    leaveMeetingRoom,
    inviteContactToMeeting,
    toggleMicMute,
    toggleCamera,
    toggleScreenShare,
    switchMeetingScreenShare,
    toggleHandRaise,
    sendMeetingReaction,
    sendMeetingChat,
    startRecording,
    stopRecording,
  } from "../stores/meeting";
  import {
    Mic,
    MicOff,
    Video,
    VideoOff,
    Monitor,
    MonitorOff,
    Hand,
    Smile,
    MessageSquare,
    PhoneOff,
    Copy,
    Check,
    Users,
    UserPlus,
    CircleDot,
    Send,
    FolderUp,
    Upload,
    Volume2,
    ShieldCheck,
    Lock,
    Sparkles,
    Radio,
    ChevronRight,
    Headphones,
    X,
    Loader2,
    PictureInPicture2,
    PenTool,
  } from "lucide-svelte";
  import AnnotationOverlay from "./AnnotationOverlay.svelte";

  // Lobby States
  let inputRoomName = "General Discussion";
  let inputRoomCode = "";
  let previewMicLevel = 0;
  let isMeetingPiP = false;
  let showMeetingAnnotation = false;

  async function toggleMeetingPiP() {
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
        isMeetingPiP = false;
        return;
      }
      const videos = Array.from(document.querySelectorAll("video")).filter(
        (v) => v.srcObject && v.readyState >= 2
      );
      const targetVideo = videos[0];
      if (targetVideo && (targetVideo as any).requestPictureInPicture) {
        await (targetVideo as any).requestPictureInPicture();
        isMeetingPiP = true;
        targetVideo.addEventListener(
          "leavepictureinpicture",
          () => (isMeetingPiP = false),
          { once: true }
        );
      }
    } catch (err) {
      console.warn("Meeting PiP error:", err);
    }
  }
  let previewStream: MediaStream | null = null;
  let previewVideoEl: HTMLVideoElement;
  let isCheckingPreview = false;

  // Meeting Room States
  let showChatDrawer = false;
  let showInviteDropdown = false;
  let showReactionPicker = false;
  let chatInputText = "";
  let copiedRoomCode = false;
  let chatScrollContainer: HTMLDivElement;

  // Svelte action to reliably attach MediaStream to video/audio DOM elements
  function attachStream(node: HTMLMediaElement, stream: MediaStream | null) {
    node.srcObject = stream;
    if (stream) {
      if (node instanceof HTMLVideoElement) {
        node.muted = true;
      }
      const playPromise = node.play();
      if (playPromise !== undefined) {
        playPromise.catch(() => {
          if (node instanceof HTMLVideoElement && !node.muted) {
            node.muted = true;
            node.play().catch(() => {});
          }
        });
      }
    }
    return {
      update(newStream: MediaStream | null) {
        if (node.srcObject !== newStream) {
          node.srcObject = newStream;
        }
        if (newStream) {
          if (node instanceof HTMLVideoElement) {
            node.muted = true;
          }
          const playPromise = node.play();
          if (playPromise !== undefined) {
            playPromise.catch(() => {
              if (node instanceof HTMLVideoElement && !node.muted) {
                node.muted = true;
                node.play().catch(() => {});
              }
            });
          }
        }
      },
      destroy() {
        node.srcObject = null;
      },
    };
  }

  $: activeParticipantList = Object.values($participants);
  $: screenSharingPeer =
    activeParticipantList.find((p) => p.isScreenSharing) ||
    ($isScreenSharing
      ? { deviceId: "local", alias: "You", stream: $localVideoStream }
      : null);

  $: raisedHandParticipants = [
    ...($isHandRaised ? [{ deviceId: "local", alias: "You" }] : []),
    ...activeParticipantList.filter((p) => p.isHandRaised),
  ];

  // Auto-preload peer avatars
  $: {
    for (const p of activeParticipantList) {
      if (p.deviceId && p.deviceId !== "local" && !$peerAvatars[p.deviceId]) {
        loadPeerAvatar(p.deviceId);
      }
    }
  }

  interface InvitableUser {
    key: string;
    displayName: string;
    avatar: string | null;
    deviceIds: string[];
    deviceCount: number;
    isOnline: boolean;
  }

  $: invitableUsers = (() => {
    const userMap = new Map<string, InvitableUser>();

    for (const c of $contacts) {
      if (c.device_id === $appSettings.device_id || c.is_own_device) continue;

      const key =
        (c.user_email && c.user_email.trim().toLowerCase()) ||
        (c.user_name && c.user_name.trim().toLowerCase()) ||
        (c.custom_name && c.custom_name.trim().toLowerCase()) ||
        c.device_id;

      let displayName = c.user_name?.trim() || c.custom_name?.trim();
      if (!displayName && c.user_email) {
        displayName = c.user_email.split("@")[0];
      }
      if (!displayName) {
        displayName = c.alias_name || "Contact";
      }

      const avatar = $peerAvatars[c.device_id] || null;

      if (!userMap.has(key)) {
        userMap.set(key, {
          key,
          displayName,
          avatar,
          deviceIds: [c.device_id],
          deviceCount: 1,
          isOnline: c.is_online,
        });
      } else {
        const existing = userMap.get(key)!;
        if (!existing.deviceIds.includes(c.device_id)) {
          existing.deviceIds.push(c.device_id);
          existing.deviceCount++;
        }
        if (c.is_online) existing.isOnline = true;
        if (!existing.avatar && avatar) existing.avatar = avatar;
      }
    }

    return Array.from(userMap.values());
  })();

  onMount(async () => {
    // Keydown shortcuts for meeting controls (PTT, Mute, Camera)
    window.addEventListener("keydown", handleGlobalKeydown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleGlobalKeydown);
    stopPreview();
  });

  function handleGlobalKeydown(e: KeyboardEvent) {
    if (!$isInMeeting) return;
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;

    if (e.key === "m" || e.key === "M") {
      toggleMicMute();
    } else if (e.key === "v" || e.key === "V") {
      toggleCamera();
    } else if (e.key === "h" || e.key === "H") {
      toggleHandRaise();
    }
  }

  // Auto-scroll meeting chat
  $: if (chatScrollContainer && $meetingChatMessages.length) {
    setTimeout(() => {
      chatScrollContainer.scrollTop = chatScrollContainer.scrollHeight;
    }, 50);
  }

  async function handleCreateRoom() {
    await createMeetingRoom(inputRoomName.trim() || "Online Meeting");
    inputRoomName = "General Discussion";
  }

  async function handleJoinRoom() {
    const raw = inputRoomCode.trim();
    if (!raw) return;
    // Support raydrop://room/xxx?host=yyy or room_xxx?host=yyy or plain code
    let cleanId = raw.replace(/^raydrop:\/\/room\//, "").trim();
    let hostDeviceId: string | undefined = undefined;

    if (cleanId.includes("?")) {
      const [rId, query] = cleanId.split("?");
      cleanId = rId;
      const params = new URLSearchParams(query);
      hostDeviceId = params.get("host") || undefined;
    }

    await joinMeetingRoom(cleanId, "Online Meeting", hostDeviceId);
    inputRoomCode = "";
  }

  async function copyRoomLink() {
    const code = $currentRoomId;
    const myId = $appSettings.device_id;
    await navigator.clipboard.writeText(`raydrop://room/${code}?host=${myId}`);
    copiedRoomCode = true;
    setTimeout(() => (copiedRoomCode = false), 2000);
  }

  async function handleInviteUser(deviceIds: string[]) {
    await inviteContactToMeeting(deviceIds);
    showInviteDropdown = false;
  }

  function handleSendChat() {
    if (!chatInputText.trim()) return;
    sendMeetingChat(chatInputText);
    chatInputText = "";
  }

  async function handleFileDropToMeeting() {
    const currentParticipants = Object.keys($participants);
    if (currentParticipants.length === 0) {
      alert(
        "Tidak ada peserta lain di room meeting saat ini. Silakan undang kontak terlebih dahulu!",
      );
      return;
    }

    try {
      const selected = await openWithLastDir({
        multiple: true,
        directory: false,
        title: "Pilih File untuk Dibagikan ke Peserta Meeting",
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length === 0) return;

      // Transfer to all active participants in room
      for (const peerId of currentParticipants) {
        await raydropApi.initiateTransfer(peerId, paths);
      }
      sendMeetingChat(
        `📎 Berhasil mengirim ${paths.length} file ke ${currentParticipants.length} peserta meeting!`,
      );
    } catch (err) {
      console.warn("Failed to share files to meeting:", err);
    }
  }

  function formatDuration(sec: number): string {
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  // Quick Hardware Preview
  async function startDevicePreview() {
    isCheckingPreview = true;
    try {
      previewStream = await navigator.mediaDevices.getUserMedia({
        audio: true,
        video: { width: 640, height: 360 },
      });
      if (previewVideoEl) {
        previewVideoEl.srcObject = previewStream;
      }
    } catch (err) {
      console.warn("Preview device failed:", err);
    }
  }

  function stopPreview() {
    if (previewStream) {
      previewStream.getTracks().forEach((t) => t.stop());
      previewStream = null;
    }
    isCheckingPreview = false;
  }
</script>

<div
  class="h-full w-full flex flex-col bg-slate-950 text-white select-none overflow-hidden relative"
>
  <!-- Floating Reactions Layer -->
  {#if $floatingReactions.length > 0}
    <div class="absolute inset-0 pointer-events-none z-50 overflow-hidden">
      {#each $floatingReactions as r (r.id)}
        <div
          class="absolute bottom-20 left-1/2 transform -translate-x-1/2 flex flex-col items-center animate-out fade-out slide-out-to-top duration-1000 fill-mode-forwards"
          style="margin-left: {(Math.random() - 0.5) * 300}px"
        >
          <span class="text-5xl filter drop-shadow-lg">{r.emoji}</span>
          <span
            class="text-[10px] font-mono font-bold bg-slate-900/80 px-2 py-0.5 rounded-full border border-slate-700 text-slate-300 mt-1"
          >
            {r.from}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  {#if !$isInMeeting}
    <!-- ============================================================== -->
    <!-- LOBBY VIEW: Create or Join Room & Device Preview               -->
    <!-- ============================================================== -->
    <div
      class="flex-1 overflow-y-auto p-4 sm:p-8 flex flex-col items-center justify-center max-w-4xl mx-auto w-full gap-6"
    >
      <!-- Title & Branding -->
      <div class="text-center space-y-2">
        <div
          class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-indigo-500/10 border border-indigo-500/30 text-indigo-300 text-xs font-semibold"
        >
          <Sparkles class="w-3.5 h-3.5 text-indigo-400" />
          <span>Discord & Zoom Style P2P Collaboration</span>
        </div>
        <h1
          class="text-2xl sm:text-4xl font-extrabold text-white tracking-tight"
        >
          Raydrop <span
            class="bg-gradient-to-r from-indigo-400 via-sky-400 to-cyan-300 bg-clip-text text-transparent"
            >Voice & Online Meeting</span
          >
        </h1>
        <p class="text-xs sm:text-sm text-slate-400 max-w-md mx-auto">
          Ultra-low latency, crystal-clear audio, HD screen sharing, and in-call
          P2P file transfers with zero cloud recording or fees.
        </p>
      </div>

      <!-- Action Cards Grid -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 w-full">
        <!-- Card 1: Start Instant Meeting -->
        <div
          class="p-6 rounded-2xl bg-slate-900/90 border border-indigo-500/30 shadow-xl flex flex-col justify-between gap-4"
        >
          <div class="space-y-2">
            <div
              class="w-10 h-10 rounded-xl bg-indigo-500/20 text-indigo-400 flex items-center justify-center border border-indigo-500/30"
            >
              <Radio class="w-5 h-5" />
            </div>
            <h3 class="text-base font-bold text-white">
              Create Voice / Meeting Room
            </h3>
            <p class="text-xs text-slate-400">
              Launch a private room and invite your team or contacts to join
              instantly.
            </p>
          </div>

          <div class="space-y-3 pt-2">
            <div>
              <span class="block text-[11px] font-semibold text-slate-300 mb-1"
                >Room Topic / Name</span
              >
              <input
                type="text"
                bind:value={inputRoomName}
                placeholder="e.g. Daily Standup, Chill Lounge..."
                class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-indigo-500"
              />
            </div>

            <button
              type="button"
              on:click={handleCreateRoom}
              class="w-full py-2.5 rounded-xl bg-gradient-to-r from-indigo-500 to-sky-600 hover:from-indigo-400 hover:to-sky-500 text-white font-bold text-xs shadow-lg shadow-indigo-500/20 transition flex items-center justify-center gap-2 cursor-pointer"
            >
              <Video class="w-4 h-4" />
              <span>Start Instant Meeting</span>
            </button>
          </div>
        </div>

        <!-- Card 2: Join by Room Code -->
        <div
          class="p-6 rounded-2xl bg-slate-900/90 border border-slate-800 shadow-xl flex flex-col justify-between gap-4"
        >
          <div class="space-y-2">
            <div
              class="w-10 h-10 rounded-xl bg-cyan-500/20 text-cyan-400 flex items-center justify-center border border-cyan-500/30"
            >
              <Users class="w-5 h-5" />
            </div>
            <h3 class="text-base font-bold text-white">Join with Room Code</h3>
            <p class="text-xs text-slate-400">
              Enter a room code or paste a meeting link shared by your host.
            </p>
          </div>

          <div class="space-y-3 pt-2">
            <div>
              <span class="block text-[11px] font-semibold text-slate-300 mb-1"
                >Room Code or Invite Link</span
              >
              <input
                type="text"
                bind:value={inputRoomCode}
                placeholder="e.g. room_abc123 or raydrop://room/..."
                class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 placeholder-slate-500 focus:outline-none focus:border-cyan-500"
              />
            </div>

            <button
              type="button"
              on:click={handleJoinRoom}
              disabled={!inputRoomCode.trim()}
              class="w-full py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-white font-bold text-xs border border-slate-700 transition flex items-center justify-center gap-2 cursor-pointer"
            >
              <ChevronRight class="w-4 h-4" />
              <span>Join Meeting</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Quick Preset Discord-style Voice Channels -->
      <div
        class="w-full p-4 rounded-2xl bg-slate-900/60 border border-slate-800 flex flex-col sm:flex-row items-center justify-between gap-3"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-9 h-9 rounded-xl bg-indigo-500/15 text-indigo-400 flex items-center justify-center border border-indigo-500/30"
          >
            <Headphones class="w-4 h-4" />
          </div>
          <div>
            <h4 class="text-xs font-bold text-white">Quick Voice Channels</h4>
            <p class="text-[11px] text-slate-400">
              Persistent P2P voice lounges for your team
            </p>
          </div>
        </div>

        <div class="flex items-center gap-2 flex-wrap">
          {#each ["Lounge Room", "Dev Standup", "Gaming Channel"] as channel}
            <button
              type="button"
              on:click={() =>
                joinMeetingRoom(
                  channel.toLowerCase().replace(/\s+/g, "_"),
                  channel,
                )}
              class="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-medium text-slate-300 border border-slate-700 transition flex items-center gap-1.5"
            >
              <Volume2 class="w-3.5 h-3.5 text-indigo-400" />
              <span>{channel}</span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Device Check & Privacy Notice -->
      <div class="flex items-center gap-2 text-xs text-slate-500 font-mono">
        <ShieldCheck class="w-4 h-4 text-emerald-400" />
        <span
          >End-to-End Encrypted via DTLS-SRTP • Direct Device-to-Device Mesh</span
        >
      </div>
    </div>
  {:else}
    <!-- ============================================================== -->
    <!-- ACTIVE MEETING VIEW: Participant Grid, Stage & Controls        -->
    <!-- ============================================================== -->

    <!-- Top Meeting Bar -->
    <div
      class="px-4 py-2.5 bg-slate-900/90 border-b border-slate-800 flex items-center justify-between gap-3 shrink-0"
    >
      <!-- Left: Room Info -->
      <div class="flex items-center gap-2.5 min-w-0">
        <div
          class="w-8 h-8 rounded-lg bg-indigo-500/20 text-indigo-400 flex items-center justify-center border border-indigo-500/30 shrink-0"
        >
          <Radio class="w-4 h-4 animate-pulse" />
        </div>
        <div class="min-w-0">
          <div class="flex items-center gap-2">
            <h2 class="text-xs sm:text-sm font-bold text-white truncate">
              {$currentRoomTitle}
            </h2>
            {#if $isRecording}
              <span
                class="flex items-center gap-1 text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-rose-500/20 text-rose-300 border border-rose-500/40 animate-pulse"
              >
                <CircleDot class="w-2.5 h-2.5 fill-rose-500 text-rose-500" />
                <span>REC {formatDuration($recordingDuration)}</span>
              </span>
            {/if}
          </div>
          <div
            class="flex items-center gap-2 text-[10px] text-slate-400 font-mono"
          >
            <span>ID: {$currentRoomId}</span>
            <button
              type="button"
              on:click={copyRoomLink}
              class="text-indigo-400 hover:text-indigo-300 transition flex items-center gap-0.5"
              title="Copy Room Link"
            >
              {#if copiedRoomCode}
                <Check class="w-3 h-3 text-emerald-400" />
                <span>Copied!</span>
              {:else}
                <Copy class="w-3 h-3" />
                <span>Copy Link</span>
              {/if}
            </button>
          </div>
        </div>
      </div>

      <!-- Right: Participants Count & Top Controls -->
      <div class="flex items-center gap-2 shrink-0">
        <!-- Invite Button & Dropdown -->
        <div class="relative">
          <button
            type="button"
            on:click={() => (showInviteDropdown = !showInviteDropdown)}
            class="px-2.5 py-1.5 rounded-xl bg-indigo-600/30 hover:bg-indigo-600/50 text-indigo-200 border border-indigo-500/40 text-xs font-semibold transition flex items-center gap-1.5"
          >
            <UserPlus class="w-3.5 h-3.5" />
            <span class="hidden sm:inline">Invite Contacts</span>
          </button>

          {#if showInviteDropdown}
            <div
              class="absolute right-0 top-full mt-2 w-64 max-h-72 overflow-y-auto rounded-2xl bg-slate-900 border border-slate-800 shadow-2xl p-2 z-50 space-y-1"
            >
              <span
                class="block px-2 py-1 text-[10px] font-bold text-slate-400 uppercase tracking-wider"
              >
                Invite Users
              </span>
              {#if invitableUsers.length === 0}
                <div class="p-3 text-center text-xs text-slate-500 italic">
                  No contacts available to invite
                </div>
              {:else}
                {#each invitableUsers as u (u.key)}
                  <button
                    type="button"
                    on:click={() => handleInviteUser(u.deviceIds)}
                    class="w-full px-2.5 py-1.5 rounded-xl text-left text-xs text-slate-200 hover:bg-slate-800 hover:text-white flex items-center justify-between transition gap-2"
                  >
                    <div class="flex items-center gap-2 min-w-0">
                      <div
                        class="w-6 h-6 rounded-lg bg-indigo-500/20 text-indigo-300 flex items-center justify-center font-bold text-[10px] shrink-0 overflow-hidden"
                      >
                        {#if u.avatar}
                          <img
                            src={u.avatar}
                            alt=""
                            class="w-full h-full object-cover"
                          />
                        {:else}
                          {u.displayName.slice(0, 2).toUpperCase()}
                        {/if}
                      </div>
                      <div class="min-w-0">
                        <span class="block truncate font-medium text-white"
                          >{u.displayName}</span
                        >
                        <span class="block text-[9px] text-slate-400">
                          {u.deviceCount} device{u.deviceCount > 1 ? "s" : ""}
                        </span>
                      </div>
                    </div>
                    <span
                      class="text-[9px] font-mono px-2 py-0.5 rounded-md bg-indigo-500/20 text-indigo-300 font-semibold shrink-0"
                    >
                      Invite
                    </span>
                  </button>
                {/each}
              {/if}
            </div>
          {/if}
        </div>

        <!-- Participants Counter -->
        <span
          class="px-2.5 py-1.5 rounded-xl bg-slate-800/80 border border-slate-700/60 text-xs font-mono text-slate-300 flex items-center gap-1.5"
        >
          <Users class="w-3.5 h-3.5 text-slate-400" />
          <span>{activeParticipantList.length + 1}</span>
        </span>

        <!-- Toggle In-Meeting Chat -->
        <button
          type="button"
          on:click={() => (showChatDrawer = !showChatDrawer)}
          class="p-2 rounded-xl transition {showChatDrawer
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
            : 'bg-slate-800 text-slate-300 hover:text-white'}"
          title="Meeting Chat"
        >
          <MessageSquare class="w-4 h-4" />
        </button>
      </div>
    </div>

    <!-- Main Meeting Workspace (Video Grid + Optional Chat Drawer) -->
    <div class="flex-1 flex overflow-hidden relative">
      <AnnotationOverlay active={showMeetingAnnotation} on:close={() => (showMeetingAnnotation = false)} />
      <!-- Video / Participant Grid Area -->
      <div
        class="flex-1 p-3 sm:p-5 flex flex-col justify-center items-center overflow-y-auto"
      >
        {#if screenSharingPeer}
          <!-- Spotlight Screen Sharing Mode -->
          <div class="w-full h-full flex flex-col gap-3">
            <div
              class="flex-1 rounded-2xl bg-black border border-indigo-500/40 relative overflow-hidden flex items-center justify-center shadow-2xl"
            >
              <!-- Floating Notification: Raised Hand banner during presentation -->
              {#if raisedHandParticipants.length > 0}
                <div class="absolute top-4 left-1/2 transform -translate-x-1/2 z-30 flex items-center gap-2 px-4 py-2 rounded-2xl bg-amber-500/90 text-amber-950 font-bold text-xs sm:text-sm shadow-2xl backdrop-blur-md animate-bounce border border-amber-300">
                  <span class="text-base sm:text-lg">✋</span>
                  <span>
                    {raisedHandParticipants.map(p => p.alias).join(", ")} {raisedHandParticipants.length > 1 ? "mengangkat tangan" : "mengangkat tangan"}!
                  </span>
                </div>
              {/if}

              {#if screenSharingPeer.deviceId === "local"}
                <video
                  autoplay
                  playsinline
                  muted
                  class="w-full h-full object-contain"
                  use:attachStream={$localVideoStream}
                ></video>
              {:else if screenSharingPeer.stream}
                <!-- Remote Peer Video -->
                <video
                  autoplay
                  playsinline
                  muted
                  class="w-full h-full object-contain"
                  use:attachStream={screenSharingPeer.stream}
                ></video>
              {:else}
                <div class="flex flex-col items-center justify-center gap-3 text-slate-400">
                  <Loader2 class="w-8 h-8 animate-spin text-cyan-400" />
                  <span class="text-xs font-medium">Menghubungkan layar {screenSharingPeer.alias}...</span>
                </div>
              {/if}

              <!-- Screen Presenter Label -->
              <div
                class="absolute bottom-3 left-3 bg-slate-900/80 backdrop-blur-md px-3 py-1.5 rounded-xl text-xs font-mono border border-indigo-500/50 text-indigo-200 flex items-center gap-2 shadow-lg"
              >
                <span class="w-2 h-2 rounded-full bg-emerald-400 animate-ping"></span>
                <span>🖥️ {screenSharingPeer.alias} is presenting screen</span>
              </div>
            </div>

            <!-- Participant strip underneath presentation -->
            <div class="h-28 flex items-center gap-2.5 overflow-x-auto py-1 px-1">
              <!-- Local Tile -->
              <div
                class="h-full aspect-video rounded-xl bg-slate-900/90 border-2 {$isHandRaised
                  ? 'border-amber-400 ring-2 ring-amber-400/40'
                  : $isMicMuted
                  ? 'border-slate-800'
                  : 'border-emerald-500/50'} relative flex flex-col items-center justify-center shrink-0 overflow-hidden shadow-md group transition-all"
              >
                <!-- Video or Avatar -->
                {#if $isCameraOn && $localVideoStream}
                  <video
                    autoplay
                    playsinline
                    muted
                    class="w-full h-full object-cover transform -scale-x-100"
                    use:attachStream={$localVideoStream}
                  ></video>
                {:else if $myAvatar}
                  <img
                    src={$myAvatar}
                    alt="You"
                    class="w-10 h-10 rounded-full object-cover border border-slate-700/60 shadow-sm"
                  />
                {:else}
                  <div
                    class="w-10 h-10 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center text-xs font-bold text-slate-300"
                  >
                    {$appSettings.alias_name?.slice(0, 2).toUpperCase() || "YOU"}
                  </div>
                {/if}

                <!-- Sharing Screen Badge -->
                {#if $isScreenSharing}
                  <div
                    class="absolute top-1.5 left-1.5 px-1.5 py-0.5 rounded-md bg-indigo-600/90 text-[10px] font-bold text-white flex items-center gap-1 shadow-sm border border-indigo-400/50 animate-pulse"
                  >
                    <span>🖥️ Sharing</span>
                  </div>
                {/if}

                <!-- Raised Hand Badge -->
                {#if $isHandRaised}
                  <div
                    class="absolute top-1.5 right-1.5 px-1.5 py-0.5 rounded-md bg-amber-400 text-amber-950 font-bold text-[11px] flex items-center gap-1 shadow-md border border-amber-300 animate-bounce"
                  >
                    <span>✋</span>
                  </div>
                {/if}

                <!-- Bottom Label -->
                <div
                  class="absolute bottom-1 left-1.5 right-1.5 flex items-center justify-between gap-1 text-[11px] font-medium text-slate-200 bg-slate-950/80 backdrop-blur-sm px-1.5 py-0.5 rounded border border-slate-800"
                >
                  <div class="flex items-center gap-1 min-w-0">
                    <span class="truncate">You</span>
                    <span
                      class="shrink-0 inline-flex items-center gap-0.5 px-1 py-0.5 rounded text-[9px] font-bold leading-none {getRankInfo($myRankExp).badgeStyle}"
                      title="Raydrop Rank: {getRankInfo($myRankExp).tierName} Lv.{getRankInfo($myRankExp).level} ({getRankInfo($myRankExp).exp} EXP)"
                    >
                      <span>{getRankInfo($myRankExp).icon}</span>
                      <span>Lv.{getRankInfo($myRankExp).level}</span>
                    </span>
                  </div>
                  {#if $isMicMuted}
                    <MicOff class="w-3 h-3 text-rose-400 shrink-0" />
                  {/if}
                </div>
              </div>

              <!-- Remote Peers Tiles -->
              {#each activeParticipantList as peer (peer.deviceId)}
                {@const peerImg = peer.avatar || $peerAvatars[peer.deviceId]}
                {@const peerRank = getRankInfo($contacts.find((c) => c.device_id === peer.deviceId)?.rank_exp ?? 0)}
                <div
                  class="h-full aspect-video rounded-xl bg-slate-900/90 border-2 {peer.isHandRaised
                    ? 'border-amber-400 ring-2 ring-amber-400/40'
                    : peer.isSpeaking
                    ? 'border-emerald-400 ring-2 ring-emerald-500/30'
                    : 'border-slate-800'} relative flex flex-col items-center justify-center shrink-0 overflow-hidden shadow-md group transition-all"
                >
                  <!-- Video or Avatar -->
                  {#if peer.isCameraOn && peer.stream}
                    <video
                      autoplay
                      playsinline
                      muted
                      class="w-full h-full object-cover"
                      use:attachStream={peer.stream}
                    ></video>
                  {:else if peerImg}
                    <img
                      src={peerImg}
                      alt={peer.alias}
                      class="w-10 h-10 rounded-full object-cover border border-slate-700/60 shadow-sm"
                    />
                  {:else}
                    <div
                      class="w-10 h-10 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center text-xs font-bold text-slate-300"
                    >
                      {peer.alias?.slice(0, 2).toUpperCase() || "P"}
                    </div>
                  {/if}

                  <!-- Sharing Screen Badge -->
                  {#if peer.isScreenSharing}
                    <div
                      class="absolute top-1.5 left-1.5 px-1.5 py-0.5 rounded-md bg-indigo-600/90 text-[10px] font-bold text-white flex items-center gap-1 shadow-sm border border-indigo-400/50 animate-pulse"
                    >
                      <span>🖥️ Sharing</span>
                    </div>
                  {/if}

                  <!-- Raised Hand Badge -->
                  {#if peer.isHandRaised}
                    <div
                      class="absolute top-1.5 right-1.5 px-1.5 py-0.5 rounded-md bg-amber-400 text-amber-950 font-bold text-[11px] flex items-center gap-1 shadow-md border border-amber-300 animate-bounce"
                    >
                      <span>✋</span>
                    </div>
                  {/if}

                  <!-- Bottom Label -->
                  <div
                    class="absolute bottom-1 left-1.5 right-1.5 flex items-center justify-between gap-1 text-[11px] font-medium text-slate-200 bg-slate-950/80 backdrop-blur-sm px-1.5 py-0.5 rounded border border-slate-800"
                  >
                    <div class="flex items-center gap-1 min-w-0">
                      <span class="truncate">{peer.alias}</span>
                      <span
                        class="shrink-0 inline-flex items-center gap-0.5 px-1 py-0.5 rounded text-[9px] font-bold leading-none {peerRank.badgeStyle}"
                        title="Raydrop Rank: {peerRank.tierName} Lv.{peerRank.level} ({peerRank.exp} EXP)"
                      >
                        <span>{peerRank.icon}</span>
                        <span>Lv.{peerRank.level}</span>
                      </span>
                    </div>
                    {#if peer.isMuted}
                      <MicOff class="w-3 h-3 text-rose-400 shrink-0" />
                    {/if}
                  </div>

                  {#if peer.stream}
                    <!-- Hidden audio element auto-playing remote participant's voice -->
                    <audio autoplay playsinline use:attachStream={peer.stream}></audio>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <!-- Normal Dynamic Gallery Grid Mode -->
          <div
            class="w-full h-full max-w-5xl grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3 sm:gap-4 auto-rows-fr"
          >
            <!-- 1. Local User Tile -->
            <div
              class="rounded-2xl bg-slate-900 border-2 {$isHandRaised
                ? 'border-amber-400 ring-2 ring-amber-400/40'
                : $isMicMuted
                ? 'border-slate-800'
                : 'border-emerald-500/40'} shadow-lg relative flex flex-col items-center justify-center overflow-hidden transition-all group"
            >
              {#if $isCameraOn && $localVideoStream}
                <video
                  autoplay
                  playsinline
                  muted
                  class="w-full h-full object-cover transform -scale-x-100"
                  use:attachStream={$localVideoStream}
                ></video>
              {:else}
                <!-- Avatar / Initials Tile -->
                <div
                  class="w-20 h-20 rounded-2xl bg-gradient-to-tr from-indigo-500/20 to-sky-500/20 border border-indigo-500/40 flex items-center justify-center text-xl font-bold text-indigo-300 shadow-inner overflow-hidden"
                >
                  {#if $myAvatar}
                    <img
                      src={$myAvatar}
                      alt="Avatar"
                      class="w-full h-full rounded-2xl object-cover"
                    />
                  {:else}
                    {$appSettings.alias_name?.slice(0, 2).toUpperCase() || "ME"}
                  {/if}
                </div>
              {/if}

              <!-- Sharing Screen Badge -->
              {#if $isScreenSharing}
                <div class="absolute top-2.5 left-2.5 px-2 py-0.5 rounded-lg bg-indigo-600/90 text-xs font-bold text-white flex items-center gap-1 shadow-md border border-indigo-400/50 animate-pulse">
                  <span>🖥️ Sharing Screen</span>
                </div>
              {/if}

              <!-- Badges -->
              <div
                class="absolute bottom-2.5 left-2.5 bg-slate-950/80 backdrop-blur-md px-2.5 py-1 rounded-xl text-xs font-medium text-slate-200 border border-slate-800 flex items-center gap-1.5"
              >
                <span>{$appSettings.alias_name || "You"} (You)</span>
                <span
                  class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-bold leading-none {getRankInfo($myRankExp).badgeStyle}"
                  title="Raydrop Rank: {getRankInfo($myRankExp).tierName} Lv.{getRankInfo($myRankExp).level} ({getRankInfo($myRankExp).exp} EXP)"
                >
                  <span>{getRankInfo($myRankExp).icon}</span>
                  <span>{getRankInfo($myRankExp).tierName} Lv.{getRankInfo($myRankExp).level}</span>
                </span>
                {#if $isHandRaised}
                  <span class="text-amber-400 animate-bounce">✋</span>
                {/if}
              </div>

              {#if $isMicMuted}
                <div
                  class="absolute top-2.5 right-2.5 p-1.5 rounded-lg bg-rose-500/20 text-rose-400 border border-rose-500/40"
                >
                  <MicOff class="w-3.5 h-3.5" />
                </div>
              {/if}
            </div>

            <!-- 2. Remote Participants Tiles -->
            {#each activeParticipantList as peer (peer.deviceId)}
              {@const peerImg = peer.avatar || $peerAvatars[peer.deviceId]}
              {@const peerRank = getRankInfo($contacts.find((c) => c.device_id === peer.deviceId)?.rank_exp ?? 0)}
              <div
                class="rounded-2xl bg-slate-900 border-2 {peer.isHandRaised
                  ? 'border-amber-400 ring-2 ring-amber-400/40'
                  : peer.isSpeaking
                  ? 'border-emerald-400 shadow-lg shadow-emerald-500/25 ring-2 ring-emerald-500/30'
                  : 'border-slate-800'} shadow-lg relative flex flex-col items-center justify-center overflow-hidden transition-all"
              >
                {#if peer.isCameraOn && peer.stream}
                  <video
                    autoplay
                    playsinline
                    muted
                    class="w-full h-full object-cover"
                    use:attachStream={peer.stream}
                  ></video>
                {:else}
                  <!-- Remote Avatar -->
                  <div
                    class="w-20 h-20 rounded-2xl bg-gradient-to-tr from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 flex items-center justify-center text-xl font-bold text-cyan-300 shadow-inner overflow-hidden"
                  >
                    {#if peerImg}
                      <img
                        src={peerImg}
                        alt={peer.alias}
                        class="w-full h-full rounded-2xl object-cover"
                      />
                    {:else}
                      {peer.alias.slice(0, 2).toUpperCase()}
                    {/if}
                  </div>
                {/if}

                <!-- Audio Playback -->
                {#if peer.stream}
                  <audio autoplay playsinline use:attachStream={peer.stream}
                  ></audio>
                {/if}

                <!-- Remote Sharing Badge -->
                {#if peer.isScreenSharing}
                  <div class="absolute top-2.5 left-2.5 px-2 py-0.5 rounded-lg bg-indigo-600/90 text-xs font-bold text-white flex items-center gap-1 shadow-md border border-indigo-400/50 animate-pulse">
                    <span>🖥️ Sharing Screen</span>
                  </div>
                {/if}

                <!-- Peer Badges -->
                <div
                  class="absolute bottom-2.5 left-2.5 bg-slate-950/80 backdrop-blur-md px-2.5 py-1 rounded-xl text-xs font-medium text-slate-200 border border-slate-800 flex items-center gap-1.5"
                >
                  <span>{peer.alias}</span>
                  <span
                    class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-bold leading-none {peerRank.badgeStyle}"
                    title="Raydrop Rank: {peerRank.tierName} Lv.{peerRank.level} ({peerRank.exp} EXP)"
                  >
                    <span>{peerRank.icon}</span>
                    <span>{peerRank.tierName} Lv.{peerRank.level}</span>
                  </span>
                  {#if peer.isHandRaised}
                    <span class="text-amber-400 animate-bounce">✋</span>
                  {/if}
                </div>

                {#if peer.isMuted}
                  <div
                    class="absolute top-2.5 right-2.5 p-1.5 rounded-lg bg-rose-500/20 text-rose-400 border border-rose-500/40"
                  >
                    <MicOff class="w-3.5 h-3.5" />
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- In-Meeting Side Drawer (Chat & File Share) -->
      {#if showChatDrawer}
        <div
          class="w-full sm:w-80 absolute sm:relative inset-y-0 right-0 z-40 border-l border-slate-800 bg-slate-900/95 flex flex-col h-full shrink-0 animate-in slide-in-from-right duration-150"
        >
          <!-- Drawer Header -->
          <div
            class="p-3 border-b border-slate-800 flex items-center justify-between"
          >
            <span
              class="text-xs font-bold text-white flex items-center gap-1.5"
            >
              <MessageSquare class="w-4 h-4 text-cyan-400" />
              <span>Meeting Chat</span>
            </span>
            <button
              type="button"
              on:click={() => (showChatDrawer = false)}
              class="text-slate-400 hover:text-white p-1 rounded-lg"
            >
              <X class="w-4 h-4" />
            </button>
          </div>

          <!-- Messages Container -->
          <div
            bind:this={chatScrollContainer}
            class="flex-1 overflow-y-auto p-3 space-y-2.5 text-xs"
          >
            {#if $meetingChatMessages.length === 0}
              <div class="p-8 text-center text-slate-500 italic">
                No in-meeting messages yet
              </div>
            {:else}
              {#each $meetingChatMessages as msg (msg.id)}
                <div class="space-y-0.5">
                  <div
                    class="flex items-center gap-1.5 text-[10px] text-slate-400"
                  >
                    <span class="font-bold text-cyan-300">{msg.senderName}</span
                    >
                    <span
                      >{new Date(msg.timestamp).toLocaleTimeString([], {
                        hour: "2-digit",
                        minute: "2-digit",
                      })}</span
                    >
                  </div>
                  <div
                    class="p-2 rounded-xl bg-slate-950 border border-slate-800 text-slate-200 break-words"
                  >
                    {msg.text}
                  </div>
                </div>
              {/each}
            {/if}
          </div>

          <!-- Chat Input -->
          <form
            on:submit|preventDefault={handleSendChat}
            class="p-2 border-t border-slate-800 flex items-center gap-1.5"
          >
            <input
              type="text"
              bind:value={chatInputText}
              placeholder="Message everyone..."
              class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
            />
            <button
              type="submit"
              disabled={!chatInputText.trim()}
              class="p-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-slate-950 font-bold transition"
            >
              <Send class="w-3.5 h-3.5" />
            </button>
          </form>
        </div>
      {/if}
    </div>

    <!-- Floating / Bottom Meeting Control Bar (Discord + Zoom style) -->
    <div
      class="px-2 sm:px-4 py-2.5 sm:py-3 bg-slate-900/95 border-t border-slate-800 flex items-center justify-between gap-1.5 sm:gap-2 overflow-x-auto max-w-full shrink-0 z-30"
    >
      <!-- Left Controls: Audio / Video Toggles -->
      <div class="flex items-center gap-1.5 sm:gap-2">
        <!-- Mic Mute Toggle -->
        <button
          type="button"
          on:click={toggleMicMute}
          class="flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold transition {$isMicMuted
            ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40 hover:bg-rose-500/30'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title={$isMicMuted ? "Unmute Mic (M)" : "Mute Mic (M)"}
        >
          {#if $isMicMuted}
            <MicOff class="w-4 h-4 text-rose-400" />
            <span class="hidden sm:inline">Unmute</span>
          {:else}
            <Mic class="w-4 h-4 text-emerald-400" />
            <span class="hidden sm:inline">Mute</span>
          {/if}
        </button>

        <!-- Camera Video Toggle -->
        <button
          type="button"
          on:click={toggleCamera}
          class="flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold transition {$isCameraOn
            ? 'bg-indigo-600/30 text-indigo-300 border border-indigo-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title={$isCameraOn ? "Turn Off Camera (V)" : "Turn On Camera (V)"}
        >
          {#if $isCameraOn}
            <Video class="w-4 h-4 text-indigo-400" />
            <span class="hidden sm:inline">Stop Video</span>
          {:else}
            <VideoOff class="w-4 h-4 text-slate-400" />
            <span class="hidden sm:inline">Start Video</span>
          {/if}
        </button>

        <!-- Screen Sharing Toggle -->
        <button
          type="button"
          on:click={toggleScreenShare}
          class="flex items-center gap-1.5 px-3 py-2 rounded-xl text-xs font-semibold transition {$isScreenSharing
            ? 'bg-sky-500/20 text-sky-300 border border-sky-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title="Share Desktop / Application Window"
        >
          <Monitor
            class="w-4 h-4 {$isScreenSharing
              ? 'text-sky-400'
              : 'text-slate-400'}"
          />
          <span class="hidden sm:inline"
            >{$isScreenSharing ? "Stop Share" : "Share Screen"}</span
          >
        </button>

        {#if $isScreenSharing}
          <button
            type="button"
            on:click={switchMeetingScreenShare}
            class="flex items-center gap-1 px-2.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-cyan-300 border border-cyan-500/30 text-xs font-semibold transition cursor-pointer"
            title="Switch Shared App Window or Monitor"
          >
            <Monitor class="w-3.5 h-3.5" />
            <span class="hidden sm:inline">Switch</span>
          </button>
        {/if}
      </div>

      <!-- Center Controls: Reactions, Raise Hand, File Drop, Record -->
      <div class="flex items-center gap-1 sm:gap-2">
        <!-- Raise Hand -->
        <button
          type="button"
          on:click={toggleHandRaise}
          class="p-2 sm:px-3 sm:py-2 rounded-xl text-xs font-semibold transition flex items-center gap-1.5 whitespace-nowrap {$isHandRaised
            ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title="Raise Hand (H)"
        >
          <Hand
            class="w-4 h-4 shrink-0 {$isHandRaised
              ? 'text-amber-400'
              : 'text-slate-400'}"
          />
          <span class="hidden md:inline">Raise Hand</span>
        </button>

        <!-- Reactions Picker Popover -->
        <div class="relative">
          <button
            type="button"
            on:click={() => (showReactionPicker = !showReactionPicker)}
            class="p-2 sm:px-3 sm:py-2 rounded-xl bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700 text-xs font-semibold transition flex items-center gap-1"
            title="Send Emoji Reaction"
          >
            <Smile class="w-4 h-4 text-slate-400" />
            <span class="hidden md:inline">React</span>
          </button>

          {#if showReactionPicker}
            <div
              class="absolute bottom-full mb-2 left-1/2 transform -translate-x-1/2 bg-slate-900 border border-slate-800 rounded-2xl p-2 shadow-2xl flex items-center gap-1 z-50"
            >
              {#each ["👍", "❤️", "👏", "😂", "🎉", "🔥"] as emoji}
                <button
                  type="button"
                  on:click={() => {
                    sendMeetingReaction(emoji);
                    showReactionPicker = false;
                  }}
                  class="text-xl p-1.5 hover:bg-slate-800 rounded-xl transition hover:scale-125 transform"
                >
                  {emoji}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <!-- In-Meeting Direct P2P File Drop -->
        <button
          type="button"
          on:click={handleFileDropToMeeting}
          class="p-2 sm:px-3 sm:py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-cyan-300 border border-cyan-500/30 text-xs font-semibold transition flex items-center gap-1"
          title="Send Files directly to all meeting participants"
        >
          <FolderUp class="w-4 h-4" />
          <span class="hidden md:inline">Drop Files</span>
        </button>

        <!-- Local Meeting Recording Toggle -->
        <button
          type="button"
          on:click={$isRecording ? stopRecording : startRecording}
          class="p-2 sm:px-3 sm:py-2 rounded-xl text-xs font-semibold transition flex items-center gap-1.5 whitespace-nowrap {$isRecording
            ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title="Record meeting audio/video locally to disk"
        >
          <CircleDot
            class="w-4 h-4 shrink-0 {$isRecording
              ? 'text-rose-500 animate-pulse'
              : 'text-slate-400'}"
          />
          <span class="hidden md:inline"
            >{$isRecording ? "Stop Rec" : "Record"}</span
          >
        </button>

        <!-- Floating Mini Player (PiP) -->
        <button
          type="button"
          on:click={toggleMeetingPiP}
          class="p-2 sm:px-3 sm:py-2 rounded-xl text-xs font-semibold transition flex items-center gap-1.5 whitespace-nowrap {isMeetingPiP
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title={isMeetingPiP ? "Exit Mini Player (PiP)" : "Floating Mini Player (PiP)"}
        >
          <PictureInPicture2
            class="w-4 h-4 shrink-0 {isMeetingPiP ? 'text-cyan-400' : 'text-slate-400'}"
          />
          <span class="hidden md:inline">PiP</span>
        </button>

        <!-- Laser Pointer & Annotation -->
        <button
          type="button"
          on:click={() => (showMeetingAnnotation = !showMeetingAnnotation)}
          class="p-2 sm:px-3 sm:py-2 rounded-xl text-xs font-semibold transition flex items-center gap-1.5 whitespace-nowrap {showMeetingAnnotation
            ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
            : 'bg-slate-800 text-slate-200 border border-slate-700 hover:bg-slate-700'}"
          title="Laser Pointer & Screen Annotation"
        >
          <PenTool
            class="w-4 h-4 shrink-0 {showMeetingAnnotation ? 'text-rose-400' : 'text-slate-400'}"
          />
          <span class="hidden md:inline">Annotate</span>
        </button>
      </div>

      <!-- Right: Leave / End Call Button -->
      <div>
        <button
          type="button"
          on:click={leaveMeetingRoom}
          class="px-4 py-2 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-bold text-xs shadow-md shadow-rose-600/30 transition flex items-center gap-1.5 cursor-pointer"
        >
          <PhoneOff class="w-4 h-4" />
          <span>Leave Room</span>
        </button>
      </div>
    </div>
  {/if}
</div>
