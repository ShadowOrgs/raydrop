<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    downloadModalOpen,
    prefilledDownloadUrl,
    torrentTasks,
    httpTasks,
    browserExtensionModalOpen,
  } from '../stores/torrents';
  import { appSettings } from '../stores/state';
  import { raydropApi } from '../utils/tauri-ipc';
  import {
    Magnet,
    Download,
    FolderOpen,
    X,
    Clipboard,
    Check,
    AlertCircle,
    Loader2,
    Zap,
    Globe,
    FileText,
    Puzzle,
  } from 'lucide-svelte';

  let urlInput = '';
  let customDownloadPath = '';
  let customFilename = '';
  let isLoading = false;
  let errorMessage = '';
  let successMessage = '';
  let detectedType: 'magnet' | 'http' | 'unknown' = 'unknown';
  let detectedTitle = '';

  $: if ($downloadModalOpen) {
    if ($prefilledDownloadUrl) {
      urlInput = $prefilledDownloadUrl;
      $prefilledDownloadUrl = '';
    }
    if (!customDownloadPath) {
      customDownloadPath = $appSettings.download_dir || '';
    }
    inspectUrl(urlInput);
  }

  function inspectUrl(input: string) {
    errorMessage = '';
    detectedTitle = '';
    const trimmed = (input || '').trim();
    if (!trimmed) {
      detectedType = 'unknown';
      return;
    }

    if (trimmed.startsWith('magnet:?')) {
      detectedType = 'magnet';
      try {
        const parsed = new URL(trimmed);
        const dn = parsed.searchParams.get('dn');
        if (dn) {
          detectedTitle = decodeURIComponent(dn.replace(/\+/g, ' '));
        }
      } catch (_) {
        const match = trimmed.match(/[?&]dn=([^&]+)/i);
        if (match && match[1]) {
          detectedTitle = decodeURIComponent(match[1].replace(/\+/g, ' '));
        }
      }
    } else if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
      detectedType = 'http';
      try {
        const parsed = new URL(trimmed);
        const pathname = parsed.pathname;
        const last = pathname.split('/').pop() || '';
        if (last && last.includes('.')) {
          detectedTitle = decodeURIComponent(last);
        }
      } catch (_) {}
    } else {
      detectedType = 'unknown';
      errorMessage = "URL must start with 'http://', 'https://', or 'magnet:?'";
    }
  }

  $: inspectUrl(urlInput);

  async function browseFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: customDownloadPath || undefined,
        title: 'Select Destination Folder for Download',
      });
      if (typeof selected === 'string' && selected) {
        customDownloadPath = selected;
      }
    } catch (err) {
      console.warn('Failed to select directory:', err);
    }
  }

  async function pasteFromClipboard() {
    try {
      const clip = await raydropApi.readSystemClipboard();
      const trimmed = (clip || '').trim();
      if (
        trimmed.startsWith('magnet:?') ||
        trimmed.startsWith('http://') ||
        trimmed.startsWith('https://')
      ) {
        urlInput = trimmed;
        return;
      }
    } catch (_) {}

    try {
      const text = await navigator.clipboard.readText();
      const trimmed = (text || '').trim();
      if (
        trimmed.startsWith('magnet:?') ||
        trimmed.startsWith('http://') ||
        trimmed.startsWith('https://')
      ) {
        urlInput = trimmed;
        return;
      }
    } catch (_) {}

    errorMessage = 'Clipboard does not contain a valid HTTP, HTTPS, or Magnet URL.';
    setTimeout(() => {
      if (errorMessage.includes('Clipboard does not contain')) {
        errorMessage = '';
      }
    }, 3000);
  }

  async function handleStartDownload() {
    const trimmed = (urlInput || '').trim();
    if (
      !trimmed.startsWith('http://') &&
      !trimmed.startsWith('https://') &&
      !trimmed.startsWith('magnet:?')
    ) {
      errorMessage = 'Please enter a valid HTTP, HTTPS, or Magnet download URL.';
      return;
    }

    isLoading = true;
    errorMessage = '';
    successMessage = '';

    try {
      if (trimmed.startsWith('magnet:?')) {
        // BitTorrent Swarm download
        await raydropApi.addTorrentMagnet(
          trimmed,
          customDownloadPath || undefined
        );
        const list = await raydropApi.listTorrentTasks();
        torrentTasks.set(list);
        successMessage = 'Torrent task added to BitTorrent engine!';
      } else {
        // HTTP Multi-Segment Accelerator download
        await raydropApi.addHttpDownload(
          trimmed,
          customDownloadPath || undefined,
          customFilename.trim() || undefined
        );
        const list = await raydropApi.listHttpDownloads();
        httpTasks.set(list);
        successMessage = 'HTTP Multi-Segment download initiated!';
      }

      setTimeout(() => {
        downloadModalOpen.set(false);
        urlInput = '';
        customFilename = '';
        errorMessage = '';
        successMessage = '';
        isLoading = false;
      }, 1000);
    } catch (err: any) {
      console.error('Failed to initiate download:', err);
      errorMessage =
        typeof err === 'string'
          ? err
          : err?.message || 'Failed to start download';
      isLoading = false;
    }
  }

  function handleClose() {
    downloadModalOpen.set(false);
    errorMessage = '';
    successMessage = '';
  }
</script>

{#if $downloadModalOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-md animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    on:click|self={handleClose}
  >
    <div
      class="w-full max-w-lg max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl p-5 sm:p-6 space-y-5 text-white animate-in zoom-in-95 duration-200"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-slate-800/80 pb-3.5">
        <div class="flex items-center gap-2.5">
          <div
            class="w-9 h-9 rounded-xl flex items-center justify-center {detectedType ===
            'magnet'
              ? 'bg-amber-500/15 border border-amber-500/30 text-amber-400'
              : 'bg-cyan-500/15 border border-cyan-500/30 text-cyan-400'}"
          >
            {#if detectedType === 'magnet'}
              <Magnet class="w-5 h-5" />
            {:else}
              <Download class="w-5 h-5" />
            {/if}
          </div>
          <div>
            <h3 class="text-sm font-bold text-white flex items-center gap-2">
              <span>Add URL / Magnet Download</span>
              {#if detectedType === 'magnet'}
                <span
                  class="text-[9px] font-mono font-semibold px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-300 border border-amber-500/30"
                >
                  BitTorrent P2P Swarm
                </span>
              {:else if detectedType === 'http'}
                <span
                  class="text-[9px] font-mono font-semibold px-2 py-0.5 rounded-full bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 flex items-center gap-1"
                >
                  <Zap class="w-2.5 h-2.5" />
                  HTTP Multi-Segment
                </span>
              {/if}
            </h3>
            <p class="text-[11px] text-slate-400 mt-0.5">
              Direct download accelerator with multi-part streams and pause/resume
            </p>
          </div>
        </div>
        <button
          type="button"
          on:click={handleClose}
          class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition-colors cursor-pointer"
          title="Close modal"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- URL Input Area -->
      <div class="space-y-1.5">
        <div class="flex items-center justify-between text-xs">
          <label for="urlDownloadInput" class="font-semibold text-slate-300">
            Download URL or Magnet Link
          </label>
          <button
            type="button"
            on:click={pasteFromClipboard}
            class="text-[11px] text-cyan-400 hover:text-cyan-300 flex items-center gap-1 font-medium transition cursor-pointer"
          >
            <Clipboard class="w-3 h-3" />
            <span>Paste from Clipboard</span>
          </button>
        </div>
        <div class="relative">
          <textarea
            id="urlDownloadInput"
            bind:value={urlInput}
            rows="3"
            placeholder="Paste http://, https:// direct download link, or magnet:?xt=urn:btih:..."
            class="w-full p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-white placeholder-slate-600 focus:outline-none focus:border-cyan-500 focus:ring-1 focus:ring-cyan-500/30 transition resize-none break-all"
          ></textarea>
        </div>
      </div>

      <!-- Detected Resource Info -->
      {#if detectedTitle}
        <div
          class="p-3 rounded-xl {detectedType === 'magnet'
            ? 'bg-amber-950/20 border border-amber-800/30 text-amber-300'
            : 'bg-cyan-950/20 border border-cyan-800/30 text-cyan-300'} text-xs space-y-1"
        >
          <div class="flex items-center justify-between">
            <span class="text-[10px] font-mono uppercase tracking-wider font-bold">
              Detected File
            </span>
            <span class="text-[10px] font-mono opacity-80">
              {detectedType === 'magnet' ? 'Swarm metadata' : 'URL target'}
            </span>
          </div>
          <p class="font-semibold text-white truncate text-xs" title={detectedTitle}>
            {detectedTitle}
          </p>
        </div>
      {/if}

      <!-- Save Location -->
      <div class="space-y-1.5">
        <label for="downloadFolderInput" class="text-xs font-semibold text-slate-300 block">
          Save Destination Folder
        </label>
        <div class="flex items-center gap-2">
          <input
            id="downloadFolderInput"
            type="text"
            bind:value={customDownloadPath}
            placeholder="C:\Users\...\Downloads"
            class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 focus:outline-none focus:border-cyan-500 transition font-mono truncate"
          />
          <button
            type="button"
            on:click={browseFolder}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white border border-slate-700 text-xs font-semibold flex items-center gap-1.5 transition cursor-pointer shrink-0"
          >
            <FolderOpen class="w-3.5 h-3.5" />
            <span>Browse</span>
          </button>
        </div>
      </div>

      <!-- Optional Custom Filename (for HTTP) -->
      {#if detectedType === 'http'}
        <div class="space-y-1.5">
          <label for="customFilenameInput" class="text-xs font-semibold text-slate-300 block">
            Custom Filename <span class="text-slate-500 font-normal">(Optional)</span>
          </label>
          <input
            id="customFilenameInput"
            type="text"
            bind:value={customFilename}
            placeholder={detectedTitle || "e.g. installer.exe"}
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white placeholder-slate-600 focus:outline-none focus:border-cyan-500 transition font-mono"
          />
        </div>
      {/if}

      <!-- Feedback Alerts -->
      {#if errorMessage}
        <div
          class="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs flex items-center gap-2"
        >
          <AlertCircle class="w-4 h-4 text-rose-400 shrink-0" />
          <span class="break-words">{errorMessage}</span>
        </div>
      {/if}

      {#if successMessage}
        <div
          class="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-xs flex items-center gap-2"
        >
          <Check class="w-4 h-4 text-emerald-400 shrink-0" />
          <span>{successMessage}</span>
        </div>
      {/if}

      <!-- Browser Extension Tip Banner -->
      <div class="p-3 rounded-xl bg-slate-950/80 border border-slate-800 flex items-center justify-between text-xs text-slate-400">
        <span class="flex items-center gap-2">
          <Puzzle class="w-4 h-4 text-cyan-400 shrink-0" />
          <span>Want automatic download catching from Chrome or Edge?</span>
        </span>
        <button
          type="button"
          on:click={() => {
            handleClose();
            $browserExtensionModalOpen = true;
          }}
          class="text-cyan-400 hover:text-cyan-300 font-semibold flex items-center gap-1 transition cursor-pointer shrink-0 ml-2"
        >
          <span>Install Extension</span>
          <span>&rarr;</span>
        </button>
      </div>

      <!-- Action Buttons -->
      <div class="flex items-center justify-end gap-2.5 pt-2 border-t border-slate-800/80">
        <button
          type="button"
          on:click={handleClose}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 text-xs font-semibold transition cursor-pointer"
        >
          Cancel
        </button>
        <button
          type="button"
          on:click={handleStartDownload}
          disabled={isLoading || !urlInput.trim()}
          class="px-5 py-2 rounded-xl bg-gradient-to-r from-cyan-600 to-indigo-600 hover:from-cyan-500 hover:to-indigo-500 text-white text-xs font-semibold shadow-lg shadow-cyan-500/20 flex items-center gap-2 transition active:scale-95 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer"
        >
          {#if isLoading}
            <Loader2 class="w-4 h-4 animate-spin" />
            <span>Connecting...</span>
          {:else}
            <Download class="w-4 h-4" />
            <span>Start Accelerated Download</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
