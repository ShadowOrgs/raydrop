<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    Monitor,
    Shield,
    Lock,
    Key,
    RefreshCw,
    Play,
    Check,
    X,
    Maximize,
    Minimize,
    ArrowDownToLine,
    ArrowUpFromLine,
    LogOut,
    Copy,
    Laptop,
    AlertCircle,
    MonitorPlay,
    Square,
    Sparkles,
    ChevronDown,
    ChevronUp,
    MousePointer,
    Search,
    Plus,
    Tv,
    Smartphone,
    Radio,
  } from "lucide-svelte";
  import {
    remoteHostSettings,
    isHostSharingScreen,
    clientRemoteStream,
    activeRemoteSession,
    remoteConnectionStatus,
    remoteErrorMessage,
    remoteLatencyMs,
    loadRemoteDesktopSettings,
    saveRemoteDesktopSettings,
    connectToRemoteHost,
    disconnectRemoteSession,
    sendInputToHost,
    sendLocalClipboardToHost,
    requestClipboardFromHost,
    sendShortcutToHost,
    initRemoteDesktopListener,
    startHostScreenCapture,
    stopHostScreenCapture,
    pendingRemoteAuthRequest,
    acceptPendingRemoteRequest,
    rejectPendingRemoteRequest,
  } from "../stores/remote_desktop";
  import { contacts, lanContacts } from "../stores/contacts";
  import { cloudMyDevices, appSettings } from "../stores/state";
  import { peerAvatars, loadPeerAvatar } from "../stores/avatars";
  import { formatLastOnline, formatLastOnlineCompact } from "../utils/time";

  let selectedTargetDeviceId = "";
  let inputPin = "";

  // Modals & Navigation state
  let isHostSettingsModalOpen = false;
  let isConnectPinModalOpen = false;
  let isManualConnectModalOpen = false;
  let manualDeviceIdInput = "";
  let selectedPeer: {
    id: string;
    name: string;
    type: string;
    isOnline: boolean;
    lastSeenAt?: number;
  } | null = null;
  let searchQuery = "";
  let filterType: "all" | "cloud" | "lan" | "contacts" = "all";

  // Host configuration state
  let hostEnabled = true;
  let hostPin = "";
  let hostUnattended = true;
  let isSavingSettings = false;
  let saveSuccess = false;

  // Change PIN modal state
  let isChangePinModalOpen = false;
  let newPin = "";
  let confirmPin = "";
  let pinChangeError = "";
  let pinChangeSuccess = false;

  // Viewer state
  let videoEl: HTMLVideoElement | null = null;
  let viewerContainerEl: HTMLDivElement | null = null;
  let isFullscreen = false;
  let showToolbar = false;
  let hideLocalCursor = false;
  let lastMouseMoveTime = 0;

  $: isClientConnected =
    $remoteConnectionStatus === "connected" &&
    $activeRemoteSession?.isClient &&
    !!$clientRemoteStream;

  $: isHostActive =
    $activeRemoteSession?.isHost && $remoteConnectionStatus === "connected";

  $: if (isClientConnected && isConnectPinModalOpen) {
    isConnectPinModalOpen = false;
  }

  // Svelte action to reliably attach MediaStream to video DOM element with autoplay & unfreeze handling
  function attachStream(node: HTMLVideoElement, stream: MediaStream | null) {
    node.srcObject = stream;

    const playSafe = () => {
      if (node && node.srcObject) {
        node.muted = true;
        const playPromise = node.play();
        if (playPromise !== undefined) {
          playPromise.catch(() => {
            node.muted = true;
            node.play().catch(() => {});
          });
        }
      }
    };

    if (stream) {
      playSafe();
    }

    // Auto-resume playback if paused by browser on maximize, minimize, resize, or tab blur
    const handlePause = () => {
      if (node && node.srcObject) {
        playSafe();
      }
    };

    const handleResume = () => {
      if (node && node.srcObject && node.paused) {
        playSafe();
      }
    };

    node.addEventListener("pause", handlePause);
    window.addEventListener("resize", handleResume);
    window.addEventListener("focus", handleResume);
    document.addEventListener("visibilitychange", handleResume);

    return {
      update(newStream: MediaStream | null) {
        if (node.srcObject !== newStream) {
          node.srcObject = newStream;
        }
        if (newStream) {
          playSafe();
        }
      },
      destroy() {
        node.removeEventListener("pause", handlePause);
        window.removeEventListener("resize", handleResume);
        window.removeEventListener("focus", handleResume);
        document.removeEventListener("visibilitychange", handleResume);
        node.srcObject = null;
      },
    };
  }

  // Combined list of reachable remote target devices
  $: availablePeers = (() => {
    const list: {
      id: string;
      name: string;
      type: string;
      isOnline: boolean;
      lastSeenAt: number;
    }[] = [];
    const seen = new Set<string>();
    const myDevId = $appSettings.device_id;

    for (const d of $cloudMyDevices) {
      if (d.device_id === myDevId || seen.has(d.device_id)) continue;
      seen.add(d.device_id);
      list.push({
        id: d.device_id,
        name: d.device_name || "My Cloud Device",
        type: "My Cloud Device",
        isOnline: d.is_online,
        lastSeenAt: d.last_seen_at || 0,
      });
    }

    for (const c of $contacts) {
      if (c.device_id === myDevId || seen.has(c.device_id)) continue;
      seen.add(c.device_id);
      list.push({
        id: c.device_id,
        name: c.custom_name || c.alias_name || "Contact",
        type: "Contact",
        isOnline: c.is_online,
        lastSeenAt: c.last_seen_at || 0,
      });
    }

    for (const l of $lanContacts) {
      if (l.device_id === myDevId || seen.has(l.device_id)) continue;
      seen.add(l.device_id);
      list.push({
        id: l.device_id,
        name: l.alias_name || "LAN Peer",
        type: "LAN Peer",
        isOnline: true,
        lastSeenAt: l.last_seen_at || 0,
      });
    }

    return list;
  })();

  $: filteredPeers = availablePeers.filter((p) => {
    if (filterType === "cloud" && p.type !== "My Cloud Device") return false;
    if (filterType === "lan" && p.type !== "LAN Peer") return false;
    if (filterType === "contacts" && p.type !== "Contact") return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase().trim();
      return p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q);
    }
    return true;
  });

  // Automatically request avatars for all reachable peers
  $: {
    for (const p of availablePeers) {
      if (!$peerAvatars[p.id]) {
        loadPeerAvatar(p.id);
      }
    }
  }

  function openConnectModalForPeer(peer: {
    id: string;
    name: string;
    type: string;
    isOnline: boolean;
    lastSeenAt?: number;
  }) {
    selectedPeer = peer;
    selectedTargetDeviceId = peer.id;
    inputPin = "";
    isConnectPinModalOpen = true;
  }

  function openManualConnectModal() {
    manualDeviceIdInput = "";
    isManualConnectModalOpen = true;
  }

  function proceedManualToPin() {
    const id = manualDeviceIdInput.trim();
    if (!id) return;
    selectedPeer = {
      id,
      name: id,
      type: "Custom Device",
      isOnline: true,
    };
    selectedTargetDeviceId = id;
    inputPin = "";
    isManualConnectModalOpen = false;
    isConnectPinModalOpen = true;
  }

  function handleModalConnect(withPin: boolean = true) {
    if (!selectedTargetDeviceId) return;
    const pinToSend = withPin ? inputPin.trim() : "";
    connectToRemoteHost(selectedTargetDeviceId, pinToSend);
  }

  onMount(async () => {
    initRemoteDesktopListener();
    await loadRemoteDesktopSettings();
    hostEnabled = $remoteHostSettings.enabled;
    hostPin = $remoteHostSettings.pin;
    hostUnattended = $remoteHostSettings.unattended;

    if (hostEnabled && !$isHostSharingScreen) {
      // Warm up host screen capture in background so unattended access is instant
      startHostScreenCapture().catch(() => {});
    }

    document.addEventListener("fullscreenchange", handleFullscreenChange);
  });

  onDestroy(() => {
    document.removeEventListener("fullscreenchange", handleFullscreenChange);
    if (typeof navigator !== "undefined" && (navigator as any).keyboard?.unlock) {
      (navigator as any).keyboard.unlock();
    }
  });

  function handleFullscreenChange() {
    isFullscreen = !!document.fullscreenElement;
    if (isFullscreen) {
      // Auto hide toolbar on fullscreen enter for clean view
      showToolbar = false;
      // Lock keyboard so all system shortcuts (Alt+Tab, Windows key, PrintScreen, Escape) pass to host
      if (typeof navigator !== "undefined" && (navigator as any).keyboard?.lock) {
        (navigator as any).keyboard.lock().catch(() => {});
      }
    } else {
      if (typeof navigator !== "undefined" && (navigator as any).keyboard?.unlock) {
        (navigator as any).keyboard.unlock();
      }
    }
  }

  async function handleToggleHostEnabled() {
    try {
      isSavingSettings = true;
      await saveRemoteDesktopSettings(hostEnabled, hostPin, hostUnattended);
      saveSuccess = true;
      setTimeout(() => (saveSuccess = false), 2500);

      if (hostEnabled && !$isHostSharingScreen) {
        await startHostScreenCapture();
      } else if (!hostEnabled && $isHostSharingScreen) {
        stopHostScreenCapture();
      }
    } catch (err: any) {
      alert(`Failed to save settings: ${err?.message || err}`);
    } finally {
      isSavingSettings = false;
    }
  }

  function openChangePinModal() {
    newPin = "";
    confirmPin = "";
    pinChangeError = "";
    pinChangeSuccess = false;
    isChangePinModalOpen = true;
  }

  function closeChangePinModal() {
    isChangePinModalOpen = false;
  }

  function generateRandomPinInModal() {
    const r = Math.floor(100000 + Math.random() * 900000).toString();
    newPin = r;
    confirmPin = r;
    pinChangeError = "";
  }

  async function handleSaveNewPin() {
    pinChangeError = "";
    if (!/^\d{6}$/.test(newPin.trim())) {
      pinChangeError = "PIN must be exactly 6 digits (numbers only).";
      return;
    }
    if (newPin.trim() !== confirmPin.trim()) {
      pinChangeError = "PINs do not match. Please re-enter.";
      return;
    }

    try {
      hostPin = newPin.trim();
      await saveRemoteDesktopSettings(hostEnabled, hostPin, hostUnattended);
      pinChangeSuccess = true;
      setTimeout(() => {
        closeChangePinModal();
        pinChangeSuccess = false;
      }, 1200);
    } catch (err: any) {
      pinChangeError = `Failed to update PIN: ${err?.message || err}`;
    }
  }

  async function handleToggleScreenCapture() {
    if ($isHostSharingScreen) {
      stopHostScreenCapture();
    } else {
      await startHostScreenCapture();
    }
  }

  function handleConnect() {
    if (!selectedTargetDeviceId) {
      alert("Please select or enter a target device first.");
      return;
    }
    // Connect with PIN (direct auto-connect) or without PIN (requires host approval)
    connectToRemoteHost(selectedTargetDeviceId, inputPin.trim());
  }

  function toggleFullscreen() {
    if (!viewerContainerEl) return;
    if (!document.fullscreenElement) {
      viewerContainerEl.requestFullscreen().catch((err) => {
        console.warn("Fullscreen request error:", err);
      });
    } else {
      document.exitFullscreen().catch((err) => {
        console.warn("Exit fullscreen error:", err);
      });
    }
  }

  // Mouse & Keyboard Input Event Handlers on Remote Video Canvas
  function handleMouseMove(e: MouseEvent) {
    if (!videoEl || !isClientConnected) return;

    const now = performance.now();
    // Throttle to ~60 updates per second
    if (now - lastMouseMoveTime < 16) return;
    lastMouseMoveTime = now;

    const rect = videoEl.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));

    sendInputToHost({
      t: "mm",
      x,
      y,
    });
  }

  function handleMouseDown(e: MouseEvent) {
    if (!isClientConnected) return;
    const button =
      e.button === 0 ? "left" : e.button === 2 ? "right" : "middle";
    sendInputToHost({
      t: "mc",
      b: button,
      a: "down",
      alt: e.altKey,
      ctrl: e.ctrlKey,
      shift: e.shiftKey,
      meta: e.metaKey,
    });
  }

  function handleMouseUp(e: MouseEvent) {
    if (!isClientConnected) return;
    const button =
      e.button === 0 ? "left" : e.button === 2 ? "right" : "middle";
    sendInputToHost({
      t: "mc",
      b: button,
      a: "up",
      alt: e.altKey,
      ctrl: e.ctrlKey,
      shift: e.shiftKey,
      meta: e.metaKey,
    });
  }

  function handleWindowBlur() {
    if (!isClientConnected) return;
    // Release all modifiers when viewer window loses focus so Alt/Ctrl never get stuck on host
    sendInputToHost({
      t: "release_modifiers",
    });
  }

  function handleWheel(e: WheelEvent) {
    if (!isClientConnected) return;
    e.preventDefault();
    const dy = e.deltaY > 0 ? 120 : -120;
    sendInputToHost({
      t: "mw",
      dy,
    });
  }

  function handleContextMenu(e: MouseEvent) {
    // Prevent browser default context menu so right-click operates on remote host
    e.preventDefault();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!isClientConnected) return;
    // Don't intercept if user is typing in a local input field
    const target = e.target as HTMLElement;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA"))
      return;

    // Send key down to host
    sendInputToHost({
      t: "kk",
      k: e.key,
      c: e.code,
      a: "down",
    });

    // In Fullscreen mode: intercept and prevent default for ALL shortcuts
    // (Alt+Tab, Windows key/Meta, PrintScreen, F-keys, etc.) so host handles them!
    if (isFullscreen) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    // Windowed mode: prevent common browser defaults
    if (
      ["Tab", "F5", "Alt", "Meta"].includes(e.key) ||
      (e.ctrlKey && ["w", "r", "t"].includes(e.key.toLowerCase()))
    ) {
      e.preventDefault();
    }
  }

  function handleKeyUp(e: KeyboardEvent) {
    if (!isClientConnected) return;
    const target = e.target as HTMLElement;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA"))
      return;

    sendInputToHost({
      t: "kk",
      k: e.key,
      c: e.code,
      a: "up",
    });

    if (isFullscreen) {
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<svelte:window on:keydown={handleKeyDown} on:keyup={handleKeyUp} on:blur={handleWindowBlur} />

<div
  class="h-full w-full flex flex-col bg-slate-950 text-slate-100 overflow-hidden relative select-none"
>
  {#if isClientConnected}
    <!-- Active Remote Desktop Viewer Screen -->
    <div
      bind:this={viewerContainerEl}
      class="flex-1 w-full h-full relative flex flex-col bg-black overflow-hidden"
    >
      <!-- Tiny Top Arrow Trigger Handle (Click-to-expand, tiny footprint so it never blocks apps) -->
      <button
        type="button"
        on:click={() => (showToolbar = !showToolbar)}
        class="absolute top-0 left-1/2 -translate-x-1/2 w-12 h-3.5 z-50 bg-slate-900/90 hover:bg-slate-800 text-slate-400 hover:text-cyan-400 border-x border-b border-slate-700/80 rounded-b-md flex items-center justify-center cursor-pointer shadow-md transition-all group"
        title={showToolbar ? "Hide remote controls" : "Show remote controls"}
      >
        {#if showToolbar}
          <ChevronUp class="w-3 h-3 text-cyan-400 transition-transform" />
        {:else}
          <ChevronDown class="w-3 h-3 group-hover:translate-y-0.5 transition-transform" />
        {/if}
      </button>

      <!-- Click-Expanded Floating Top Control Bar (Clean, non-wrapping, generous spacing) -->
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="absolute top-4 left-1/2 -translate-x-1/2 z-40 transition-all duration-300 ease-out {showToolbar
          ? 'translate-y-0 opacity-100 pointer-events-auto'
          : '-translate-y-16 opacity-0 pointer-events-none'}"
      >
        <div
          class="flex items-center gap-3 px-3.5 py-1.5 rounded-xl bg-slate-900/95 border border-slate-700/80 shadow-2xl backdrop-blur-md text-xs whitespace-nowrap"
        >
          <!-- Host & Latency Info -->
          <div class="flex items-center gap-2 pr-3 border-r border-slate-800 shrink-0">
            <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse shadow-[0_0_8px_rgba(52,211,153,0.6)]"></span>
            <span
              class="font-semibold text-white max-w-[140px] truncate"
              title={$activeRemoteSession?.peerAlias}
            >
              {$activeRemoteSession?.peerAlias}
            </span>
            <span
              class="text-[11px] font-mono px-2 py-0.5 rounded-md bg-slate-800/90 text-cyan-300 border border-slate-700 shrink-0 whitespace-nowrap flex items-center gap-1"
            >
              ⚡ {$remoteLatencyMs} ms
            </span>
          </div>

          <!-- Clipboard Actions (Bidirectional Text & Image) -->
          <div class="flex items-center gap-1.5 pr-3 border-r border-slate-800 shrink-0">
            <button
              type="button"
              on:click={sendLocalClipboardToHost}
              class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white transition flex items-center gap-1.5 cursor-pointer whitespace-nowrap font-medium"
              title="Send your local clipboard (Text or Image) to Host PC"
            >
              <ArrowUpFromLine class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span>Send Clipboard</span>
            </button>
            <button
              type="button"
              on:click={requestClipboardFromHost}
              class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white transition flex items-center gap-1.5 cursor-pointer whitespace-nowrap font-medium"
              title="Get clipboard contents from Host PC to your local computer"
            >
              <ArrowDownToLine class="w-3.5 h-3.5 text-indigo-400 shrink-0" />
              <span>Get Clipboard</span>
            </button>
          </div>

          <!-- Shortcuts Dropdown / Quick Buttons -->
          <div class="flex items-center gap-1 pr-3 border-r border-slate-800 shrink-0">
            <button
              type="button"
              on:click={() => sendShortcutToHost("win_d")}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white text-[11px] font-mono transition"
              title="Show Host Desktop (Win + D)"
            >
              Desktop
            </button>
            <button
              type="button"
              on:click={() => sendShortcutToHost("ctrl_shift_esc")}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white text-[11px] font-mono transition"
              title="Open Task Manager on Host (Ctrl + Shift + Esc)"
            >
              TaskMgr
            </button>
            <button
              type="button"
              on:click={() => sendShortcutToHost("win_l")}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-rose-400 text-[11px] font-mono transition"
              title="Lock Host PC (Win + L)"
            >
              Lock
            </button>
          </div>

          <!-- Display & Window Controls -->
          <div class="flex items-center gap-1.5 shrink-0">
            <!-- Toggle Local Cursor Mode (Client Cursor vs Hidden) -->
            <button
              type="button"
              on:click={() => (hideLocalCursor = !hideLocalCursor)}
              class="p-1 rounded-lg transition cursor-pointer {!hideLocalCursor
                ? 'text-cyan-400 bg-slate-800 border border-cyan-500/30'
                : 'text-slate-400 hover:text-white hover:bg-slate-800'}"
              title={!hideLocalCursor
                ? "Client Cursor Active (Responsive 0ms local pointer)"
                : "Client Cursor Hidden"}
            >
              <MousePointer class="w-3.5 h-3.5" />
            </button>

            <!-- Fullscreen Button -->
            <button
              type="button"
              on:click={toggleFullscreen}
              class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
              title={isFullscreen ? "Exit Fullscreen" : "Fullscreen"}
            >
              {#if isFullscreen}
                <Minimize class="w-3.5 h-3.5" />
              {:else}
                <Maximize class="w-3.5 h-3.5" />
              {/if}
            </button>

            <!-- Disconnect Button -->
            <button
              type="button"
              on:click={disconnectRemoteSession}
              class="px-2.5 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-semibold transition flex items-center gap-1.5 cursor-pointer active:scale-95 whitespace-nowrap"
              title="Disconnect Remote Session"
            >
              <LogOut class="w-3.5 h-3.5 shrink-0" />
              <span>Disconnect</span>
            </button>

            <!-- Collapse Toolbar Button -->
            <button
              type="button"
              on:click={() => (showToolbar = false)}
              class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition ml-0.5 cursor-pointer"
              title="Hide toolbar"
            >
              <ChevronUp class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- Main Video Display Canvas -->
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="flex-1 w-full h-full flex items-center justify-center bg-black overflow-hidden relative {hideLocalCursor
          ? 'cursor-none'
          : 'cursor-default'}"
        on:mousemove={handleMouseMove}
        on:mousedown={handleMouseDown}
        on:mouseup={handleMouseUp}
        on:wheel={handleWheel}
        on:contextmenu={handleContextMenu}
      >
        <video
          bind:this={videoEl}
          use:attachStream={$clientRemoteStream}
          autoplay
          playsinline
          muted
          class="max-w-full max-h-full object-contain pointer-events-none select-none shadow-2xl"
        ></video>
      </div>
    </div>
  {:else}
    <!-- Remote Desktop Full-Page Device List Dashboard -->
    <div
      class="flex-1 p-4 sm:p-6 overflow-y-auto max-w-6xl w-full mx-auto space-y-6"
    >
      <!-- Header Banner & Action Buttons -->
      <div
        class="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 pb-4 border-b border-slate-800"
      >
        <div class="flex items-center gap-3">
          <div
            class="w-12 h-12 rounded-2xl bg-gradient-to-br from-cyan-500/20 to-indigo-600/30 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shadow-xl shadow-cyan-500/10"
          >
            <Monitor class="w-6 h-6 stroke-[1.8]" />
          </div>
          <div>
            <h2 class="text-lg font-bold text-white flex items-center gap-2">
              <span>Raydrop Remote Desktop</span>
              <span
                class="px-2 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 font-mono text-[10px] border border-cyan-500/30"
              >
                P2P Ultra-Low Latency
              </span>
            </h2>
            <p class="text-xs text-slate-400 mt-0.5">
              Click any device below to connect with PIN or request access.
            </p>
          </div>
        </div>

        <div class="flex items-center gap-2.5 self-end sm:self-auto flex-wrap">
          {#if isHostActive}
            <div
              class="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-semibold"
            >
              <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"
              ></span>
              <span
                >Host Active ({$activeRemoteSession?.peerAlias || "Remote User"})</span
              >
              <button
                type="button"
                on:click={disconnectRemoteSession}
                class="ml-1.5 px-2 py-0.5 rounded bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/30 text-[11px]"
              >
                Stop
              </button>
            </div>
          {/if}

          <!-- Connect by ID Quick Button -->
          <button
            type="button"
            on:click={openManualConnectModal}
            class="px-3 py-2 rounded-xl bg-slate-800/90 hover:bg-slate-700 text-slate-200 border border-slate-700 text-xs font-semibold transition flex items-center gap-1.5 cursor-pointer active:scale-95 shadow-sm"
          >
            <Plus class="w-3.5 h-3.5 text-cyan-400" />
            <span>Connect by ID</span>
          </button>

          <!-- Host Settings Modal Trigger -->
          <button
            type="button"
            on:click={() => (isHostSettingsModalOpen = true)}
            class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-white border border-slate-700 text-xs font-semibold transition flex items-center gap-2 cursor-pointer shadow-sm active:scale-95"
            title="Configure Host Remote Access, Device ID & PIN"
          >
            <Shield
              class="w-4 h-4 {hostEnabled ? 'text-indigo-400' : 'text-slate-400'}"
            />
            <span>Host Settings</span>
            <span
              class="w-2 h-2 rounded-full {hostEnabled
                ? 'bg-emerald-400 animate-pulse'
                : 'bg-slate-600'}"
            ></span>
          </button>
        </div>
      </div>

      <!-- Error Alert Banner -->
      {#if $remoteErrorMessage}
        <div
          class="p-4 rounded-xl bg-rose-950/70 border border-rose-800/80 flex items-center justify-between gap-3 text-rose-200 text-xs shadow-lg"
        >
          <div class="flex items-center gap-2.5">
            <AlertCircle class="w-5 h-5 text-rose-400 shrink-0" />
            <span>{$remoteErrorMessage}</span>
          </div>
          <button
            type="button"
            on:click={() => remoteErrorMessage.set(null)}
            class="p-1 rounded text-rose-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {/if}

      <!-- Search Bar & Category Filter Tabs -->
      <div
        class="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3"
      >
        <!-- Filter Pills -->
        <div
          class="flex items-center gap-1.5 p-1 bg-slate-900/80 border border-slate-800 rounded-xl overflow-x-auto"
        >
          <button
            type="button"
            on:click={() => (filterType = "all")}
            class="px-3 py-1.5 rounded-lg text-xs font-semibold transition cursor-pointer {filterType ===
            'all'
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
              : 'text-slate-400 hover:text-slate-200'}"
          >
            All Devices ({availablePeers.length})
          </button>
          <button
            type="button"
            on:click={() => (filterType = "cloud")}
            class="px-3 py-1.5 rounded-lg text-xs font-semibold transition cursor-pointer {filterType ===
            'cloud'
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
              : 'text-slate-400 hover:text-slate-200'}"
          >
            Cloud Devices
          </button>
          <button
            type="button"
            on:click={() => (filterType = "lan")}
            class="px-3 py-1.5 rounded-lg text-xs font-semibold transition cursor-pointer {filterType ===
            'lan'
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
              : 'text-slate-400 hover:text-slate-200'}"
          >
            LAN Radar
          </button>
          <button
            type="button"
            on:click={() => (filterType = "contacts")}
            class="px-3 py-1.5 rounded-lg text-xs font-semibold transition cursor-pointer {filterType ===
            'contacts'
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
              : 'text-slate-400 hover:text-slate-200'}"
          >
            Contacts
          </button>
        </div>

        <!-- Search Input -->
        <div class="relative w-full sm:w-64">
          <Search
            class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-slate-500"
          />
          <input
            type="text"
            bind:value={searchQuery}
            placeholder="Search devices or ID..."
            class="w-full pl-9 pr-8 py-2 rounded-xl bg-slate-900/80 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500 transition"
          />
          {#if searchQuery}
            <button
              type="button"
              on:click={() => (searchQuery = "")}
              class="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-400 hover:text-white"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          {/if}
        </div>
      </div>

      <!-- Full-Page Device Cards Grid -->
      {#if filteredPeers.length === 0}
        <div
          class="p-12 rounded-2xl bg-slate-900/40 border border-slate-800 text-center space-y-3"
        >
          <div
            class="w-14 h-14 mx-auto rounded-2xl bg-slate-800/80 flex items-center justify-center text-slate-500"
          >
            <Laptop class="w-7 h-7" />
          </div>
          <div class="space-y-1">
            <h3 class="text-sm font-semibold text-slate-300">
              {searchQuery
                ? "No matching devices found"
                : "No reachable remote devices detected"}
            </h3>
            <p class="text-xs text-slate-500 max-w-md mx-auto">
              {searchQuery
                ? `No devices match "${searchQuery}". Try a different name or device ID.`
                : "Make sure other devices are signed into the same cloud account, on the same local Wi-Fi/LAN, or connect directly by Device ID."}
            </p>
          </div>
          <div class="pt-2">
            <button
              type="button"
              on:click={openManualConnectModal}
              class="px-4 py-2 rounded-xl bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/40 text-xs font-semibold transition inline-flex items-center gap-1.5 cursor-pointer active:scale-95"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Connect by Device ID</span>
            </button>
          </div>
        </div>
      {:else}
        <div
          class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4"
        >
          {#each filteredPeers as peer}
            <!-- Device Card with User Avatar -->
            <!-- svelte-ignore a11y-click-events-have-key-events -->
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div
              on:click={() => openConnectModalForPeer(peer)}
              class="p-4 rounded-2xl bg-slate-900/60 hover:bg-slate-800/80 border border-slate-800/90 hover:border-cyan-500/50 transition-all duration-200 cursor-pointer shadow-lg hover:shadow-cyan-500/10 group flex flex-col justify-between space-y-3 relative overflow-hidden"
            >
              <!-- Card Top: Avatar with Online Badge + Device Type Pill -->
              <div class="flex items-start justify-between gap-3">
                <div class="relative">
                  {#if $peerAvatars[peer.id]}
                    <img
                      src={$peerAvatars[peer.id]}
                      alt={peer.name}
                      class="w-12 h-12 rounded-full object-cover border-2 border-slate-700 group-hover:border-cyan-400 transition shadow-md"
                    />
                  {:else}
                    <div
                      class="w-12 h-12 rounded-full bg-gradient-to-br from-cyan-600 to-indigo-700 flex items-center justify-center text-white font-bold text-base border-2 border-slate-700 group-hover:border-cyan-400 transition shadow-md select-none"
                    >
                      {peer.name.charAt(0).toUpperCase()}
                    </div>
                  {/if}

                  <!-- Online indicator pill on avatar -->
                  <span
                    class="absolute bottom-0 right-0 w-3.5 h-3.5 rounded-full border-2 border-slate-900 {peer.isOnline
                      ? 'bg-emerald-400'
                      : 'bg-slate-600'}"
                    title={peer.isOnline ? "Online" : "Offline"}
                  ></span>
                </div>

                <!-- Device Type Pill & Status -->
                <div class="flex flex-col items-end gap-1">
                  <span
                    class="px-2 py-0.5 rounded-md text-[10px] font-semibold border {peer.isOnline
                      ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30'
                      : 'bg-slate-800 text-slate-300 border-slate-700'}"
                    title={peer.isOnline ? "Online now" : formatLastOnline(peer.lastSeenAt, false)}
                  >
                    {peer.isOnline ? "Online" : formatLastOnlineCompact(peer.lastSeenAt, false)}
                  </span>
                  <span
                    class="text-[10px] text-slate-400 flex items-center gap-1"
                  >
                    {#if peer.type === "My Cloud Device"}
                      <Laptop class="w-3 h-3 text-cyan-400" />
                    {:else if peer.type === "LAN Peer"}
                      <Radio class="w-3 h-3 text-emerald-400" />
                    {:else}
                      <Laptop class="w-3 h-3 text-indigo-400" />
                    {/if}
                    <span class="truncate max-w-[90px]">{peer.type}</span>
                  </span>
                </div>
              </div>

              <!-- Card Middle: Device Name, ID & Last Online -->
              <div class="space-y-1">
                <h4
                  class="text-sm font-bold text-white group-hover:text-cyan-300 transition truncate"
                  title={peer.name}
                >
                  {peer.name}
                </h4>
                <div class="flex items-center justify-between text-[11px] gap-2">
                  <p
                    class="font-mono text-slate-400 group-hover:text-slate-300 transition truncate"
                    title={peer.id}
                  >
                    {peer.id}
                  </p>
                  <span
                    class="text-[10px] {peer.isOnline ? 'text-emerald-400' : 'text-slate-400'} shrink-0 truncate font-sans"
                  >
                    {peer.isOnline ? "Online now" : formatLastOnline(peer.lastSeenAt, false)}
                  </span>
                </div>
              </div>

              <!-- Card Bottom: Connect Action -->
              <div
                class="pt-2 border-t border-slate-800/80 flex items-center justify-between text-xs"
              >
                <span
                  class="text-[11px] text-slate-500 group-hover:text-cyan-400 font-medium transition flex items-center gap-1"
                >
                  <Key class="w-3 h-3" />
                  <span>Connect with PIN</span>
                </span>
                <div
                  class="w-7 h-7 rounded-lg bg-cyan-500/10 group-hover:bg-cyan-500 text-cyan-400 group-hover:text-slate-950 flex items-center justify-center transition shadow-sm"
                >
                  <Play class="w-3.5 h-3.5 fill-current ml-0.5" />
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Connect PIN Modal (Triggered by clicking device card or manual connect) -->
  {#if isConnectPinModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-sm rounded-2xl bg-slate-900 border border-cyan-500/40 p-5 shadow-2xl space-y-4"
      >
        <!-- Modal Header -->
        <div
          class="flex items-center justify-between pb-3 border-b border-slate-800"
        >
          <div class="flex items-center gap-3">
            {#if selectedPeer && $peerAvatars[selectedPeer.id]}
              <img
                src={$peerAvatars[selectedPeer.id]}
                alt={selectedPeer.name}
                class="w-10 h-10 rounded-full object-cover border border-cyan-500/40"
              />
            {:else}
              <div
                class="w-10 h-10 rounded-full bg-gradient-to-br from-cyan-600 to-indigo-700 flex items-center justify-center text-white font-bold text-sm"
              >
                {selectedPeer?.name?.charAt(0).toUpperCase() || "R"}
              </div>
            {/if}
            <div class="overflow-hidden">
              <h3 class="text-sm font-bold text-white truncate max-w-[200px]">
                {selectedPeer?.name || "Remote Host"}
              </h3>
              <p
                class="text-[11px] font-mono text-cyan-300 truncate max-w-[200px]"
              >
                {selectedPeer?.id || selectedTargetDeviceId}
              </p>
              {#if selectedPeer}
                <p class="text-[10px] {selectedPeer.isOnline ? 'text-emerald-400 font-medium' : 'text-slate-400'} truncate">
                  {selectedPeer.isOnline ? "🟢 Online now" : `⚪ ${formatLastOnline(selectedPeer.lastSeenAt, false)}`}
                </p>
              {/if}
            </div>
          </div>
          <button
            type="button"
            on:click={() => (isConnectPinModalOpen = false)}
            class="p-1 rounded text-slate-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Enter PIN Form with immediate ENTER submission -->
        <form
          on:submit|preventDefault={() => handleModalConnect(true)}
          class="space-y-4"
        >
          <div>
            <div class="flex items-center justify-between mb-1.5">
              <label
                for="rd-modal-pin"
                class="block text-xs font-semibold text-slate-300"
              >
                Host Security PIN:
              </label>
              <span class="text-[10px] text-cyan-400 font-medium">
                Press Enter to Connect
              </span>
            </div>
            <!-- svelte-ignore a11y-autofocus -->
            <input
              id="rd-modal-pin"
              type="password"
              maxlength="6"
              inputmode="numeric"
              autofocus
              bind:value={inputPin}
              on:keydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  handleModalConnect(true);
                }
              }}
              placeholder="••••••"
              class="w-full px-4 py-3 rounded-xl bg-slate-950 border border-slate-700 text-base text-white placeholder-slate-600 font-mono tracking-[0.4em] focus:outline-none focus:border-cyan-400 focus:ring-1 focus:ring-cyan-400 transition text-center shadow-inner"
            />
            <p class="text-[10px] text-slate-500 mt-1.5 leading-relaxed">
              Enter host's 6-digit PIN for instant unattended access. Press
              <b>Enter</b> to connect immediately without clicking.
            </p>
          </div>

          <!-- Status indicator if currently connecting -->
          {#if $remoteConnectionStatus === "verifying_pin" || $remoteConnectionStatus === "connecting"}
            <div
              class="p-2.5 rounded-xl bg-cyan-950/60 border border-cyan-800/80 flex items-center gap-2 text-cyan-300 text-xs"
            >
              <RefreshCw class="w-4 h-4 animate-spin text-cyan-400 shrink-0" />
              <span>
                {$remoteConnectionStatus === "verifying_pin"
                  ? "Verifying PIN & authenticating..."
                  : "Connecting WebRTC stream..."}
              </span>
            </div>
          {/if}

          <!-- Action Buttons -->
          <div class="space-y-2 pt-2 border-t border-slate-800">
            <button
              type="submit"
              disabled={$remoteConnectionStatus === "verifying_pin" ||
                $remoteConnectionStatus === "connecting"}
              class="w-full py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 disabled:opacity-50 text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition flex items-center justify-center gap-2 cursor-pointer active:scale-95"
            >
              {#if $remoteConnectionStatus === "verifying_pin" || $remoteConnectionStatus === "connecting"}
                <RefreshCw class="w-4 h-4 animate-spin" />
                <span>Connecting...</span>
              {:else}
                <Play class="w-4 h-4 fill-white" />
                <span>Connect with PIN (Instant)</span>
              {/if}
            </button>

            <div class="flex items-center gap-2">
              <button
                type="button"
                on:click={() => handleModalConnect(false)}
                disabled={$remoteConnectionStatus === "verifying_pin" ||
                  $remoteConnectionStatus === "connecting"}
                class="flex-1 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-300 text-xs font-semibold border border-slate-700 transition cursor-pointer"
                title="Connect without PIN and ask host user to accept permission"
              >
                Request Approval
              </button>
              <button
                type="button"
                on:click={() => (isConnectPinModalOpen = false)}
                class="px-3.5 py-2 rounded-xl bg-slate-900 hover:bg-slate-800 text-slate-400 hover:text-white text-xs border border-slate-800 transition cursor-pointer"
              >
                Cancel
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  {/if}

  <!-- Host Settings Modal (Allow Remote Access, Device ID, PIN) -->
  {#if isHostSettingsModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-md rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
      >
        <!-- Modal Header -->
        <div
          class="flex items-center justify-between pb-3 border-b border-slate-800"
        >
          <div class="flex items-center gap-2.5">
            <Shield class="w-5 h-5 text-indigo-400" />
            <div>
              <h3 class="text-sm font-bold text-white">
                Host Remote Access Settings
              </h3>
              <p class="text-[11px] text-slate-400">
                Configure remote control permissions and security PIN
              </p>
            </div>
          </div>
          <button
            type="button"
            on:click={() => (isHostSettingsModalOpen = false)}
            class="p-1 rounded text-slate-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <div class="space-y-4">
          <!-- Allow Remote Access Toggle -->
          <div
            class="flex items-center justify-between p-3 rounded-xl bg-slate-950 border border-slate-800"
          >
            <div>
              <span class="block text-xs font-semibold text-white">
                Allow Remote Access
              </span>
              <p class="text-[11px] text-slate-400">
                Enable remote control of this computer by authorized peers
              </p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                bind:checked={hostEnabled}
                on:change={handleToggleHostEnabled}
                class="sr-only peer"
              />
              <div
                class="w-9 h-5 bg-slate-800 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-cyan-500"
              ></div>
            </label>
          </div>

          <!-- Host Device ID -->
          <div>
            <span class="block text-xs font-semibold text-slate-300 mb-1"
              >Your Device ID:</span
            >
            <div
              class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950 border border-slate-800 font-mono text-xs text-cyan-300"
            >
              <span>{$appSettings.device_id}</span>
              <button
                type="button"
                on:click={() => {
                  navigator.clipboard.writeText($appSettings.device_id);
                  alert("Device ID copied to clipboard!");
                }}
                class="p-1 text-slate-400 hover:text-white"
                title="Copy Device ID"
              >
                <Copy class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <!-- Host Security PIN -->
          <div>
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs font-semibold text-slate-300"
                >Security PIN:</span
              >
              <span
                class="text-[11px] text-emerald-400 flex items-center gap-1"
              >
                <Lock class="w-3 h-3" />
                Protected & Masked
              </span>
            </div>

            <div
              class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950 border border-slate-800"
            >
              <div class="flex items-center gap-2">
                <span
                  class="font-mono text-base tracking-[0.35em] text-slate-300 select-none"
                  >••••••</span
                >
                <span class="text-[10px] text-slate-500 italic"
                  >(Always hidden)</span
                >
              </div>
              <button
                type="button"
                on:click={openChangePinModal}
                class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-cyan-300 font-semibold border border-slate-700 flex items-center gap-1.5 transition cursor-pointer active:scale-95"
              >
                <Key class="w-3.5 h-3.5" />
                <span>Change PIN</span>
              </button>
            </div>
            <p class="text-[10px] text-slate-500 mt-1">
              Clients with this PIN can connect instantly without prompting you.
            </p>
          </div>

          <!-- Host Screen Sharing -->
          <div>
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs font-semibold text-slate-300"
                >Host Screen Sharing:</span
              >
              <span
                class="text-[11px] {$isHostSharingScreen
                  ? 'text-emerald-400'
                  : 'text-slate-400'} flex items-center gap-1"
              >
                <span
                  class="w-2 h-2 rounded-full {$isHostSharingScreen
                    ? 'bg-emerald-400 animate-pulse'
                    : 'bg-slate-600'}"
                ></span>
                {$isHostSharingScreen ? "Active & Ready" : "Standby"}
              </span>
            </div>
            <div
              class="flex items-center justify-between p-2.5 rounded-xl bg-slate-950 border border-slate-800"
            >
              <div class="text-[11px] text-slate-400">
                {#if $isHostSharingScreen}
                  Screen capture active. Clients connect seamlessly.
                {:else}
                  Warm up capture for instant unattended remote control.
                {/if}
              </div>
              <button
                type="button"
                on:click={handleToggleScreenCapture}
                class="ml-2 px-2.5 py-1 rounded-lg text-xs font-semibold border transition flex items-center gap-1.5 cursor-pointer {$isHostSharingScreen
                  ? 'bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border-rose-500/40'
                  : 'bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border-cyan-500/40'}"
              >
                {#if $isHostSharingScreen}
                  <Square class="w-3 h-3 fill-rose-300" />
                  <span>Stop Capture</span>
                {:else}
                  <MonitorPlay class="w-3.5 h-3.5 text-cyan-300" />
                  <span>Start Capture</span>
                {/if}
              </button>
            </div>
          </div>

          <!-- Security Info Banner -->
          <div
            class="p-3 rounded-xl bg-slate-950/70 border border-slate-800 text-xs text-slate-400 space-y-1.5"
          >
            <div
              class="flex items-center gap-1.5 text-slate-200 font-semibold"
            >
              <Lock class="w-3.5 h-3.5 text-emerald-400" />
              <span>P2P End-to-End Encrypted (Zero-Cloud)</span>
            </div>
            <p class="text-[11px] leading-relaxed">
              Remote desktop sessions are direct peer-to-peer and encrypted
              end-to-end. No video, keystrokes, or screen data are stored on
              any server.
            </p>
          </div>
        </div>

        <div class="pt-2 border-t border-slate-800 flex justify-end">
          <button
            type="button"
            on:click={() => (isHostSettingsModalOpen = false)}
            class="px-4 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-white text-xs font-semibold transition"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Manual Connect by ID Modal -->
  {#if isManualConnectModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-sm rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
      >
        <div
          class="flex items-center justify-between pb-2 border-b border-slate-800"
        >
          <div class="flex items-center gap-2 text-white font-bold text-sm">
            <Laptop class="w-4 h-4 text-cyan-400" />
            <span>Connect by Device ID</span>
          </div>
          <button
            type="button"
            on:click={() => (isManualConnectModalOpen = false)}
            class="p-1 rounded text-slate-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <form on:submit|preventDefault={proceedManualToPin} class="space-y-3">
          <div>
            <label
              for="rd-manual-target-id"
              class="block text-xs font-semibold text-slate-300 mb-1"
            >
              Enter Host Device ID:
            </label>
            <input
              id="rd-manual-target-id"
              type="text"
              autofocus
              bind:value={manualDeviceIdInput}
              placeholder="RAY-XXXX-XXXX-XXXX"
              class="w-full px-3 py-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono focus:outline-none focus:border-cyan-500 transition"
            />
          </div>

          <div
            class="flex items-center justify-end gap-2 pt-2 border-t border-slate-800"
          >
            <button
              type="button"
              on:click={() => (isManualConnectModalOpen = false)}
              class="px-3 py-1.5 rounded-xl text-xs text-slate-400 hover:text-white hover:bg-slate-800 transition"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!manualDeviceIdInput.trim()}
              class="px-4 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-slate-950 font-bold text-xs shadow transition active:scale-95"
            >
              Next: Enter PIN
            </button>
          </div>
        </form>
      </div>
    </div>
  {/if}

  <!-- Incoming Remote Access Approval Modal (for requests without PIN) -->
  {#if $pendingRemoteAuthRequest}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-sm rounded-2xl bg-slate-900 border border-cyan-500/40 p-5 shadow-2xl space-y-4"
      >
        <div class="flex items-center gap-3 pb-3 border-b border-slate-800">
          <div
            class="w-10 h-10 rounded-xl bg-cyan-500/20 border border-cyan-500/30 flex items-center justify-center text-cyan-400"
          >
            <Laptop class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-sm font-bold text-white">Remote Access Request</h3>
            <p class="text-xs text-cyan-300 font-semibold">
              {$pendingRemoteAuthRequest.fromAlias}
            </p>
          </div>
        </div>

        <p class="text-xs text-slate-300 leading-relaxed">
          This device wants to remotely view and control your computer. Since no
          PIN was entered, your explicit approval is required.
        </p>

        <div
          class="flex items-center justify-end gap-2.5 pt-2 border-t border-slate-800"
        >
          <button
            type="button"
            on:click={rejectPendingRemoteRequest}
            class="px-3.5 py-1.5 rounded-xl text-xs font-semibold text-rose-300 bg-rose-500/10 hover:bg-rose-500/20 border border-rose-500/30 transition cursor-pointer"
          >
            Decline
          </button>
          <button
            type="button"
            on:click={acceptPendingRemoteRequest}
            class="px-4 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs shadow-lg shadow-cyan-500/20 transition cursor-pointer active:scale-95"
          >
            Allow Remote Control
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Change PIN Modal -->
  {#if isChangePinModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-sm rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
      >
        <div
          class="flex items-center justify-between pb-2 border-b border-slate-800"
        >
          <div class="flex items-center gap-2 text-white font-bold text-sm">
            <Key class="w-4 h-4 text-cyan-400" />
            <span>Set New Security PIN</span>
          </div>
          <button
            type="button"
            on:click={closeChangePinModal}
            class="p-1 rounded text-slate-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <p class="text-xs text-slate-400">
          Enter a new 6-digit numeric PIN to protect this computer. For
          security, your PIN will never be visible on screen.
        </p>

        {#if pinChangeError}
          <div
            class="p-2.5 rounded-xl bg-rose-950/70 border border-rose-800/80 text-rose-300 text-xs flex items-center gap-2"
          >
            <AlertCircle class="w-4 h-4 shrink-0" />
            <span>{pinChangeError}</span>
          </div>
        {/if}

        {#if pinChangeSuccess}
          <div
            class="p-2.5 rounded-xl bg-emerald-950/70 border border-emerald-800/80 text-emerald-300 text-xs flex items-center gap-2"
          >
            <Check class="w-4 h-4 shrink-0 text-emerald-400" />
            <span>New Security PIN saved successfully!</span>
          </div>
        {/if}

        <div class="space-y-3">
          <div>
            <label
              for="rd-new-pin"
              class="block text-xs font-semibold text-slate-300 mb-1"
            >
              New 6-Digit PIN:
            </label>
            <input
              id="rd-new-pin"
              type="password"
              maxlength="6"
              inputmode="numeric"
              bind:value={newPin}
              placeholder="••••••"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition text-center sm:text-left"
            />
          </div>

          <div>
            <label
              for="rd-confirm-pin"
              class="block text-xs font-semibold text-slate-300 mb-1"
            >
              Confirm New PIN:
            </label>
            <input
              id="rd-confirm-pin"
              type="password"
              maxlength="6"
              inputmode="numeric"
              bind:value={confirmPin}
              placeholder="••••••"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition text-center sm:text-left"
            />
          </div>

          <button
            type="button"
            on:click={generateRandomPinInModal}
            class="text-[11px] text-cyan-400 hover:text-cyan-300 transition flex items-center gap-1 cursor-pointer"
          >
            <Sparkles class="w-3.5 h-3.5" />
            <span>Generate Random 6-Digit PIN</span>
          </button>
        </div>

        <div
          class="flex items-center justify-end gap-2 pt-2 border-t border-slate-800"
        >
          <button
            type="button"
            on:click={closeChangePinModal}
            class="px-3 py-1.5 rounded-xl text-xs text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            Cancel
          </button>
          <button
            type="button"
            on:click={handleSaveNewPin}
            class="px-4 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs shadow transition active:scale-95"
          >
            Save PIN
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
