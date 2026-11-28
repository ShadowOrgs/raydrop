<script lang="ts">
  import { createEventDispatcher, onMount, onDestroy, tick } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeFile } from "@tauri-apps/plugin-fs";
  import {
    PenTool,
    Highlighter,
    Eraser,
    Square,
    Circle,
    ArrowUpRight,
    Type,
    Crop,
    EyeOff,
    Undo2,
    Redo2,
    RotateCcw,
    Download,
    Copy,
    Check,
    X,
    Loader2,
  } from "lucide-svelte";

  export let imageSrc: string;
  export let saving = false;

  const dispatch = createEventDispatcher<{
    close: void;
    save: { dataUrl: string; copyToClipboard: boolean };
  }>();

  type ToolId =
    | "pen"
    | "highlighter"
    | "eraser"
    | "arrow"
    | "rect"
    | "circle"
    | "blur"
    | "text"
    | "crop";

  const TOOLS: Array<{ id: ToolId; label: string; icon: any }> = [
    { id: "pen", label: "Pen", icon: PenTool },
    { id: "highlighter", label: "Highlighter", icon: Highlighter },
    { id: "eraser", label: "Eraser", icon: Eraser },
    { id: "arrow", label: "Arrow", icon: ArrowUpRight },
    { id: "rect", label: "Rectangle", icon: Square },
    { id: "circle", label: "Circle", icon: Circle },
    { id: "blur", label: "Blur / Redact", icon: EyeOff },
    { id: "text", label: "Text", icon: Type },
    { id: "crop", label: "Crop", icon: Crop },
  ];

  const COLORS = [
    "#ef4444",
    "#facc15",
    "#22c55e",
    "#06b6d4",
    "#3b82f6",
    "#a855f7",
    "#ffffff",
    "#0f172a",
  ];

  const SIZES = [2, 4, 8, 14, 22];

  let activeTool: ToolId = "pen";
  let strokeColor = "#ef4444";
  let strokeSize = 4;

  let baseCanvas: HTMLCanvasElement;
  let annoCanvas: HTMLCanvasElement;
  let previewCanvas: HTMLCanvasElement;

  let imgWidth = 800;
  let imgHeight = 600;
  let originalImg: HTMLImageElement | null = null;

  let drawing = false;
  let startX = 0;
  let startY = 0;
  let lastX = 0;
  let lastY = 0;

  // Crop selection in canvas coordinates
  let cropRect: { x: number; y: number; w: number; h: number } | null = null;

  // Floating text input
  let textPrompt: {
    x: number;
    y: number;
    cssLeft: number;
    cssTop: number;
    value: string;
  } | null = null;
  let textInputEl: HTMLInputElement | null = null;

  // Snapshot history (base + anno data URLs) for full Undo/Redo including Crop
  interface HistorySnapshot {
    base: string;
    anno: string;
    w: number;
    h: number;
  }
  let undoStack: HistorySnapshot[] = [];
  let redoStack: HistorySnapshot[] = [];
  let exportedCopied = false;

  function loadImageIntoCanvases(src: string, resetHistory = true) {
    const img = new Image();
    img.onload = () => {
      originalImg = img;
      imgWidth = img.naturalWidth || 800;
      imgHeight = img.naturalHeight || 600;
      tick().then(() => {
        if (!baseCanvas || !annoCanvas || !previewCanvas) return;
        baseCanvas.width = imgWidth;
        baseCanvas.height = imgHeight;
        annoCanvas.width = imgWidth;
        annoCanvas.height = imgHeight;
        previewCanvas.width = imgWidth;
        previewCanvas.height = imgHeight;

        const bctx = baseCanvas.getContext("2d");
        if (bctx) {
          bctx.clearRect(0, 0, imgWidth, imgHeight);
          bctx.drawImage(img, 0, 0, imgWidth, imgHeight);
        }
        annoCanvas.getContext("2d")?.clearRect(0, 0, imgWidth, imgHeight);
        previewCanvas.getContext("2d")?.clearRect(0, 0, imgWidth, imgHeight);

        if (resetHistory) {
          undoStack = [captureSnapshot()];
          redoStack = [];
        }
      });
    };
    img.src = src;
  }

  function captureSnapshot(): HistorySnapshot {
    return {
      base: baseCanvas.toDataURL("image/png"),
      anno: annoCanvas.toDataURL("image/png"),
      w: imgWidth,
      h: imgHeight,
    };
  }

  function pushUndo() {
    if (!baseCanvas || !annoCanvas) return;
    undoStack = [...undoStack.slice(-29), captureSnapshot()];
    redoStack = [];
  }

  function restoreSnapshot(snap: HistorySnapshot) {
    imgWidth = snap.w;
    imgHeight = snap.h;
    tick().then(() => {
      if (!baseCanvas || !annoCanvas || !previewCanvas) return;
      baseCanvas.width = snap.w;
      baseCanvas.height = snap.h;
      annoCanvas.width = snap.w;
      annoCanvas.height = snap.h;
      previewCanvas.width = snap.w;
      previewCanvas.height = snap.h;

      const bImg = new Image();
      bImg.onload = () => {
        const bctx = baseCanvas.getContext("2d");
        bctx?.clearRect(0, 0, snap.w, snap.h);
        bctx?.drawImage(bImg, 0, 0);
      };
      bImg.src = snap.base;

      const aImg = new Image();
      aImg.onload = () => {
        const actx = annoCanvas.getContext("2d");
        actx?.clearRect(0, 0, snap.w, snap.h);
        actx?.drawImage(aImg, 0, 0);
      };
      aImg.src = snap.anno;
      previewCanvas.getContext("2d")?.clearRect(0, 0, snap.w, snap.h);
    });
  }

  function undo() {
    if (undoStack.length <= 1) return;
    const current = undoStack[undoStack.length - 1];
    const prev = undoStack[undoStack.length - 2];
    undoStack = undoStack.slice(0, -1);
    redoStack = [...redoStack, current];
    cropRect = null;
    restoreSnapshot(prev);
  }

  function redo() {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    undoStack = [...undoStack, next];
    cropRect = null;
    restoreSnapshot(next);
  }

  function resetAll() {
    cropRect = null;
    textPrompt = null;
    loadImageIntoCanvases(imageSrc, true);
  }

  function getCanvasCoords(e: PointerEvent): { x: number; y: number } {
    const rect = previewCanvas.getBoundingClientRect();
    const scaleX = previewCanvas.width / Math.max(1, rect.width);
    const scaleY = previewCanvas.height / Math.max(1, rect.height);
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  }

  function drawArrow(
    ctx: CanvasRenderingContext2D,
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    width: number,
  ) {
    const headLen = Math.max(12, width * 3.2);
    const angle = Math.atan2(y2 - y1, x2 - x1);
    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(x2, y2);
    ctx.lineTo(
      x2 - headLen * Math.cos(angle - Math.PI / 6),
      y2 - headLen * Math.sin(angle - Math.PI / 6),
    );
    ctx.lineTo(
      x2 - headLen * Math.cos(angle + Math.PI / 6),
      y2 - headLen * Math.sin(angle + Math.PI / 6),
    );
    ctx.closePath();
    ctx.fillStyle = ctx.strokeStyle;
    ctx.fill();
  }

  function drawPixelatedBlur(
    targetCtx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
  ) {
    const rx = Math.max(0, Math.floor(Math.min(x, x + w)));
    const ry = Math.max(0, Math.floor(Math.min(y, y + h)));
    const rw = Math.min(imgWidth - rx, Math.abs(Math.round(w)));
    const rh = Math.min(imgHeight - ry, Math.abs(Math.round(h)));
    if (rw < 4 || rh < 4) return;

    const merged = exportMergedCanvas();
    const mctx = merged.getContext("2d");
    if (!mctx) return;

    const block = Math.max(8, Math.round(strokeSize * 1.8));
    const imgData = mctx.getImageData(rx, ry, rw, rh);
    const data = imgData.data;

    for (let by = 0; by < rh; by += block) {
      for (let bx = 0; bx < rw; bx += block) {
        const bw = Math.min(block, rw - bx);
        const bh = Math.min(block, rh - by);
        const cx = Math.min(rw - 1, bx + Math.floor(bw / 2));
        const cy = Math.min(rh - 1, by + Math.floor(bh / 2));
        const idx = (cy * rw + cx) * 4;
        targetCtx.fillStyle = `rgb(${data[idx]}, ${data[idx + 1]}, ${data[idx + 2]})`;
        targetCtx.fillRect(rx + bx, ry + by, bw, bh);
      }
    }
  }

  async function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if (textPrompt) {
      commitText();
      return;
    }
    const { x, y } = getCanvasCoords(e);

    if (activeTool === "text") {
      const rect = previewCanvas.getBoundingClientRect();
      textPrompt = {
        x,
        y,
        cssLeft: Math.max(8, Math.min(rect.width - 180, e.clientX - rect.left)),
        cssTop: Math.max(8, Math.min(rect.height - 40, e.clientY - rect.top)),
        value: "",
      };
      await tick();
      textInputEl?.focus();
      return;
    }

    previewCanvas.setPointerCapture(e.pointerId);
    drawing = true;
    startX = x;
    startY = y;
    lastX = x;
    lastY = y;

    if (activeTool === "pen" || activeTool === "highlighter" || activeTool === "eraser") {
      const ctx = annoCanvas.getContext("2d");
      if (!ctx) return;
      ctx.save();
      if (activeTool === "eraser") {
        ctx.globalCompositeOperation = "destination-out";
        ctx.lineWidth = strokeSize * 2.5;
      } else if (activeTool === "highlighter") {
        ctx.globalAlpha = 0.42;
        ctx.strokeStyle = strokeColor;
        ctx.lineWidth = strokeSize * 3;
      } else {
        ctx.strokeStyle = strokeColor;
        ctx.lineWidth = strokeSize;
      }
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x + 0.01, y + 0.01);
      ctx.stroke();
      ctx.restore();
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!drawing) return;
    const { x, y } = getCanvasCoords(e);

    if (activeTool === "pen" || activeTool === "highlighter" || activeTool === "eraser") {
      const ctx = annoCanvas.getContext("2d");
      if (!ctx) return;
      ctx.save();
      if (activeTool === "eraser") {
        ctx.globalCompositeOperation = "destination-out";
        ctx.lineWidth = strokeSize * 2.5;
      } else if (activeTool === "highlighter") {
        ctx.globalAlpha = 0.42;
        ctx.strokeStyle = strokeColor;
        ctx.lineWidth = strokeSize * 3;
      } else {
        ctx.strokeStyle = strokeColor;
        ctx.lineWidth = strokeSize;
      }
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.beginPath();
      ctx.moveTo(lastX, lastY);
      ctx.lineTo(x, y);
      ctx.stroke();
      ctx.restore();
      lastX = x;
      lastY = y;
      return;
    }

    const pctx = previewCanvas.getContext("2d");
    if (!pctx) return;
    pctx.clearRect(0, 0, imgWidth, imgHeight);
    pctx.save();
    pctx.strokeStyle = strokeColor;
    pctx.lineWidth = strokeSize;
    pctx.lineCap = "round";
    pctx.lineJoin = "round";

    if (activeTool === "rect") {
      pctx.strokeRect(startX, startY, x - startX, y - startY);
    } else if (activeTool === "circle") {
      const rx = Math.abs(x - startX) / 2;
      const ry = Math.abs(y - startY) / 2;
      const cx = (startX + x) / 2;
      const cy = (startY + y) / 2;
      pctx.beginPath();
      pctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
      pctx.stroke();
    } else if (activeTool === "arrow") {
      drawArrow(pctx, startX, startY, x, y, strokeSize);
    } else if (activeTool === "blur") {
      pctx.setLineDash([6, 4]);
      pctx.strokeStyle = "#22d3ee";
      pctx.lineWidth = 2;
      pctx.strokeRect(startX, startY, x - startX, y - startY);
    } else if (activeTool === "crop") {
      const rx = Math.min(startX, x);
      const ry = Math.min(startY, y);
      const rw = Math.abs(x - startX);
      const rh = Math.abs(y - startY);
      pctx.fillStyle = "rgba(0, 0, 0, 0.52)";
      pctx.fillRect(0, 0, imgWidth, imgHeight);
      pctx.clearRect(rx, ry, rw, rh);
      pctx.strokeStyle = "#22d3ee";
      pctx.lineWidth = 2;
      pctx.setLineDash([6, 4]);
      pctx.strokeRect(rx, ry, rw, rh);
    }
    pctx.restore();
  }

  function onPointerUp(e: PointerEvent) {
    if (!drawing) return;
    drawing = false;
    try {
      previewCanvas.releasePointerCapture(e.pointerId);
    } catch (_) {}

    const { x, y } = getCanvasCoords(e);
    const pctx = previewCanvas.getContext("2d");
    const actx = annoCanvas.getContext("2d");

    if (activeTool === "crop") {
      const rx = Math.max(0, Math.min(startX, x));
      const ry = Math.max(0, Math.min(startY, y));
      const rw = Math.min(imgWidth - rx, Math.abs(x - startX));
      const rh = Math.min(imgHeight - ry, Math.abs(y - startY));
      if (rw >= 12 && rh >= 12) {
        cropRect = { x: Math.round(rx), y: Math.round(ry), w: Math.round(rw), h: Math.round(rh) };
      } else {
        cropRect = null;
        pctx?.clearRect(0, 0, imgWidth, imgHeight);
      }
      return;
    }

    pctx?.clearRect(0, 0, imgWidth, imgHeight);
    if (!actx) return;

    if (activeTool === "rect") {
      actx.save();
      actx.strokeStyle = strokeColor;
      actx.lineWidth = strokeSize;
      actx.strokeRect(startX, startY, x - startX, y - startY);
      actx.restore();
    } else if (activeTool === "circle") {
      actx.save();
      actx.strokeStyle = strokeColor;
      actx.lineWidth = strokeSize;
      const rx = Math.abs(x - startX) / 2;
      const ry = Math.abs(y - startY) / 2;
      const cx = (startX + x) / 2;
      const cy = (startY + y) / 2;
      actx.beginPath();
      actx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
      actx.stroke();
      actx.restore();
    } else if (activeTool === "arrow") {
      actx.save();
      actx.strokeStyle = strokeColor;
      actx.lineWidth = strokeSize;
      actx.lineCap = "round";
      drawArrow(actx, startX, startY, x, y, strokeSize);
      actx.restore();
    } else if (activeTool === "blur") {
      drawPixelatedBlur(actx, startX, startY, x - startX, y - startY);
    }

    pushUndo();
  }

  function applyCrop() {
    if (!cropRect) return;
    const merged = exportMergedCanvas();
    const cropped = document.createElement("canvas");
    cropped.width = cropRect.w;
    cropped.height = cropRect.h;
    const cctx = cropped.getContext("2d");
    if (!cctx) return;
    cctx.drawImage(
      merged,
      cropRect.x,
      cropRect.y,
      cropRect.w,
      cropRect.h,
      0,
      0,
      cropRect.w,
      cropRect.h,
    );

    const newSrc = cropped.toDataURL("image/png");
    cropRect = null;
    imgWidth = cropped.width;
    imgHeight = cropped.height;
    tick().then(() => {
      baseCanvas.width = imgWidth;
      baseCanvas.height = imgHeight;
      annoCanvas.width = imgWidth;
      annoCanvas.height = imgHeight;
      previewCanvas.width = imgWidth;
      previewCanvas.height = imgHeight;
      const img = new Image();
      img.onload = () => {
        baseCanvas.getContext("2d")?.drawImage(img, 0, 0);
        annoCanvas.getContext("2d")?.clearRect(0, 0, imgWidth, imgHeight);
        previewCanvas.getContext("2d")?.clearRect(0, 0, imgWidth, imgHeight);
        pushUndo();
      };
      img.src = newSrc;
    });
  }

  function cancelCrop() {
    cropRect = null;
    previewCanvas?.getContext("2d")?.clearRect(0, 0, imgWidth, imgHeight);
  }

  function commitText() {
    if (!textPrompt) return;
    const txt = textPrompt.value.trim();
    const { x, y } = textPrompt;
    textPrompt = null;
    if (!txt) return;

    const actx = annoCanvas?.getContext("2d");
    if (!actx) return;
    const fontSize = Math.max(16, strokeSize * 4);
    actx.save();
    actx.font = `bold ${fontSize}px system-ui, -apple-system, sans-serif`;
    actx.fillStyle = strokeColor;
    actx.textBaseline = "top";
    actx.shadowColor = "rgba(0,0,0,0.65)";
    actx.shadowBlur = 4;
    actx.fillText(txt, x, y);
    actx.restore();
    pushUndo();
  }

  function exportMergedCanvas(): HTMLCanvasElement {
    const out = document.createElement("canvas");
    out.width = imgWidth;
    out.height = imgHeight;
    const ctx = out.getContext("2d");
    if (ctx) {
      ctx.drawImage(baseCanvas, 0, 0);
      ctx.drawImage(annoCanvas, 0, 0);
    }
    return out;
  }

  function exportDataUrl(): string {
    return exportMergedCanvas().toDataURL("image/png");
  }

  async function handleSaveToFile() {
    const dataUrl = exportDataUrl();
    const base64 = dataUrl.split(",")[1] || "";
    const raw = atob(base64);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);

    try {
      const targetPath = await save({
        defaultPath: `raydrop-snip-${Date.now()}.png`,
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });
      if (targetPath) {
        await writeFile(targetPath, bytes);
        return;
      }
    } catch (_) {
      const a = document.createElement("a");
      a.href = dataUrl;
      a.download = `raydrop-snip-${Date.now()}.png`;
      a.click();
    }
  }

  function handleSaveHistory(copyToClipboard: boolean) {
    if (cropRect) applyCrop();
    const dataUrl = exportDataUrl();
    if (copyToClipboard) {
      exportedCopied = true;
      setTimeout(() => (exportedCopied = false), 1500);
    }
    dispatch("save", { dataUrl, copyToClipboard });
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (textPrompt) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") {
      e.preventDefault();
      redo();
    } else if (e.key === "Escape") {
      if (cropRect) cancelCrop();
      else dispatch("close");
    }
  }

  onMount(() => {
    loadImageIntoCanvases(imageSrc, true);
    window.addEventListener("keydown", handleKeyDown);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleKeyDown);
  });
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div
  class="fixed inset-0 z-[70] bg-black/85 backdrop-blur-sm flex flex-col p-3 sm:p-5 select-none"
  on:click={() => dispatch("close")}
>
  <div
    class="flex-1 min-h-0 flex flex-col bg-slate-950 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden"
    on:click|stopPropagation
  >
    <!-- Top Snipping Toolbar -->
    <div class="px-4 py-2.5 bg-slate-900/95 border-b border-slate-800 flex flex-wrap items-center justify-between gap-2.5">
      <!-- Tools -->
      <div class="flex flex-wrap items-center gap-1">
        {#each TOOLS as t}
          <button
            type="button"
            on:click={() => {
              if (activeTool === "crop" && t.id !== "crop") cancelCrop();
              activeTool = t.id;
            }}
            class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-semibold transition active:scale-[0.97] cursor-pointer {activeTool === t.id
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/50'
              : 'text-slate-300 hover:bg-slate-800 border border-transparent'}"
            title={t.label}
          >
            <svelte:component this={t.icon} class="w-3.5 h-3.5" />
            <span class="hidden md:inline">{t.label}</span>
          </button>
        {/each}
      </div>

      <!-- Colors & Size -->
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex items-center gap-1 bg-slate-950/80 px-2 py-1 rounded-lg border border-slate-800">
          {#each COLORS as c}
            <button
              type="button"
              on:click={() => (strokeColor = c)}
              class="w-4 h-4 rounded-full border transition cursor-pointer {strokeColor === c
                ? 'scale-125 border-white ring-2 ring-cyan-400/50'
                : 'border-slate-600'}"
              style="background-color: {c}"
              title={c}
            />
          {/each}
          <input
            type="color"
            bind:value={strokeColor}
            class="w-5 h-5 rounded bg-transparent border-0 cursor-pointer"
            title="Custom Color"
          />
        </div>

        <div class="flex items-center gap-1 bg-slate-950/80 px-2 py-1 rounded-lg border border-slate-800">
          {#each SIZES as sz}
            <button
              type="button"
              on:click={() => (strokeSize = sz)}
              class="px-1.5 py-0.5 rounded text-[10px] font-mono transition cursor-pointer {strokeSize === sz
                ? 'bg-cyan-500 text-slate-950 font-bold'
                : 'text-slate-400 hover:text-white'}"
            >
              {sz}px
            </button>
          {/each}
        </div>
      </div>

      <!-- Undo / Redo / Save / Copy / Close -->
      <div class="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          on:click={undo}
          disabled={undoStack.length <= 1}
          class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 disabled:opacity-35 cursor-pointer"
          title="Undo (Ctrl+Z)"
        >
          <Undo2 class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={redo}
          disabled={redoStack.length === 0}
          class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 disabled:opacity-35 cursor-pointer"
          title="Redo (Ctrl+Y)"
        >
          <Redo2 class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={resetAll}
          class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 cursor-pointer"
          title="Reset to Original"
        >
          <RotateCcw class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-slate-700 mx-0.5"></div>

        <button
          type="button"
          on:click={handleSaveToFile}
          class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold transition active:scale-[0.97] cursor-pointer"
          title="Save PNG to Disk"
        >
          <Download class="w-3.5 h-3.5" />
          <span class="hidden sm:inline">Save PNG</span>
        </button>

        <button
          type="button"
          disabled={saving}
          on:click={() => handleSaveHistory(true)}
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold transition active:scale-[0.97] disabled:opacity-50 cursor-pointer"
          title="Update Copy History & Copy to Clipboard"
        >
          {#if saving}
            <Loader2 class="w-3.5 h-3.5 animate-spin" />
          {:else if exportedCopied}
            <Check class="w-3.5 h-3.5" />
          {:else}
            <Copy class="w-3.5 h-3.5" />
          {/if}
          <span>{exportedCopied ? "Saved & Copied!" : "Save & Copy"}</span>
        </button>

        <button
          type="button"
          on:click={() => dispatch("close")}
          class="p-1.5 rounded-lg bg-slate-800 hover:bg-rose-500/20 text-slate-300 hover:text-rose-300 cursor-pointer"
          title="Close (Esc)"
        >
          <X class="w-4 h-4" />
        </button>
      </div>
    </div>

    {#if cropRect}
      <div class="px-4 py-2 bg-cyan-500/10 border-b border-cyan-500/30 flex items-center justify-between text-xs">
        <span class="text-cyan-200 font-medium">
          Crop selection: {cropRect.w} × {cropRect.h} px
        </span>
        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={cancelCrop}
            class="px-2.5 py-1 rounded-lg bg-slate-800 text-slate-300 hover:text-white cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            on:click={applyCrop}
            class="px-3 py-1 rounded-lg bg-cyan-500 text-slate-950 font-bold hover:bg-cyan-400 cursor-pointer"
          >
            Apply Crop
          </button>
        </div>
      </div>
    {/if}

    <!-- Canvas Workspace -->
    <div class="flex-1 min-h-0 overflow-auto flex items-center justify-center p-4 bg-[#070b14]">
      <div class="relative inline-block shadow-2xl border border-slate-800 rounded-lg overflow-hidden">
        <canvas
          bind:this={baseCanvas}
          class="block max-w-full max-h-[72vh] object-contain"
        />
        <canvas
          bind:this={annoCanvas}
          class="absolute inset-0 w-full h-full pointer-events-none"
        />
        <canvas
          bind:this={previewCanvas}
          on:pointerdown={onPointerDown}
          on:pointermove={onPointerMove}
          on:pointerup={onPointerUp}
          class="absolute inset-0 w-full h-full touch-none {activeTool === 'text' ? 'cursor-text' : 'cursor-crosshair'}"
        />

        {#if textPrompt}
          <div
            class="absolute z-20 flex items-center gap-1 bg-slate-900/95 border border-cyan-500 rounded-lg p-1 shadow-xl"
            style="left: {textPrompt.cssLeft}px; top: {textPrompt.cssTop}px;"
          >
            <input
              bind:this={textInputEl}
              type="text"
              bind:value={textPrompt.value}
              placeholder="Type text..."
              on:keydown={(e) => {
                if (e.key === "Enter") commitText();
                if (e.key === "Escape") textPrompt = null;
              }}
              class="bg-transparent text-xs text-white px-1.5 py-0.5 outline-none w-40"
            />
            <button
              type="button"
              on:click={commitText}
              class="px-2 py-0.5 rounded bg-cyan-500 text-slate-950 text-[11px] font-bold cursor-pointer"
            >
              OK
            </button>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
