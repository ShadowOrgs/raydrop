<script lang="ts">
  import { onMount } from 'svelte';
  import {
    raydropApi,
    type RemoteShareEntry,
    type RemoteFileItem,
  } from '../utils/tauri-ipc';
  import {
    Folder,
    FolderOpen,
    FileText,
    FileImage,
    FileVideo,
    FileAudio,
    FileCode,
    FileArchive,
    File,
    Download,
    RefreshCw,
    X,
    ChevronRight,
    ArrowUp,
    Search,
    AlertCircle,
    CheckCircle2,
    HardDrive,
  } from 'lucide-svelte';

  export let peerDeviceId: string;
  export let peerAlias: string = 'Remote Peer';
  export let onClose: () => void;

  let shares: RemoteShareEntry[] = [];
  let selectedShareId: string = '';
  let currentSubpath: string = '';
  let entries: RemoteFileItem[] = [];
  let loadingShares = true;
  let loadingDir = false;
  let errorMessage = '';
  let successBanner = '';
  let searchQuery = '';
  let pullingPaths: Record<string, boolean> = {};

  $: breadcrumbs = currentSubpath
    ? currentSubpath.split('/').filter(Boolean)
    : [];

  $: filteredEntries = entries.filter((e) =>
    e.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

  onMount(async () => {
    await loadShares();
  });

  async function loadShares() {
    loadingShares = true;
    errorMessage = '';
    try {
      shares = await raydropApi.browseRemoteShares(peerDeviceId);
      if (shares.length > 0) {
        selectedShareId = shares[0].id;
        await loadDirectory('', selectedShareId);
      } else {
        errorMessage = 'No shared folders exposed by this device';
      }
    } catch (err: any) {
      errorMessage = err?.toString() || 'Failed to connect to remote device';
    } finally {
      loadingShares = false;
    }
  }

  async function loadDirectory(subpath: string, shareId = selectedShareId) {
    if (!shareId) return;
    loadingDir = true;
    errorMessage = '';
    searchQuery = '';
    try {
      entries = await raydropApi.browseRemoteDir(peerDeviceId, shareId, subpath);
      currentSubpath = subpath;
    } catch (err: any) {
      errorMessage = err?.toString() || 'Failed to list directory contents';
    } finally {
      loadingDir = false;
    }
  }

  function handleSelectShare(shareId: string) {
    if (selectedShareId === shareId) return;
    selectedShareId = shareId;
    currentSubpath = '';
    loadDirectory('', shareId);
  }

  function handleNavigateBreadcrumb(index: number) {
    if (index === -1) {
      loadDirectory('');
      return;
    }
    const newPath = breadcrumbs.slice(0, index + 1).join('/');
    loadDirectory(newPath);
  }

  function handleGoUp() {
    if (!breadcrumbs.length) return;
    const parentPath = breadcrumbs.slice(0, -1).join('/');
    loadDirectory(parentPath);
  }

  function handleClickItem(item: RemoteFileItem) {
    if (item.is_dir) {
      loadDirectory(item.relative_path);
    } else {
      handlePull(item);
    }
  }

  async function handlePull(item: RemoteFileItem) {
    const key = item.relative_path;
    pullingPaths[key] = true;
    pullingPaths = { ...pullingPaths };
    errorMessage = '';
    successBanner = '';

    try {
      const msg = await raydropApi.pullRemoteFile(
        peerDeviceId,
        selectedShareId,
        item.relative_path
      );
      successBanner = `Requested "${item.name}"! Peer is sending the file now.`;
      setTimeout(() => (successBanner = ''), 4000);
    } catch (err: any) {
      errorMessage = err?.toString() || `Failed to pull ${item.name}`;
    } finally {
      pullingPaths[key] = false;
      pullingPaths = { ...pullingPaths };
    }
  }

  function formatBytes(bytes: number): string {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatModified(ms: number): string {
    if (!ms) return '--';
    const d = new Date(ms);
    return `${d.toLocaleDateString()} ${d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
  }

  function getFileIcon(item: RemoteFileItem) {
    if (item.is_dir) return Folder;
    const ext = item.name.split('.').pop()?.toLowerCase() || '';
    if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg'].includes(ext))
      return FileImage;
    if (['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(ext)) return FileVideo;
    if (['mp3', 'wav', 'flac', 'aac', 'm4a', 'ogg'].includes(ext))
      return FileAudio;
    if (['zip', 'rar', '7z', 'tar', 'gz'].includes(ext)) return FileArchive;
    if (['ts', 'js', 'json', 'html', 'css', 'rs', 'py', 'go'].includes(ext))
      return FileCode;
    if (['pdf', 'doc', 'docx', 'txt', 'md'].includes(ext)) return FileText;
    return File;
  }
</script>

<div
  class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-4"
  role="dialog"
  aria-modal="true"
>
  <div
    class="w-full max-w-3xl bg-slate-900 border border-indigo-500/30 rounded-2xl p-6 shadow-2xl flex flex-col gap-4 h-[85vh] max-h-[750px] overflow-hidden"
  >
    <!-- Header -->
    <div
      class="flex items-center justify-between pb-3 border-b border-slate-800 shrink-0"
    >
      <div class="flex items-center gap-3">
        <div
          class="w-10 h-10 rounded-xl bg-gradient-to-br from-indigo-500/20 to-sky-500/20 border border-indigo-500/40 flex items-center justify-center text-indigo-400"
        >
          <HardDrive class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white flex items-center gap-2">
            <span>Remote Folder Browser</span>
            <span
              class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-indigo-500/20 text-indigo-300 border border-indigo-500/30"
            >
              {peerAlias}
            </span>
          </h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Browse remote folders on-demand and pull files directly to your machine
          </p>
        </div>
      </div>
      <button
        type="button"
        on:click={onClose}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors"
        title="Close"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Alert / Banner -->
    {#if errorMessage}
      <div
        class="p-2.5 rounded-xl text-xs bg-rose-500/10 border border-rose-500/30 text-rose-300 flex items-center justify-between shrink-0"
      >
        <div class="flex items-center gap-2">
          <AlertCircle class="w-4 h-4 shrink-0 text-rose-400" />
          <span>{errorMessage}</span>
        </div>
        <button
          type="button"
          on:click={() => (errorMessage = '')}
          class="text-rose-400 hover:text-white ml-2"
        >
          ✕
        </button>
      </div>
    {/if}

    {#if successBanner}
      <div
        class="p-2.5 rounded-xl text-xs bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 flex items-center justify-between shrink-0"
      >
        <div class="flex items-center gap-2">
          <CheckCircle2 class="w-4 h-4 shrink-0 text-emerald-400" />
          <span>{successBanner}</span>
        </div>
        <button
          type="button"
          on:click={() => (successBanner = '')}
          class="text-emerald-400 hover:text-white ml-2"
        >
          ✕
        </button>
      </div>
    {/if}

    <!-- Shared Folders Tabs & Search Bar -->
    <div class="flex flex-wrap items-center justify-between gap-2 shrink-0">
      <!-- Tabs of Shares -->
      <div class="flex items-center gap-1.5 overflow-x-auto py-1 max-w-full">
        {#if loadingShares}
          <span class="text-xs text-slate-500 italic">Discovering shared folders...</span>
        {:else}
          {#each shares as share}
            <button
              type="button"
              on:click={() => handleSelectShare(share.id)}
              class="px-3 py-1.5 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition border shrink-0 {selectedShareId ===
              share.id
                ? 'bg-indigo-600/30 border-indigo-500/60 text-indigo-200'
                : 'bg-slate-800/60 border-slate-700/60 text-slate-400 hover:text-slate-200 hover:bg-slate-800'}"
            >
              <Folder class="w-3.5 h-3.5" />
              <span>{share.name}</span>
            </button>
          {/each}
        {/if}
      </div>

      <!-- Quick Search Input -->
      <div class="relative w-48 shrink-0">
        <Search class="w-3.5 h-3.5 absolute left-2.5 top-2.5 text-slate-500" />
        <input
          type="text"
          bind:value={searchQuery}
          placeholder="Filter files..."
          class="w-full pl-8 pr-3 py-1.5 rounded-xl bg-slate-950/70 border border-slate-800 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-indigo-500"
        />
      </div>
    </div>

    <!-- Breadcrumb Path Bar -->
    <div
      class="flex items-center justify-between px-3 py-2 rounded-xl bg-slate-950/60 border border-slate-800/80 text-xs shrink-0"
    >
      <div class="flex items-center gap-1 overflow-x-auto min-w-0 pr-2">
        <button
          type="button"
          on:click={handleGoUp}
          disabled={breadcrumbs.length === 0}
          class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 disabled:opacity-30 disabled:hover:bg-transparent mr-1"
          title="Parent directory"
        >
          <ArrowUp class="w-3.5 h-3.5" />
        </button>

        <button
          type="button"
          on:click={() => handleNavigateBreadcrumb(-1)}
          class="text-indigo-400 hover:underline font-semibold shrink-0"
        >
          Root
        </button>

        {#each breadcrumbs as part, idx}
          <ChevronRight class="w-3 h-3 text-slate-600 shrink-0" />
          <button
            type="button"
            on:click={() => handleNavigateBreadcrumb(idx)}
            class="text-slate-300 hover:text-white hover:underline truncate max-w-[120px]"
          >
            {part}
          </button>
        {/each}
      </div>

      <button
        type="button"
        on:click={() => loadDirectory(currentSubpath)}
        class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition shrink-0"
        title="Reload folder"
      >
        <RefreshCw class="w-3.5 h-3.5 {loadingDir ? 'animate-spin' : ''}" />
      </button>
    </div>

    <!-- Explorer File Table -->
    <div class="flex-1 overflow-y-auto rounded-xl border border-slate-800/80 bg-slate-950/40 divide-y divide-slate-800/60">
      {#if loadingDir}
        <div class="p-12 text-center text-xs text-slate-400 flex flex-col items-center gap-2">
          <RefreshCw class="w-6 h-6 animate-spin text-indigo-400" />
          <span>Listing remote directory contents...</span>
        </div>
      {:else if filteredEntries.length === 0}
        <div class="p-12 text-center text-xs text-slate-500 flex flex-col items-center gap-2">
          <FolderOpen class="w-8 h-8 text-slate-700" />
          <span>This folder is empty</span>
        </div>
      {:else}
        {#each filteredEntries as item (item.relative_path)}
          {@const IconComponent = getFileIcon(item)}
          <div
            class="px-3.5 py-2.5 flex items-center justify-between gap-3 hover:bg-slate-800/40 transition group"
          >
            <!-- Left: Icon + Name -->
            <button
              type="button"
              on:click={() => handleClickItem(item)}
              class="flex items-center gap-2.5 min-w-0 text-left flex-1"
            >
              <div
                class="w-7 h-7 rounded-lg flex items-center justify-center shrink-0 {item.is_dir
                  ? 'bg-amber-500/15 text-amber-400'
                  : 'bg-indigo-500/15 text-indigo-400'}"
              >
                <svelte:component this={IconComponent} class="w-4 h-4" />
              </div>
              <div class="min-w-0">
                <span
                  class="text-xs font-medium text-slate-200 group-hover:text-white truncate block"
                  title={item.name}
                >
                  {item.name}
                </span>
                <span class="text-[10px] text-slate-500 font-mono">
                  {formatModified(item.modified_unix_ms)}
                </span>
              </div>
            </button>

            <!-- Right: Size & Pull Action -->
            <div class="flex items-center gap-3 shrink-0">
              <span class="text-xs font-mono text-slate-400 w-20 text-right">
                {item.is_dir ? '--' : formatBytes(item.size_bytes)}
              </span>

              {#if item.is_dir}
                <button
                  type="button"
                  on:click={() => loadDirectory(item.relative_path)}
                  class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-medium transition"
                >
                  Open
                </button>
              {:else}
                <button
                  type="button"
                  on:click={() => handlePull(item)}
                  disabled={pullingPaths[item.relative_path]}
                  class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-indigo-600/30 hover:bg-indigo-600/50 text-indigo-300 border border-indigo-500/40 text-xs font-semibold shadow-sm transition disabled:opacity-50"
                  title="Pull / Download file to your device"
                >
                  <Download
                    class="w-3 h-3 {pullingPaths[item.relative_path]
                      ? 'animate-bounce'
                      : ''}"
                  />
                  <span>{pullingPaths[item.relative_path] ? 'Pulling...' : 'Pull'}</span>
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Footer -->
    <div
      class="flex items-center justify-between pt-2 border-t border-slate-800 text-xs shrink-0"
    >
      <span class="text-[11px] font-mono text-slate-500">
        Direct P2P On-Demand Fetch • End-to-End Encrypted
      </span>
      <button
        type="button"
        on:click={onClose}
        class="px-4 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 font-semibold text-xs transition"
      >
        Close
      </button>
    </div>
  </div>
</div>
