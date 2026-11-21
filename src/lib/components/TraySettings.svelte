<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { appSettings, settingsModalOpen } from "../stores/state";
  import {
    updateInfo,
    updateModalOpen,
    updateChecking,
    updateStatusMessage,
  } from "../stores/update";
  import { checkForUpdates } from "../utils/updater";
  import { raydropApi } from "../utils/tauri-ipc";
  import { browserExtensionModalOpen } from "../stores/torrents";
  import { myAvatar } from "../stores/avatars";
  import {
    FolderOpen,
    Terminal,
    Save,
    X,
    CheckCircle2,
    Power,
    RefreshCw,
    Sparkles,
    Gauge,
    Puzzle,
    Camera,
    Upload,
    Folders,
    Layers,
    Plus,
    Trash2,
    HardDrive,
  } from "lucide-svelte";

  let aliasName = "";
  let downloadDir = "";
  let signalingUrl = "";
  let stunAddr = "";
  let autostart = true;
  let speedLimitMbps = 0;
  let autoCategorize = true;
  let maxConcurrentDownloads = 3;
  let win11DirectMenu = false;
  let statusBanner = "";
  let sharedFolders: Array<{
    id: string;
    name: string;
    local_path: string;
    is_active: boolean;
  }> = [];

  $: if ($settingsModalOpen) {
    aliasName = $appSettings.alias_name;
    downloadDir = $appSettings.download_dir;
    signalingUrl = $appSettings.signaling_url;
    stunAddr = $appSettings.stun_addr;
    autostart = $appSettings.autostart ?? true;
    speedLimitMbps = $appSettings.speed_limit_mbps ?? 0;
    autoCategorize = $appSettings.auto_categorize ?? true;
    maxConcurrentDownloads = $appSettings.max_concurrent_downloads ?? 3;
    raydropApi
      .getWin11DirectMenu()
      .then((res) => (win11DirectMenu = res))
      .catch(() => {});
    loadSharedFolders();
  }

  async function loadSharedFolders() {
    try {
      sharedFolders = await raydropApi.listSharedFolders();
    } catch (err) {
      console.warn("Failed to list shared folders:", err);
    }
  }

  async function handleAddSharedFolder() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Select Local Folder to Share for Remote Browsing",
      });
      if (typeof picked === "string") {
        const parts = picked.split(/[/\\]/);
        const defaultName =
          parts[parts.length - 1] || parts[parts.length - 2] || "Shared Folder";
        await raydropApi.addSharedFolder(defaultName, picked);
        await loadSharedFolders();
        statusBanner = "Shared folder added successfully!";
        setTimeout(() => (statusBanner = ""), 3000);
      }
    } catch (err) {
      console.warn("Failed to add shared folder:", err);
    }
  }

  async function handleDeleteSharedFolder(id: string) {
    try {
      await raydropApi.deleteSharedFolder(id);
      await loadSharedFolders();
    } catch (err) {
      console.warn("Failed to delete shared folder:", err);
    }
  }

  async function handleUploadAvatar() {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Images",
            extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"],
          },
        ],
        title: "Choose Profile Avatar",
      });
      if (typeof picked === "string") {
        const b64 = await raydropApi.setMyAvatar(picked);
        myAvatar.set(b64);
        statusBanner =
          "Profile avatar updated and synced across all your devices!";
        setTimeout(() => (statusBanner = ""), 3000);
      }
    } catch (err) {
      console.warn("Failed to upload avatar:", err);
    }
  }

  async function browseDownloadDir() {
    const dir = await open({
      directory: true,
      multiple: false,
      defaultPath: downloadDir,
      title: "Select Default Download Directory",
    });
    if (typeof dir === "string") {
      downloadDir = dir;
    }
  }

  async function saveSettings() {
    const updated = await raydropApi.updateSettings({
      aliasName,
      downloadDir,
      signalingUrl,
      stunAddr,
      autostart,
      speedLimitMbps,
      autoCategorize,
      maxConcurrentDownloads,
    });
    appSettings.set(updated);
    statusBanner = "Settings saved! Autorun & Signaling configuration updated.";
    setTimeout(() => (statusBanner = ""), 3000);
  }

  async function handleSpeedLimitChange(val: number) {
    speedLimitMbps = val;
    try {
      await raydropApi.setSpeedLimit(val);
      appSettings.update((s) => ({ ...s, speed_limit_mbps: val }));
      statusBanner =
        val === 0
          ? "Speed limit removed (Unlimited Max Speed)."
          : `Outgoing transfer speed limited to ${val} MB/s.`;
      setTimeout(() => (statusBanner = ""), 2500);
    } catch (err: any) {
      statusBanner = `Speed limit error: ${String(err)}`;
    }
  }

  async function handleToggleAutostart() {
    try {
      const next = !autostart;
      autostart = await raydropApi.toggleAutostart(next);
      appSettings.update((s) => ({ ...s, autostart }));
      statusBanner = autostart
        ? "Autorun at OS Startup enabled (starts minimized in System Tray)."
        : "Autorun at OS Startup disabled.";
      setTimeout(() => (statusBanner = ""), 3000);
    } catch (e: any) {
      statusBanner = `Autorun error: ${String(e)}`;
    }
  }

  async function handleRegisterContextMenu() {
    try {
      const msg = await raydropApi.registerOsContextMenu();
      statusBanner = msg;
    } catch (e: any) {
      statusBanner = `Context menu error: ${String(e)}`;
    }
  }

  async function handleToggleWin11DirectMenu() {
    try {
      const next = !win11DirectMenu;
      win11DirectMenu = await raydropApi.toggleWin11DirectMenu(next);
      statusBanner = win11DirectMenu
        ? "Direct Right-Click enabled! Explorer refreshed. Raydrop now shows on normal right-click."
        : "Windows 11 modern context menu restored.";
      setTimeout(() => (statusBanner = ""), 4500);
    } catch (e: any) {
      statusBanner = `Windows 11 menu error: ${String(e)}`;
    }
  }
</script>

{#if $settingsModalOpen}
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4 select-none"
  >
    <div
      class="w-full max-w-lg bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl flex flex-col max-h-[90vh] overflow-hidden"
    >
      <div
        class="flex items-center justify-between pb-3 border-b border-slate-800 shrink-0"
      >
        <div>
          <h3 class="text-base font-bold text-white">Raydrop Configuration</h3>
          <p class="text-xs text-slate-400">
            Configure Device Alias, Dokploy WAN Signaling/STUN endpoints, and OS
            Context Menus
          </p>
        </div>
        <button
          on:click={() => settingsModalOpen.set(false)}
          class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="space-y-4 py-4 flex-1 overflow-y-auto pr-1">
        <!-- Profile Avatar Section -->
        <div
          class="p-3 rounded-xl bg-slate-950/70 border border-slate-800/80 flex items-center justify-between gap-3"
        >
          <div class="flex items-center gap-3">
            <div
              class="relative w-12 h-12 rounded-xl overflow-hidden border border-slate-700 bg-slate-800 flex items-center justify-center shrink-0"
            >
              {#if $myAvatar}
                <img
                  src={$myAvatar}
                  alt="Profile Avatar"
                  class="w-full h-full object-cover"
                />
              {:else}
                <span class="text-sm font-bold text-cyan-300">
                  {(aliasName || "RD").slice(0, 2).toUpperCase()}
                </span>
              {/if}
            </div>
            <div>
              <p class="text-xs font-semibold text-white">Profile Avatar</p>
              <p class="text-[11px] text-slate-400">
                Synced to other peers during chat & transfer
              </p>
            </div>
          </div>
          <button
            type="button"
            on:click={handleUploadAvatar}
            class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold border border-slate-700 flex items-center gap-1.5 transition cursor-pointer"
          >
            <Camera class="w-3.5 h-3.5 text-cyan-400" />
            <span>Upload Avatar</span>
          </button>
        </div>

        <div>
          <label
            for="setting-alias"
            class="block text-xs font-medium text-slate-300 mb-1"
            >Device Alias Name</label
          >
          <input
            id="setting-alias"
            type="text"
            bind:value={aliasName}
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
          />
        </div>

        <div>
          <label
            for="setting-dl-dir"
            class="block text-xs font-medium text-slate-300 mb-1"
          >
            Default Download Folder (Used for Auto-Accept & Default Prompt)
          </label>
          <div class="flex items-center gap-2">
            <input
              id="setting-dl-dir"
              type="text"
              bind:value={downloadDir}
              class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-500"
            />
            <button
              on:click={browseDownloadDir}
              class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs text-slate-200 flex items-center gap-1.5"
            >
              <FolderOpen class="w-3.5 h-3.5 text-cyan-400" />
              <span>Browse</span>
            </button>
          </div>
        </div>

        <!-- Shared Folders (Remote Browser / On-Demand Pulling) -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex flex-col gap-2.5"
        >
          <div class="flex items-center justify-between">
            <div>
              <p
                class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
              >
                <HardDrive class="w-3.5 h-3.5 text-cyan-400" />
                <span>Shared Folders (Remote On-Demand Browser)</span>
              </p>
              <p class="text-[11px] text-slate-400 mt-0.5">
                Folders configured here can be browsed and pulled on-demand by
                your connected devices and trusted peers.
              </p>
            </div>
            <button
              type="button"
              on:click={handleAddSharedFolder}
              class="px-2.5 py-1.5 rounded-xl bg-cyan-500/15 hover:bg-cyan-500/25 border border-cyan-500/40 text-cyan-300 text-xs font-semibold flex items-center gap-1 shrink-0 transition"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>+ Share Folder</span>
            </button>
          </div>

          {#if sharedFolders.length > 0}
            <div class="space-y-1.5 pt-1">
              {#each sharedFolders as sf (sf.id)}
                <div
                  class="p-2 rounded-lg bg-slate-900 border border-slate-800/80 flex items-center justify-between gap-2"
                >
                  <div class="min-w-0">
                    <span
                      class="text-xs font-medium text-slate-200 block truncate"
                      >{sf.name}</span
                    >
                    <span
                      class="text-[10px] font-mono text-slate-500 block truncate"
                      >{sf.local_path}</span
                    >
                  </div>
                  <button
                    type="button"
                    on:click={() => handleDeleteSharedFolder(sf.id)}
                    class="p-1 rounded-md text-slate-500 hover:text-rose-400 hover:bg-rose-500/10 transition shrink-0"
                    title="Remove shared folder"
                  >
                    <Trash2 class="w-3.5 h-3.5" />
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <div class="text-[11px] text-slate-500 italic pt-0.5">
              Raydrop Downloads folder is shared by default. Click "+ Share
              Folder" to share additional directories.
            </div>
          {/if}
        </div>

        <!-- IDM-style Auto-Categorize Subfolders -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <p
              class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
            >
              <Folders class="w-3.5 h-3.5 text-cyan-400" />
              <span>Smart Download Categorization</span>
            </p>
            <p class="text-[11px] text-slate-400">
              Automatically sort downloads into subfolders: <span
                class="text-slate-300"
                >Documents, Videos, Music, Compressed, Programs</span
              >.
            </p>
          </div>
          <button
            type="button"
            on:click={() => (autoCategorize = !autoCategorize)}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {autoCategorize
              ? 'bg-cyan-500/15 border-cyan-500/40 text-cyan-300'
              : 'bg-slate-800 border-slate-700 text-slate-400'}"
          >
            <span>{autoCategorize ? "Enabled (ON)" : "Disabled (OFF)"}</span>
          </button>
        </div>

        <!-- Download Queue & Concurrency Limiter -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex flex-col gap-2.5"
        >
          <div class="flex items-center justify-between">
            <div>
              <p
                class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
              >
                <Layers class="w-3.5 h-3.5 text-cyan-400" />
                <span>Max Concurrent Downloads (Queue)</span>
              </p>
              <p class="text-[11px] text-slate-400 mt-0.5">
                Limit simultaneously running downloads. Extra tasks are queued
                and start automatically.
              </p>
            </div>
            <span
              class="text-xs font-mono font-bold px-2.5 py-0.5 rounded-full {maxConcurrentDownloads ===
              0
                ? 'bg-purple-500/15 text-purple-300 border border-purple-500/30'
                : 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/30'}"
            >
              {maxConcurrentDownloads === 0
                ? "Unlimited"
                : `${maxConcurrentDownloads} Active`}
            </span>
          </div>
          <div class="flex items-center gap-1.5 flex-wrap pt-1">
            {#each [{ val: 1, label: "1 Download" }, { val: 2, label: "2 Downloads" }, { val: 3, label: "3 (Default)" }, { val: 5, label: "5 Downloads" }, { val: 0, label: "Unlimited" }] as opt}
              <button
                type="button"
                on:click={() => (maxConcurrentDownloads = opt.val)}
                class="px-2.5 py-1 rounded-lg text-xs font-mono font-semibold transition-all {maxConcurrentDownloads ===
                opt.val
                  ? 'bg-cyan-500 text-slate-950 font-bold shadow-sm shadow-cyan-500/30'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700'}"
              >
                {opt.label}
              </button>
            {/each}
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label
              for="setting-sig-url"
              class="block text-xs font-medium text-slate-300 mb-1"
            >
              WAN Signaling WSS URL (Traefik)
            </label>
            <input
              id="setting-sig-url"
              type="text"
              bind:value={signalingUrl}
              placeholder="wss://stun.raydrop.web.id/ws"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 focus:outline-none focus:border-cyan-500"
            />
          </div>
          <div>
            <label
              for="setting-stun-addr"
              class="block text-xs font-medium text-slate-300 mb-1"
            >
              UDP STUN Server (Port 3478)
            </label>
            <input
              id="setting-stun-addr"
              type="text"
              bind:value={stunAddr}
              placeholder="stun.raydrop.web.id:3478"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-cyan-300 focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        <!-- Bandwidth Speed Limiter (QoS) -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex flex-col gap-2.5"
        >
          <div class="flex items-center justify-between">
            <div>
              <p
                class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
              >
                <Gauge class="w-3.5 h-3.5 text-cyan-400" />
                <span>Bandwidth Speed Limiter (QoS)</span>
              </p>
              <p class="text-[11px] text-slate-400 mt-0.5">
                Cap outgoing transfer speeds so transfers don't saturate home or
                office Wi-Fi.
              </p>
            </div>
            <span
              class="text-xs font-mono font-bold px-2.5 py-0.5 rounded-full {speedLimitMbps ===
              0
                ? 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/30'
                : 'bg-amber-500/15 text-amber-300 border border-amber-500/30'}"
            >
              {speedLimitMbps === 0 ? "Unlimited" : `${speedLimitMbps} MB/s`}
            </span>
          </div>

          <div class="flex items-center gap-1.5 flex-wrap pt-1">
            {#each [0, 5, 10, 25, 50] as preset}
              <button
                type="button"
                on:click={() => handleSpeedLimitChange(preset)}
                class="px-2.5 py-1 rounded-lg text-xs font-mono font-semibold transition-all {speedLimitMbps ===
                preset
                  ? 'bg-cyan-500 text-slate-950 font-bold shadow-sm shadow-cyan-500/30'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700'}"
              >
                {preset === 0 ? "Unlimited (Max)" : `${preset} MB/s`}
              </button>
            {/each}
          </div>
        </div>

        <!-- Autorun at Startup Toggle -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <p
              class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
            >
              <Power class="w-3.5 h-3.5 text-cyan-400" />
              <span>Autorun at OS Startup</span>
            </p>
            <p class="text-[11px] text-slate-400">
              Automatically launch Raydrop minimized in the System Tray when
              Windows, macOS, or Linux boots up.
            </p>
          </div>
          <button
            type="button"
            on:click={handleToggleAutostart}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {autostart
              ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300'
              : 'bg-slate-800 border-slate-700 text-slate-400'}"
          >
            <span>{autostart ? "Enabled (ON)" : "Disabled (OFF)"}</span>
          </button>
        </div>

        <!-- OS Context Menu Integration -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <p class="text-xs font-semibold text-slate-200">
              Native OS Right-Click Integration
            </p>
            <p class="text-[11px] text-slate-400">
              Adds "Send with Raydrop" to Windows Explorer (Files, Folders &
              SendTo), Ubuntu Nautilus, or macOS Finder.
            </p>
          </div>
          <button
            on:click={handleRegisterContextMenu}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 border border-cyan-500/30 text-cyan-300 text-xs font-semibold flex items-center gap-1.5 shrink-0"
          >
            <Terminal class="w-3.5 h-3.5" />
            <span>Register Menu</span>
          </button>
        </div>

        <!-- Windows 11 Direct Right-Click Bypass -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <div class="flex items-center gap-2">
              <p class="text-xs font-semibold text-slate-200">
                Windows 11 Direct Right-Click
              </p>
              <span
                class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-300 border border-cyan-500/20"
              >
                No Shift Needed
              </span>
            </div>
            <p class="text-[11px] text-slate-400 mt-0.5">
              Bypasses Windows 11's collapsed "Show more options" so "Send with
              Raydrop" appears immediately on normal right-click.
            </p>
          </div>
          <button
            type="button"
            on:click={handleToggleWin11DirectMenu}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {win11DirectMenu
              ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300 shadow-sm shadow-emerald-500/10'
              : 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-400 hover:text-white'}"
          >
            <span
              >{win11DirectMenu
                ? "Enabled (Direct)"
                : "Disabled (Default Win 11)"}</span
            >
          </button>
        </div>

        <!-- Browser Extension Integration (IDM Mode) -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <div class="flex items-center gap-2">
              <p
                class="text-xs font-semibold text-slate-200 flex items-center gap-1.5"
              >
                <Puzzle class="w-3.5 h-3.5 text-cyan-400" />
                <span>Browser Extension (IDM Mode)</span>
              </p>
              <span
                class="text-[9px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-300 border border-cyan-500/20"
              >
                Chrome / Edge
              </span>
            </div>
            <p class="text-[11px] text-slate-400 mt-0.5">
              Automatically intercept and accelerate large file downloads
              directly from Chrome, Edge, and Brave into Raydrop.
            </p>
          </div>
          <button
            type="button"
            on:click={() => {
              settingsModalOpen.set(false);
              browserExtensionModalOpen.set(true);
            }}
            class="px-3 py-2 rounded-xl bg-gradient-to-r from-cyan-600/80 to-indigo-600/80 hover:from-cyan-500 hover:to-indigo-500 text-white text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all cursor-pointer shadow-md shadow-cyan-500/20"
          >
            <Puzzle class="w-3.5 h-3.5" />
            <span>Install Extension</span>
          </button>
        </div>

        <!-- Software Updates -->
        <div
          class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 flex items-center justify-between gap-3"
        >
          <div>
            <div class="flex items-center gap-2">
              <p class="text-xs font-semibold text-slate-200">
                Software Updates
              </p>
              <span
                class="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 border border-slate-700"
              >
                Current: v1.2.32
              </span>
            </div>
            <p class="text-[11px] text-slate-400 mt-0.5">
              {#if $updateInfo?.has_update}
                <span class="text-cyan-300 font-medium"
                  >New release {$updateInfo.latest_version} available!</span
                >
              {:else if $updateStatusMessage}
                <span class="text-emerald-400">{$updateStatusMessage}</span>
              {:else}
                Check for updates and automatically upgrade to latest release.
              {/if}
            </p>
          </div>
          <button
            type="button"
            on:click={() => {
              if ($updateInfo?.has_update) {
                updateModalOpen.set(true);
              } else {
                checkForUpdates(true);
              }
            }}
            disabled={$updateChecking}
            class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 shrink-0 transition-all {$updateInfo?.has_update
              ? 'bg-cyan-500 hover:bg-cyan-400 text-slate-950 border-cyan-400 shadow-md shadow-cyan-500/20'
              : 'bg-slate-800 hover:bg-slate-700 border-slate-700 text-slate-300'}"
          >
            {#if $updateChecking}
              <RefreshCw class="w-3.5 h-3.5 animate-spin text-cyan-400" />
              <span>Checking...</span>
            {:else if $updateInfo?.has_update}
              <Sparkles class="w-3.5 h-3.5" />
              <span>View Update</span>
            {:else}
              <RefreshCw class="w-3.5 h-3.5" />
              <span>Check Updates</span>
            {/if}
          </button>
        </div>

        {#if statusBanner}
          <div
            class="flex items-center gap-2 text-xs text-emerald-300 bg-emerald-500/10 border border-emerald-500/30 px-3 py-2 rounded-xl"
          >
            <CheckCircle2 class="w-4 h-4 shrink-0" />
            <span>{statusBanner}</span>
          </div>
        {/if}
      </div>

      <div
        class="flex items-center justify-end gap-2 pt-4 border-t border-slate-800 shrink-0"
      >
        <button
          on:click={() => settingsModalOpen.set(false)}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300 transition-colors"
        >
          Close
        </button>
        <button
          on:click={saveSettings}
          class="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-xs font-bold text-slate-950 shadow-md shadow-cyan-500/20 active:scale-95 transition-all"
        >
          <Save class="w-3.5 h-3.5" />
          <span>Save Configuration</span>
        </button>
      </div>
    </div>
  </div>
{/if}
