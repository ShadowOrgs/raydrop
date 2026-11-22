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
    Volume2,
    VolumeX,
    Power,
    PictureInPicture2,
    PenTool,
    Zap,
    QrCode,
  } from "lucide-svelte";
  import QRCode from "qrcode";
  import AnnotationOverlay from "./AnnotationOverlay.svelte";
  import { triggerWakeOnLan } from "../stores/wol";
  import { isDraggingFiles } from "../stores/state";
  import { raydropApi } from "../utils/tauri-ipc";
  import {
    remoteHostSettings,
    isHostSharingScreen,
    clientRemoteStream,
    activeRemoteSession,
    remoteConnectionStatus,
    remoteErrorMessage,
    remoteLatencyMs,
    remoteHostMonitors,
    activeMonitorIndex,
    loadRemoteDesktopSettings,
    saveRemoteDesktopSettings,
    connectToRemoteHost,
    disconnectRemoteSession,
    sendInputToHost,
    triggerLocalClipboardSyncNow,
    sendShortcutToHost,
    switchRemoteMonitor,
    sendPowerActionToHost,
    initRemoteDesktopListener,
    startHostScreenCapture,
    stopHostScreenCapture,
    acquireHostScreenStream,
    resumeClientAudio,
    setClientAudioMuted,
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
  let isAudioMuted = false;
  let isPiP = false;
  let showAnnotation = false;
  let isSecondDisplayFill = false;
  let isPowerMenuOpen = false;
  let isSecondScreenModalOpen = false;
  let isSecondScreenActive = false;
  let secondScreenUrl = "";
  let secondScreenQrDataUrl = "";
  let secondScreenError = "";
  let secondScreenTimer: any = null;
  let secondScreenVideoEl: HTMLVideoElement | null = null;
  let secondScreenCanvasEl: HTMLCanvasElement | null = null;
  let secondScreenStream: MediaStream | null = null;
  let secondScreenMonitors: { index: number; name: string; width: number; height: number; is_primary: boolean }[] = [];
  let selectedSecondScreenMonitorIdx = 0;
  let isBootServiceInstalled = false;
  let isInstallingBootService = false;
  let windowsServiceStatus = "not_installed";
  let isManagingWinService = false;
  let pendingPowerAction: "lock" | "sleep" | "restart" | "shutdown" | null = null;
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

  $: if (!isClientConnected && hideLocalCursor) {
    hideLocalCursor = false;
  }

  // Svelte action to reliably attach MediaStream to video DOM element with autoplay & unfreeze handling
  function attachStream(node: HTMLVideoElement, stream: MediaStream | null) {
    node.srcObject = stream;

    const playSafe = () => {
      if (node && node.srcObject) {
        node.muted = isAudioMuted;
        const playPromise = node.play();
        if (playPromise !== undefined) {
          playPromise.catch(() => {
            node.muted = true;
            isAudioMuted = true;
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
      if (node && node.srcObject) {
        playSafe();
      }
    };

    node.addEventListener("pause", handlePause);
    node.addEventListener("enterpictureinpicture", () => (isPiP = true));
    node.addEventListener("leavepictureinpicture", () => (isPiP = false));
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

    raydropApi.checkUnattendedBootService().then((res) => {
      isBootServiceInstalled = res;
    }).catch(() => {});

    raydropApi.checkWindowsServiceStatus().then((st) => {
      windowsServiceStatus = st;
    }).catch(() => {});

    document.addEventListener("fullscreenchange", handleFullscreenChange);
  });

  onDestroy(() => {
    stopSecondScreenSharing();
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

  async function handleInstallBootService() {
    try {
      isInstallingBootService = true;
      const msg = await raydropApi.installUnattendedBootService();
      isBootServiceInstalled = await raydropApi.checkUnattendedBootService();
      alert(msg);
    } catch (err: any) {
      alert(`Boot service setup failed: ${err?.message || err}`);
    } finally {
      isInstallingBootService = false;
    }
  }

  async function handleInstallWindowsService() {
    try {
      isManagingWinService = true;
      const msg = await raydropApi.installWindowsService();
      windowsServiceStatus = await raydropApi.checkWindowsServiceStatus();
      alert(msg);
    } catch (err: any) {
      alert(`Windows Service setup failed: ${err?.message || err}`);
    } finally {
      isManagingWinService = false;
    }
  }

  async function handleUninstallWindowsService() {
    try {
      isManagingWinService = true;
      const msg = await raydropApi.uninstallWindowsService();
      windowsServiceStatus = await raydropApi.checkWindowsServiceStatus();
      alert(msg);
    } catch (err: any) {
      alert(`Windows Service uninstall failed: ${err?.message || err}`);
    } finally {
      isManagingWinService = false;
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

  function toggleAudioMute() {
    isAudioMuted = !isAudioMuted;
    setClientAudioMuted(isAudioMuted);
    if (videoEl) {
      videoEl.muted = isAudioMuted;
    }
  }

  async function refreshSecondScreenMonitors() {
    try {
      secondScreenMonitors = await raydropApi.getHostMonitors();
      if (secondScreenMonitors.length > 1) {
        const secondary = secondScreenMonitors.find((m) => !m.is_primary);
        if (secondary) {
          selectedSecondScreenMonitorIdx = secondary.index;
          await raydropApi.setHostTargetMonitor(secondary.index).catch(() => {});
        }
      }
    } catch (_) {}
  }

  async function selectSecondScreenTouchMonitor(idx: number) {
    selectedSecondScreenMonitorIdx = idx;
    await raydropApi.setHostTargetMonitor(idx).catch(() => {});
  }

  async function startSecondScreenSharing() {
    secondScreenError = "";
    try {
      await refreshSecondScreenMonitors();
      if (secondScreenStream) {
        secondScreenStream.getTracks().forEach((t) => t.stop());
        secondScreenStream = null;
      }

      const stream: MediaStream = await (navigator.mediaDevices as any).getDisplayMedia({
        video: true,
        audio: false,
        surfaceSwitching: "include",
        selfBrowserSurface: "include",
        monitorTypeSurfaces: "include",
      });
      if (!stream || stream.getVideoTracks().length === 0) {
        secondScreenError = "Please select a Display 2 monitor or App Window to extend to your iPad/Tablet.";
        return;
      }
      secondScreenStream = stream;

      const info = await raydropApi.startSecondScreenServer();
      secondScreenUrl = info.url;
      secondScreenQrDataUrl = await QRCode.toDataURL(info.url, {
        width: 240,
        margin: 2,
        color: { dark: "#0f172a", light: "#ffffff" },
      });
      isSecondScreenActive = true;

      if (!secondScreenVideoEl) {
        secondScreenVideoEl = document.createElement("video");
        secondScreenVideoEl.muted = true;
        secondScreenVideoEl.playsInline = true;
      }
      secondScreenVideoEl.srcObject = stream;
      await secondScreenVideoEl.play().catch(() => {});

      if (!secondScreenCanvasEl) {
        secondScreenCanvasEl = document.createElement("canvas");
      }

      if (secondScreenTimer) clearInterval(secondScreenTimer);
      secondScreenTimer = setInterval(() => {
        if (!isSecondScreenActive || !secondScreenVideoEl || !secondScreenCanvasEl) return;
        const vw = secondScreenVideoEl.videoWidth;
        const vh = secondScreenVideoEl.videoHeight;
        if (!vw || !vh) return;
        const scale = Math.min(1, 1600 / vw);
        const cw = Math.round(vw * scale);
        const ch = Math.round(vh * scale);
        if (secondScreenCanvasEl.width !== cw) secondScreenCanvasEl.width = cw;
        if (secondScreenCanvasEl.height !== ch) secondScreenCanvasEl.height = ch;
        const ctx = secondScreenCanvasEl.getContext("2d");
        if (ctx) {
          ctx.drawImage(secondScreenVideoEl, 0, 0, cw, ch);
          const dataUrl = secondScreenCanvasEl.toDataURL("image/jpeg", 0.72);
          raydropApi.pushSecondScreenFrame(dataUrl).catch(() => {});
        }
      }, 35); // ~28 fps smooth stream
    } catch (err: any) {
      secondScreenError = err?.message || String(err);
    }
  }

  async function stopSecondScreenSharing() {
    isSecondScreenActive = false;
    if (secondScreenTimer) {
      clearInterval(secondScreenTimer);
      secondScreenTimer = null;
    }
    if (secondScreenStream) {
      secondScreenStream.getTracks().forEach((t) => t.stop());
      secondScreenStream = null;
    }
    if (secondScreenVideoEl) {
      secondScreenVideoEl.srcObject = null;
    }
    secondScreenUrl = "";
    secondScreenQrDataUrl = "";
    await raydropApi.stopSecondScreenServer().catch(() => {});
  }

  function openSecondScreenModal() {
    isSecondScreenModalOpen = true;
    refreshSecondScreenMonitors();
    if (!isSecondScreenActive) {
      startSecondScreenSharing();
    }
  }

  async function togglePictureInPicture() {
    if (!videoEl) return;
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
        isPiP = false;
      } else if ((videoEl as any).requestPictureInPicture) {
        await (videoEl as any).requestPictureInPicture();
        isPiP = true;
      }
    } catch (err) {
      console.warn("Picture-in-Picture error:", err);
    }
  }

  function requestPowerAction(action: "lock" | "sleep" | "restart" | "shutdown") {
    isPowerMenuOpen = false;
    if (action === "lock") {
      sendPowerActionToHost("lock");
    } else {
      pendingPowerAction = action;
    }
  }

  function confirmPowerAction() {
    if (!pendingPowerAction) return;
    sendPowerActionToHost(pendingPowerAction);
    pendingPowerAction = null;
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
    resumeClientAudio();
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

  let lastTouchY = 0;

  function getNormalizedTouchCoords(touch: Touch): { x: number; y: number } | null {
    if (!videoEl) return null;
    const rect = videoEl.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const x = Math.max(0, Math.min(1, (touch.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (touch.clientY - rect.top) / rect.height));
    return { x, y };
  }

  function handleTouchStart(e: TouchEvent) {
    if (!isClientConnected || !e.touches.length) return;
    resumeClientAudio();
    if (e.touches.length === 2) {
      lastTouchY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      return;
    }
    const coords = getNormalizedTouchCoords(e.touches[0]);
    if (!coords) return;
    sendInputToHost({ t: "mm", x: coords.x, y: coords.y });
    sendInputToHost({ t: "mc", b: "left", a: "down", alt: false, ctrl: false, shift: false, meta: false });
  }

  function handleTouchMove(e: TouchEvent) {
    if (!isClientConnected || !e.touches.length) return;
    e.preventDefault();
    if (e.touches.length === 2) {
      const midY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
      const diff = midY - lastTouchY;
      if (Math.abs(diff) > 12) {
        sendInputToHost({ t: "mw", dy: diff > 0 ? -120 : 120 });
        lastTouchY = midY;
      }
      return;
    }
    const now = performance.now();
    if (now - lastMouseMoveTime < 16) return;
    lastMouseMoveTime = now;
    const coords = getNormalizedTouchCoords(e.touches[0]);
    if (!coords) return;
    sendInputToHost({ t: "mm", x: coords.x, y: coords.y });
  }

  function handleTouchEnd(e: TouchEvent) {
    if (!isClientConnected) return;
    if (e.touches.length === 0) {
      sendInputToHost({ t: "mc", b: "left", a: "up", alt: false, ctrl: false, shift: false, meta: false });
    }
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

    if ((e.ctrlKey || e.metaKey) && ["c", "x"].includes(e.key.toLowerCase())) {
      triggerLocalClipboardSyncNow();
    }

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
      on:dragenter|preventDefault={(e) => {
        if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      }}
      on:dragover|preventDefault={(e) => {
        if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      }}
      on:drop|preventDefault
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
            <!-- Multi-Monitor Switcher (when host has multiple displays) -->
            {#if $remoteHostMonitors.length > 1}
              <div class="flex items-center gap-1 px-2 py-0.5 rounded-lg bg-slate-800 border border-slate-700 text-[11px]">
                <Monitor class="w-3 h-3 text-cyan-400 shrink-0" />
                <select
                  value={$activeMonitorIndex}
                  on:change={(e) => switchRemoteMonitor(Number(e.currentTarget.value))}
                  class="bg-transparent text-slate-200 focus:outline-none cursor-pointer pr-1"
                  title="Switch Target Monitor Bounds"
                >
                  {#each $remoteHostMonitors as mon}
                    <option value={mon.index} class="bg-slate-900 text-white">
                      {mon.name} ({mon.width}x{mon.height})
                    </option>
                  {/each}
                  <option value={-1} class="bg-slate-900 text-white">
                    All Displays (Virtual)
                  </option>
                </select>
              </div>
            {/if}

            <!-- Remote Audio Streaming Toggle (Mute/Unmute) -->
            <button
              type="button"
              on:click={toggleAudioMute}
              class="p-1 rounded-lg transition cursor-pointer {!isAudioMuted
                ? 'text-emerald-400 bg-slate-800 border border-emerald-500/30'
                : 'text-slate-400 hover:text-white hover:bg-slate-800'}"
              title={isAudioMuted ? "Unmute Host Audio" : "Host Audio Active (Click to Mute)"}
            >
              {#if isAudioMuted}
                <VolumeX class="w-3.5 h-3.5" />
              {:else}
                <Volume2 class="w-3.5 h-3.5" />
              {/if}
            </button>

            <!-- Remote Power Management Dropdown -->
            <div class="relative">
              <button
                type="button"
                on:click={() => (isPowerMenuOpen = !isPowerMenuOpen)}
                class="p-1 rounded-lg transition cursor-pointer {isPowerMenuOpen
                  ? 'text-amber-400 bg-slate-800 border border-amber-500/30'
                  : 'text-slate-400 hover:text-amber-300 hover:bg-slate-800'}"
                title="Remote Power Management (Lock, Sleep, Restart, Shutdown)"
              >
                <Power class="w-3.5 h-3.5" />
              </button>
              {#if isPowerMenuOpen}
                <div
                  class="absolute right-0 mt-2 w-40 rounded-xl bg-slate-900 border border-slate-700 shadow-2xl py-1 z-50 text-xs"
                >
                  <button
                    type="button"
                    on:click={() => requestPowerAction("lock")}
                    class="w-full px-3 py-1.5 text-left text-slate-200 hover:bg-slate-800 hover:text-cyan-300 flex items-center gap-2 cursor-pointer"
                  >
                    <Lock class="w-3.5 h-3.5 text-cyan-400" />
                    <span>Lock Screen</span>
                  </button>
                  <button
                    type="button"
                    on:click={() => requestPowerAction("sleep")}
                    class="w-full px-3 py-1.5 text-left text-slate-200 hover:bg-slate-800 hover:text-indigo-300 flex items-center gap-2 cursor-pointer"
                  >
                    <Minimize class="w-3.5 h-3.5 text-indigo-400" />
                    <span>Sleep PC</span>
                  </button>
                  <button
                    type="button"
                    on:click={() => requestPowerAction("restart")}
                    class="w-full px-3 py-1.5 text-left text-amber-300 hover:bg-slate-800 flex items-center gap-2 cursor-pointer"
                  >
                    <RefreshCw class="w-3.5 h-3.5 text-amber-400" />
                    <span>Restart PC</span>
                  </button>
                  <button
                    type="button"
                    on:click={() => requestPowerAction("shutdown")}
                    class="w-full px-3 py-1.5 text-left text-rose-400 hover:bg-rose-950/50 flex items-center gap-2 cursor-pointer"
                  >
                    <Power class="w-3.5 h-3.5 text-rose-400" />
                    <span>Shutdown PC</span>
                  </button>
                </div>
              {/if}
            </div>

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

            <!-- Floating Mini Player / Picture-in-Picture Button -->
            <button
              type="button"
              on:click={togglePictureInPicture}
              class="p-1 rounded-lg transition cursor-pointer {isPiP
                ? 'text-cyan-400 bg-slate-800 border border-cyan-500/30'
                : 'text-slate-400 hover:text-white hover:bg-slate-800'}"
              title={isPiP ? "Exit Mini Player (PiP)" : "Floating Mini Player (PiP)"}
            >
              <PictureInPicture2 class="w-3.5 h-3.5" />
            </button>

            <!-- Laser Pointer & Screen Annotation Toggle -->
            <button
              type="button"
              on:click={() => (showAnnotation = !showAnnotation)}
              class="p-1 rounded-lg transition cursor-pointer {showAnnotation
                ? 'text-rose-400 bg-slate-800 border border-rose-500/40'
                : 'text-slate-400 hover:text-white hover:bg-slate-800'}"
              title="Laser Pointer & Screen Annotation"
            >
              <PenTool class="w-3.5 h-3.5" />
            </button>

            <!-- Second Display / Tablet Stretch Fill Mode -->
            <button
              type="button"
              on:click={() => (isSecondDisplayFill = !isSecondDisplayFill)}
              class="p-1 rounded-lg transition cursor-pointer {isSecondDisplayFill
                ? 'text-indigo-400 bg-slate-800 border border-indigo-500/40'
                : 'text-slate-400 hover:text-white hover:bg-slate-800'}"
              title={isSecondDisplayFill ? "Fit Aspect Ratio" : "Second Display Mode (Fill Screen)"}
            >
              <Tv class="w-3.5 h-3.5" />
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
        on:touchstart={handleTouchStart}
        on:touchmove={handleTouchMove}
        on:touchend={handleTouchEnd}
      >
        <video
          bind:this={videoEl}
          use:attachStream={$clientRemoteStream}
          autoplay
          playsinline
          muted={isAudioMuted}
          class="{isSecondDisplayFill ? 'w-full h-full object-fill' : 'max-w-full max-h-full object-contain'} pointer-events-none select-none shadow-2xl"
        ></video>
        <AnnotationOverlay active={showAnnotation} on:close={() => (showAnnotation = false)} />
      </div>

      {#if isFullscreen && $isDraggingFiles}
        <div
          class="absolute inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex flex-col items-center justify-center pointer-events-none"
        >
          <div
            class="p-6 rounded-2xl border-2 border-dashed border-cyan-400 bg-slate-900/95 shadow-2xl text-center max-w-sm mx-4"
          >
            <p class="text-base font-bold text-white">
              Drop to Send Directly to {$activeRemoteSession?.peerAlias || "Host"}
            </p>
            <p class="text-xs text-slate-300 mt-1">
              Files dropped onto the Remote Desktop screen will transfer immediately.
            </p>
          </div>
        </div>
      {/if}
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
            <h2 class="text-lg font-bold text-white flex items-center gap-2 flex-wrap">
              <span>Raydrop Remote Desktop</span>
              <span
                class="px-2 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 font-mono text-[10px] border border-cyan-500/30"
              >
                P2P Ultra-Low Latency
              </span>
              <!-- Host Ready / Disabled Badge -->
              <button
                type="button"
                on:click={() => (isHostSettingsModalOpen = true)}
                class="px-2 py-0.5 rounded-full text-[10px] font-semibold border transition cursor-pointer flex items-center gap-1 {hostEnabled
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30 hover:bg-emerald-500/25'
                  : 'bg-rose-500/15 text-rose-300 border-rose-500/30 hover:bg-rose-500/25'}"
                title="Click to configure Host Remote Access & PIN"
              >
                <span class="w-1.5 h-1.5 rounded-full {hostEnabled ? 'bg-emerald-400' : 'bg-rose-400'}"></span>
                <span>
                  {hostEnabled
                    ? $remoteHostSettings.pin
                      ? 'Host Ready • PIN Set'
                      : 'Host Ready • Approval'
                    : 'Host Disabled'}
                </span>
              </button>
              <!-- Pre-Login Service Active Badge -->
              {#if windowsServiceStatus === "running"}
                <span
                  class="px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 text-[10px] font-semibold flex items-center gap-1"
                  title="Raydrop Windows Service is running: pre-login PIN screen remote access is active"
                >
                  <Shield class="w-3 h-3 text-emerald-400" />
                  <span>Pre-Login Active</span>
                </span>
              {/if}
              <!-- Admin / UIPI Elevated Control Badge -->
              {#if windowsServiceStatus !== "unsupported"}
                {#if isBootServiceInstalled}
                  <span
                    class="px-2 py-0.5 rounded-full bg-indigo-500/15 text-indigo-300 border border-indigo-500/30 text-[10px] font-semibold"
                    title="Elevated UIPI task is installed: can control Admin apps & Task Manager"
                  >
                    Admin/UIPI Active
                  </span>
                {:else}
                  <button
                    type="button"
                    disabled={isInstallingBootService}
                    on:click={handleInstallBootService}
                    class="px-2 py-0.5 rounded-full bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/35 text-[10px] font-semibold transition cursor-pointer disabled:opacity-60"
                    title="Click to enable Admin/UIPI privileges so remote sessions can control Task Manager & elevated apps"
                  >
                    {isInstallingBootService ? "Enabling Admin..." : "+ Enable Admin Control"}
                  </button>
                {/if}
              {/if}
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

          <!-- Extend to iPad / Tablet (QR Code) Quick Button -->
          <button
            type="button"
            on:click={openSecondScreenModal}
            class="px-3 py-2 rounded-xl bg-gradient-to-r from-indigo-500/20 to-cyan-500/20 hover:from-indigo-500/30 hover:to-cyan-500/30 text-cyan-300 border border-cyan-500/40 text-xs font-semibold transition flex items-center gap-1.5 cursor-pointer active:scale-95 shadow-sm"
            title="Use iPad, Tablet or Phone as Wireless Second Display via QR Code"
          >
            <QrCode class="w-3.5 h-3.5 text-cyan-400" />
            <span>Extend to iPad / Tablet</span>
            {#if isSecondScreenActive}
              <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
            {/if}
          </button>

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

      <!-- Windows System Service Install Prompt Banner (shown when service is not running on Windows) -->
      {#if windowsServiceStatus !== "running" && windowsServiceStatus !== "unsupported"}
        <div
          class="p-3.5 rounded-xl bg-amber-950/40 border border-amber-500/40 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs shadow-lg"
        >
          <div class="flex items-start sm:items-center gap-2.5">
            <Shield class="w-4 h-4 text-amber-400 shrink-0 mt-0.5 sm:mt-0" />
            <div class="space-y-0.5">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="font-semibold text-amber-200">
                  Windows System Service
                </span>
                <span
                  class="text-[10px] px-2 py-0.5 rounded-full font-semibold bg-amber-500/20 text-amber-300 border border-amber-500/30"
                >
                  {windowsServiceStatus === "stopped" ? "Stopped" : "Not Installed"}
                </span>
              </div>
              <p class="text-[11px] text-amber-200/80">
                Install the Windows System Service so this PC can be remoted directly on the Windows PIN / Lock screen right after restart.
              </p>
            </div>
          </div>
          <button
            type="button"
            disabled={isManagingWinService}
            on:click={handleInstallWindowsService}
            class="px-3.5 py-2 rounded-xl bg-amber-500 hover:bg-amber-400 disabled:opacity-60 text-slate-950 font-bold text-xs transition flex items-center justify-center gap-1.5 shrink-0 cursor-pointer active:scale-95 shadow-sm"
          >
            <Shield class="w-3.5 h-3.5" />
            <span>
              {isManagingWinService
                ? "Installing..."
                : windowsServiceStatus === "stopped"
                  ? "Start / Reinstall Service"
                  : "Install Service"}
            </span>
          </button>
        </div>
      {/if}

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
                <div class="flex items-center gap-1.5">
                  {#if !peer.isOnline}
                    <button
                      type="button"
                      on:click|stopPropagation={() => triggerWakeOnLan(peer.id, peer.name)}
                      class="px-2 py-1 rounded-lg bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/30 text-[10px] font-semibold flex items-center gap-1 transition cursor-pointer"
                      title="Send Wake-on-LAN Magic Packet"
                    >
                      <Zap class="w-3 h-3" />
                      <span>Wake</span>
                    </button>
                  {/if}
                  <div
                    class="w-7 h-7 rounded-lg bg-cyan-500/10 group-hover:bg-cyan-500 text-cyan-400 group-hover:text-slate-950 flex items-center justify-center transition shadow-sm"
                  >
                    <Play class="w-3.5 h-3.5 fill-current ml-0.5" />
                  </div>
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
        class="w-full max-w-sm max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-cyan-500/40 p-5 shadow-2xl space-y-4"
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
        class="w-full max-w-md max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
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

          <!-- Windows SYSTEM Service (Pre-Login / Lock Screen Access) -->
          <div
            class="p-3 rounded-xl bg-slate-950 border border-cyan-500/30 space-y-2"
          >
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-slate-200 flex items-center gap-1.5">
                <Shield class="w-3.5 h-3.5 text-cyan-400" />
                <span>Windows System Service (Pre-Login PIN)</span>
              </span>
              <span
                class="text-[10px] px-2 py-0.5 rounded-full font-semibold {windowsServiceStatus === 'running'
                  ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                  : windowsServiceStatus === 'stopped'
                  ? 'bg-amber-500/15 text-amber-300 border border-amber-500/30'
                  : 'bg-slate-800 text-slate-400 border border-slate-700'}"
              >
                {windowsServiceStatus === 'running'
                  ? "Running (Boot Ready)"
                  : windowsServiceStatus === 'stopped'
                  ? "Stopped"
                  : "Not Installed"}
              </span>
            </div>
            <p class="text-[11px] text-slate-400 leading-relaxed">
              Runs as a Windows <code class="text-cyan-300">SYSTEM</code> service on boot, enabling remote access and PIN entry directly on the Windows Lock/Login screen before signing in.
            </p>
            <div class="flex items-center gap-2">
              <button
                type="button"
                disabled={isManagingWinService}
                on:click={handleInstallWindowsService}
                class="flex-1 py-1.5 rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 disabled:opacity-50 text-cyan-300 border border-cyan-500/40 text-xs font-semibold transition cursor-pointer"
              >
                {isManagingWinService
                  ? "Working..."
                  : windowsServiceStatus === 'running'
                  ? "Sync Config & Restart Service"
                  : "Install & Start Windows Service"}
              </button>
              {#if windowsServiceStatus !== 'not_installed' && windowsServiceStatus !== 'unsupported'}
                <button
                  type="button"
                  disabled={isManagingWinService}
                  on:click={handleUninstallWindowsService}
                  class="px-3 py-1.5 rounded-lg bg-rose-500/15 hover:bg-rose-500/25 disabled:opacity-50 text-rose-300 border border-rose-500/30 text-xs font-semibold transition cursor-pointer"
                >
                  Uninstall
                </button>
              {/if}
            </div>
          </div>

          <!-- Unattended Boot & Elevated Admin Service -->
          <div
            class="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-2"
          >
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-slate-200 flex items-center gap-1.5">
                <Zap class="w-3.5 h-3.5 text-amber-400" />
                <span>Unattended Boot & Admin Control</span>
              </span>
              <span
                class="text-[10px] px-2 py-0.5 rounded-full font-semibold {isBootServiceInstalled
                  ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                  : 'bg-slate-800 text-slate-400 border border-slate-700'}"
              >
                {isBootServiceInstalled ? "Installed (Active)" : "Standard User"}
              </span>
            </div>
            <p class="text-[11px] text-slate-400 leading-relaxed">
              Registers an elevated Windows boot/logon task so Raydrop starts automatically after a PC restart with zero screen-share prompts and full UAC/Admin control.
            </p>
            <button
              type="button"
              disabled={isInstallingBootService}
              on:click={handleInstallBootService}
              class="w-full py-1.5 rounded-lg bg-indigo-500/20 hover:bg-indigo-500/30 disabled:opacity-50 text-indigo-300 border border-indigo-500/40 text-xs font-semibold transition cursor-pointer"
            >
              {isInstallingBootService
                ? "Requesting Admin Elevation..."
                : isBootServiceInstalled
                ? "Re-verify / Update Unattended Boot Task"
                : "Enable Unattended Boot & Admin Service"}
            </button>
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
        class="w-full max-w-sm max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
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

  <!-- Remote Power Action Confirmation Modal -->
  {#if pendingPowerAction}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-sm max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-amber-500/40 p-5 shadow-2xl space-y-4"
      >
        <div class="flex items-center gap-3 pb-3 border-b border-slate-800">
          <div
            class="w-10 h-10 rounded-xl bg-amber-500/20 border border-amber-500/30 flex items-center justify-center text-amber-400"
          >
            <Power class="w-5 h-5" />
          </div>
          <div>
            <h3 class="text-sm font-bold text-white capitalize">
              Confirm Remote {pendingPowerAction}
            </h3>
            <p class="text-xs text-slate-400">
              Target: {$activeRemoteSession?.peerAlias || "Host PC"}
            </p>
          </div>
        </div>

        <p class="text-xs text-slate-300 leading-relaxed">
          Are you sure you want to <b>{pendingPowerAction}</b> the remote host computer?
          {pendingPowerAction === "shutdown" || pendingPowerAction === "sleep"
            ? "Your active remote desktop session will be disconnected."
            : "You will need to wait until the computer finishes rebooting to reconnect."}
        </p>

        <div class="flex items-center justify-end gap-2 pt-2 border-t border-slate-800">
          <button
            type="button"
            on:click={() => (pendingPowerAction = null)}
            class="px-3.5 py-1.5 rounded-xl text-xs text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            on:click={confirmPowerAction}
            class="px-4 py-1.5 rounded-xl bg-rose-500 hover:bg-rose-400 text-white font-bold text-xs shadow transition cursor-pointer active:scale-95 capitalize"
          >
            Yes, {pendingPowerAction}
          </button>
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
        class="w-full max-w-sm max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-cyan-500/40 p-5 shadow-2xl space-y-4"
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
        class="w-full max-w-sm max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-slate-800 p-5 shadow-2xl space-y-4"
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

  <!-- Extend to iPad / Tablet / Phone (QR Code Second Display Modal) -->
  {#if isSecondScreenModalOpen}
    <div
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/80 backdrop-blur-sm animate-in fade-in duration-150"
    >
      <div
        class="w-full max-w-md max-h-[90vh] overflow-y-auto rounded-2xl bg-slate-900 border border-cyan-500/40 p-5 shadow-2xl space-y-4"
      >
        <div
          class="flex items-center justify-between pb-3 border-b border-slate-800"
        >
          <div class="flex items-center gap-2.5">
            <div
              class="w-9 h-9 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400"
            >
              <QrCode class="w-5 h-5" />
            </div>
            <div>
              <h3 class="text-sm font-bold text-white">
                Extend to iPad / Tablet / Phone
              </h3>
              <p class="text-[11px] text-slate-400">
                Scan QR Code using Camera to open Wireless Second Display
              </p>
            </div>
          </div>
          <button
            type="button"
            on:click={() => (isSecondScreenModalOpen = false)}
            class="p-1 rounded text-slate-400 hover:text-white"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        {#if secondScreenError}
          <div
            class="p-3 rounded-xl bg-rose-950/70 border border-rose-800/80 text-rose-200 text-xs flex items-center gap-2"
          >
            <AlertCircle class="w-4 h-4 text-rose-400 shrink-0" />
            <span>{secondScreenError}</span>
          </div>
        {/if}

        {#if isSecondScreenActive && secondScreenQrDataUrl}
          <div class="flex flex-col items-center space-y-3 py-2">
            <div
              class="p-3 rounded-2xl bg-white shadow-xl border-4 border-cyan-500/30"
            >
              <img
                src={secondScreenQrDataUrl}
                alt="QR Code for Second Display"
                class="w-48 h-48 object-contain"
              />
            </div>

            <div class="w-full space-y-1.5">
              <span class="block text-[11px] font-semibold text-slate-400">
                Direct Local Wi-Fi URL (No App Required on iPad):
              </span>
              <div
                class="flex items-center justify-between p-2 rounded-xl bg-slate-950 border border-slate-800 font-mono text-[11px] text-cyan-300 gap-2"
              >
                <span class="truncate">{secondScreenUrl}</span>
                <button
                  type="button"
                  on:click={() => {
                    navigator.clipboard.writeText(secondScreenUrl);
                    alert("Second Display URL copied!");
                  }}
                  class="p-1 text-slate-400 hover:text-white shrink-0 cursor-pointer"
                  title="Copy URL"
                >
                  <Copy class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <!-- Detected Monitors & True Extend Setup -->
            <div
              class="w-full p-3 rounded-xl bg-slate-950/90 border border-slate-800 space-y-2.5 text-xs"
            >
              <div class="flex items-center justify-between">
                <span class="font-semibold text-slate-200 flex items-center gap-1.5">
                  <Monitor class="w-3.5 h-3.5 text-cyan-400" />
                  <span>Detected Displays ({secondScreenMonitors.length})</span>
                </span>
                <button
                  type="button"
                  on:click={refreshSecondScreenMonitors}
                  class="text-[11px] text-cyan-400 hover:text-cyan-300 flex items-center gap-1 cursor-pointer"
                  title="Refresh detected Windows displays"
                >
                  <RefreshCw class="w-3 h-3" />
                  <span>Refresh</span>
                </button>
              </div>

              {#if secondScreenMonitors.length > 1}
                <div class="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/30 text-[11px] text-emerald-300">
                  ✅ Extended Display detected! Select which display iPad touch controls target:
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each secondScreenMonitors as mon}
                    <button
                      type="button"
                      on:click={() => selectSecondScreenTouchMonitor(mon.index)}
                      class="px-2.5 py-1 rounded-lg text-[11px] font-medium border transition cursor-pointer {selectedSecondScreenMonitorIdx === mon.index
                        ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40'
                        : 'bg-slate-900 text-slate-400 border-slate-700 hover:text-white'}"
                    >
                      {mon.name} ({mon.width}x{mon.height})
                    </button>
                  {/each}
                </div>
              {:else}
                <div class="p-2.5 rounded-lg bg-amber-500/10 border border-amber-500/30 text-[11px] text-amber-200 space-y-1.5 leading-relaxed">
                  <p class="font-semibold text-amber-300">
                    ℹ️ Only 1 physical monitor detected (Mirror / App Window Mode)
                  </p>
                  <p>
                    To make iPad a true independent <b>Extended 2nd Monitor</b> (not mirrored), either pick a specific <b>App Window</b> when sharing, or add a free Windows Virtual Display Driver:
                  </p>
                  <div class="flex flex-wrap gap-2 pt-1">
                    <button
                      type="button"
                      on:click={() => raydropApi.openSystemDisplaySettings()}
                      class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700 text-[10px] font-semibold cursor-pointer"
                    >
                      Open Display Settings
                    </button>
                    <a
                      href="https://github.com/VirtualDrivers/Virtual-Display-Driver/releases"
                      target="_blank"
                      rel="noopener"
                      class="px-2.5 py-1 rounded-lg bg-indigo-500/20 hover:bg-indigo-500/30 text-indigo-300 border border-indigo-500/40 text-[10px] font-semibold"
                    >
                      Get Virtual Display Driver ↗
                    </a>
                  </div>
                </div>
              {/if}

              <button
                type="button"
                on:click={startSecondScreenSharing}
                class="w-full py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-cyan-300 border border-slate-700 text-[11px] font-semibold transition cursor-pointer"
              >
                🔄 Switch Shared Screen or App Window
              </button>
            </div>
          </div>
        {:else if !secondScreenError}
          <div class="py-8 flex flex-col items-center justify-center gap-3 text-xs text-slate-400">
            <RefreshCw class="w-6 h-6 animate-spin text-cyan-400" />
            <span>Initializing Second Display stream & QR Code...</span>
          </div>
        {/if}

        <div class="flex items-center justify-between pt-2 border-t border-slate-800">
          {#if isSecondScreenActive}
            <button
              type="button"
              on:click={stopSecondScreenSharing}
              class="px-3.5 py-1.5 rounded-xl bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-semibold transition cursor-pointer"
            >
              Stop Streaming
            </button>
          {:else}
            <button
              type="button"
              on:click={startSecondScreenSharing}
              class="px-3.5 py-1.5 rounded-xl bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/40 text-xs font-semibold transition cursor-pointer"
            >
              Start / Retry
            </button>
          {/if}

          <button
            type="button"
            on:click={() => (isSecondScreenModalOpen = false)}
            class="px-4 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-white text-xs font-semibold transition cursor-pointer"
          >
            {isSecondScreenActive ? "Minimize (Keep Active)" : "Close"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
