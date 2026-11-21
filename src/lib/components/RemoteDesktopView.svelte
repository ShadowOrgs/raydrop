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

  let selectedTargetDeviceId = "";
  let inputPin = "";

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

  // Svelte action to reliably attach MediaStream to video DOM element with autoplay handling
  function attachStream(node: HTMLVideoElement, stream: MediaStream | null) {
    node.srcObject = stream;
    if (stream) {
      node.muted = true;
      const playPromise = node.play();
      if (playPromise !== undefined) {
        playPromise.catch(() => {
          node.muted = true;
          node.play().catch(() => {});
        });
      }
    }
    return {
      update(newStream: MediaStream | null) {
        if (node.srcObject !== newStream) {
          node.srcObject = newStream;
        }
        if (newStream) {
          node.muted = true;
          const playPromise = node.play();
          if (playPromise !== undefined) {
            playPromise.catch(() => {
              node.muted = true;
              node.play().catch(() => {});
            });
          }
        }
      },
      destroy() {
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
      });
    }

    return list;
  })();

  onMount(async () => {
    initRemoteDesktopListener();
    await loadRemoteDesktopSettings();
    hostEnabled = $remoteHostSettings.enabled;
    hostPin = $remoteHostSettings.pin;
    hostUnattended = $remoteHostSettings.unattended;

    document.addEventListener("fullscreenchange", handleFullscreenChange);
  });

  onDestroy(() => {
    document.removeEventListener("fullscreenchange", handleFullscreenChange);
  });

  function handleFullscreenChange() {
    isFullscreen = !!document.fullscreenElement;
    if (isFullscreen) {
      // Auto hide toolbar on fullscreen enter for clean view
      showToolbar = false;
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

    // Prevent default browser behavior for common shortcuts (like F5 reload or Alt+Tab)
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
    <!-- Remote Desktop Dashboard (Host & Connect Client Views) -->
    <div
      class="flex-1 p-4 sm:p-6 overflow-y-auto max-w-6xl w-full mx-auto space-y-6"
    >
      <!-- Header Banner -->
      <div
        class="flex items-center justify-between pb-4 border-b border-slate-800"
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
              Securely control remote PCs and devices with 60fps high
              resolution, encrypted PIN protection, and bidirectional clipboard
              sync.
            </p>
          </div>
        </div>

        {#if isHostActive}
          <div
            class="flex items-center gap-2.5 px-3 py-1.5 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-semibold"
          >
            <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"
            ></span>
            <span
              >Host Active: Controlled by {$activeRemoteSession?.peerAlias}</span
            >
            <button
              type="button"
              on:click={disconnectRemoteSession}
              class="ml-2 px-2 py-0.5 rounded bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/30 text-[11px]"
            >
              Stop Session
            </button>
          </div>
        {/if}
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

      <!-- Main Two Column Grid: Connect to Client (Left) + Host Settings (Right) -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <!-- 1. Connect to Remote Device (Client Mode) -->
        <div
          class="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 backdrop-blur-xl space-y-4 shadow-xl"
        >
          <div class="flex items-center gap-2.5 pb-2 border-b border-slate-800">
            <Laptop class="w-5 h-5 text-cyan-400" />
            <div>
              <h3 class="text-sm font-bold text-white">
                Control Remote Device (Client)
              </h3>
              <p class="text-[11px] text-slate-400">
                Access and control another computer or mobile device with PIN or
                host approval
              </p>
            </div>
          </div>

          <div class="space-y-3.5">
            <!-- Target Device Selector -->
            <div>
              <label
                for="rd-target-device"
                class="block text-xs font-semibold text-slate-300 mb-1.5"
              >
                Select Target Device:
              </label>
              {#if availablePeers.length === 0}
                <div
                  class="p-3 rounded-xl bg-slate-950/80 border border-slate-800 text-xs text-slate-500 text-center"
                >
                  No online devices detected in Contacts or LAN. You can enter a
                  Host Device ID manually below.
                </div>
              {:else}
                <select
                  id="rd-target-device"
                  bind:value={selectedTargetDeviceId}
                  class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500 transition"
                >
                  <option value="">-- Select Online Device --</option>
                  {#each availablePeers as peer}
                    <option value={peer.id}>
                      {peer.name} ({peer.type}) {peer.isOnline
                        ? "🟢 Online"
                        : "⚪ Offline"}
                    </option>
                  {/each}
                </select>
              {/if}
            </div>

            <!-- Manual Device ID Input -->
            <div>
              <label
                for="rd-manual-id"
                class="block text-xs font-semibold text-slate-300 mb-1"
              >
                Or Enter Host Device ID Manually:
              </label>
              <input
                id="rd-manual-id"
                type="text"
                bind:value={selectedTargetDeviceId}
                placeholder="RAY-XXXX-XXXX-XXXX"
                class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono focus:outline-none focus:border-cyan-500 transition"
              />
            </div>

            <!-- Security PIN Input (Strictly Masked) -->
            <div>
              <div class="flex items-center justify-between mb-1">
                <label
                  for="rd-host-pin"
                  class="block text-xs font-semibold text-slate-300"
                >
                  Host Security PIN:
                </label>
                <span
                  class="text-[10px] {inputPin.trim()
                    ? 'text-emerald-400 font-semibold'
                    : 'text-slate-400'}"
                >
                  {inputPin.trim()
                    ? "⚡ Instant Auto-Connect"
                    : "✋ Host Approval"}
                </span>
              </div>
              <input
                id="rd-host-pin"
                type="password"
                maxlength="6"
                inputmode="numeric"
                bind:value={inputPin}
                placeholder="•••••• (Leave blank for Host Approval)"
                class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition text-center sm:text-left"
              />
              <p class="text-[10px] text-slate-500 mt-1">
                Enter the host PIN for instant unattended access without host
                approval, or leave it blank to request manual permission.
              </p>
            </div>

            <!-- Connect Button -->
            <button
              type="button"
              on:click={handleConnect}
              disabled={$remoteConnectionStatus === "verifying_pin" ||
                $remoteConnectionStatus === "connecting"}
              class="w-full py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 disabled:opacity-50 text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition flex items-center justify-center gap-2 cursor-pointer active:scale-95"
            >
              {#if $remoteConnectionStatus === "verifying_pin"}
                <RefreshCw class="w-4 h-4 animate-spin text-white" />
                <span
                  >{inputPin.trim()
                    ? "Verifying PIN & Auto-Connecting..."
                    : "Waiting for Host Approval..."}</span
                >
              {:else if $remoteConnectionStatus === "connecting"}
                <RefreshCw class="w-4 h-4 animate-spin text-white" />
                <span>Connecting WebRTC Stream...</span>
              {:else if inputPin.trim()}
                <Play class="w-4 h-4 fill-white" />
                <span>Connect with PIN (Instant Access)</span>
              {:else}
                <Play class="w-4 h-4 fill-white" />
                <span>Request Remote Access (Host Approval)</span>
              {/if}
            </button>
          </div>
        </div>

        <!-- 2. This Device Host Configuration (Host Mode) -->
        <div
          class="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 backdrop-blur-xl space-y-4 shadow-xl"
        >
          <div
            class="flex items-center justify-between pb-2 border-b border-slate-800"
          >
            <div class="flex items-center gap-2.5">
              <Shield class="w-5 h-5 text-indigo-400" />
              <div>
                <h3 class="text-sm font-bold text-white">
                  Allow Remote Access (Host)
                </h3>
                <p class="text-[11px] text-slate-400">
                  Configure remote control permissions and security PIN
                </p>
              </div>
            </div>

            <!-- Enable/Disable Toggle -->
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

          <div class="space-y-4">
            <!-- Host Device ID -->
            <div>
              <span class="block text-xs font-semibold text-slate-300 mb-1"
                >Your Device ID:</span
              >
              <div
                class="flex items-center justify-between p-2 rounded-xl bg-slate-950 border border-slate-800 font-mono text-xs text-cyan-300"
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

            <!-- Host Security PIN (Strictly Hidden & Protected) -->
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
                  <span
                    class="text-[10px] text-slate-500 italic hidden sm:inline"
                    >(Always hidden for security)</span
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
                Clients with this PIN can connect instantly without prompting
                you. If no PIN is entered, you must manually approve the
                request.
              </p>
            </div>

            <!-- Screen Capture Readiness (For Unattended Access) -->
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
                    Screen capture is active. Remote clients connect
                    automatically without prompt.
                  {:else}
                    Start capture in advance for seamless unattended remote
                    control.
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
        </div>
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
