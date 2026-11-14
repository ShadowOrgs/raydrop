<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import QRCode from 'qrcode';
  import {
    raydropApi,
    type WebShareInfo,
  } from '../utils/tauri-ipc';
  import {
    QrCode,
    Smartphone,
    Wifi,
    Globe,
    Copy,
    Check,
    X,
    FileText,
    ExternalLink,
    Download,
    CheckCircle2,
    ShieldCheck,
    Share2,
    Image as ImageIcon,
    Mail,
    Send,
  } from 'lucide-svelte';

  export let shareInfo: WebShareInfo;
  export let onClose: () => void;

  let shareMode: 'internet' | 'local' = 'internet';
  let qrDataUrl = '';
  let copiedLink = false;
  let copiedQrImage = false;
  let showSocialMenu = false;
  let activeDownloads: Array<{ file_name: string; peer_ip: string; completed?: boolean }> = [];
  let unlistenStart: (() => void) | null = null;
  let unlistenComplete: (() => void) | null = null;

  $: activeUrl = shareMode === 'internet' ? shareInfo.url : (shareInfo.local_url || shareInfo.url);

  $: if (activeUrl) {
    updateQr(activeUrl);
  }

  async function updateQr(url: string) {
    try {
      qrDataUrl = await QRCode.toDataURL(url, {
        width: 280,
        margin: 2,
        color: {
          dark: '#070b13',
          light: '#ffffff',
        },
      });
    } catch (e) {
      console.error('Failed to generate QR Code:', e);
    }
  }

  onMount(async () => {
    updateQr(activeUrl);

    try {
      unlistenStart = await raydropApi.listenEvent<{
        peer_ip: string;
        file_id: number;
        file_name: string;
      }>('web_share_download_started', (payload) => {
        activeDownloads = [
          { file_name: payload.file_name, peer_ip: payload.peer_ip, completed: false },
          ...activeDownloads.slice(0, 4),
        ];
      });

      unlistenComplete = await raydropApi.listenEvent<{
        peer_ip: string;
        file_id: number;
        file_name: string;
      }>('web_share_download_completed', (payload) => {
        activeDownloads = activeDownloads.map((d) =>
          d.file_name === payload.file_name ? { ...d, completed: true } : d
        );
      });
    } catch (err) {
      console.warn('Could not attach Web Share listener:', err);
    }
  });

  onDestroy(() => {
    unlistenStart?.();
    unlistenComplete?.();
  });

  async function copyLink() {
    try {
      await navigator.clipboard.writeText(activeUrl);
      copiedLink = true;
      setTimeout(() => (copiedLink = false), 2000);
    } catch (err) {
      console.error('Failed to copy link:', err);
    }
  }

  async function copyQrImage() {
    if (!qrDataUrl) return;
    try {
      await raydropApi.writeSystemClipboard(qrDataUrl);
      copiedQrImage = true;
      setTimeout(() => (copiedQrImage = false), 2000);
    } catch (err) {
      console.error('Failed to copy QR code image:', err);
      // Fallback: copy link text
      copyLink();
    }
  }

  async function shareNative() {
    const shareText = `Download ${shareInfo.total_files} file(s) (${shareInfo.total_bytes_human}) via Raydrop Web Share: ${activeUrl}`;
    if (navigator.share) {
      try {
        await navigator.share({
          title: 'Raydrop Web Share',
          text: shareText,
          url: activeUrl,
        });
        return;
      } catch (_) {}
    }
    showSocialMenu = !showSocialMenu;
  }

  async function openSocialShare(platform: string) {
    const text = encodeURIComponent(`Download ${shareInfo.total_files} file (${shareInfo.total_bytes_human}) via Raydrop Web Share:`);
    const encodedUrl = encodeURIComponent(activeUrl);
    let target = '';

    switch (platform) {
      case 'whatsapp':
        target = `https://api.whatsapp.com/send?text=${text}%20${encodedUrl}`;
        break;
      case 'telegram':
        target = `https://t.me/share/url?url=${encodedUrl}&text=${text}`;
        break;
      case 'twitter':
        target = `https://twitter.com/intent/tweet?text=${text}&url=${encodedUrl}`;
        break;
      case 'email':
        target = `mailto:?subject=${encodeURIComponent('Raydrop Shared Files')}&body=${text}%0A%0A${encodedUrl}`;
        break;
    }

    if (target) {
      await raydropApi.openExternalUrl(target);
      showSocialMenu = false;
    }
  }

  async function handleClose() {
    await raydropApi.stopWebShare();
    onClose();
  }
</script>

<div
  class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex items-center justify-center p-3 sm:p-4"
  role="dialog"
  aria-modal="true"
>
  <div class="w-full max-w-lg bg-slate-900 border border-cyan-500/30 rounded-2xl p-5 sm:p-6 shadow-2xl flex flex-col gap-4 max-h-[92vh] overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between pb-3 border-b border-slate-800">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-500/20 to-indigo-500/20 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shrink-0">
          <QrCode class="w-5 h-5" />
        </div>
        <div>
          <h3 class="text-base font-bold text-white flex items-center gap-2">
            <span>Instant Web Share</span>
            <span class="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
              No App Needed
            </span>
          </h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Share files to any browser on phone, tablet, or laptop via QR code or link
          </p>
        </div>
      </div>
      <button
        on:click={handleClose}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors cursor-pointer shrink-0"
        title="Close & Stop Sharing"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Mode Toggle: Over Internet vs Local Wi-Fi -->
    <div class="grid grid-cols-2 p-1 bg-slate-950/80 rounded-xl border border-slate-800 gap-1 text-xs font-semibold">
      <button
        type="button"
        on:click={() => (shareMode = 'internet')}
        class="flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-lg transition-all cursor-pointer {shareMode === 'internet'
          ? 'bg-gradient-to-r from-cyan-600 to-indigo-600 text-white shadow-md shadow-cyan-500/20'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Globe class="w-3.5 h-3.5" />
        <span>Over Internet (Anywhere)</span>
      </button>

      <button
        type="button"
        on:click={() => (shareMode = 'local')}
        class="flex items-center justify-center gap-1.5 py-1.5 px-3 rounded-lg transition-all cursor-pointer {shareMode === 'local'
          ? 'bg-gradient-to-r from-emerald-600 to-teal-600 text-white shadow-md shadow-emerald-500/20'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Wifi class="w-3.5 h-3.5" />
        <span>Local Wi-Fi Only</span>
      </button>
    </div>

    <!-- QR Code & Link Card -->
    <div class="flex flex-col sm:flex-row items-center gap-4 sm:gap-5 p-4 rounded-xl bg-slate-950/70 border border-slate-800">
      <!-- QR Frame with Quick Copy Image Button -->
      <div class="flex flex-col items-center gap-2 shrink-0">
        <div class="p-2.5 bg-white rounded-2xl shadow-lg shadow-cyan-500/10 relative group">
          {#if qrDataUrl}
            <img src={qrDataUrl} alt="Scan QR Code to download" class="w-36 h-36 object-contain block" />
          {:else}
            <div class="w-36 h-36 flex items-center justify-center bg-slate-100 text-slate-400 text-xs">
              Generating QR...
            </div>
          {/if}
        </div>
        <button
          type="button"
          on:click={copyQrImage}
          class="flex items-center gap-1.5 px-3 py-1 rounded-lg bg-slate-800/90 hover:bg-slate-700 text-slate-300 hover:text-white text-[11px] font-semibold border border-slate-700 transition-all cursor-pointer"
          title="Copy QR Code Image to Clipboard (Ready to Paste Ctrl+V in WhatsApp, Discord, etc.)"
        >
          {#if copiedQrImage}
            <Check class="w-3.5 h-3.5 text-emerald-400" />
            <span class="text-emerald-400">QR Copied!</span>
          {:else}
            <ImageIcon class="w-3.5 h-3.5 text-cyan-400" />
            <span>Copy QR Image</span>
          {/if}
        </button>
      </div>

      <!-- Instructions, Copy Link & Social Share -->
      <div class="flex-1 flex flex-col justify-between gap-3 min-w-0 w-full">
        <div class="space-y-1.5">
          <div class="flex items-center gap-2 text-xs font-semibold text-slate-200">
            <Smartphone class="w-4 h-4 text-cyan-400 shrink-0" />
            <span>Scan with Phone Camera</span>
          </div>
          <p class="text-[11px] text-slate-400 leading-relaxed">
            {#if shareMode === 'internet'}
              Accessible over cellular data (4G/5G) or any Wi-Fi in the world via secure relay tunnel.
            {:else}
              Devices must be connected to the same local Wi-Fi or hotspot.
            {/if}
          </p>
        </div>

        <!-- URL pill -->
        <div>
          <span class="block text-[10px] uppercase font-mono text-slate-500 font-bold mb-1">
            {shareMode === 'internet' ? 'Public Internet Link' : 'Local Wi-Fi Link'}
          </span>
          <div class="flex items-center gap-1.5 bg-slate-900 border border-slate-700/80 rounded-xl px-2.5 py-1.5">
            <span class="text-xs font-mono text-cyan-300 truncate flex-1">{activeUrl}</span>
            <button
              on:click={copyLink}
              class="flex items-center gap-1 px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-[11px] font-semibold text-slate-200 transition-colors shrink-0 cursor-pointer"
              title="Copy URL"
            >
              {#if copiedLink}
                <Check class="w-3 h-3 text-emerald-400" />
                <span class="text-emerald-400">Copied!</span>
              {:else}
                <Copy class="w-3 h-3" />
                <span>Copy</span>
              {/if}
            </button>
          </div>
        </div>

        <!-- Share to Social Media Buttons -->
        <div class="pt-1">
          <div class="flex items-center gap-1.5 flex-wrap">
            <button
              type="button"
              on:click={shareNative}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 text-white text-xs font-semibold shadow-md shadow-indigo-500/20 transition-all cursor-pointer"
              title="Share via other applications"
            >
              <Share2 class="w-3.5 h-3.5" />
              <span>Share Link</span>
            </button>

            <!-- Quick Social Icons -->
            <button
              type="button"
              on:click={() => openSocialShare('whatsapp')}
              class="p-1.5 rounded-lg bg-emerald-600/80 hover:bg-emerald-500 text-white text-xs font-semibold transition-all cursor-pointer"
              title="Share to WhatsApp"
            >
              <Send class="w-3.5 h-3.5" />
            </button>

            <button
              type="button"
              on:click={() => openSocialShare('telegram')}
              class="p-1.5 rounded-lg bg-sky-600/80 hover:bg-sky-500 text-white text-xs font-semibold transition-all cursor-pointer"
              title="Share to Telegram"
            >
              <Send class="w-3.5 h-3.5 rotate-45" />
            </button>

            <button
              type="button"
              on:click={() => openSocialShare('email')}
              class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white border border-slate-700 transition-all cursor-pointer"
              title="Share via Email"
            >
              <Mail class="w-3.5 h-3.5" />
            </button>
          </div>

          {#if showSocialMenu}
            <div class="mt-2 p-2 rounded-xl bg-slate-900 border border-slate-800 grid grid-cols-2 gap-1.5 text-xs">
              <button
                type="button"
                on:click={() => openSocialShare('whatsapp')}
                class="flex items-center gap-1.5 p-1.5 rounded-lg hover:bg-emerald-500/20 text-emerald-300 transition-colors"
              >
                <span>WhatsApp</span>
              </button>
              <button
                type="button"
                on:click={() => openSocialShare('telegram')}
                class="flex items-center gap-1.5 p-1.5 rounded-lg hover:bg-sky-500/20 text-sky-300 transition-colors"
              >
                <span>Telegram</span>
              </button>
              <button
                type="button"
                on:click={() => openSocialShare('twitter')}
                class="flex items-center gap-1.5 p-1.5 rounded-lg hover:bg-cyan-500/20 text-cyan-300 transition-colors"
              >
                <span>Twitter / X</span>
              </button>
              <button
                type="button"
                on:click={() => openSocialShare('email')}
                class="flex items-center gap-1.5 p-1.5 rounded-lg hover:bg-slate-800 text-slate-300 transition-colors"
              >
                <span>Email</span>
              </button>
            </div>
          {/if}
        </div>
      </div>
    </div>

    <!-- Live Download Activity if any -->
    {#if activeDownloads.length > 0}
      <div class="p-3 rounded-xl bg-cyan-950/30 border border-cyan-500/30 space-y-2">
        <div class="text-[11px] font-bold text-cyan-300 uppercase tracking-wider flex items-center gap-1.5">
          <Download class="w-3.5 h-3.5 animate-pulse" />
          <span>Live Downloads</span>
        </div>
        <div class="space-y-1">
          {#each activeDownloads as act}
            <div class="flex items-center justify-between text-xs text-slate-200 bg-slate-900/60 px-2.5 py-1 rounded-lg">
              <span class="truncate max-w-[240px] font-medium">{act.file_name}</span>
              {#if act.completed}
                <span class="text-[11px] text-emerald-400 font-semibold flex items-center gap-1">
                  <CheckCircle2 class="w-3 h-3" /> Complete
                </span>
              {:else}
                <span class="text-[11px] text-cyan-400 animate-pulse font-mono">Streaming to {act.peer_ip}...</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Shared Files Breakdown -->
    <div class="space-y-2">
      <div class="flex items-center justify-between text-xs text-slate-400">
        <span class="font-semibold text-slate-300">Shared Files ({shareInfo.total_files})</span>
        <span class="font-mono">{shareInfo.total_bytes_human}</span>
      </div>
      <div class="max-h-36 overflow-y-auto space-y-1 pr-1">
        {#each shareInfo.files as item (item.id)}
          <div class="flex items-center justify-between p-2 rounded-lg bg-slate-950/60 border border-slate-800 text-xs">
            <div class="flex items-center gap-2 min-w-0">
              <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              <span class="truncate font-medium text-slate-200">{item.name}</span>
            </div>
            <span class="text-[11px] font-mono text-slate-400 ml-2 shrink-0">{item.size_human}</span>
          </div>
        {/each}
      </div>
    </div>

    <!-- Security Note & Stop Action -->
    <div class="pt-3 border-t border-slate-800 flex items-center justify-between gap-3">
      <div class="flex items-center gap-1.5 text-[11px] text-slate-400">
        <ShieldCheck class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
        <span>Zero Server Storage &bull; Direct Peer Streaming</span>
      </div>
      <button
        on:click={handleClose}
        class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 font-semibold text-xs transition-colors cursor-pointer"
      >
        Done / Stop Sharing
      </button>
    </div>
  </div>
</div>
