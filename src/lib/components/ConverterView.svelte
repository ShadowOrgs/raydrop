<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    Wand2,
    Image as ImageIcon,
    Smartphone,
    FileText,
    Film,
    Music,
    Plus,
    Trash2,
    FolderOpen,
    CheckCircle2,
    AlertCircle,
    Archive,
    Sliders,
    ExternalLink,
  } from "lucide-svelte";
  import { openWithLastDir } from "../utils/filePicker";
  import { cliConvertFiles } from "../stores/state";
  import {
    raydropApi,
    formatBytes,
    type ImageMeta,
    type ConvertResult,
    type MobileIconZipResult,
    type DocumentEnginesInfo,
    type DocumentConvertResult,
    type MediaEngineInfo,
    type MediaConvertResult,
  } from "../utils/tauri-ipc";

  let mode: "convert" | "document" | "media" | "mobile_icons" = "convert";

  // Convert / Resize / Compress state
  let selectedFiles: ImageMeta[] = [];
  let targetFormat = "ico";
  let enableResize = false;
  let resizeWidth: number | "" = "";
  let resizeHeight: number | "" = "";
  let keepAspect = true;
  let quality = 85;
  let outputDir = "";
  let processing = false;
  let convertResults: ConvertResult[] = [];
  let errorMsg = "";

  // Document Converter (DOC/DOCX <-> PDF) state
  let docPaths: string[] = [];
  let docTargetFormat: "pdf" | "docx" = "pdf";
  let docOutputDir = "";
  let docEngines: DocumentEnginesInfo | null = null;
  let docProcessing = false;
  let docResults: DocumentConvertResult[] = [];
  let docErrorMsg = "";

  // Video / Audio Media Converter state
  let mediaPaths: string[] = [];
  let mediaSubMode: "video_to_video" | "video_to_audio" = "video_to_audio";
  let mediaTargetFormat = "mp3";
  let mediaQualityPreset = "balanced";
  let mediaResolution = "original";
  let mediaAudioBitrate = "192k";
  let mediaOutputDir = "";
  let mediaEngine: MediaEngineInfo | null = null;
  let mediaProcessing = false;
  let mediaResults: MediaConvertResult[] = [];
  let mediaErrorMsg = "";
  let mediaProgressText = "";
  let mediaProgressPercent = 0;
  let unlistenMediaProgress: (() => void) | null = null;
  let unlistenFfmpegStatus: (() => void) | null = null;

  onMount(async () => {
    try {
      docEngines = await raydropApi.detectDocumentEngines();
      mediaEngine = await raydropApi.detectMediaConverterEngine();
    } catch (_) {}
    unlistenMediaProgress = await raydropApi.listenEvent<{
      file_index: number;
      total_files: number;
      filename: string;
      percent: number;
      status: string;
    }>("media_convert_progress", (p) => {
      mediaProgressPercent = Math.round(p.percent);
      mediaProgressText = `(${p.file_index + 1}/${p.total_files}) ${p.filename} — ${mediaProgressPercent}%`;
    });
    unlistenFfmpegStatus = await raydropApi.listenEvent<{
      status: string;
      percent?: number;
      message: string;
    }>("media_ffmpeg_status", (s) => {
      if ((s.status === "downloading" || s.status === "extracting") && s.message) {
        mediaProgressText = s.message;
        if (typeof s.percent === "number") {
          mediaProgressPercent = Math.round(s.percent);
        }
      }
    });
  });

  onDestroy(() => {
    if (unlistenMediaProgress) unlistenMediaProgress();
    if (unlistenFfmpegStatus) unlistenFfmpegStatus();
  });

  function switchMediaSubMode(sub: "video_to_video" | "video_to_audio") {
    mediaSubMode = sub;
    mediaTargetFormat = sub === "video_to_video" ? "mp4" : "mp3";
  }

  async function pickMediaFiles() {
    mediaErrorMsg = "";
    const picked = await openWithLastDir("converter_media_files", {
      multiple: true,
      title: "Select Video or Audio Files",
      filters: [
        {
          name: "Video & Audio",
          extensions: [
            "mp4",
            "mkv",
            "webm",
            "mov",
            "avi",
            "flv",
            "wmv",
            "m4v",
            "ts",
            "mp3",
            "wav",
            "flac",
            "m4a",
            "ogg",
          ],
        },
      ],
    });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    const set = new Set([...mediaPaths, ...paths]);
    mediaPaths = Array.from(set);
  }

  async function pickMediaOutputFolder() {
    const picked = await openWithLastDir("converter_media_out", {
      directory: true,
      multiple: false,
      title: "Choose Output Folder",
    });
    if (typeof picked === "string") {
      mediaOutputDir = picked;
    }
  }

  async function runMediaConvert() {
    if (mediaPaths.length === 0 || mediaProcessing) return;
    mediaProcessing = true;
    mediaErrorMsg = "";
    mediaResults = [];
    mediaProgressPercent = 0;
    mediaProgressText = "Preparing FFmpeg...";
    try {
      mediaResults = await raydropApi.convertMediaFiles({
        inputPaths: mediaPaths,
        mode: mediaSubMode,
        targetFormat: mediaTargetFormat,
        qualityPreset: mediaQualityPreset,
        resolution: mediaResolution,
        audioBitrate: mediaAudioBitrate,
        outputDir: mediaOutputDir.trim() || null,
      });
      mediaEngine = await raydropApi.detectMediaConverterEngine().catch(() => mediaEngine);
    } catch (err: any) {
      mediaErrorMsg = err?.message || String(err);
    } finally {
      mediaProcessing = false;
    }
  }

  // Mobile Icon Generator state
  let iconSource: ImageMeta | null = null;
  let iconPlatform: "both" | "android" | "ios" = "both";
  let generatingIcons = false;
  let iconZipResult: MobileIconZipResult | null = null;
  let iconErrorMsg = "";

  async function addPaths(paths: string[]) {
    errorMsg = "";
    for (const p of paths) {
      if (selectedFiles.some((f) => f.path === p)) continue;
      try {
        const meta = await raydropApi.getImageMetadata(p);
        selectedFiles = [...selectedFiles, meta];
      } catch (err: any) {
        errorMsg = err?.message || String(err);
      }
    }
  }

  // Consume paths sent via OS Right-Click Context Menu (--convert)
  $: if ($cliConvertFiles.length > 0) {
    const incoming = [...$cliConvertFiles];
    cliConvertFiles.set([]);
    const docIncoming = incoming.filter((p) => /\.(doc|docx|pdf|rtf|odt)$/i.test(p));
    const imgIncoming = incoming.filter((p) => !/\.(doc|docx|pdf|rtf|odt)$/i.test(p));
    if (docIncoming.length > 0) {
      mode = "document";
      docPaths = Array.from(new Set([...docPaths, ...docIncoming]));
      if (/\.pdf$/i.test(docIncoming[0])) {
        docTargetFormat = "docx";
      } else {
        docTargetFormat = "pdf";
      }
    }
    if (imgIncoming.length > 0) {
      if (docIncoming.length === 0) mode = "convert";
      addPaths(imgIncoming);
      if (imgIncoming[0] && !iconSource) {
        raydropApi
          .getImageMetadata(imgIncoming[0])
          .then((m) => (iconSource = m))
          .catch(() => {});
      }
    }
  }

  async function pickDocuments() {
    docErrorMsg = "";
    const picked = await openWithLastDir("converter_docs", {
      multiple: true,
      title: "Select DOC / DOCX / PDF Documents",
      filters: [
        { name: "Documents", extensions: ["doc", "docx", "pdf", "rtf", "odt"] },
      ],
    });
    const list = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (list.length > 0) {
      docPaths = Array.from(new Set([...docPaths, ...list]));
      if (/\.pdf$/i.test(list[0])) docTargetFormat = "docx";
      else docTargetFormat = "pdf";
    }
  }

  async function pickDocOutputFolder() {
    const picked = await openWithLastDir("converter_doc_outdir", {
      directory: true,
      multiple: false,
      title: "Select Document Output Folder (Optional)",
    });
    if (typeof picked === "string") {
      docOutputDir = picked;
    }
  }

  async function runConvertDocuments() {
    if (docPaths.length === 0 || docProcessing) return;
    docProcessing = true;
    docErrorMsg = "";
    docResults = [];
    try {
      docResults = await raydropApi.convertDocuments({
        inputPaths: docPaths,
        targetFormat: docTargetFormat,
        outputDir: docOutputDir.trim() || null,
      });
    } catch (err: any) {
      docErrorMsg = err?.message || String(err);
    } finally {
      docProcessing = false;
    }
  }

  async function pickImages() {
    const picked = await openWithLastDir("converter_images", {
      multiple: true,
      title: "Select Images to Convert / Resize / Compress",
      filters: [
        {
          name: "Images",
          extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif", "ico"],
        },
      ],
    });
    if (Array.isArray(picked)) {
      await addPaths(picked);
    } else if (typeof picked === "string") {
      await addPaths([picked]);
    }
  }

  async function pickOutputFolder() {
    const picked = await openWithLastDir("converter_outdir", {
      directory: true,
      multiple: false,
      title: "Select Output Folder (Optional)",
    });
    if (typeof picked === "string") {
      outputDir = picked;
    }
  }

  async function runConvert() {
    if (selectedFiles.length === 0 || processing) return;
    processing = true;
    errorMsg = "";
    convertResults = [];
    try {
      convertResults = await raydropApi.convertAndProcessImages({
        inputPaths: selectedFiles.map((f) => f.path),
        targetFormat,
        resizeWidth: enableResize && Number(resizeWidth) > 0 ? Number(resizeWidth) : null,
        resizeHeight: enableResize && Number(resizeHeight) > 0 ? Number(resizeHeight) : null,
        keepAspect,
        quality,
        outputDir: outputDir.trim() || null,
      });
    } catch (err: any) {
      errorMsg = err?.message || String(err);
    } finally {
      processing = false;
    }
  }

  async function pickIconSource() {
    iconErrorMsg = "";
    iconZipResult = null;
    const picked = await openWithLastDir("converter_mobile_icon", {
      multiple: false,
      title: "Select App Icon Source (PNG / JPG)",
      filters: [{ name: "PNG / JPG Image", extensions: ["png", "jpg", "jpeg"] }],
    });
    if (typeof picked === "string") {
      try {
        iconSource = await raydropApi.getImageMetadata(picked);
      } catch (err: any) {
        iconErrorMsg = err?.message || String(err);
      }
    }
  }

  async function runGenerateIcons() {
    if (!iconSource || generatingIcons) return;
    generatingIcons = true;
    iconErrorMsg = "";
    iconZipResult = null;
    try {
      iconZipResult = await raydropApi.generateMobileIconsZip({
        inputPath: iconSource.path,
        platform: iconPlatform,
      });
    } catch (err: any) {
      iconErrorMsg = err?.message || String(err);
    } finally {
      generatingIcons = false;
    }
  }
</script>

<div class="h-full w-full flex flex-col gap-4 overflow-y-auto pr-1">
  <!-- Header -->
  <div
    class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 shrink-0"
  >
    <div class="flex items-center gap-3">
      <div
        class="w-10 h-10 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shrink-0"
      >
        <Wand2 class="w-5 h-5" />
      </div>
      <div>
        <h2 class="text-base font-bold text-white">File Converter, Image Studio &amp; App Icons</h2>
        <p class="text-xs text-slate-400">
          Convert PNG ↔ ICO / JPG / WEBP / BMP, resize &amp; compress images, or build Android &amp; iOS icon packs (.zip).
        </p>
      </div>
    </div>

    <div class="flex items-center gap-1 bg-slate-950/80 p-1 rounded-xl border border-slate-800 shrink-0 flex-wrap">
      <button
        type="button"
        on:click={() => (mode = "convert")}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer {mode ===
        'convert'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Sliders class="w-3.5 h-3.5" />
        <span>Image Studio</span>
      </button>
      <button
        type="button"
        on:click={() => (mode = "document")}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer {mode ===
        'document'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <FileText class="w-3.5 h-3.5" />
        <span>DOCX ↔ PDF</span>
      </button>
      <button
        type="button"
        on:click={() => (mode = "media")}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer {mode ===
        'media'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Film class="w-3.5 h-3.5" />
        <span>Video / Audio</span>
      </button>
      <button
        type="button"
        on:click={() => (mode = "mobile_icons")}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer {mode ===
        'mobile_icons'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-400 hover:text-slate-200'}"
      >
        <Smartphone class="w-3.5 h-3.5" />
        <span>Mobile Icon Pack (ZIP)</span>
      </button>
    </div>
  </div>

  {#if mode === "convert"}
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <!-- Left: Selected Files -->
      <div class="lg:col-span-2 bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
            Selected Images ({selectedFiles.length})
          </h3>
          <div class="flex items-center gap-2">
            {#if selectedFiles.length > 0}
              <button
                type="button"
                on:click={() => {
                  selectedFiles = [];
                  convertResults = [];
                }}
                class="text-[11px] text-slate-400 hover:text-rose-400 cursor-pointer"
              >
                Clear All
              </button>
            {/if}
            <button
              type="button"
              on:click={pickImages}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Add Images</span>
            </button>
          </div>
        </div>

        {#if selectedFiles.length === 0}
          <div
            class="h-44 flex flex-col items-center justify-center text-center border border-dashed border-slate-800 rounded-2xl bg-slate-950/40 p-6"
          >
            <ImageIcon class="w-8 h-8 text-slate-600 mb-2" />
            <p class="text-xs font-semibold text-slate-300">No images selected yet</p>
            <p class="text-[11px] text-slate-500 mt-1">
              Click "Add Images" or right-click any image in Explorer → "Convert / Optimize with Raydrop".
            </p>
          </div>
        {:else}
          <div class="space-y-2 max-h-72 overflow-y-auto pr-1">
            {#each selectedFiles as f}
              <div
                class="flex items-center justify-between gap-3 px-3 py-2 rounded-xl bg-slate-950/70 border border-slate-800/80"
              >
                <div class="min-w-0">
                  <p class="text-xs font-semibold text-white truncate" title={f.path}>
                    {f.filename}
                  </p>
                  <p class="text-[10px] font-mono text-slate-400">
                    {f.format} • {f.width}×{f.height} • {formatBytes(f.size_bytes)}
                  </p>
                </div>
                <button
                  type="button"
                  on:click={() => (selectedFiles = selectedFiles.filter((x) => x.path !== f.path))}
                  class="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          </div>
        {/if}

        {#if convertResults.length > 0}
          <div class="pt-3 border-t border-slate-800 space-y-2">
            <div class="flex items-center gap-1.5 text-xs font-bold text-emerald-400">
              <CheckCircle2 class="w-4 h-4" />
              <span>Converted {convertResults.length} file(s)!</span>
            </div>
            <div class="space-y-1.5 max-h-44 overflow-y-auto pr-1">
              {#each convertResults as res}
                {@const savedPct = res.original_bytes > 0
                  ? Math.round(((res.original_bytes - res.output_bytes) / res.original_bytes) * 100)
                  : 0}
                <div class="flex items-center justify-between gap-2 px-3 py-2 rounded-xl bg-emerald-500/10 border border-emerald-500/25 text-xs">
                  <div class="min-w-0">
                    <p class="text-slate-100 font-semibold truncate" title={res.output_path}>
                      {res.output_path.split(/[/\\]/).pop()}
                    </p>
                    <p class="text-[10px] font-mono text-emerald-300">
                      {res.width}×{res.height} • {formatBytes(res.original_bytes)} → {formatBytes(res.output_bytes)}
                      {#if savedPct > 0} ({savedPct}% smaller){/if}
                    </p>
                  </div>
                  <button
                    type="button"
                    on:click={() => raydropApi.openFolderPath(res.output_path)}
                    class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-900 hover:bg-slate-800 text-slate-200 border border-slate-700 text-[11px] shrink-0 cursor-pointer"
                  >
                    <FolderOpen class="w-3 h-3" />
                    <span>Open Folder</span>
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Right: Options -->
      <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-4">
        <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
          Conversion &amp; Optimization
        </h3>

        <div>
          <label for="conv-format" class="block text-xs font-semibold text-slate-300 mb-1.5">
            Output Format
          </label>
          <select
            id="conv-format"
            bind:value={targetFormat}
            class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
          >
            <option value="ico">ICO (Windows Icon, max 256×256)</option>
            <option value="png">PNG (Lossless / Transparent)</option>
            <option value="jpg">JPG / JPEG (Compressed Photo)</option>
            <option value="webp">WEBP (Modern Web Format)</option>
            <option value="bmp">BMP (Bitmap)</option>
            <option value="gif">GIF</option>
            <option value="keep">Keep Original Format (Resize / Compress only)</option>
          </select>
        </div>

        <div class="space-y-2 pt-2 border-t border-slate-800/80">
          <label class="flex items-center justify-between text-xs font-semibold text-slate-300 cursor-pointer">
            <span>Resize Image</span>
            <input type="checkbox" bind:checked={enableResize} class="accent-cyan-500" />
          </label>
          {#if enableResize}
            <div class="grid grid-cols-2 gap-2">
              <input
                type="number"
                min="1"
                bind:value={resizeWidth}
                placeholder="Width (px)"
                class="px-3 py-1.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
              />
              <input
                type="number"
                min="1"
                bind:value={resizeHeight}
                placeholder="Height (px)"
                class="px-3 py-1.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
              />
            </div>
            <div class="flex flex-wrap gap-1.5">
              {#each [1920, 1080, 512, 256, 128] as px}
                <button
                  type="button"
                  on:click={() => {
                    resizeWidth = px;
                    resizeHeight = "";
                  }}
                  class="px-2 py-0.5 rounded-md bg-slate-800 hover:bg-slate-700 text-[10px] font-mono text-slate-300 cursor-pointer"
                >
                  {px}px
                </button>
              {/each}
            </div>
            <label class="flex items-center gap-2 text-[11px] text-slate-400 cursor-pointer">
              <input type="checkbox" bind:checked={keepAspect} class="accent-cyan-500" />
              <span>Keep aspect ratio</span>
            </label>
          {/if}
        </div>

        <div class="space-y-1.5 pt-2 border-t border-slate-800/80">
          <div class="flex items-center justify-between text-xs font-semibold text-slate-300">
            <span>Compression Quality</span>
            <span class="font-mono text-cyan-400">{quality}%</span>
          </div>
          <input
            type="range"
            min="10"
            max="100"
            step="5"
            bind:value={quality}
            class="w-full accent-cyan-500 cursor-pointer"
          />
          <p class="text-[10px] text-slate-500">
            Lower quality reduces JPG/PNG file size significantly.
          </p>
        </div>

        <div class="space-y-1.5 pt-2 border-t border-slate-800/80">
          <span class="block text-xs font-semibold text-slate-300">Output Folder</span>
          <div class="flex items-center gap-2">
            <input
              type="text"
              bind:value={outputDir}
              placeholder="Same folder as source file"
              class="flex-1 px-2.5 py-1.5 rounded-xl bg-slate-950 border border-slate-800 text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none"
            />
            <button
              type="button"
              on:click={pickOutputFolder}
              class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
              title="Choose output folder"
            >
              <FolderOpen class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {#if errorMsg}
          <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
            <AlertCircle class="w-4 h-4 shrink-0" />
            <span>{errorMsg}</span>
          </div>
        {/if}

        <button
          type="button"
          on:click={runConvert}
          disabled={selectedFiles.length === 0 || processing}
          class="w-full py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold shadow-lg shadow-cyan-500/20 disabled:opacity-40 cursor-pointer"
        >
          {processing ? "Processing..." : `Convert / Optimize (${selectedFiles.length})`}
        </button>
      </div>
    </div>
  {:else if mode === "document"}
    <!-- Document Converter: DOC / DOCX <-> PDF -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <div class="lg:col-span-2 bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-3">
        <div class="flex items-center justify-between flex-wrap gap-2">
          <div class="flex items-center gap-2">
            <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
              Selected Documents ({docPaths.length})
            </h3>
            {#if docEngines}
              {#if docEngines.has_word || docEngines.has_libreoffice}
                <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                  Engine: {docEngines.has_word ? "Microsoft Word" : "LibreOffice"} Ready
                </span>
              {:else}
                <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-300 border border-amber-500/30">
                  Install Word or LibreOffice for DOC/DOCX ↔ PDF
                </span>
              {/if}
            {/if}
          </div>
          <div class="flex items-center gap-2">
            {#if docPaths.length > 0}
              <button
                type="button"
                on:click={() => {
                  docPaths = [];
                  docResults = [];
                }}
                class="text-[11px] text-slate-400 hover:text-rose-400 cursor-pointer"
              >
                Clear All
              </button>
            {/if}
            <button
              type="button"
              on:click={pickDocuments}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Add DOC / PDF</span>
            </button>
          </div>
        </div>

        {#if docPaths.length === 0}
          <div class="h-44 flex flex-col items-center justify-center text-center border border-dashed border-slate-800 rounded-2xl bg-slate-950/40 p-6">
            <FileText class="w-8 h-8 text-slate-600 mb-2" />
            <p class="text-xs font-semibold text-slate-300">No documents selected</p>
            <p class="text-[11px] text-slate-500 mt-1">
              Supports <code>.doc</code>, <code>.docx</code> → <code>.pdf</code> and <code>.pdf</code> → <code>.docx</code> (also via Explorer right-click menu).
            </p>
          </div>
        {:else}
          <div class="space-y-2 max-h-72 overflow-y-auto pr-1">
            {#each docPaths as p}
              <div class="flex items-center justify-between gap-3 px-3 py-2 rounded-xl bg-slate-950/70 border border-slate-800/80">
                <div class="min-w-0">
                  <p class="text-xs font-semibold text-white truncate" title={p}>
                    {p.split(/[/\\]/).pop()}
                  </p>
                  <p class="text-[10px] font-mono text-slate-400 truncate">{p}</p>
                </div>
                <button
                  type="button"
                  on:click={() => (docPaths = docPaths.filter((x) => x !== p))}
                  class="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          </div>
        {/if}

        {#if docResults.length > 0}
          <div class="pt-3 border-t border-slate-800 space-y-2">
            <div class="flex items-center gap-1.5 text-xs font-bold text-emerald-400">
              <CheckCircle2 class="w-4 h-4" />
              <span>Converted {docResults.length} document(s)!</span>
            </div>
            <div class="space-y-1.5 max-h-44 overflow-y-auto pr-1">
              {#each docResults as res}
                <div class="flex items-center justify-between gap-2 px-3 py-2 rounded-xl bg-emerald-500/10 border border-emerald-500/25 text-xs">
                  <div class="min-w-0">
                    <p class="text-slate-100 font-semibold truncate" title={res.output_path}>
                      {res.output_path.split(/[/\\]/).pop()}
                    </p>
                    <p class="text-[10px] font-mono text-emerald-300">
                      {res.engine_used} • {formatBytes(res.original_bytes)} → {formatBytes(res.output_bytes)}
                    </p>
                  </div>
                  <div class="flex items-center gap-1.5 shrink-0">
                    <button
                      type="button"
                      on:click={() => raydropApi.openFilePath(res.output_path)}
                      class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/30 text-[11px] cursor-pointer"
                    >
                      <ExternalLink class="w-3 h-3" />
                      <span>Open</span>
                    </button>
                    <button
                      type="button"
                      on:click={() => raydropApi.openFolderPath(res.output_path)}
                      class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-900 hover:bg-slate-800 text-slate-200 border border-slate-700 text-[11px] cursor-pointer"
                    >
                      <FolderOpen class="w-3 h-3" />
                      <span>Folder</span>
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-4">
        <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
          Document Settings
        </h3>

        <div class="space-y-1.5">
          <span class="block text-xs font-semibold text-slate-300">Target Format</span>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              on:click={() => (docTargetFormat = "pdf")}
              class="py-2 rounded-xl text-xs font-semibold border transition-all cursor-pointer {docTargetFormat ===
              'pdf'
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50'
                : 'bg-slate-950/60 text-slate-400 border-slate-800 hover:text-slate-200'}"
            >
              DOC / DOCX → PDF
            </button>
            <button
              type="button"
              on:click={() => (docTargetFormat = "docx")}
              class="py-2 rounded-xl text-xs font-semibold border transition-all cursor-pointer {docTargetFormat ===
              'docx'
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50'
                : 'bg-slate-950/60 text-slate-400 border-slate-800 hover:text-slate-200'}"
            >
              PDF → DOCX
            </button>
          </div>
        </div>

        <div class="space-y-1.5 pt-2 border-t border-slate-800/80">
          <span class="block text-xs font-semibold text-slate-300">Output Folder</span>
          <div class="flex items-center gap-2">
            <input
              type="text"
              bind:value={docOutputDir}
              placeholder="Same folder as source file"
              class="flex-1 px-2.5 py-1.5 rounded-xl bg-slate-950 border border-slate-800 text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none"
            />
            <button
              type="button"
              on:click={pickDocOutputFolder}
              class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
              title="Choose output folder"
            >
              <FolderOpen class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {#if docErrorMsg}
          <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
            <AlertCircle class="w-4 h-4 shrink-0" />
            <span>{docErrorMsg}</span>
          </div>
        {/if}

        <button
          type="button"
          on:click={runConvertDocuments}
          disabled={docPaths.length === 0 || docProcessing}
          class="w-full py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold shadow-lg shadow-cyan-500/20 disabled:opacity-40 cursor-pointer"
        >
          {docProcessing ? "Converting..." : `Convert to ${docTargetFormat.toUpperCase()} (${docPaths.length})`}
        </button>
      </div>
    </div>
  {:else if mode === "mobile_icons"}
    <!-- Mobile Icon Pack Generator (Android + iOS ZIP) -->
    <div class="max-w-2xl w-full mx-auto bg-slate-900/80 border border-slate-800/80 rounded-2xl p-5 space-y-4">
      <div class="flex items-start justify-between gap-3">
        <div>
          <h3 class="text-sm font-bold text-white">Android &amp; iOS App Icon Generator (.zip)</h3>
          <p class="text-xs text-slate-400 mt-0.5">
            Upload 1 PNG/JPG (recommended 1024×1024) to generate all Android <code>mipmap-*</code> + Play Store (512px) and iOS <code>AppIcon.appiconset</code> + <code>Contents.json</code> in one ZIP.
          </p>
        </div>
      </div>

      <div class="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-3 p-4 rounded-xl bg-slate-950/70 border border-slate-800">
        {#if iconSource}
          <div class="min-w-0">
            <p class="text-xs font-bold text-white truncate">{iconSource.filename}</p>
            <p class="text-[11px] font-mono text-slate-400">
              {iconSource.format} • {iconSource.width}×{iconSource.height} • {formatBytes(iconSource.size_bytes)}
            </p>
          </div>
        {:else}
          <p class="text-xs text-slate-400">No source PNG/JPG selected</p>
        {/if}
        <button
          type="button"
          on:click={pickIconSource}
          class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white shrink-0 cursor-pointer"
        >
          {iconSource ? "Change Image" : "Select PNG / JPG"}
        </button>
      </div>

      <div class="space-y-1.5">
        <span class="block text-xs font-semibold text-slate-300">Target Platforms</span>
        <div class="grid grid-cols-3 gap-2">
          {#each [
            { id: "both", label: "Android + iOS" },
            { id: "android", label: "Android Only" },
            { id: "ios", label: "iOS Only" },
          ] as opt}
            <button
              type="button"
              on:click={() => (iconPlatform = opt.id)}
              class="py-2 rounded-xl text-xs font-semibold border transition-all cursor-pointer {iconPlatform ===
              opt.id
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50'
                : 'bg-slate-950/60 text-slate-400 border-slate-800 hover:text-slate-200'}"
            >
              {opt.label}
            </button>
          {/each}
        </div>
      </div>

      {#if iconErrorMsg}
        <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
          <AlertCircle class="w-4 h-4 shrink-0" />
          <span>{iconErrorMsg}</span>
        </div>
      {/if}

      {#if iconZipResult}
        <div class="flex items-center justify-between gap-3 p-3.5 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-xs">
          <div class="min-w-0">
            <p class="font-bold text-emerald-300">
              Generated {iconZipResult.total_files} icon files ({formatBytes(iconZipResult.zip_bytes)})!
            </p>
            <p class="text-[11px] font-mono text-slate-300 truncate" title={iconZipResult.zip_path}>
              {iconZipResult.zip_path}
            </p>
          </div>
          <button
            type="button"
            on:click={() => iconZipResult && raydropApi.openFolderPath(iconZipResult.zip_path)}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-slate-900 hover:bg-slate-800 text-white border border-slate-700 shrink-0 cursor-pointer"
          >
            <FolderOpen class="w-3.5 h-3.5" />
            <span>Open Folder</span>
          </button>
        </div>
      {/if}

      <button
        type="button"
        on:click={runGenerateIcons}
        disabled={!iconSource || generatingIcons}
        class="w-full py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center justify-center gap-2 shadow-lg shadow-cyan-500/20 disabled:opacity-40 cursor-pointer"
      >
        <Archive class="w-4 h-4" />
        <span>{generatingIcons ? "Generating ZIP..." : "Generate Mobile Icons (.zip)"}</span>
      </button>
    </div>
  {:else if mode === "media"}
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <div class="lg:col-span-2 bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-3">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-xs font-bold uppercase tracking-wider text-slate-400">
            Selected Video / Audio Files ({mediaPaths.length})
          </h3>
          <div class="flex items-center gap-2">
            {#if mediaPaths.length > 0}
              <button
                type="button"
                on:click={() => {
                  mediaPaths = [];
                  mediaResults = [];
                }}
                class="text-xs text-slate-400 hover:text-rose-300 cursor-pointer"
              >
                Clear
              </button>
            {/if}
            <button
              type="button"
              on:click={pickMediaFiles}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>Add Video / Audio</span>
            </button>
          </div>
        </div>

        {#if mediaPaths.length === 0}
          <div class="border border-dashed border-slate-700/80 rounded-xl p-8 text-center space-y-2">
            <Film class="w-8 h-8 text-slate-600 mx-auto" />
            <p class="text-xs text-slate-400">
              Select video or audio files to convert (Video → Video or Video → MP3/Audio).
            </p>
            <p class="text-[11px] text-slate-500">
              FFmpeg engine is auto-detected or auto-downloaded on first conversion.
            </p>
          </div>
        {:else}
          <div class="space-y-1.5 max-h-64 overflow-y-auto pr-1">
            {#each mediaPaths as p, idx (p)}
              <div class="flex items-center justify-between gap-2 px-3 py-2 rounded-xl bg-slate-950/70 border border-slate-800 text-xs">
                <span class="font-mono text-slate-200 truncate" title={p}>
                  {p.split(/[/\\]/).pop()}
                </span>
                <button
                  type="button"
                  on:click={() => (mediaPaths = mediaPaths.filter((_, i) => i !== idx))}
                  class="p-1 text-slate-500 hover:text-rose-400 cursor-pointer shrink-0"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          </div>
        {/if}

        {#if mediaProcessing}
          <div class="p-3 rounded-xl bg-slate-950/80 border border-cyan-500/30 space-y-2">
            <div class="flex items-center justify-between text-xs text-cyan-300 font-medium">
              <span class="truncate">{mediaProgressText}</span>
              <span>{mediaProgressPercent}%</span>
            </div>
            <div class="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden">
              <div
                class="h-full bg-cyan-400 transition-all duration-200"
                style="width: {mediaProgressPercent}%"
              ></div>
            </div>
          </div>
        {/if}

        {#if mediaErrorMsg}
          <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
            <AlertCircle class="w-4 h-4 shrink-0" />
            <span class="break-words">{mediaErrorMsg}</span>
          </div>
        {/if}

        {#if mediaResults.length > 0}
          <div class="space-y-2 pt-2 border-t border-slate-800">
            <p class="text-xs font-bold text-emerald-400 flex items-center gap-1.5">
              <CheckCircle2 class="w-4 h-4" />
              <span>Converted {mediaResults.length} file(s)</span>
            </p>
            {#each mediaResults as res (res.output_path)}
              <div class="flex items-center justify-between gap-2 p-2.5 rounded-xl bg-emerald-500/10 border border-emerald-500/25 text-xs">
                <div class="min-w-0">
                  <p class="font-mono text-white truncate" title={res.output_path}>
                    {res.output_path.split(/[/\\]/).pop()}
                  </p>
                  <p class="text-[10px] text-slate-400">
                    {formatBytes(res.original_bytes)} → <span class="text-emerald-300 font-semibold">{formatBytes(res.output_bytes)}</span>
                  </p>
                </div>
                <button
                  type="button"
                  on:click={() => raydropApi.openFolderPath(res.output_path)}
                  class="px-2.5 py-1 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-700 text-[11px] text-white flex items-center gap-1 shrink-0 cursor-pointer"
                >
                  <FolderOpen class="w-3 h-3" />
                  <span>Open</span>
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Right: Conversion Settings -->
      <div class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 space-y-4">
        <div class="space-y-1.5">
          <span class="block text-xs font-semibold text-slate-300">Conversion Mode</span>
          <div class="grid grid-cols-2 gap-1.5">
            <button
              type="button"
              on:click={() => switchMediaSubMode("video_to_audio")}
              class="py-2 rounded-xl text-xs font-semibold border flex items-center justify-center gap-1.5 cursor-pointer {mediaSubMode ===
              'video_to_audio'
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50'
                : 'bg-slate-950/60 text-slate-400 border-slate-800'}"
            >
              <Music class="w-3.5 h-3.5" />
              <span>Video → Audio</span>
            </button>
            <button
              type="button"
              on:click={() => switchMediaSubMode("video_to_video")}
              class="py-2 rounded-xl text-xs font-semibold border flex items-center justify-center gap-1.5 cursor-pointer {mediaSubMode ===
              'video_to_video'
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/50'
                : 'bg-slate-950/60 text-slate-400 border-slate-800'}"
            >
              <Film class="w-3.5 h-3.5" />
              <span>Video → Video</span>
            </button>
          </div>
        </div>

        {#if mediaSubMode === "video_to_audio"}
          <div>
            <label for="media-audio-fmt" class="block text-xs font-semibold text-slate-300 mb-1">
              Target Audio Format
            </label>
            <select
              id="media-audio-fmt"
              bind:value={mediaTargetFormat}
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white"
            >
              <option value="mp3">MP3 (Universal)</option>
              <option value="m4a">M4A (AAC)</option>
              <option value="wav">WAV (Lossless PCM)</option>
              <option value="flac">FLAC (Lossless)</option>
              <option value="ogg">OGG (Vorbis)</option>
              <option value="opus">OPUS</option>
            </select>
          </div>

          <div>
            <label for="media-audio-br" class="block text-xs font-semibold text-slate-300 mb-1">
              Audio Bitrate
            </label>
            <select
              id="media-audio-br"
              bind:value={mediaAudioBitrate}
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white"
            >
              <option value="128k">128 kbps</option>
              <option value="192k">192 kbps (Recommended)</option>
              <option value="256k">256 kbps</option>
              <option value="320k">320 kbps (Max)</option>
            </select>
          </div>
        {:else}
          <div>
            <label for="media-video-fmt" class="block text-xs font-semibold text-slate-300 mb-1">
              Target Video Format
            </label>
            <select
              id="media-video-fmt"
              bind:value={mediaTargetFormat}
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white"
            >
              <option value="mp4">MP4 (H.264 + AAC)</option>
              <option value="mkv">MKV</option>
              <option value="webm">WEBM (VP9 + Opus)</option>
              <option value="mov">MOV</option>
              <option value="avi">AVI</option>
              <option value="gif">GIF (Animated)</option>
            </select>
          </div>

          <div>
            <label for="media-video-q" class="block text-xs font-semibold text-slate-300 mb-1">
              Quality Preset
            </label>
            <select
              id="media-video-q"
              bind:value={mediaQualityPreset}
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white"
            >
              <option value="balanced">Balanced (CRF 23)</option>
              <option value="high">High Quality (CRF 18)</option>
              <option value="compact">Small File Size (CRF 28)</option>
              <option value="copy">Fast Remux (Copy Codec)</option>
            </select>
          </div>

          <div>
            <label for="media-video-res" class="block text-xs font-semibold text-slate-300 mb-1">
              Resolution
            </label>
            <select
              id="media-video-res"
              bind:value={mediaResolution}
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white"
            >
              <option value="original">Original</option>
              <option value="1080p">1080p (Full HD)</option>
              <option value="720p">720p (HD)</option>
              <option value="480p">480p (SD)</option>
            </select>
          </div>
        {/if}

        <div>
          <span class="block text-xs font-semibold text-slate-300 mb-1">Output Folder</span>
          <div class="flex items-center gap-1.5">
            <input
              type="text"
              readonly
              value={mediaOutputDir || "Same as source file"}
              class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-800 text-[11px] text-slate-300 truncate"
            />
            <button
              type="button"
              on:click={pickMediaOutputFolder}
              class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-white cursor-pointer"
            >
              Browse
            </button>
          </div>
        </div>

        <p class="text-[10px] text-slate-400">
          FFmpeg Engine:
          {#if mediaEngine?.has_ffmpeg}
            <span class="text-emerald-400 font-semibold">Ready</span>
          {:else}
            <span class="text-amber-300">Auto-downloads on first convert</span>
          {/if}
        </p>

        <button
          type="button"
          on:click={runMediaConvert}
          disabled={mediaPaths.length === 0 || mediaProcessing}
          class="w-full py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center justify-center gap-2 shadow-lg shadow-cyan-500/20 disabled:opacity-40 cursor-pointer"
        >
          <Wand2 class="w-4 h-4" />
          <span>{mediaProcessing ? "Converting..." : "Start Media Conversion"}</span>
        </button>
      </div>
    </div>
  {/if}
</div>
