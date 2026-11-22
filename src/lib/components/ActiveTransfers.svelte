<script lang="ts">
  import { openWithLastDir } from '../utils/filePicker';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { activeTransfers, historicalTransfers } from '../stores/transfers';
  import { appSettings, cliQueuedFiles, activeMainTab } from '../stores/state';
  import { torrentTasks, httpTasks, magnetModalOpen, browserExtensionModalOpen } from '../stores/torrents';
  import { raydropApi, formatBytes, formatEta, type TransferRecord, type WebShareInfo, type TorrentTaskInfo, type HttpDownloadTaskInfo } from '../utils/tauri-ipc';
  import WebShareModal from './WebShareModal.svelte';
  import ClipboardShareModal from './ClipboardShareModal.svelte';
  import FolderSyncModal from './FolderSyncModal.svelte';
  import {
    ArrowUpRight,
    ArrowDownLeft,
    XCircle,
    CheckCircle2,
    AlertTriangle,
    Activity,
    ShieldCheck,
    Pause,
    Play,
    Zap,
    RotateCcw,
    FolderOpen,
    ExternalLink,
    FileText,
    ChevronDown,
    ChevronUp,
    Clock,
    Upload,
    FolderUp,
    Plus,
    Sparkles,
    Send,
    Radio,
    Copy,
    Check,
    QrCode,
    Clipboard,
    Gauge,
    FolderSync,
    Magnet,
    Download,
    Puzzle,
    Link,
    ZoomIn,
    ZoomOut,
    Eye,
    Image as ImageIcon,
    X,
    Video,
    Music,
    Hash,
    Power,
  } from 'lucide-svelte';

  let resumingId = '';
  let actionError = '';
  let showRefreshModal = false;
  let refreshTask: HttpDownloadTaskInfo | null = null;
  let refreshNewUrl = '';
  let isRefreshingUrl = false;
  let expandedTransferIds: Record<string, boolean> = {};
  let transferFilter: 'all' | 'incoming' | 'outgoing' = 'all';
  let isDraggingOver = false;
  let webShareInfo: WebShareInfo | null = null;
  let showClipboardModal = false;
  let showFolderSyncModal = false;
  let showSpeedLimitMenu = false;
  let showPowerMenu = false;

  let previewImage: { path: string; name: string } | null = null;
  let previewMedia: { path: string; name: string; type: 'video' | 'audio' } | null = null;
  let zoomScale = 1;
  let zoomTranslate = { x: 0, y: 0 };
  let isPanning = false;
  let startPan = { x: 0, y: 0 };
  let transferThumbnails: Record<string, string> = {};
  let failedThumbnails: Record<string, boolean> = {};

  let showHashModal = false;
  let hashTargetFile: { name: string; path: string } | null = null;
  let calculatedHash = '';
  let isCalculatingHash = false;
  let compareHashInput = '';
  let hashCopied = false;

  let autoPowerAction: 'none' | 'sleep' | 'shutdown' | 'hibernate' = 'none';
  let powerCountdown = 0;
  let powerCountdownTimer: any = null;
  let hadActiveTasks = false;

  $: visibleHttpTasks = $httpTasks.filter((t) => t.status !== 'completed' && t.status !== 'cancelled');

  $: {
    const activeCount =
      $activeTransfers.length +
      $httpTasks.filter((t) => t.status === 'downloading' || t.status === 'starting' || t.status === 'queued').length +
      $torrentTasks.filter((t) => t.status === 'downloading').length;

    if (activeCount > 0) {
      hadActiveTasks = true;
    } else if (hadActiveTasks && activeCount === 0 && autoPowerAction !== 'none') {
      hadActiveTasks = false;
      startPowerCountdown();
    }
  }

  function startPowerCountdown() {
    if (powerCountdownTimer) clearInterval(powerCountdownTimer);
    powerCountdown = 30;
    powerCountdownTimer = setInterval(async () => {
      powerCountdown -= 1;
      if (powerCountdown <= 0) {
        clearInterval(powerCountdownTimer);
        powerCountdownTimer = null;
        const act = autoPowerAction;
        autoPowerAction = 'none';
        if (act !== 'none') {
          try {
            await raydropApi.executeSystemPowerAction(act);
          } catch (e) {
            console.error('Power action failed:', e);
          }
        }
      }
    }, 1000);
  }

  function cancelPowerCountdown() {
    if (powerCountdownTimer) {
      clearInterval(powerCountdownTimer);
      powerCountdownTimer = null;
    }
    powerCountdown = 0;
    autoPowerAction = 'none';
  }

  function executePowerNow() {
    if (powerCountdownTimer) {
      clearInterval(powerCountdownTimer);
      powerCountdownTimer = null;
    }
    const act = autoPowerAction;
    powerCountdown = 0;
    autoPowerAction = 'none';
    if (act !== 'none') {
      raydropApi.executeSystemPowerAction(act).catch(console.error);
    }
  }

  function selectAutoPower(val: any) {
    autoPowerAction = val;
    showPowerMenu = false;
  }

  function isVideoFile(fileName: string): boolean {
    return /\.(mp4|webm|mkv|mov|avi|wmv|flv|m4v)$/i.test(fileName || '');
  }

  function isAudioFile(fileName: string): boolean {
    return /\.(mp3|wav|ogg|flac|aac|m4a|wma|opus)$/i.test(fileName || '');
  }

  function openMediaPreview(path: string, name: string) {
    if (isVideoFile(name)) {
      previewMedia = { path, name, type: 'video' };
    } else if (isAudioFile(name)) {
      previewMedia = { path, name, type: 'audio' };
    }
  }

  function closeMediaPreview() {
    previewMedia = null;
  }

  async function openHashModal(name: string, path: string) {
    hashTargetFile = { name, path };
    calculatedHash = '';
    compareHashInput = '';
    hashCopied = false;
    isCalculatingHash = true;
    showHashModal = true;
    try {
      calculatedHash = await raydropApi.calculateFileHash(path);
    } catch (err: any) {
      calculatedHash = `Error: ${String(err)}`;
    } finally {
      isCalculatingHash = false;
    }
  }

  function closeHashModal() {
    showHashModal = false;
    hashTargetFile = null;
    calculatedHash = '';
    compareHashInput = '';
    hashCopied = false;
  }

  async function copyHashToClipboard() {
    if (!calculatedHash || calculatedHash.startsWith('Error')) return;
    try {
      await navigator.clipboard.writeText(calculatedHash);
      hashCopied = true;
      setTimeout(() => (hashCopied = false), 2000);
    } catch (_) {}
  }

  function isImageFile(fileName: string): boolean {
    return /\.(jpe?g|png|webp|gif|bmp|svg|ico|avif)$/i.test(fileName || '');
  }

  async function ensureTransferThumbnail(path: string | null) {
    if (!path || transferThumbnails[path] || failedThumbnails[path]) return;
    try {
      const thumb = await raydropApi.getFileThumbnail(path);
      if (thumb) {
        transferThumbnails = { ...transferThumbnails, [path]: thumb };
      } else {
        failedThumbnails = { ...failedThumbnails, [path]: true };
      }
    } catch (_) {
      failedThumbnails = { ...failedThumbnails, [path]: true };
    }
  }

  // Preload thumbnails for all image items in active transfers & history
  $: if (filteredHistory.length > 0 || filteredActive.length > 0) {
    for (const h of filteredHistory) {
      const files = parseFilePaths(h.source_paths);
      const primaryFilePath = files[0] || h.target_path;
      if (primaryFilePath && isImageFile(primaryFilePath) && !transferThumbnails[primaryFilePath] && !failedThumbnails[primaryFilePath]) {
        ensureTransferThumbnail(primaryFilePath);
      }
    }
    for (const t of filteredActive) {
      const activeFiles = parseFilePaths(t.source_paths);
      if (activeFiles[0] && isImageFile(activeFiles[0]) && !transferThumbnails[activeFiles[0]] && !failedThumbnails[activeFiles[0]]) {
        ensureTransferThumbnail(activeFiles[0]);
      }
    }
  }

  function openImagePreview(path: string, name: string) {
    previewImage = { path, name };
    zoomScale = 1;
    zoomTranslate = { x: 0, y: 0 };
    isPanning = false;
  }

  function closeImagePreview() {
    previewImage = null;
    zoomScale = 1;
    zoomTranslate = { x: 0, y: 0 };
    isPanning = false;
  }

  function zoomIn() {
    zoomScale = Math.min(zoomScale + 0.25, 5);
  }

  function zoomOut() {
    zoomScale = Math.max(zoomScale - 0.25, 0.3);
  }

  function resetZoom() {
    zoomScale = 1;
    zoomTranslate = { x: 0, y: 0 };
  }

  function handleWheelZoom(e: WheelEvent) {
    if (e.deltaY < 0) {
      zoomScale = Math.min(zoomScale + 0.15, 5);
    } else {
      zoomScale = Math.max(zoomScale - 0.15, 0.3);
    }
  }

  function handleMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    isPanning = true;
    startPan = { x: e.clientX - zoomTranslate.x * zoomScale, y: e.clientY - zoomTranslate.y * zoomScale };
  }

  function handleMouseMove(e: MouseEvent) {
    if (!isPanning) return;
    zoomTranslate = {
      x: (e.clientX - startPan.x) / zoomScale,
      y: (e.clientY - startPan.y) / zoomScale,
    };
  }

  function handleMouseUp() {
    isPanning = false;
  }

  async function handleStartHttpNow(id: string) {
    try {
      await raydropApi.startHttpDownloadNow(id);
      httpTasks.update((tasks) =>
        tasks.map((t) => (t.id === id ? { ...t, status: 'downloading' } : t))
      );
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to start download';
    }
  }

  async function handlePauseHttp(id: string) {
    try {
      await raydropApi.pauseHttpDownload(id);
      httpTasks.update((tasks) =>
        tasks.map((t) => (t.id === id ? { ...t, status: 'paused' } : t))
      );
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to pause download';
    }
  }

  async function handleResumeHttp(id: string) {
    try {
      await raydropApi.resumeHttpDownload(id);
      httpTasks.update((tasks) =>
        tasks.map((t) => (t.id === id ? { ...t, status: 'downloading' } : t))
      );
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to resume download';
    }
  }

  function openRefreshModal(task: HttpDownloadTaskInfo) {
    refreshTask = task;
    refreshNewUrl = task.url || '';
    showRefreshModal = true;
  }

  async function handleConfirmRefreshUrl() {
    if (!refreshTask || !refreshNewUrl.trim()) return;
    isRefreshingUrl = true;
    try {
      const updated = await raydropApi.updateHttpDownloadUrl(refreshTask.id, refreshNewUrl.trim());
      httpTasks.update((tasks) =>
        tasks.map((t) => (t.id === updated.id ? updated : t))
      );
      showRefreshModal = false;
      refreshTask = null;
      refreshNewUrl = '';
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to update download address';
    } finally {
      isRefreshingUrl = false;
    }
  }

  async function handleDeleteHttp(id: string) {
    const task = $httpTasks.find((t) => t.id === id);
    const isCompleted = task?.status === 'completed';
    const confirmMsg = isCompleted
      ? 'Remove this completed task from the list?'
      : 'Cancel and delete this incomplete download?';

    if (confirm(confirmMsg)) {
      try {
        await raydropApi.deleteHttpDownload(id, !isCompleted);
        httpTasks.update((tasks) => tasks.filter((t) => t.id !== id));
      } catch (err: any) {
        actionError = err?.toString() || 'Failed to remove download';
      }
    }
  }

  async function handlePauseTorrent(id: string) {
    try {
      await raydropApi.pauseTorrentTask(id);
      torrentTasks.update((tasks) =>
        tasks.map((t) => (t.id === id ? { ...t, status: 'paused' } : t))
      );
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to pause torrent';
    }
  }

  async function handleResumeTorrent(id: string) {
    try {
      await raydropApi.resumeTorrentTask(id);
      torrentTasks.update((tasks) =>
        tasks.map((t) => (t.id === id ? { ...t, status: 'downloading' } : t))
      );
    } catch (err: any) {
      actionError = err?.toString() || 'Failed to resume torrent';
    }
  }

  async function handleDeleteTorrent(id: string) {
    if (confirm('Remove this torrent download task?')) {
      try {
        await raydropApi.deleteTorrentTask(id, false);
        torrentTasks.update((tasks) => tasks.filter((t) => t.id !== id));
      } catch (err: any) {
        actionError = err?.toString() || 'Failed to remove torrent';
      }
    }
  }

  function toggleExpanded(id: string) {
    expandedTransferIds = {
      ...expandedTransferIds,
      [id]: !expandedTransferIds[id],
    };
  }

  function calcPercent(transferred: number, total: number): number {
    if (total <= 0) return 0;
    const pct = (transferred / total) * 100;
    if (pct > 0 && pct < 1) return 1;
    return Math.min(100, Math.round(pct));
  }

  function formatTransferTime(unixSecs: number): string {
    if (!unixSecs) return '';
    const date = new Date(unixSecs * 1000);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();
    const hours = String(date.getHours()).padStart(2, '0');
    const minutes = String(date.getMinutes()).padStart(2, '0');
    const timeStr = `${hours}:${minutes}`;

    if (isToday) {
      return `Today, ${timeStr}`;
    }

    const yesterday = new Date();
    yesterday.setDate(now.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return `Yesterday, ${timeStr}`;
    }

    const day = date.getDate();
    const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    const month = months[date.getMonth()];
    const year = date.getFullYear();
    return `${day} ${month} ${year}, ${timeStr}`;
  }

  function parseFilePaths(sourcePathsJson?: string | null): string[] {
    if (!sourcePathsJson) return [];
    try {
      const parsed = JSON.parse(sourcePathsJson);
      if (Array.isArray(parsed)) {
        return parsed.filter((x) => typeof x === 'string' && x.trim().length > 0);
      }
      if (typeof parsed === 'string' && parsed.trim().length > 0) {
        return [parsed.trim()];
      }
    } catch {
      if (typeof sourcePathsJson === 'string' && sourcePathsJson.trim().length > 0) {
        return [sourcePathsJson.trim()];
      }
    }
    return [];
  }

  function getBaseName(fullPath: string): string {
    const parts = fullPath.split(/[/\\]/);
    return parts[parts.length - 1] || fullPath;
  }

  function getFileTypeBadge(filename: string): { label: string; color: string } {
    const ext = filename.split('.').pop()?.toUpperCase() || 'FILE';
    if (['SQL', 'DB', 'SQLITE'].includes(ext)) {
      return { label: ext, color: 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40' };
    }
    if (['ZIP', 'RAR', '7Z', 'TAR', 'GZ'].includes(ext)) {
      return { label: ext, color: 'bg-amber-500/20 text-amber-300 border-amber-500/40' };
    }
    if (['MP4', 'MKV', 'MOV', 'AVI', 'WEBM'].includes(ext)) {
      return { label: ext, color: 'bg-purple-500/20 text-purple-300 border-purple-500/40' };
    }
    if (['JPG', 'JPEG', 'PNG', 'WEBP', 'SVG', 'GIF'].includes(ext)) {
      return { label: ext, color: 'bg-pink-500/20 text-pink-300 border-pink-500/40' };
    }
    if (['PDF', 'DOC', 'DOCX', 'XLS', 'XLSX', 'PPTX'].includes(ext)) {
      return { label: ext, color: 'bg-blue-500/20 text-blue-300 border-blue-500/40' };
    }
    if (['ISO', 'EXE', 'MSI', 'DMG', 'BIN'].includes(ext)) {
      return { label: ext, color: 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40' };
    }
    return { label: ext.slice(0, 4), color: 'bg-slate-800 text-slate-300 border-slate-700' };
  }

  $: filteredActive = $activeTransfers.filter((t) => {
    if (transferFilter === 'all') return true;
    return t.direction === transferFilter;
  });

  $: filteredHistory = $historicalTransfers.filter((h) => {
    if (transferFilter === 'all') return true;
    return h.direction === transferFilter;
  });

  $: incomingCount =
    $activeTransfers.filter((t) => t.direction === 'incoming').length +
    $historicalTransfers.filter((h) => h.direction === 'incoming').length;

  $: outgoingCount =
    $activeTransfers.filter((t) => t.direction === 'outgoing').length +
    $historicalTransfers.filter((h) => h.direction === 'outgoing').length;

  $: totalActiveSpeed = $activeTransfers
    .filter((t) => t.status === 'in_progress')
    .reduce((sum, t) => sum + (t.speed_mbps || 0), 0);

  async function handlePickFiles() {
    try {
      const picked = await openWithLastDir({
        multiple: true,
        title: 'Select Files to Send via Raydrop',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        cliQueuedFiles.set(list);
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to select files';
    }
  }

  async function handlePickFolder() {
    try {
      const picked = await openWithLastDir({
        directory: true,
        multiple: false,
        title: 'Select Folder to Send via Raydrop',
      });
      if (typeof picked === 'string') {
        cliQueuedFiles.set([picked]);
      }
    } catch (e: any) {
      const msg = e?.toString() || '';
      if (msg.includes('FolderPickerNotImplemented') || /Android|iPhone|iPad/i.test(navigator.userAgent)) {
        actionError = 'Direct folder selection is not supported on mobile platforms due to OS security sandboxing. Please use "Send Files" to select multiple files at once.';
      } else {
        actionError = msg || 'Failed to select folder';
      }
    }
  }

  async function handleShareViaWeb() {
    try {
      const picked = await openWithLastDir({
        multiple: true,
        title: 'Select Files or Folder to Share to Phone / Web',
      });
      if (picked) {
        const list = Array.isArray(picked) ? picked : [picked];
        const info = await raydropApi.startWebShare(list);
        webShareInfo = info;
      }
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to start Web Share';
    }
  }

  async function handleOpenFile(path: string) {
    actionError = '';
    try {
      await raydropApi.openFilePath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to open file';
    }
  }

  async function handleOpenFolder(path: string) {
    actionError = '';
    try {
      await raydropApi.openFolderPath(path);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to open folder';
    }
  }

  async function handlePause(sessionId: string) {
    actionError = '';
    try {
      await raydropApi.pauseTransfer(sessionId);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to pause transfer';
    }
  }

  async function handleResume(sessionId: string) {
    actionError = '';
    resumingId = sessionId;
    try {
      await raydropApi.resumeTransfer(sessionId);
    } catch (e: any) {
      actionError = e?.toString() || 'Failed to resume transfer';
    } finally {
      resumingId = '';
    }
  }
</script>

<div class="flex flex-col h-full bg-slate-900/40 border border-slate-800/80 rounded-2xl overflow-hidden backdrop-blur-xl">
  <!-- Top Hub: Quick Action Toolbar & Full-Width Drop Zone -->
  <div class="p-3 sm:p-4 border-b border-slate-800/80 bg-slate-950/60 flex flex-col gap-2.5 sm:gap-3">
    <!-- Row 1: Action Toolbar (Compact, Logically Grouped, Responsive) -->
    <div class="flex flex-wrap items-center justify-between gap-2">
      <!-- P2P Transfer & Sharing Group -->
      <div class="flex items-center flex-wrap gap-1.5 sm:gap-2">
        <button
          type="button"
          on:click={handlePickFiles}
          class="flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-semibold text-xs shadow-md shadow-cyan-500/20 transition-all transform hover:-translate-y-0.5 active:translate-y-0 cursor-pointer"
          title="Pick files to send via P2P"
        >
          <Plus class="w-3.5 h-3.5 stroke-[2.5]" />
          <span>Send Files</span>
        </button>

        <button
          type="button"
          on:click={handlePickFolder}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-slate-800/90 hover:bg-slate-700 text-slate-200 hover:text-white font-medium text-xs border border-slate-700/80 transition-all hover:-translate-y-0.5 cursor-pointer shadow-sm"
          title="Pick an entire folder to send"
        >
          <FolderUp class="w-3.5 h-3.5 text-cyan-400" />
          <span>Folder</span>
        </button>

        <button
          type="button"
          on:click={handleShareViaWeb}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-purple-600/25 hover:bg-purple-600/40 text-purple-200 hover:text-white font-medium text-xs border border-purple-500/40 transition-all hover:-translate-y-0.5 shadow-sm cursor-pointer"
          title="Share files to phone or any device via QR code (no app needed)"
        >
          <QrCode class="w-3.5 h-3.5 text-purple-300" />
          <span>Web Share</span>
        </button>

        <button
          type="button"
          on:click={() => (showClipboardModal = true)}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-indigo-600/25 hover:bg-indigo-600/40 text-indigo-200 hover:text-white font-medium text-xs border border-indigo-500/40 transition-all hover:-translate-y-0.5 shadow-sm cursor-pointer"
          title="Sync text, links, or code directly to your other devices' clipboard"
        >
          <Clipboard class="w-3.5 h-3.5 text-indigo-300" />
          <span>Clipboard</span>
        </button>

        <button
          type="button"
          on:click={() => (showFolderSyncModal = true)}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-emerald-600/25 hover:bg-emerald-600/40 text-emerald-200 hover:text-white font-medium text-xs border border-emerald-500/40 transition-all hover:-translate-y-0.5 shadow-sm cursor-pointer"
          title="Mirror / sync whole folders directly to your trusted peers"
        >
          <FolderSync class="w-3.5 h-3.5 text-emerald-300" />
          <span>Folder Sync</span>
        </button>
      </div>

      <!-- Downloader & Extension Tools Group -->
      <div class="flex items-center flex-wrap gap-1.5 sm:gap-2">
        <button
          type="button"
          on:click={() => ($magnetModalOpen = true)}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-gradient-to-r from-amber-500/25 to-cyan-600/25 hover:from-amber-500/35 hover:to-cyan-600/35 text-amber-200 hover:text-white font-medium text-xs border border-amber-500/40 transition-all hover:-translate-y-0.5 cursor-pointer shadow-sm"
          title="Download file via URL Multi-Segment Accelerator (IDM-style) or BitTorrent Magnet"
        >
          <Download class="w-3.5 h-3.5 text-amber-300" />
          <span>+ Add URL / Torrent</span>
        </button>

        <button
          type="button"
          on:click={() => ($browserExtensionModalOpen = true)}
          class="flex items-center justify-center gap-1.5 px-2.5 sm:px-3 py-1.5 rounded-lg bg-slate-800/90 hover:bg-slate-700 text-slate-200 hover:text-white font-medium text-xs border border-slate-700/80 transition-all hover:-translate-y-0.5 cursor-pointer shadow-sm"
          title="Install Chrome / Edge Extension for automatic download catching"
        >
          <Puzzle class="w-3.5 h-3.5 text-cyan-400" />
          <span>Extension</span>
        </button>
      </div>
    </div>

    <!-- Row 2: Dedicated Full-Width Drop Zone Strip -->
    <button
      type="button"
      on:click={handlePickFiles}
      class="w-full px-3.5 py-2 sm:py-2.5 rounded-xl border border-dashed border-slate-700/80 hover:border-cyan-500/60 bg-slate-900/40 hover:bg-slate-900/80 transition-all cursor-pointer flex items-center justify-between gap-3 text-xs text-slate-400 group text-left"
      title="Click or drag files here to send"
    >
      <div class="flex items-center gap-2.5 min-w-0">
        <div class="w-6 h-6 sm:w-7 sm:h-7 rounded-lg bg-cyan-500/10 group-hover:bg-cyan-500/20 text-cyan-400 flex items-center justify-center shrink-0 transition-colors">
          <Upload class="w-3.5 h-3.5 sm:w-4 sm:h-4" />
        </div>
        <div class="truncate min-w-0">
          <span class="text-slate-200 font-semibold group-hover:text-cyan-300 transition-colors text-xs">
            Drop files or folders anywhere here to send
          </span>
          <span class="text-slate-500 hidden md:inline text-[11px]">
            — Direct End-to-End Encrypted P2P • Zero Cloud Storage
          </span>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <span class="hidden sm:inline-flex items-center gap-1 text-[11px] text-slate-400 group-hover:text-slate-300">
          Click to browse
        </span>
        <span class="px-2 py-0.5 rounded-full bg-slate-800/80 border border-slate-700 text-[10px] font-mono text-cyan-400">
          Unlimited Size
        </span>
      </div>
    </button>
  </div>

  <!-- Filter Ribbon & Aggregate Metrics -->
  <div class="px-3 sm:px-5 py-2.5 sm:py-3 border-b border-slate-800/80 flex flex-wrap items-center justify-between gap-2.5 sm:gap-3 bg-slate-950/40">
    <!-- Filter Tabs: All / Received / Sent -->
    <div class="flex items-center gap-1 bg-slate-950 p-1 rounded-xl border border-slate-800">
      <button
        type="button"
        on:click={() => (transferFilter = 'all')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'all'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <span>All</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-slate-300 font-mono">
          {$activeTransfers.length + $historicalTransfers.length}
        </span>
      </button>

      <button
        type="button"
        on:click={() => (transferFilter = 'incoming')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'incoming'
          ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <ArrowDownLeft class="w-3.5 h-3.5 text-emerald-400" />
        <span>Received</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-emerald-400 font-mono">
          {incomingCount}
        </span>
      </button>

      <button
        type="button"
        on:click={() => (transferFilter = 'outgoing')}
        class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold transition-all {transferFilter === 'outgoing'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <ArrowUpRight class="w-3.5 h-3.5 text-cyan-400" />
        <span>Sent</span>
        <span class="text-[10px] px-1.5 py-0.2 rounded-full bg-slate-800 text-cyan-300 font-mono">
          {outgoingCount}
        </span>
      </button>
    </div>

    <!-- Live Performance Badge -->
    <div class="flex items-center gap-3">
      {#if totalActiveSpeed > 0}
        <div class="flex items-center gap-2 px-3 py-1 rounded-full bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 text-xs font-mono font-bold animate-pulse">
          <Zap class="w-3.5 h-3.5 text-emerald-400" />
          <span>{totalActiveSpeed.toFixed(1)} MB/s Active</span>
        </div>
      {:else}
        <div class="hidden sm:flex items-center gap-1.5 text-xs text-slate-400 bg-slate-900/60 px-3 py-1 rounded-full border border-slate-800">
          <ShieldCheck class="w-3.5 h-3.5 text-cyan-400" />
          <span>ChaCha20-Poly1305 + BLAKE3</span>
        </div>
      {/if}

      {#if $appSettings.download_dir}
        <button
          type="button"
          on:click={() => handleOpenFolder($appSettings.download_dir)}
          class="flex items-center gap-1.5 text-xs text-slate-300 hover:text-white bg-slate-800/90 hover:bg-slate-700 px-3 py-1 rounded-xl border border-slate-700 transition shrink-0"
          title="Open Downloads Folder ({$appSettings.download_dir})"
        >
          <FolderOpen class="w-3.5 h-3.5 text-amber-400" />
          <span>Downloads</span>
        </button>
      {/if}

      <!-- Live Speed Limiter QoS Pill -->
      <div class="relative flex items-center">
        <button
          type="button"
          on:click={() => (showSpeedLimitMenu = !showSpeedLimitMenu)}
          class="flex items-center gap-1.5 px-3 py-1 rounded-xl text-xs font-mono font-semibold transition-all {$appSettings.speed_limit_mbps
            ? 'bg-amber-500/15 text-amber-300 border border-amber-500/30'
            : 'bg-slate-800 text-slate-300 border border-slate-700 hover:text-white'}"
          title="Click to set transfer speed limit"
        >
          <Gauge class="w-3.5 h-3.5 {$appSettings.speed_limit_mbps ? 'text-amber-400' : 'text-cyan-400'}" />
          <span>{$appSettings.speed_limit_mbps ? `${$appSettings.speed_limit_mbps} MB/s` : 'Max Speed'}</span>
          <ChevronDown class="w-3 h-3 text-slate-400" />
        </button>

        {#if showSpeedLimitMenu}
          <div
            class="absolute right-0 top-full mt-2 w-48 rounded-xl bg-slate-900 border border-slate-700 shadow-2xl p-2 z-30 flex flex-col gap-1 backdrop-blur-xl"
          >
            <span class="text-[10px] font-mono uppercase text-slate-400 px-2 py-0.5">Speed Cap (QoS)</span>
            {#each [0, 5, 10, 25, 50] as preset}
              <button
                type="button"
                on:click={async () => {
                  await raydropApi.setSpeedLimit(preset);
                  appSettings.update((s) => ({ ...s, speed_limit_mbps: preset }));
                  showSpeedLimitMenu = false;
                }}
                class="flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-mono transition-colors {($appSettings.speed_limit_mbps ?? 0) === preset
                  ? 'bg-cyan-500/20 text-cyan-300 font-bold'
                  : 'text-slate-300 hover:bg-slate-800'}"
              >
                <span>{preset === 0 ? '⚡ Unlimited (Max)' : `${preset} MB/s`}</span>
                {#if ($appSettings.speed_limit_mbps ?? 0) === preset}
                  <Check class="w-3.5 h-3.5 text-cyan-400" />
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Auto-Power Action Pill -->
      <div class="relative flex items-center">
        <button
          type="button"
          on:click={() => (showPowerMenu = !showPowerMenu)}
          class="flex items-center gap-1.5 px-3 py-1 rounded-xl text-xs font-mono font-semibold transition-all {autoPowerAction !== 'none'
            ? 'bg-rose-500/15 text-rose-300 border border-rose-500/30 shadow-sm'
            : 'bg-slate-800 text-slate-300 border border-slate-700 hover:text-white'}"
          title="Action when all downloads & transfers finish"
        >
          <Power class="w-3.5 h-3.5 {autoPowerAction !== 'none' ? 'text-rose-400 animate-pulse' : 'text-slate-400'}" />
          <span>{autoPowerAction === 'none' ? 'Auto-Power: Off' : `On Finish: ${autoPowerAction.toUpperCase()}`}</span>
          <ChevronDown class="w-3 h-3 text-slate-400" />
        </button>

        {#if showPowerMenu}
          <div
            class="absolute right-0 top-full mt-2 w-52 rounded-xl bg-slate-900 border border-slate-700 shadow-2xl p-2 z-30 flex flex-col gap-1 backdrop-blur-xl"
          >
            <span class="text-[10px] font-mono uppercase text-slate-400 px-2 py-0.5">When All Tasks Complete</span>
            {#each [
              { val: 'none', label: 'Do Nothing (Normal)' },
              { val: 'sleep', label: 'Sleep PC' },
              { val: 'shutdown', label: 'Shutdown PC' },
              { val: 'hibernate', label: 'Hibernate PC' },
            ] as opt}
              <button
                type="button"
                on:click={() => selectAutoPower(opt.val)}
                class="flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-mono transition-colors {autoPowerAction === opt.val
                  ? 'bg-rose-500/20 text-rose-300 font-bold'
                  : 'text-slate-300 hover:bg-slate-800'}"
              >
                <span>{opt.label}</span>
                {#if autoPowerAction === opt.val}
                  <Check class="w-3.5 h-3.5 text-rose-400" />
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>

  <!-- Content: Active Transfers & History (Full View) -->
  <div class="flex-1 overflow-y-auto p-5 space-y-6">
    {#if actionError}
      <div class="p-3 rounded-xl bg-rose-500/10 border border-rose-500/30 text-xs text-rose-300 flex items-center justify-between">
        <span>{actionError}</span>
        <button on:click={() => (actionError = '')} class="text-rose-400 hover:text-white ml-2">✕</button>
      </div>
    {/if}

    <!-- HTTP Multi-Segment Parallel Accelerator Downloads (IDM Engine) -->
    {#if visibleHttpTasks.length > 0}
      <div class="space-y-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Zap class="w-4 h-4 text-cyan-400" />
            <h3 class="text-xs font-bold uppercase tracking-wider text-cyan-400">
              HTTP Accelerator Downloads ({visibleHttpTasks.length})
            </h3>
          </div>
          <span class="text-xs text-slate-400 font-mono">Parallel Segment Accelerator</span>
        </div>

        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
          {#each visibleHttpTasks as task (task.id)}
            {@const pct = task.total_bytes > 0 ? Math.min(100, Math.round((task.downloaded_bytes / task.total_bytes) * 100)) : 0}
            <div class="p-4 rounded-2xl bg-slate-900/90 border border-cyan-500/30 shadow-xl shadow-cyan-500/5 space-y-3 transition-all">
              <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-11 h-11 rounded-xl bg-cyan-500/15 border border-cyan-500/40 text-cyan-300 flex items-center justify-center font-mono font-extrabold text-xs shrink-0 shadow-sm">
                    <Download class="w-5 h-5 text-cyan-400" />
                  </div>

                  <div class="min-w-0">
                    <h4 class="text-sm font-bold text-white truncate" title={task.filename || task.url}>
                      {task.filename || 'Direct Download'}
                    </h4>

                    <div class="flex items-center gap-2 mt-0.5 text-xs text-slate-400">
                      <span class="text-[10px] font-mono uppercase px-1.5 py-0.2 rounded {task.status === 'completed' ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/30' : task.status === 'paused' ? 'bg-amber-500/10 text-amber-300 border border-amber-500/30' : task.status === 'error' ? 'bg-rose-500/10 text-rose-300 border border-rose-500/30' : task.status === 'queued' ? 'bg-purple-500/10 text-purple-300 border border-purple-500/30' : 'bg-cyan-500/10 text-cyan-300 border border-cyan-500/30'}">
                        {task.status.toUpperCase()}
                      </span>
                      <span>•</span>
                      <span class="text-[10px] font-mono text-cyan-300 bg-cyan-950/60 px-1.5 py-0.5 rounded border border-cyan-800/40">
                        {(task.segments_count || 1) > 1 ? `${task.segments_count}x Segments` : 'Single Stream'}
                      </span>
                      {#if task.download_speed_bps > 0 && task.status === 'downloading'}
                        <span>•</span>
                        <span class="text-xs font-mono text-emerald-400 flex items-center gap-1">
                          <Zap class="w-3 h-3" />
                          <span>{formatBytes(task.download_speed_bps)}/s</span>
                          {#if task.total_bytes > task.downloaded_bytes}
                            {@const httpEta = (task.total_bytes - task.downloaded_bytes) / task.download_speed_bps}
                            <span class="text-slate-400">•</span>
                            <span class="text-cyan-300">ETA {formatEta(httpEta)}</span>
                          {/if}
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Action Controls -->
                <div class="flex items-center gap-2 shrink-0">
                  {#if task.status === 'queued'}
                    <button
                      type="button"
                      on:click={() => handleStartHttpNow(task.id)}
                      class="text-xs text-purple-300 hover:text-purple-200 bg-purple-500/15 hover:bg-purple-500/25 border border-purple-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Force start download immediately"
                    >
                      <Play class="w-3.5 h-3.5" />
                      <span>Start Now</span>
                    </button>
                  {:else if task.status === 'downloading'}
                    <button
                      type="button"
                      on:click={() => handlePauseHttp(task.id)}
                      class="text-xs text-amber-300 hover:text-amber-200 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Pause download"
                    >
                      <Pause class="w-3.5 h-3.5" />
                      <span>Pause</span>
                    </button>
                  {:else if task.status === 'paused'}
                    <button
                      type="button"
                      on:click={() => handleResumeHttp(task.id)}
                      class="text-xs text-emerald-300 hover:text-emerald-200 bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/30 px-3 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Resume download"
                    >
                      <Play class="w-3.5 h-3.5" />
                      <span>Resume</span>
                    </button>
                  {:else if task.status === 'error'}
                    <button
                      type="button"
                      on:click={() => handleResumeHttp(task.id)}
                      class="text-xs text-rose-300 hover:text-rose-200 bg-rose-500/15 hover:bg-rose-500/25 border border-rose-500/30 px-3 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Retry download"
                    >
                      <RotateCcw class="w-3.5 h-3.5" />
                      <span>Retry</span>
                    </button>
                  {/if}

                  {#if task.status === 'error' || task.status === 'paused'}
                    <button
                      type="button"
                      on:click={() => openRefreshModal(task)}
                      class="text-xs text-sky-300 hover:text-sky-200 bg-sky-500/15 hover:bg-sky-500/25 border border-sky-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Update expired session download address (IDM-style resume)"
                    >
                      <Link class="w-3.5 h-3.5" />
                      <span>Refresh Link</span>
                    </button>
                  {/if}

                  {#if (task.status === 'completed' || (task.status === 'downloading' && task.downloaded_bytes > 524288)) && task.save_path}
                    {#if isVideoFile(task.filename) || isAudioFile(task.filename)}
                      <button
                        type="button"
                        on:click={() => openMediaPreview(task.save_path, task.filename)}
                        class="px-2.5 py-1 rounded-xl bg-cyan-500/15 hover:bg-cyan-500/25 border border-cyan-500/30 text-xs text-cyan-300 flex items-center gap-1 transition cursor-pointer"
                        title={task.status === 'downloading' ? "Instant Stream while downloading" : "Play in Raydrop Media Lightbox"}
                      >
                        <Play class="w-3.5 h-3.5" />
                        <span>{task.status === 'downloading' ? 'Stream' : 'Play'}</span>
                      </button>
                    {/if}
                  {/if}

                  {#if task.status === 'completed' && task.save_path}
                    <button
                      type="button"
                      on:click={() => openHashModal(task.filename, task.save_path)}
                      class="p-1.5 text-slate-400 hover:text-cyan-300 hover:bg-slate-800 rounded-xl transition cursor-pointer"
                      title="Calculate & Verify SHA-256 Checksum"
                    >
                      <Hash class="w-4 h-4" />
                    </button>
                  {/if}

                  {#if task.save_path}
                    <button
                      type="button"
                      on:click={() => raydropApi.openFolderPath(task.save_path)}
                      class="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded-xl transition cursor-pointer"
                      title="Open destination folder"
                    >
                      <FolderOpen class="w-4 h-4" />
                    </button>
                  {/if}

                  <button
                    type="button"
                    on:click={() => handleDeleteHttp(task.id)}
                    class="p-1.5 text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 rounded-xl transition cursor-pointer"
                    title="Remove download task"
                  >
                    <XCircle class="w-4 h-4" />
                  </button>
                </div>
              </div>

              <!-- Progress Bar -->
              <div class="space-y-1.5">
                <div class="w-full bg-slate-950 h-2 rounded-full overflow-hidden border border-slate-800">
                  <div
                    class="bg-gradient-to-r from-cyan-500 to-indigo-500 h-full transition-all duration-300"
                    style="width: {pct}%"
                  ></div>
                </div>

                <div class="flex items-center justify-between text-xs text-slate-400 font-mono">
                  <span>{pct}%</span>
                  <span>
                    {formatBytes(task.downloaded_bytes)}
                    {#if task.total_bytes > 0}
                      / {formatBytes(task.total_bytes)}
                    {/if}
                  </span>
                </div>
              </div>

              {#if task.error || task.error_message}
                <div class="p-2 rounded-lg bg-rose-500/10 border border-rose-500/30 text-[11px] text-rose-300 flex items-center justify-between gap-2">
                  <div class="flex items-center gap-2 min-w-0">
                    <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
                    <span class="truncate">{task.error || task.error_message}</span>
                  </div>
                  <button
                    type="button"
                    on:click={() => openRefreshModal(task)}
                    class="shrink-0 text-[10px] font-bold text-sky-400 hover:text-sky-300 underline cursor-pointer"
                  >
                    Refresh Address
                  </button>
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- 0. BitTorrent Downloads (Magnet Swarm Engine) -->
    {#if $torrentTasks.length > 0}
      <div class="space-y-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Magnet class="w-4 h-4 text-amber-400" />
            <h3 class="text-xs font-bold uppercase tracking-wider text-amber-400">
              BitTorrent Downloads ({$torrentTasks.length})
            </h3>
          </div>
          <span class="text-xs text-slate-400 font-mono">P2P Swarm Engine</span>
        </div>

        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
          {#each $torrentTasks as task (task.id)}
            {@const pct = Math.round(task.progress * 100)}
            <div class="p-4 rounded-2xl bg-slate-900/90 border border-amber-500/30 shadow-xl shadow-amber-500/5 space-y-3 transition-all">
              <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-11 h-11 rounded-xl bg-amber-500/15 border border-amber-500/40 text-amber-300 flex items-center justify-center font-mono font-extrabold text-xs shrink-0 shadow-sm">
                    <Magnet class="w-5 h-5 text-amber-400" />
                  </div>

                  <div class="min-w-0">
                    <h4 class="text-sm font-bold text-white truncate" title={task.name}>
                      {task.name}
                    </h4>

                    <div class="flex items-center gap-2 mt-0.5 text-xs text-slate-400">
                      <span class="text-[10px] font-mono uppercase px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-300 border border-amber-500/30">
                        {task.status.toUpperCase()}
                      </span>
                      <span>•</span>
                      <span class="text-xs font-mono text-cyan-300">
                        {task.peers_connected} peer(s)
                      </span>
                      {#if task.download_speed_bps > 0}
                        <span>•</span>
                        <span class="text-xs font-mono text-emerald-400 flex items-center gap-1">
                          <Zap class="w-3 h-3" />
                          <span>{formatBytes(task.download_speed_bps)}/s</span>
                          {#if task.total_bytes > task.downloaded_bytes}
                            {@const torrentEta = (task.total_bytes - task.downloaded_bytes) / task.download_speed_bps}
                            <span class="text-slate-400">•</span>
                            <span class="text-cyan-300">ETA {formatEta(torrentEta)}</span>
                          {/if}
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Action Controls -->
                <div class="flex items-center gap-2 shrink-0">
                  {#if task.status === 'downloading' || task.status === 'fetching_metadata'}
                    <button
                      type="button"
                      on:click={() => handlePauseTorrent(task.id)}
                      class="text-xs text-amber-300 hover:text-amber-200 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Pause download"
                    >
                      <Pause class="w-3.5 h-3.5" />
                      <span>Pause</span>
                    </button>
                  {:else if task.status === 'paused'}
                    <button
                      type="button"
                      on:click={() => handleResumeTorrent(task.id)}
                      class="text-xs text-emerald-300 hover:text-emerald-200 bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/30 px-3 py-1 rounded-xl flex items-center gap-1 transition cursor-pointer"
                      title="Resume download"
                    >
                      <Play class="w-3.5 h-3.5" />
                      <span>Resume</span>
                    </button>
                  {/if}

                  {#if task.save_path}
                    <button
                      type="button"
                      on:click={() => raydropApi.openFolderPath(task.save_path)}
                      class="p-1.5 text-slate-400 hover:text-white hover:bg-slate-800 rounded-xl transition cursor-pointer"
                      title="Open destination folder"
                    >
                      <FolderOpen class="w-4 h-4" />
                    </button>
                  {/if}

                  <button
                    type="button"
                    on:click={() => handleDeleteTorrent(task.id)}
                    class="p-1.5 text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 rounded-xl transition cursor-pointer"
                    title="Remove torrent task"
                  >
                    <XCircle class="w-4 h-4" />
                  </button>
                </div>
              </div>

              <!-- Progress Bar -->
              <div class="space-y-1.5">
                <div class="w-full bg-slate-950 h-2 rounded-full overflow-hidden border border-slate-800">
                  <div
                    class="bg-gradient-to-r from-amber-500 to-orange-500 h-full transition-all duration-300"
                    style="width: {pct}%"
                  ></div>
                </div>

                <div class="flex items-center justify-between text-[11px] font-mono text-slate-400">
                  <span>
                    {task.total_bytes > 0
                      ? `${formatBytes(task.downloaded_bytes)} / ${formatBytes(task.total_bytes)}`
                      : 'Connecting to swarm...'}
                  </span>
                  <span class="text-white font-bold">{pct}%</span>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- 1. Active / Paused Resumable Streaming Transfers -->
    {#if filteredActive.length > 0}
      <div class="space-y-3">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Activity class="w-4 h-4 text-cyan-400 animate-pulse" />
            <h3 class="text-xs font-bold uppercase tracking-wider text-cyan-400">
              Active Transfers ({filteredActive.length})
            </h3>
          </div>
          <span class="text-xs text-slate-400 font-mono">Real-time Stream Engine</span>
        </div>

        <div class="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
          {#each filteredActive as t (t.id)}
            {@const pct = calcPercent(t.transferred_bytes, t.total_bytes)}
            {@const isParallel = t.total_bytes >= 4 * 1024 * 1024}
            {@const activeFiles = parseFilePaths(t.source_paths)}
            {@const primaryName = activeFiles[0] ? getBaseName(activeFiles[0]) : 'Transfer Stream'}
            {@const primaryFilePath = activeFiles[0]}
            {@const badge = getFileTypeBadge(primaryName)}
            {@const isImg = isImageFile(primaryName)}

            <div class="p-4 rounded-2xl bg-slate-900/90 border {t.status === 'paused' ? 'border-amber-500/40 shadow-amber-500/5' : 'border-cyan-500/30 shadow-cyan-500/5'} shadow-xl space-y-3 transition-all">
              <div class="flex items-start justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <!-- File Type Icon / Thumbnail Box -->
                  {#if isImg && primaryFilePath && transferThumbnails[primaryFilePath]}
                    <div class="w-11 h-11 rounded-xl overflow-hidden border border-cyan-500/30 bg-slate-800 shrink-0 shadow-sm">
                      <img
                        src={transferThumbnails[primaryFilePath]}
                        alt=""
                        class="w-full h-full object-cover"
                      />
                    </div>
                  {:else}
                    <div class="w-11 h-11 rounded-xl {badge.color} border flex items-center justify-center font-mono font-extrabold text-xs shrink-0 shadow-sm">
                      {badge.label}
                    </div>
                  {/if}

                  <div class="min-w-0">
                    <h4 class="text-sm font-bold text-white truncate" title={primaryName}>
                      {primaryName}
                      {#if activeFiles.length > 1}
                        <span class="text-xs font-normal text-slate-400">(+{activeFiles.length - 1} more)</span>
                      {/if}
                    </h4>

                    <div class="flex items-center gap-2 mt-0.5 text-xs text-slate-400">
                      {#if t.direction === 'outgoing'}
                        <span class="text-cyan-300 font-semibold flex items-center gap-1">
                          <ArrowUpRight class="w-3.5 h-3.5 text-cyan-400" />
                          To: {t.peer_alias}
                        </span>
                      {:else}
                        <span class="text-emerald-300 font-semibold flex items-center gap-1">
                          <ArrowDownLeft class="w-3.5 h-3.5 text-emerald-400" />
                          From: {t.peer_alias}
                        </span>
                      {/if}

                      <span>•</span>
                      <span class="text-[10px] font-mono uppercase px-1.5 py-0.2 rounded bg-slate-800 text-cyan-300 border border-slate-700">
                        {t.network_route === 'lan_direct' ? 'LAN Direct' : 'WAN P2P'}
                      </span>

                      {#if isParallel}
                        <span class="text-[10px] font-semibold px-1.5 py-0.2 rounded bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 flex items-center gap-0.5">
                          <Zap class="w-2.5 h-2.5" />
                          6x Parallel
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>

                <!-- Action Controls -->
                <div class="flex items-center gap-2 shrink-0">
                  {#if t.status === 'in_progress'}
                    <button
                      type="button"
                      on:click={() => handlePause(t.id)}
                      class="text-xs text-amber-300 hover:text-amber-200 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 px-2.5 py-1 rounded-xl flex items-center gap-1 transition"
                      title="Pause transfer (resumable anytime)"
                    >
                      <Pause class="w-3.5 h-3.5" />
                      <span>Pause</span>
                    </button>
                  {:else if t.status === 'paused'}
                    {#if t.direction === 'outgoing'}
                      <button
                        type="button"
                        on:click={() => handleResume(t.id)}
                        disabled={resumingId === t.id}
                        class="text-xs text-emerald-300 hover:text-emerald-200 bg-emerald-500/15 hover:bg-emerald-500/25 border border-emerald-500/30 px-3 py-1 rounded-xl flex items-center gap-1 transition disabled:opacity-50"
                        title="Resume transfer from the last byte"
                      >
                        <Play class="w-3.5 h-3.5" />
                        <span>{resumingId === t.id ? 'Reconnecting...' : 'Resume'}</span>
                      </button>
                    {:else}
                      <span class="text-[10px] text-amber-300/90 font-mono bg-amber-500/10 px-2 py-0.5 rounded-lg border border-amber-500/20">
                        Waiting for sender...
                      </span>
                    {/if}
                  {/if}

                  <button
                    type="button"
                    on:click={() => raydropApi.cancelTransfer(t.id)}
                    class="p-1.5 text-rose-400 hover:text-rose-300 hover:bg-rose-500/10 rounded-xl transition"
                    title="Cancel transfer"
                  >
                    <XCircle class="w-4 h-4" />
                  </button>
                </div>
              </div>

              <!-- Progress Bar -->
              <div class="space-y-1.5">
                <div class="w-full h-2.5 rounded-full bg-slate-950 overflow-hidden border border-slate-800">
                  <div
                    class="h-full {t.status === 'paused'
                      ? 'bg-amber-400'
                      : 'bg-gradient-to-r from-cyan-400 via-blue-500 to-indigo-500'} transition-all duration-150"
                    style="width: {pct}%"
                  ></div>
                </div>

                <div class="flex flex-col sm:flex-row sm:items-center justify-between text-xs font-mono text-slate-400 gap-1">
                  <span>{formatBytes(t.transferred_bytes)} / {formatBytes(t.total_bytes)} ({pct}%)</span>
                  {#if t.status === 'paused'}
                    <span class="text-amber-300">Checkpoint saved (.raypart) • Ready to resume</span>
                  {:else}
                    <span class="text-emerald-400 font-bold">
                      {(t.speed_mbps ?? 0).toFixed(1)} MB/s • ETA {formatEta(t.eta_seconds)}
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Metadata & File Details Toggle -->
              <div class="flex items-center justify-between text-[11px] font-mono text-slate-500 pt-2 border-t border-slate-800/60">
                <div class="flex items-center gap-1.5 text-slate-400">
                  <Clock class="w-3 h-3 text-slate-500 shrink-0" />
                  <span>Started: {formatTransferTime(t.created_at)}</span>
                </div>

                <div class="flex items-center gap-2">
                  <span class="text-cyan-400 font-semibold">{t.total_files} file(s)</span>
                  {#if activeFiles.length > 0}
                    <button
                      type="button"
                      on:click={() => toggleExpanded(t.id)}
                      class="text-slate-400 hover:text-slate-200 flex items-center gap-0.5"
                    >
                      <span>{expandedTransferIds[t.id] ? 'Hide' : 'Files'}</span>
                      {#if expandedTransferIds[t.id]}
                        <ChevronUp class="w-3 h-3" />
                      {:else}
                        <ChevronDown class="w-3 h-3" />
                      {/if}
                    </button>
                  {/if}
                </div>
              </div>

              <!-- Expanded File List -->
              {#if expandedTransferIds[t.id] && activeFiles.length > 0}
                <div class="pt-2 border-t border-slate-800/80 space-y-1.5 max-h-40 overflow-y-auto pr-1">
                  {#each activeFiles as fp}
                    <div class="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800">
                      <div class="flex items-center gap-2 min-w-0">
                        <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                        <span class="text-xs text-slate-200 truncate" title={fp}>{getBaseName(fp)}</span>
                      </div>
                      <button
                        type="button"
                        on:click={() => handleOpenFolder(fp)}
                        class="text-[11px] text-cyan-400 hover:underline shrink-0"
                      >
                        Locate
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- 2. Transfer History (Completed Transfers) -->
    <div class="space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
          Transfer History ({filteredHistory.length})
        </h3>
        {#if filteredHistory.length > 0}
          <button
            type="button"
            on:click={async () => {
              await raydropApi.clearTransferHistory();
              historicalTransfers.set([]);
            }}
            class="text-[11px] text-slate-500 hover:text-rose-400 transition"
          >
            Clear History
          </button>
        {/if}
      </div>

      {#if filteredHistory.length === 0 && filteredActive.length === 0}
        <!-- Beautiful Empty State (Matching Landing Page Demo Aesthetic) -->
        <div class="min-h-[22rem] flex flex-col items-center justify-center text-center p-4 sm:p-8 border border-dashed border-slate-800 rounded-2xl bg-slate-950/30">
          <div class="w-14 h-14 sm:w-16 sm:h-16 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center mb-3 sm:mb-4 text-cyan-400 shadow-xl shadow-cyan-500/10">
            <Upload class="w-7 h-7 sm:w-8 sm:h-8 stroke-[1.8]" />
          </div>
          <h4 class="text-sm sm:text-base font-bold text-white font-display">Ready for High-Speed P2P Transfers</h4>
          <p class="text-xs text-slate-400 mt-1 sm:mt-1.5 max-w-md leading-relaxed">
            Send files and folders of any size directly between computers on your local network (LAN) or internet (WAN). Zero cloud servers involved.
          </p>

          <div class="grid grid-cols-1 sm:grid-cols-2 lg:flex lg:flex-wrap items-center justify-center gap-2 sm:gap-3 mt-4 sm:mt-5 w-full max-w-xl">
            <button
              type="button"
              on:click={handlePickFiles}
              class="col-span-1 sm:col-span-2 lg:col-span-1 flex items-center justify-center gap-2 px-5 py-2.5 rounded-xl bg-gradient-to-r from-cyan-500 to-indigo-600 hover:from-cyan-400 hover:to-indigo-500 text-white font-bold text-xs shadow-lg shadow-cyan-500/20 transition-all"
            >
              <Plus class="w-4 h-4" />
              <span>Select Files to Send</span>
            </button>

            <button
              type="button"
              on:click={() => activeMainTab.set('lan')}
              class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold border border-slate-700 transition"
            >
              <Radio class="w-4 h-4 text-cyan-400" />
              <span>Scan LAN Radar</span>
            </button>

            <button
              type="button"
              on:click={handleShareViaWeb}
              class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-purple-600/20 hover:bg-purple-600/30 text-purple-200 text-xs font-semibold border border-purple-500/40 transition"
            >
              <QrCode class="w-4 h-4 text-purple-300" />
              <span>Share to Phone</span>
            </button>

            <button
              type="button"
              on:click={() => (showClipboardModal = true)}
              class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-200 text-xs font-semibold border border-indigo-500/40 transition"
            >
              <Clipboard class="w-4 h-4 text-indigo-300" />
              <span>Sync Clipboard</span>
            </button>

            <button
              type="button"
              on:click={() => (showFolderSyncModal = true)}
              class="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-200 text-xs font-semibold border border-emerald-500/40 transition"
            >
              <FolderSync class="w-4 h-4 text-emerald-300" />
              <span>Folder Sync</span>
            </button>
          </div>
        </div>
      {:else if filteredHistory.length === 0}
        <p class="text-xs text-slate-500 italic p-4 text-center">No completed transfers match the filter.</p>
      {:else}
        <div class="space-y-2">
          {#each filteredHistory as h (h.id)}
            {@const files = parseFilePaths(h.source_paths)}
            {@const primaryFilePath = files[0] || h.target_path}
            {@const primaryName = files[0] ? getBaseName(files[0]) : (h.target_path ? getBaseName(h.target_path) : 'File Package')}
            {@const badge = getFileTypeBadge(primaryName)}
            {@const isImg = isImageFile(primaryName)}

            <div class="p-3.5 rounded-xl bg-slate-900/70 hover:bg-slate-900 border border-slate-800/90 hover:border-slate-700 flex flex-col gap-2.5 transition-colors">
              <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  {#if isImg && primaryFilePath && transferThumbnails[primaryFilePath]}
                    <button
                      type="button"
                      on:click={() => openImagePreview(primaryFilePath, primaryName)}
                      class="w-10 h-10 rounded-xl overflow-hidden border border-cyan-500/30 bg-slate-800 shrink-0 relative group shadow-sm hover:ring-2 hover:ring-cyan-500/50 transition cursor-zoom-in"
                      title="Click to view image"
                    >
                      <img
                        src={transferThumbnails[primaryFilePath]}
                        alt=""
                        class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                      />
                      <div class="absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity">
                        <Eye class="w-4 h-4 text-white" />
                      </div>
                    </button>
                  {:else}
                    <div class="w-9 h-9 rounded-xl {badge.color} border flex items-center justify-center font-mono font-bold text-[10px] shrink-0">
                      {badge.label}
                    </div>
                  {/if}

                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-xs font-bold text-white truncate max-w-sm" title={primaryName}>
                        {primaryName}
                      </span>
                      {#if files.length > 1}
                        <button
                          type="button"
                          on:click={() => toggleExpanded(h.id)}
                          class="text-[10px] text-cyan-400 hover:text-cyan-300 font-medium flex items-center gap-0.5"
                        >
                          <span>({files.length} files)</span>
                          {#if expandedTransferIds[h.id]}
                            <ChevronUp class="w-3 h-3" />
                          {:else}
                            <ChevronDown class="w-3 h-3" />
                          {/if}
                        </button>
                      {/if}
                    </div>

                    <div class="flex items-center gap-2 mt-0.5 text-[11px] text-slate-400">
                      {#if h.direction === 'outgoing'}
                        <span class="text-cyan-300 font-medium flex items-center gap-1">
                          <ArrowUpRight class="w-3 h-3 text-cyan-400" />
                          Sent to {h.peer_alias}
                        </span>
                      {:else}
                        <span class="text-emerald-300 font-medium flex items-center gap-1">
                          <ArrowDownLeft class="w-3 h-3 text-emerald-400" />
                          Received from {h.peer_alias}
                        </span>
                      {/if}
                      <span>•</span>
                      <span class="text-slate-500">{formatTransferTime(h.created_at)}</span>
                    </div>
                  </div>
                </div>

                <!-- Dedicated File Size Column & Quick Action Buttons -->
                <div class="flex items-center gap-2.5 shrink-0 self-end sm:self-center">
                  <span
                    class="text-[11px] font-mono font-semibold text-slate-200 bg-slate-800/80 px-2.5 py-1 rounded-lg min-w-[74px] text-center"
                    title="Total file size"
                  >
                    {formatBytes(h.total_bytes)}
                  </span>
                  {#if h.status === 'completed'}
                    <span class="flex items-center gap-1 text-[11px] text-emerald-400 font-semibold bg-emerald-500/10 px-2 py-0.5 rounded-full border border-emerald-500/20">
                      <CheckCircle2 class="w-3.5 h-3.5" />
                      Verified
                    </span>

                    {#if primaryFilePath}
                      {#if isVideoFile(primaryName) || isAudioFile(primaryName)}
                        <button
                          type="button"
                          on:click={() => openMediaPreview(primaryFilePath, primaryName)}
                          class="px-2.5 py-1 rounded-lg bg-cyan-500/15 hover:bg-cyan-500/25 text-xs text-cyan-300 hover:text-white border border-cyan-500/30 transition flex items-center gap-1 cursor-pointer"
                          title="Play in Raydrop Media Lightbox"
                        >
                          <Play class="w-3 h-3" />
                          <span>Play</span>
                        </button>
                      {/if}
                      <button
                        type="button"
                        on:click={() => openHashModal(primaryName, primaryFilePath)}
                        class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-cyan-300 border border-slate-700 transition cursor-pointer"
                        title="Calculate & Verify SHA-256 Checksum"
                      >
                        <Hash class="w-3.5 h-3.5" />
                      </button>
                      <button
                        type="button"
                        on:click={() => handleOpenFile(primaryFilePath)}
                        class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 hover:text-white border border-slate-700 transition"
                        title="Open File Directly"
                      >
                        Open
                      </button>
                      <button
                        type="button"
                        on:click={() => handleOpenFolder(primaryFilePath)}
                        class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 border border-slate-700 transition"
                        title="Open Containing Folder"
                      >
                        <FolderOpen class="w-3.5 h-3.5" />
                      </button>
                    {/if}
                  {:else if h.status === 'failed'}
                    <span class="flex items-center gap-1 text-[11px] text-rose-400 font-semibold bg-rose-500/10 px-2 py-0.5 rounded-full border border-rose-500/20">
                      <XCircle class="w-3.5 h-3.5" />
                      Failed
                    </span>
                  {:else if h.status === 'cancelled'}
                    <span class="flex items-center gap-1 text-[11px] text-slate-400 font-semibold bg-slate-800 px-2 py-0.5 rounded-full border border-slate-700">
                      Cancelled
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Expanded Multiple Files List for History -->
              {#if expandedTransferIds[h.id] && files.length > 1}
                <div class="pt-2 border-t border-slate-800/80 space-y-1.5 max-h-40 overflow-y-auto pr-1">
                  {#each files as fp}
                    {@const subImg = isImageFile(fp)}
                    <div class="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800">
                      <div class="flex items-center gap-2 min-w-0">
                        {#if subImg}
                          <ImageIcon class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                        {:else}
                          <FileText class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
                        {/if}
                        <span class="text-xs text-slate-200 truncate" title={fp}>{getBaseName(fp)}</span>
                      </div>
                      <div class="flex items-center gap-2 shrink-0">
                        {#if subImg}
                          <button
                            type="button"
                            on:click={() => openImagePreview(fp, getBaseName(fp))}
                            class="text-[11px] text-emerald-400 hover:text-emerald-300 hover:underline flex items-center gap-0.5"
                          >
                            <Eye class="w-3 h-3" />
                            <span>Preview</span>
                          </button>
                        {:else if isVideoFile(fp) || isAudioFile(fp)}
                          <button
                            type="button"
                            on:click={() => openMediaPreview(fp, getBaseName(fp))}
                            class="text-[11px] text-cyan-400 hover:text-cyan-300 hover:underline flex items-center gap-0.5 cursor-pointer"
                          >
                            <Play class="w-3 h-3" />
                            <span>Play</span>
                          </button>
                        {/if}
                        <button
                          type="button"
                          on:click={() => openHashModal(getBaseName(fp), fp)}
                          class="text-[11px] text-slate-400 hover:text-cyan-300 hover:underline flex items-center gap-0.5 cursor-pointer"
                          title="Calculate SHA-256 Checksum"
                        >
                          <Hash class="w-3 h-3" />
                          <span>Hash</span>
                        </button>
                        <button
                          type="button"
                          on:click={() => handleOpenFile(fp)}
                          class="text-[11px] text-cyan-400 hover:text-cyan-300 hover:underline"
                        >
                          Open
                        </button>
                        <button
                          type="button"
                          on:click={() => handleOpenFolder(fp)}
                          class="text-[11px] text-slate-400 hover:text-slate-200 hover:underline"
                        >
                          Locate
                        </button>
                      </div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if webShareInfo}
    <WebShareModal
      shareInfo={webShareInfo}
      onClose={() => {
        webShareInfo = null;
      }}
    />
  {/if}

  {#if showClipboardModal}
    <ClipboardShareModal
      onClose={() => {
        showClipboardModal = false;
      }}
    />
  {/if}

  {#if showFolderSyncModal}
    <FolderSyncModal
      onClose={() => {
        showFolderSyncModal = false;
      }}
    />
  {/if}

  {#if showRefreshModal && refreshTask}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm">
      <div class="w-full max-w-lg max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-800 rounded-3xl p-6 shadow-2xl space-y-5 text-left">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2.5">
            <div class="w-9 h-9 rounded-xl bg-sky-500/15 border border-sky-500/30 flex items-center justify-center text-sky-400">
              <Link class="w-5 h-5" />
            </div>
            <div>
              <h3 class="text-base font-bold text-white">Refresh Download Address</h3>
              <p class="text-xs text-slate-400">Resume expired session without restarting</p>
            </div>
          </div>
          <button
            type="button"
            on:click={() => {
              showRefreshModal = false;
              refreshTask = null;
            }}
            class="text-slate-400 hover:text-white p-1 rounded-xl cursor-pointer"
          >
            ✕
          </button>
        </div>

        <div class="p-3.5 rounded-2xl bg-slate-950/70 border border-slate-800 text-xs space-y-1.5">
          <div class="text-slate-300 font-medium truncate">
            Target file: <span class="text-white font-bold">{refreshTask.filename}</span>
          </div>
          <div class="text-slate-400">
            Progress: <span class="text-cyan-400 font-mono font-semibold">{formatBytes(refreshTask.downloaded_bytes)}</span>
            {#if refreshTask.total_bytes > 0}
              / {formatBytes(refreshTask.total_bytes)}
            {/if}
            will be preserved!
          </div>
          <div class="text-[11px] text-slate-400 leading-relaxed pt-1">
            If the temporary token or link expired (403 Forbidden / session timeout), trigger the download in your browser to obtain a new URL and paste it below. Raydrop will seamlessly continue downloading from the last byte offset.
          </div>
        </div>

        <div class="space-y-2">
          <label for="new-download-url-input" class="text-xs font-semibold text-slate-300 block">
            New Download URL:
          </label>
          <input
            id="new-download-url-input"
            type="url"
            bind:value={refreshNewUrl}
            placeholder="https://..."
            class="w-full bg-slate-950 border border-slate-800 focus:border-sky-500 rounded-xl px-3.5 py-2.5 text-xs text-white placeholder-slate-500 outline-none font-mono"
          />
        </div>

        <div class="flex items-center justify-end gap-3 pt-2">
          <button
            type="button"
            on:click={() => {
              showRefreshModal = false;
              refreshTask = null;
            }}
            class="px-4 py-2 rounded-xl text-xs font-semibold text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            on:click={handleConfirmRefreshUrl}
            disabled={isRefreshingUrl || !refreshNewUrl.trim().startsWith('http')}
            class="px-5 py-2 rounded-xl text-xs font-bold text-white bg-sky-600 hover:bg-sky-500 disabled:opacity-50 disabled:cursor-not-allowed transition flex items-center gap-2 shadow-lg shadow-sky-600/20 cursor-pointer"
          >
            {#if isRefreshingUrl}
              <Zap class="w-3.5 h-3.5 animate-spin" />
              <span>Resuming...</span>
            {:else}
              <Play class="w-3.5 h-3.5" />
              <span>Update & Resume</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Image Zoom Lightbox Modal -->
  {#if previewImage}
    <div
      class="fixed inset-0 z-50 bg-slate-950/90 backdrop-blur-md flex flex-col items-center justify-between p-3 sm:p-5 select-none"
      on:keydown={(e) => e.key === 'Escape' && closeImagePreview()}
      role="dialog"
      tabindex="-1"
    >
      <!-- Modal Header -->
      <div class="w-full max-w-4xl flex items-center justify-between gap-3 bg-slate-900/90 border border-slate-800 rounded-2xl px-4 py-2.5 shadow-2xl">
        <div class="flex items-center gap-2.5 min-w-0">
          <ImageIcon class="w-4 h-4 text-cyan-400 shrink-0" />
          <span class="text-xs font-bold text-white truncate max-w-xs sm:max-w-md">{previewImage.name}</span>
          <span class="text-[10px] font-mono text-cyan-300 bg-cyan-500/10 px-2 py-0.5 rounded-full border border-cyan-500/20 shrink-0">
            {Math.round(zoomScale * 100)}%
          </span>
        </div>

        <div class="flex items-center gap-1.5 shrink-0">
          <!-- Zoom Out -->
          <button
            type="button"
            on:click={zoomOut}
            disabled={zoomScale <= 0.3}
            class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-40 text-slate-300 hover:text-white transition"
            title="Zoom Out (-)"
          >
            <ZoomOut class="w-4 h-4" />
          </button>

          <!-- Reset Zoom -->
          <button
            type="button"
            on:click={resetZoom}
            class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-cyan-400 transition"
            title="Reset Zoom (100%)"
          >
            <RotateCcw class="w-4 h-4" />
          </button>

          <!-- Zoom In -->
          <button
            type="button"
            on:click={zoomIn}
            disabled={zoomScale >= 5}
            class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-40 text-slate-300 hover:text-white transition"
            title="Zoom In (+)"
          >
            <ZoomIn class="w-4 h-4" />
          </button>

          <div class="w-px h-5 bg-slate-800 mx-1"></div>

          <!-- Open File -->
          <button
            type="button"
            on:click={() => handleOpenFile(previewImage.path)}
            class="px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 hover:text-white transition flex items-center gap-1.5"
            title="Open in default app"
          >
            <ExternalLink class="w-3.5 h-3.5 text-cyan-400" />
            <span class="hidden sm:inline">Open</span>
          </button>

          <!-- Open Folder -->
          <button
            type="button"
            on:click={() => handleOpenFolder(previewImage.path)}
            class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-amber-300 transition"
            title="Show in folder"
          >
            <FolderOpen class="w-4 h-4" />
          </button>

          <!-- Close -->
          <button
            type="button"
            on:click={closeImagePreview}
            class="p-2 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 hover:text-rose-300 border border-rose-500/20 transition ml-1"
            title="Close (Esc)"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Image Canvas Area with Pan & Wheel Zoom -->
      <div
        class="flex-1 w-full max-w-4xl my-3 rounded-2xl bg-slate-950/70 border border-slate-800/80 overflow-hidden flex items-center justify-center relative cursor-grab active:cursor-grabbing"
        role="region"
        aria-label="Image preview canvas"
        on:wheel|preventDefault={handleWheelZoom}
        on:mousedown={handleMouseDown}
        on:mousemove={handleMouseMove}
        on:mouseup={handleMouseUp}
        on:mouseleave={handleMouseUp}
      >
        <img
          src={convertFileSrc(previewImage.path)}
          alt={previewImage.name}
          on:error={(e) => {
            const target = e.currentTarget;
            if (previewImage && transferThumbnails[previewImage.path] && target.src !== transferThumbnails[previewImage.path]) {
              target.src = transferThumbnails[previewImage.path];
            }
          }}
          class="max-w-full max-h-full object-contain pointer-events-none transition-transform duration-75"
          style="transform: scale({zoomScale}) translate({zoomTranslate.x}px, {zoomTranslate.y}px);"
          draggable="false"
        />
      </div>

      <div class="text-[11px] text-slate-400 font-mono flex items-center gap-2">
        <span>Scroll mouse wheel to zoom</span>
        <span>•</span>
        <span>Drag to pan when zoomed</span>
        <span>•</span>
        <span>Press Esc to close</span>
      </div>
    </div>
  {/if}

  <!-- In-App Media Player Lightbox (Video & Audio) -->
  {#if previewMedia}
    <div
      class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex flex-col items-center justify-center p-4 select-none"
    >
      <div
        class="w-full max-w-3xl max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-800 rounded-2xl overflow-hidden shadow-2xl flex flex-col"
      >
        <!-- Modal Header -->
        <div class="flex items-center justify-between px-4 py-3 border-b border-slate-800 bg-slate-950/60">
          <div class="flex items-center gap-2 min-w-0">
            {#if previewMedia.type === 'video'}
              <Video class="w-4 h-4 text-cyan-400 shrink-0" />
            {:else}
              <Music class="w-4 h-4 text-cyan-400 shrink-0" />
            {/if}
            <span class="text-xs sm:text-sm font-semibold text-white truncate" title={previewMedia.name}>
              {previewMedia.name}
            </span>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <button
              type="button"
              on:click={() => handleOpenFile(previewMedia?.path || '')}
              class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-300 flex items-center gap-1.5 transition cursor-pointer"
              title="Open in default system player"
            >
              <ExternalLink class="w-3.5 h-3.5 text-cyan-400" />
              <span class="hidden sm:inline">System Player</span>
            </button>
            <button
              type="button"
              on:click={closeMediaPreview}
              class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
            >
              <X class="w-4 h-4" />
            </button>
          </div>
        </div>

        <!-- Player Canvas Area -->
        <div class="p-4 sm:p-6 bg-black flex items-center justify-center min-h-[220px]">
          {#if previewMedia.type === 'video'}
            <video
              src={convertFileSrc(previewMedia.path)}
              controls
              autoplay
              class="max-h-[60vh] max-w-full rounded-lg shadow-xl"
            >
              <track kind="captions" />
            </video>
          {:else}
            <div class="w-full max-w-md py-6 flex flex-col items-center gap-5">
              <div class="w-20 h-20 rounded-2xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shadow-xl shadow-cyan-500/10">
                <Music class="w-10 h-10" />
              </div>
              <audio
                src={convertFileSrc(previewMedia.path)}
                controls
                autoplay
                class="w-full"
              ></audio>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- Checksum SHA-256 Calculator & Verifier Modal -->
  {#if showHashModal}
    <div
      class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4 select-none"
    >
      <div
        class="w-full max-w-md max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-800 rounded-2xl p-5 shadow-2xl flex flex-col gap-4"
      >
        <div class="flex items-center justify-between pb-3 border-b border-slate-800">
          <div class="flex items-center gap-2">
            <Hash class="w-4 h-4 text-cyan-400" />
            <h3 class="text-sm font-bold text-white">SHA-256 Checksum Verification</h3>
          </div>
          <button
            type="button"
            on:click={closeHashModal}
            class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <div>
          <p class="text-xs text-slate-400">Target File:</p>
          <p class="text-xs font-semibold text-white truncate mt-0.5">{hashTargetFile?.name}</p>
        </div>

        <div class="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-1.5">
          <div class="flex items-center justify-between text-xs text-slate-400">
            <span>Calculated Hash (SHA-256):</span>
            {#if !isCalculatingHash && calculatedHash && !calculatedHash.startsWith('Error')}
              <button
                type="button"
                on:click={copyHashToClipboard}
                class="flex items-center gap-1 text-[11px] text-cyan-400 hover:text-cyan-300 font-mono cursor-pointer"
              >
                {#if hashCopied}
                  <Check class="w-3 h-3 text-emerald-400" />
                  <span class="text-emerald-400">Copied</span>
                {:else}
                  <Copy class="w-3 h-3" />
                  <span>Copy</span>
                {/if}
              </button>
            {/if}
          </div>
          {#if isCalculatingHash}
            <div class="flex items-center gap-2 py-2 text-xs text-cyan-400 font-mono">
              <span class="animate-spin inline-block">⏳</span>
              <span>Calculating SHA-256 checksum across file chunks...</span>
            </div>
          {:else}
            <p class="text-xs font-mono text-cyan-300 break-all select-all font-medium">
              {calculatedHash}
            </p>
          {/if}
        </div>

        <!-- Compare Hash Input -->
        <div class="space-y-1.5">
          <label for="hash-compare-input" class="block text-xs font-medium text-slate-300">
            Compare against expected checksum:
          </label>
          <input
            id="hash-compare-input"
            type="text"
            bind:value={compareHashInput}
            placeholder="Paste SHA-256 hash from source website to compare..."
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs font-mono text-slate-200 focus:outline-none focus:border-cyan-500"
          />

          {#if compareHashInput.trim() && calculatedHash && !isCalculatingHash}
            {@const matches = compareHashInput.trim().toLowerCase() === calculatedHash.trim().toLowerCase()}
            <div class="p-2.5 rounded-xl border text-xs font-semibold flex items-center gap-2 {matches
              ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-300'
              : 'bg-rose-500/15 border-rose-500/40 text-rose-300'}">
              {#if matches}
                <CheckCircle2 class="w-4 h-4 text-emerald-400 shrink-0" />
                <span>Checksum MATCHES! File integrity verified 100%.</span>
              {:else}
                <AlertTriangle class="w-4 h-4 text-rose-400 shrink-0" />
                <span>Checksum DOES NOT MATCH! File may be corrupted or modified.</span>
              {/if}
            </div>
          {/if}
        </div>

        <div class="flex justify-end pt-2 border-t border-slate-800">
          <button
            type="button"
            on:click={closeHashModal}
            class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 transition cursor-pointer"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Auto-Power Countdown Dialog -->
  {#if powerCountdown > 0}
    <div
      class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex items-center justify-center p-4 select-none"
    >
      <div
        class="w-full max-w-md max-h-[90vh] overflow-y-auto bg-slate-900 border border-rose-500/40 rounded-2xl p-6 shadow-2xl flex flex-col items-center text-center gap-4 animate-in fade-in zoom-in-95 duration-200"
      >
        <div class="w-16 h-16 rounded-full bg-rose-500/20 border border-rose-500/40 flex items-center justify-center text-rose-400 animate-pulse">
          <Power class="w-8 h-8" />
        </div>

        <div>
          <h3 class="text-base font-bold text-white">All Tasks Completed!</h3>
          <p class="text-xs text-slate-300 mt-1">
            System will <span class="font-bold text-rose-400 uppercase">{autoPowerAction}</span> in
          </p>
          <div class="text-4xl font-extrabold font-mono text-rose-400 my-2">
            {powerCountdown}s
          </div>
          <p class="text-[11px] text-slate-400">
            Press Cancel to stay on your desktop and keep Raydrop running.
          </p>
        </div>

        <div class="flex items-center gap-3 w-full pt-2">
          <button
            type="button"
            on:click={cancelPowerCountdown}
            class="flex-1 py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white border border-slate-700 transition cursor-pointer"
          >
            Cancel Action
          </button>
          <button
            type="button"
            on:click={executePowerNow}
            class="flex-1 py-2.5 rounded-xl bg-rose-600 hover:bg-rose-500 text-xs font-bold text-white shadow-lg shadow-rose-600/30 transition cursor-pointer"
          >
            {autoPowerAction.toUpperCase()} Now
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
