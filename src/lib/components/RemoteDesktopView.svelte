<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import {
    Monitor,
    Shield,
    Lock,
    Key,
    Wifi,
    RefreshCw,
    Play,
    Square,
    Eye,
    EyeOff,
    Check,
    X,
    Maximize,
    Minimize,
    Clipboard,
    ArrowDownToLine,
    ArrowUpFromLine,
    LogOut,
    Power,
    Sliders,
    HelpCircle,
    Copy,
    Laptop,
    Sparkles,
    AlertCircle,
    Layers,
    MonitorUp,
  } from 'lucide-svelte';
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
    stopHostScreenCapture,
  } from '../stores/remote_desktop';
  import { contacts, lanContacts } from '../stores/contacts';
  import { cloudMyDevices, appSettings } from '../stores/state';

  let selectedTargetDeviceId = '';
  let inputPin = '';
  let showInputPin = false;
  let showHostPin = false;

  // Host configuration state
  let hostEnabled = true;
  let hostPin = '';
  let hostUnattended = true;
  let isSavingSettings = false;
  let saveSuccess = false;

  // Viewer state
  let videoEl: HTMLVideoElement | null = null;
  let viewerContainerEl: HTMLDivElement | null = null;
  let isFullscreen = false;
  let scaleMode: 'fit' | 'fill' | 'original' = 'fit';
  let showToolbar = true;
  let lastMouseMoveTime = 0;

  $: isClientConnected =
    $remoteConnectionStatus === 'connected' &&
    $activeRemoteSession?.isClient &&
    !!$clientRemoteStream;

  $: isHostActive =
    $activeRemoteSession?.isHost &&
    $remoteConnectionStatus === 'connected';

  // Watch remote video stream and attach to video element
  $: if (videoEl && $clientRemoteStream) {
    if (videoEl.srcObject !== $clientRemoteStream) {
      videoEl.srcObject = $clientRemoteStream;
      videoEl.play().catch((e) => console.warn('Video play warning:', e));
    }
  }

  // Combined list of reachable remote target devices
  $: availablePeers = (() => {
    const list: { id: string; name: string; type: string; isOnline: boolean }[] = [];
    const seen = new Set<string>();
    const myDevId = $appSettings.device_id;

    for (const d of $cloudMyDevices) {
      if (d.device_id === myDevId || seen.has(d.device_id)) continue;
      seen.add(d.device_id);
      list.push({
        id: d.device_id,
        name: d.device_name || 'My Device',
        type: 'Perangkat Saya (Cloud)',
        isOnline: d.is_online,
      });
    }

    for (const c of $contacts) {
      if (c.device_id === myDevId || seen.has(c.device_id)) continue;
      seen.add(c.device_id);
      list.push({
        id: c.device_id,
        name: c.custom_name || c.alias_name || 'Kontak Raydrop',
        type: 'Kontak',
        isOnline: c.is_online,
      });
    }

    for (const l of $lanContacts) {
      if (l.device_id === myDevId || seen.has(l.device_id)) continue;
      seen.add(l.device_id);
      list.push({
        id: l.device_id,
        name: l.alias_name || 'LAN Peer',
        type: 'LAN Radar',
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

    document.addEventListener('fullscreenchange', handleFullscreenChange);
  });

  onDestroy(() => {
    document.removeEventListener('fullscreenchange', handleFullscreenChange);
  });

  function handleFullscreenChange() {
    isFullscreen = !!document.fullscreenElement;
  }

  async function handleSaveSettings() {
    try {
      isSavingSettings = true;
      await saveRemoteDesktopSettings(hostEnabled, hostPin, hostUnattended);
      saveSuccess = true;
      setTimeout(() => (saveSuccess = false), 2500);
    } catch (err: any) {
      alert(`Gagal menyimpan pengaturan: ${err?.message || err}`);
    } finally {
      isSavingSettings = false;
    }
  }

  function generateRandomPin() {
    hostPin = Math.floor(100000 + Math.random() * 900000).toString();
    handleSaveSettings();
  }

  function handleConnect() {
    if (!selectedTargetDeviceId) {
      alert('Pilih perangkat tujuan yang ingin Anda remote terlebih dahulu.');
      return;
    }
    if (!inputPin.trim()) {
      alert('Masukkan PIN Keamanan perangkat host.');
      return;
    }
    connectToRemoteHost(selectedTargetDeviceId, inputPin.trim());
  }

  function toggleFullscreen() {
    if (!viewerContainerEl) return;
    if (!document.fullscreenElement) {
      viewerContainerEl.requestFullscreen().catch((err) => {
        console.warn('Fullscreen request error:', err);
      });
    } else {
      document.exitFullscreen().catch((err) => {
        console.warn('Exit fullscreen error:', err);
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
      t: 'mm',
      x,
      y,
    });
  }

  function handleMouseDown(e: MouseEvent) {
    if (!isClientConnected) return;
    const button = e.button === 0 ? 'left' : e.button === 2 ? 'right' : 'middle';
    sendInputToHost({
      t: 'mc',
      b: button,
      a: 'down',
    });
  }

  function handleMouseUp(e: MouseEvent) {
    if (!isClientConnected) return;
    const button = e.button === 0 ? 'left' : e.button === 2 ? 'right' : 'middle';
    sendInputToHost({
      t: 'mc',
      b: button,
      a: 'up',
    });
  }

  function handleWheel(e: WheelEvent) {
    if (!isClientConnected) return;
    e.preventDefault();
    const dy = e.deltaY > 0 ? 120 : -120;
    sendInputToHost({
      t: 'mw',
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
    if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;

    // Send key down to host
    sendInputToHost({
      t: 'kk',
      k: e.key,
      c: e.code,
      a: 'down',
    });

    // Prevent default browser behavior for common shortcuts (like F5 reload or Backspace navigation)
    if (['Tab', 'F5', 'Alt', 'Meta'].includes(e.key) || (e.ctrlKey && ['w', 'r', 't'].includes(e.key.toLowerCase()))) {
      e.preventDefault();
    }
  }

  function handleKeyUp(e: KeyboardEvent) {
    if (!isClientConnected) return;
    const target = e.target as HTMLElement;
    if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) return;

    sendInputToHost({
      t: 'kk',
      k: e.key,
      c: e.code,
      a: 'up',
    });
  }
</script>

<svelte:window on:keydown={handleKeyDown} on:keyup={handleKeyUp} />

<div class="h-full w-full flex flex-col bg-slate-950 text-slate-100 overflow-hidden relative select-none">
  {#if isClientConnected}
    <!-- Active Remote Desktop Viewer Screen -->
    <div
      bind:this={viewerContainerEl}
      class="flex-1 w-full h-full relative flex flex-col bg-black overflow-hidden"
    >
      <!-- Floating Top Control Bar -->
      <div
        class="absolute top-2 left-1/2 -translate-x-1/2 z-40 transition-all duration-300 {showToolbar
          ? 'translate-y-0 opacity-100'
          : '-translate-y-12 opacity-0 hover:opacity-100'}"
      >
        <div class="flex items-center gap-2 px-3 py-1.5 rounded-2xl bg-slate-900/90 border border-slate-700/80 shadow-2xl backdrop-blur-md text-xs">
          <!-- Host & Latency Info -->
          <div class="flex items-center gap-2 pr-2 border-r border-slate-800">
            <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
            <span class="font-bold text-white max-w-[130px] truncate" title={$activeRemoteSession?.peerAlias}>
              {$activeRemoteSession?.peerAlias}
            </span>
            <span class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-cyan-300 border border-slate-700">
              ⚡ {$remoteLatencyMs} ms
            </span>
          </div>

          <!-- Clipboard Actions (Bidirectional Text & Image) -->
          <div class="flex items-center gap-1 pr-2 border-r border-slate-800">
            <button
              type="button"
              on:click={sendLocalClipboardToHost}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white transition flex items-center gap-1 cursor-pointer"
              title="Kirim Clipboard lokal Anda (Teks atau Gambar) ke Host PC"
            >
              <ArrowUpFromLine class="w-3.5 h-3.5 text-cyan-400" />
              <span class="hidden sm:inline">Kirim Clipboard</span>
            </button>
            <button
              type="button"
              on:click={requestClipboardFromHost}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white transition flex items-center gap-1 cursor-pointer"
              title="Ambil isi Clipboard dari Host PC ke komputer Anda"
            >
              <ArrowDownToLine class="w-3.5 h-3.5 text-indigo-400" />
              <span class="hidden sm:inline">Ambil Clipboard</span>
            </button>
          </div>

          <!-- Shortcuts Dropdown / Quick Buttons -->
          <div class="flex items-center gap-1 pr-2 border-r border-slate-800">
            <button
              type="button"
              on:click={() => sendShortcutToHost('win_d')}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white text-[11px] font-mono transition"
              title="Tampilkan Desktop Host (Win + D)"
            >
              Desktop
            </button>
            <button
              type="button"
              on:click={() => sendShortcutToHost('ctrl_shift_esc')}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white text-[11px] font-mono transition"
              title="Buka Task Manager di Host (Ctrl + Shift + Esc)"
            >
              TaskMgr
            </button>
            <button
              type="button"
              on:click={() => sendShortcutToHost('win_l')}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-rose-400 text-[11px] font-mono transition"
              title="Kunci Host PC (Win + L)"
            >
              Lock
            </button>
          </div>

          <!-- Display & Window Controls -->
          <div class="flex items-center gap-1">
            <button
              type="button"
              on:click={toggleFullscreen}
              class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
              title={isFullscreen ? 'Keluar Fullscreen' : 'Layar Penuh (Fullscreen)'}
            >
              {#if isFullscreen}
                <Minimize class="w-3.5 h-3.5" />
              {:else}
                <Maximize class="w-3.5 h-3.5" />
              {/if}
            </button>
            <button
              type="button"
              on:click={disconnectRemoteSession}
              class="px-2.5 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-bold transition flex items-center gap-1 cursor-pointer active:scale-95"
              title="Putus Sesi Remote"
            >
              <LogOut class="w-3.5 h-3.5" />
              <span>Putus</span>
            </button>
          </div>
        </div>
      </div>

      <!-- Main Video Display Canvas -->
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="flex-1 w-full h-full flex items-center justify-center bg-black overflow-hidden relative cursor-default"
        on:mousemove={handleMouseMove}
        on:mousedown={handleMouseDown}
        on:mouseup={handleMouseUp}
        on:wheel={handleWheel}
        on:contextmenu={handleContextMenu}
      >
        <video
          bind:this={videoEl}
          autoplay
          playsinline
          muted
          class="max-w-full max-h-full object-contain pointer-events-none select-none shadow-2xl"
        ></video>
      </div>
    </div>
  {:else}
    <!-- Remote Desktop Dashboard (Host & Connect Client Views) -->
    <div class="flex-1 p-4 sm:p-6 overflow-y-auto max-w-6xl w-full mx-auto space-y-6">
      <!-- Header Banner -->
      <div class="flex items-center justify-between pb-4 border-b border-slate-800">
        <div class="flex items-center gap-3">
          <div class="w-12 h-12 rounded-2xl bg-gradient-to-br from-cyan-500/20 to-indigo-600/30 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shadow-xl shadow-cyan-500/10">
            <Monitor class="w-6 h-6 stroke-[1.8]" />
          </div>
          <div>
            <h2 class="text-lg font-bold text-white flex items-center gap-2">
              <span>Raydrop Remote Desktop</span>
              <span class="px-2 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 font-mono text-[10px] border border-cyan-500/30">
                P2P Ultra-Low Latency
              </span>
            </h2>
            <p class="text-xs text-slate-400 mt-0.5">
              Remote kontrol PC dan HP secara instan dengan resolusi 1080p 60fps, perlindungan PIN aman, dan sinkronisasi clipboard dua arah.
            </p>
          </div>
        </div>

        {#if isHostActive}
          <div class="flex items-center gap-2.5 px-3 py-1.5 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-semibold">
            <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
            <span>Host Aktif: Dikontrol oleh {$activeRemoteSession?.peerAlias}</span>
            <button
              type="button"
              on:click={disconnectRemoteSession}
              class="ml-2 px-2 py-0.5 rounded bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/30 text-[11px]"
            >
              Hentikan
            </button>
          </div>
        {/if}
      </div>

      <!-- Error Alert Banner -->
      {#if $remoteErrorMessage}
        <div class="p-4 rounded-xl bg-rose-950/70 border border-rose-800/80 flex items-center justify-between gap-3 text-rose-200 text-xs shadow-lg">
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
        <div class="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 backdrop-blur-xl space-y-4 shadow-xl">
          <div class="flex items-center gap-2.5 pb-2 border-b border-slate-800">
            <Laptop class="w-5 h-5 text-cyan-400" />
            <div>
              <h3 class="text-sm font-bold text-white">Remote Perangkat Lain (Client)</h3>
              <p class="text-[11px] text-slate-400">Kontrol PC atau HP lain dari sini dengan memasukkan PIN</p>
            </div>
          </div>

          <div class="space-y-3.5">
            <!-- Target Device Selector -->
            <div>
              <label class="block text-xs font-semibold text-slate-300 mb-1.5">
                Pilih Komputer / HP Tujuan:
              </label>
              {#if availablePeers.length === 0}
                <div class="p-3 rounded-xl bg-slate-950/80 border border-slate-800 text-xs text-slate-500 text-center">
                  Belum ada perangkat online yang terdeteksi di Kontak atau LAN. Anda juga bisa memasukkan Device ID manual di bawah.
                </div>
              {:else}
                <select
                  bind:value={selectedTargetDeviceId}
                  class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500 transition"
                >
                  <option value="">-- Pilih Perangkat Online --</option>
                  {#each availablePeers as peer}
                    <option value={peer.id}>
                      {peer.name} ({peer.type}) {peer.isOnline ? '🟢 Online' : '⚪ Offline'}
                    </option>
                  {/each}
                </select>
              {/if}
            </div>

            <!-- Manual Device ID Input -->
            <div>
              <label class="block text-xs font-semibold text-slate-300 mb-1">
                Atau Masukkan Device ID Host Manual:
              </label>
              <input
                type="text"
                bind:value={selectedTargetDeviceId}
                placeholder="RAY-XXXX-XXXX-XXXX"
                class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono focus:outline-none focus:border-cyan-500 transition"
              />
            </div>

            <!-- Security PIN Input -->
            <div>
              <label class="block text-xs font-semibold text-slate-300 mb-1">
                PIN Akses Remote Host:
              </label>
              <div class="relative">
                {#if showInputPin}
                  <input
                    type="text"
                    bind:value={inputPin}
                    placeholder="Masukkan 6-digit PIN host..."
                    class="w-full px-3 py-2 pr-10 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition"
                  />
                {:else}
                  <input
                    type="password"
                    bind:value={inputPin}
                    placeholder="Masukkan 6-digit PIN host..."
                    class="w-full px-3 py-2 pr-10 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition"
                  />
                {/if}
                <button
                  type="button"
                  on:click={() => (showInputPin = !showInputPin)}
                  class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition cursor-pointer"
                >
                  {#if showInputPin}
                    <EyeOff class="w-4 h-4" />
                  {:else}
                    <Eye class="w-4 h-4" />
                  {/if}
                </button>
              </div>
              <p class="text-[10px] text-slate-500 mt-1">
                PIN didapatkan dari menu "Akses Remote Komputer Ini" pada perangkat tujuan.
              </p>
            </div>

            <!-- Connect Button -->
            <button
              type="button"
              on:click={handleConnect}
              disabled={$remoteConnectionStatus === 'verifying_pin' || $remoteConnectionStatus === 'connecting'}
              class="w-full py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 disabled:opacity-50 text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition flex items-center justify-center gap-2 cursor-pointer active:scale-95"
            >
              {#if $remoteConnectionStatus === 'verifying_pin'}
                <RefreshCw class="w-4 h-4 animate-spin text-white" />
                <span>Memverifikasi PIN Keamanan...</span>
              {:else if $remoteConnectionStatus === 'connecting'}
                <RefreshCw class="w-4 h-4 animate-spin text-white" />
                <span>Menghubungkan WebRTC Stream...</span>
              {:else}
                <Play class="w-4 h-4 fill-white" />
                <span>Mulai Remote PC Sekarang</span>
              {/if}
            </button>
          </div>
        </div>

        <!-- 2. This Device Host Configuration (Host Mode) -->
        <div class="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 backdrop-blur-xl space-y-4 shadow-xl">
          <div class="flex items-center justify-between pb-2 border-b border-slate-800">
            <div class="flex items-center gap-2.5">
              <Shield class="w-5 h-5 text-indigo-400" />
              <div>
                <h3 class="text-sm font-bold text-white">Akses Remote Komputer Ini (Host)</h3>
                <p class="text-[11px] text-slate-400">Atur izin dan PIN keamanan agar perangkat ini bisa di-remote</p>
              </div>
            </div>

            <!-- Enable/Disable Toggle -->
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                bind:checked={hostEnabled}
                on:change={handleSaveSettings}
                class="sr-only peer"
              />
              <div class="w-9 h-5 bg-slate-800 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-cyan-500"></div>
            </label>
          </div>

          <div class="space-y-4">
            <!-- Host Device ID -->
            <div>
              <span class="block text-xs font-semibold text-slate-300 mb-1">Device ID Anda:</span>
              <div class="flex items-center justify-between p-2 rounded-xl bg-slate-950 border border-slate-800 font-mono text-xs text-cyan-300">
                <span>{$appSettings.device_id}</span>
                <button
                  type="button"
                  on:click={() => {
                    navigator.clipboard.writeText($appSettings.device_id);
                    alert('Device ID berhasil disalin ke clipboard!');
                  }}
                  class="p-1 text-slate-400 hover:text-white"
                  title="Salin Device ID"
                >
                  <Copy class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <!-- Host Security PIN -->
            <div>
              <div class="flex items-center justify-between mb-1">
                <span class="text-xs font-semibold text-slate-300">PIN Keamanan Remote:</span>
                <button
                  type="button"
                  on:click={generateRandomPin}
                  class="text-[11px] text-cyan-400 hover:text-cyan-300 transition"
                >
                  Acak PIN Baru
                </button>
              </div>

              <div class="relative">
                {#if showHostPin}
                  <input
                    type="text"
                    bind:value={hostPin}
                    placeholder="Set 6-digit PIN..."
                    class="w-full px-3 py-2 pr-20 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition"
                  />
                {:else}
                  <input
                    type="password"
                    bind:value={hostPin}
                    placeholder="Set 6-digit PIN..."
                    class="w-full px-3 py-2 pr-20 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 font-mono tracking-widest focus:outline-none focus:border-cyan-500 transition"
                  />
                {/if}
                <div class="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
                  <button
                    type="button"
                    on:click={() => (showHostPin = !showHostPin)}
                    class="p-1 text-slate-500 hover:text-slate-300 transition"
                  >
                    {#if showHostPin}
                      <EyeOff class="w-4 h-4" />
                    {:else}
                      <Eye class="w-4 h-4" />
                    {/if}
                  </button>
                  <button
                    type="button"
                    on:click={handleSaveSettings}
                    disabled={isSavingSettings}
                    class="px-2 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-xs text-cyan-300 font-semibold border border-slate-700 cursor-pointer"
                  >
                    {#if saveSuccess}
                      <Check class="w-3.5 h-3.5 text-emerald-400" />
                    {:else}
                      Simpan
                    {/if}
                  </button>
                </div>
              </div>
              <p class="text-[10px] text-slate-500 mt-1">
                Hanya perangkat yang memasukkan PIN di atas yang dapat mengontrol komputer ini.
              </p>
            </div>

            <!-- Security Info Banner -->
            <div class="p-3 rounded-xl bg-slate-950/70 border border-slate-800 text-xs text-slate-400 space-y-1.5">
              <div class="flex items-center gap-1.5 text-slate-200 font-semibold">
                <Lock class="w-3.5 h-3.5 text-emerald-400" />
                <span>Keamanan P2P Terjamin (Zero-Cloud)</span>
              </div>
              <p class="text-[11px] leading-relaxed">
                Koneksi remote desktop dienkripsi ujung-ke-ujung (E2EE) langsung antar perangkat. Tidak ada video atau ketikan keyboard yang disimpan di server.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
