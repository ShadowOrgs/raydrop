<script lang="ts">
  import { onMount } from 'svelte';
  import { browserExtensionModalOpen } from '../stores/torrents';
  import { raydropApi } from '../utils/tauri-ipc';
  import {
    Puzzle,
    X,
    FolderOpen,
    ExternalLink,
    CheckCircle2,
    Copy,
    Check,
    Zap,
    Download,
    Globe,
    Layers,
    Sparkles,
    RefreshCw,
  } from 'lucide-svelte';

  let extensionPath = '';
  let isOpeningFolder = false;
  let copiedPath = false;
  let copiedChromeUrl = false;
  let copiedEdgeUrl = false;
  let bridgeStatus: 'checking' | 'active' | 'offline' = 'checking';

  async function checkBridge() {
    bridgeStatus = 'checking';
    try {
      const res = await fetch('http://127.0.0.1:45321/api/ping');
      if (res.ok) {
        bridgeStatus = 'active';
        return;
      }
    } catch (_) {}
    bridgeStatus = 'offline';
  }

  async function handleOpenFolder() {
    isOpeningFolder = true;
    try {
      const p = await raydropApi.openBrowserExtensionDir();
      extensionPath = p;
    } catch (err) {
      console.warn('Failed to open extension folder:', err);
    } finally {
      isOpeningFolder = false;
    }
  }

  async function handleOpenBrowserUrl(browser: 'chrome' | 'edge' | 'brave') {
    try {
      await raydropApi.openBrowserExtensionUrl(browser);
    } catch (_) {}
  }

  function copyText(text: string, type: 'path' | 'chrome' | 'edge') {
    navigator.clipboard.writeText(text);
    if (type === 'path') {
      copiedPath = true;
      setTimeout(() => (copiedPath = false), 2000);
    } else if (type === 'chrome') {
      copiedChromeUrl = true;
      setTimeout(() => (copiedChromeUrl = false), 2000);
    } else if (type === 'edge') {
      copiedEdgeUrl = true;
      setTimeout(() => (copiedEdgeUrl = false), 2000);
    }
  }

  onMount(() => {
    checkBridge();
  });
</script>

{#if $browserExtensionModalOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-3 sm:p-5"
    role="dialog"
    aria-modal="true"
    on:click={() => ($browserExtensionModalOpen = false)}
  >
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div
      class="w-full max-w-xl bg-slate-900 border border-cyan-500/30 rounded-2xl p-5 sm:p-6 shadow-2xl flex flex-col gap-4 max-h-[92vh] overflow-y-auto"
      role="document"
      on:click|stopPropagation
    >
      <!-- Header -->
      <div class="flex items-start justify-between pb-3 border-b border-slate-800">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 flex items-center justify-center text-cyan-400 shrink-0">
            <Puzzle class="w-5 h-5" />
          </div>
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-base font-bold text-white">Browser Extension (IDM Mode)</h3>
              <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
                Manifest V3
              </span>
            </div>
            <p class="text-xs text-slate-400 mt-0.5">
              Auto-catch downloads from Chrome, Edge, Brave, and Opera into Raydrop Multi-Segment Accelerator.
            </p>
          </div>
        </div>

        <button
          type="button"
          on:click={() => ($browserExtensionModalOpen = false)}
          class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors cursor-pointer"
          title="Close"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Live Bridge Status Banner -->
      <div class="p-3 rounded-xl bg-slate-950/70 border border-slate-800 flex items-center justify-between">
        <div class="flex items-center gap-2.5 text-xs">
          {#if bridgeStatus === 'active'}
            <span class="w-2.5 h-2.5 rounded-full bg-emerald-400 animate-pulse"></span>
            <span class="text-emerald-300 font-semibold">Desktop Bridge Active</span>
            <span class="text-slate-500 font-mono text-[11px] hidden sm:inline">(http://127.0.0.1:45321)</span>
          {:else if bridgeStatus === 'checking'}
            <RefreshCw class="w-3.5 h-3.5 text-cyan-400 animate-spin" />
            <span class="text-slate-400">Checking local bridge...</span>
          {:else}
            <span class="w-2.5 h-2.5 rounded-full bg-rose-400"></span>
            <span class="text-rose-300 font-semibold">Bridge Offline</span>
          {/if}
        </div>

        <button
          type="button"
          on:click={checkBridge}
          class="text-[11px] font-semibold text-cyan-400 hover:text-cyan-300 flex items-center gap-1 transition"
        >
          <RefreshCw class="w-3 h-3" />
          <span>Test Bridge</span>
        </button>
      </div>

      <!-- Installation Steps Guide -->
      <div class="space-y-3 text-xs">
        <h4 class="font-bold text-slate-200 uppercase tracking-wider text-[11px] flex items-center gap-1.5">
          <Sparkles class="w-3.5 h-3.5 text-amber-400" />
          <span>Quick 30-Second Setup Guide:</span>
        </h4>

        <!-- Step 1 -->
        <div class="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-2">
          <div class="flex items-start justify-between gap-2">
            <div class="flex items-center gap-2 font-semibold text-slate-200">
              <span class="w-5 h-5 rounded-full bg-cyan-500/20 text-cyan-300 flex items-center justify-center font-bold text-xs shrink-0">1</span>
              <span>Buka Folder Ekstensi Raydrop</span>
            </div>
            <button
              type="button"
              on:click={handleOpenFolder}
              disabled={isOpeningFolder}
              class="px-3 py-1.5 rounded-lg bg-gradient-to-r from-cyan-600 to-indigo-600 hover:from-cyan-500 hover:to-indigo-500 text-white font-semibold flex items-center gap-1.5 shadow-md shadow-cyan-500/20 transition-all text-xs cursor-pointer shrink-0"
            >
              <FolderOpen class="w-3.5 h-3.5" />
              <span>{isOpeningFolder ? 'Membuka...' : 'Open Extension Folder'}</span>
            </button>
          </div>
          <p class="text-slate-400 text-[11px] leading-relaxed">
            Klik tombol di atas untuk membuka folder ekstensi Raydrop di Windows Explorer. Biarkan folder tersebut tetap terbuka.
          </p>
          {#if extensionPath}
            <div class="flex items-center justify-between p-2 rounded-lg bg-slate-900 border border-slate-800 text-[11px] font-mono text-cyan-300">
              <span class="truncate mr-2">{extensionPath}</span>
              <button
                type="button"
                on:click={() => copyText(extensionPath, 'path')}
                class="text-slate-400 hover:text-white shrink-0 p-1 rounded"
                title="Copy Path"
              >
                {#if copiedPath}
                  <Check class="w-3.5 h-3.5 text-emerald-400" />
                {:else}
                  <Copy class="w-3.5 h-3.5" />
                {/if}
              </button>
            </div>
          {/if}
        </div>

        <!-- Step 2 -->
        <div class="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-2">
          <div class="flex items-center gap-2 font-semibold text-slate-200">
            <span class="w-5 h-5 rounded-full bg-cyan-500/20 text-cyan-300 flex items-center justify-center font-bold text-xs shrink-0">2</span>
            <span>Buka Halaman Ekstensi di Browser</span>
          </div>
          <p class="text-slate-400 text-[11px] leading-relaxed">
            Buka halaman manajemen ekstensi di Google Chrome atau Microsoft Edge:
          </p>
          <div class="flex flex-wrap items-center gap-2">
            <button
              type="button"
              on:click={() => {
                handleOpenBrowserUrl('chrome');
                copyText('chrome://extensions', 'chrome');
              }}
              class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white border border-slate-700 flex items-center gap-1.5 transition text-xs font-semibold cursor-pointer"
            >
              <Globe class="w-3.5 h-3.5 text-cyan-400" />
              <span>{copiedChromeUrl ? 'Copied chrome://extensions' : 'Buka di Chrome'}</span>
            </button>

            <button
              type="button"
              on:click={() => {
                handleOpenBrowserUrl('edge');
                copyText('edge://extensions', 'edge');
              }}
              class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white border border-slate-700 flex items-center gap-1.5 transition text-xs font-semibold cursor-pointer"
            >
              <Globe class="w-3.5 h-3.5 text-indigo-400" />
              <span>{copiedEdgeUrl ? 'Copied edge://extensions' : 'Buka di Edge'}</span>
            </button>
          </div>
        </div>

        <!-- Step 3 & 4 -->
        <div class="p-3.5 rounded-xl bg-slate-950 border border-slate-800/80 space-y-2.5">
          <div class="flex items-center gap-2 font-semibold text-slate-200">
            <span class="w-5 h-5 rounded-full bg-cyan-500/20 text-cyan-300 flex items-center justify-center font-bold text-xs shrink-0">3</span>
            <span>Aktifkan Developer Mode & Load Unpacked</span>
          </div>
          <ol class="list-decimal list-inside text-slate-400 text-[11px] space-y-1.5 leading-relaxed pl-1">
            <li>Nyalakan sakelar <b>Developer mode</b> di pojok kanan atas browser Anda.</li>
            <li>Klik tombol <b>Load unpacked</b> (atau <i>Muat yang belum dibongkar</i>) di pojok kiri atas.</li>
            <li>Pilih folder <code>browser-extension</code> yang tadi terbuka di Langkah 1.</li>
          </ol>
          <div class="p-2.5 rounded-lg bg-emerald-500/10 border border-emerald-500/30 flex items-center gap-2 text-emerald-300 text-[11px]">
            <CheckCircle2 class="w-4 h-4 shrink-0 text-emerald-400" />
            <span>Selesai! Icon Raydrop akan muncul di toolbar browser dan otomatis mencegat setiap unduhan file besar (ZIP, ISO, MP4, EXE, dll) untuk di-akselerasi IDM.</span>
          </div>
        </div>
      </div>

      <!-- Action Footer -->
      <div class="flex items-center justify-between pt-3 border-t border-slate-800">
        <span class="text-[11px] text-slate-500 font-mono">
          Bridge: 127.0.0.1:45321 (Local Only)
        </span>
        <button
          type="button"
          on:click={() => ($browserExtensionModalOpen = false)}
          class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-white font-semibold text-xs transition cursor-pointer"
        >
          Tutup
        </button>
      </div>
    </div>
  </div>
{/if}
