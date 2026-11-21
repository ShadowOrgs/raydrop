<script lang="ts">
  import { onMount, onDestroy, createEventDispatcher } from 'svelte';
  import { Sparkles, PenTool, Trash2, X } from 'lucide-svelte';

  export let active: boolean = false;

  const dispatch = createEventDispatcher<{
    close: void;
  }>();

  let canvasEl: HTMLCanvasElement | null = null;
  let mode: 'laser' | 'pen' = 'laser';
  let penColor = '#22d3ee'; // cyan-400
  let isDrawing = false;
  let animFrameId: number | null = null;

  interface StrokePoint {
    x: number;
    y: number;
    t: number;
  }

  interface Stroke {
    points: StrokePoint[];
    color: string;
    isLaser: boolean;
    width: number;
  }

  let strokes: Stroke[] = [];
  let currentStroke: Stroke | null = null;

  const COLORS = [
    { name: 'Cyan', value: '#22d3ee' },
    { name: 'Rose', value: '#fb7185' },
    { name: 'Amber', value: '#fbbf24' },
    { name: 'Emerald', value: '#34d399' },
  ];

  function resizeCanvas() {
    if (!canvasEl || !canvasEl.parentElement) return;
    const rect = canvasEl.parentElement.getBoundingClientRect();
    canvasEl.width = rect.width;
    canvasEl.height = rect.height;
  }

  function startDraw(e: MouseEvent) {
    if (!active || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    isDrawing = true;
    currentStroke = {
      points: [{ x, y, t: Date.now() }],
      color: mode === 'laser' ? '#f43f5e' : penColor,
      isLaser: mode === 'laser',
      width: mode === 'laser' ? 5 : 3.5,
    };
    strokes = [...strokes, currentStroke];
  }

  function moveDraw(e: MouseEvent) {
    if (!isDrawing || !currentStroke || !canvasEl) return;
    const rect = canvasEl.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    currentStroke.points.push({ x, y, t: Date.now() });
  }

  function endDraw() {
    isDrawing = false;
    currentStroke = null;
  }

  function clearAnnotations() {
    strokes = [];
    currentStroke = null;
  }

  function renderLoop() {
    if (canvasEl) {
      const ctx = canvasEl.getContext('2d');
      if (ctx) {
        ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
        const now = Date.now();
        const LASER_TTL = 1800; // Laser pointer fades out after 1.8s

        strokes = strokes.filter((s) => {
          if (!s.isLaser) return true;
          const lastPt = s.points[s.points.length - 1];
          return lastPt && now - lastPt.t < LASER_TTL;
        });

        for (const s of strokes) {
          if (s.points.length < 2) continue;
          ctx.save();
          ctx.lineCap = 'round';
          ctx.lineJoin = 'round';
          ctx.lineWidth = s.width;

          if (s.isLaser) {
            const lastPt = s.points[s.points.length - 1];
            const age = now - lastPt.t;
            const alpha = Math.max(0, 1 - age / LASER_TTL);
            ctx.globalAlpha = alpha;
            ctx.shadowColor = '#f43f5e';
            ctx.shadowBlur = 12;
          }

          ctx.strokeStyle = s.color;
          ctx.beginPath();
          ctx.moveTo(s.points[0].x, s.points[0].y);
          for (let i = 1; i < s.points.length; i++) {
            ctx.lineTo(s.points[i].x, s.points[i].y);
          }
          ctx.stroke();

          // Glowing tip for active laser pointer
          if (s.isLaser) {
            const tip = s.points[s.points.length - 1];
            ctx.fillStyle = '#ffffff';
            ctx.beginPath();
            ctx.arc(tip.x, tip.y, 4, 0, Math.PI * 2);
            ctx.fill();
          }

          ctx.restore();
        }
      }
    }
    animFrameId = requestAnimationFrame(renderLoop);
  }

  onMount(() => {
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);
    animFrameId = requestAnimationFrame(renderLoop);
  });

  onDestroy(() => {
    window.removeEventListener('resize', resizeCanvas);
    if (animFrameId !== null) cancelAnimationFrame(animFrameId);
  });
</script>

{#if active}
  <div class="absolute inset-0 z-30 pointer-events-auto">
    <canvas
      bind:this={canvasEl}
      on:mousedown={startDraw}
      on:mousemove={moveDraw}
      on:mouseup={endDraw}
      on:mouseleave={endDraw}
      class="w-full h-full {mode === 'laser' ? 'cursor-crosshair' : 'cursor-cell'}"
    ></canvas>

    <!-- Floating Annotation Toolbar -->
    <div
      class="absolute bottom-5 left-1/2 -translate-x-1/2 z-40 flex items-center gap-2 px-3 py-1.5 rounded-2xl bg-slate-900/95 border border-slate-700/80 shadow-2xl backdrop-blur-md text-xs"
    >
      <!-- Laser Pointer Mode -->
      <button
        type="button"
        on:click={() => (mode = 'laser')}
        class="px-2.5 py-1 rounded-xl font-semibold flex items-center gap-1.5 transition cursor-pointer {mode === 'laser'
          ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
          : 'text-slate-300 hover:bg-slate-800'}"
        title="Laser Pointer (Auto-fades after 1.8s)"
      >
        <Sparkles class="w-3.5 h-3.5 text-rose-400" />
        <span>Laser</span>
      </button>

      <!-- Pen Mode -->
      <button
        type="button"
        on:click={() => (mode = 'pen')}
        class="px-2.5 py-1 rounded-xl font-semibold flex items-center gap-1.5 transition cursor-pointer {mode === 'pen'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-300 hover:bg-slate-800'}"
        title="Digital Pen Annotation"
      >
        <PenTool class="w-3.5 h-3.5 text-cyan-400" />
        <span>Pen</span>
      </button>

      <!-- Color Swatches (in Pen mode) -->
      {#if mode === 'pen'}
        <div class="flex items-center gap-1.5 px-2 border-l border-r border-slate-700">
          {#each COLORS as c}
            <button
              type="button"
              on:click={() => (penColor = c.value)}
              class="w-4 h-4 rounded-full border-2 transition cursor-pointer {penColor === c.value
                ? 'border-white scale-110'
                : 'border-transparent opacity-70 hover:opacity-100'}"
              style="background-color: {c.value}"
              title={c.name}
            ></button>
          {/each}
        </div>
      {/if}

      <!-- Clear Canvas -->
      <button
        type="button"
        on:click={clearAnnotations}
        class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 transition cursor-pointer"
        title="Clear all annotations"
      >
        <Trash2 class="w-3.5 h-3.5" />
      </button>

      <!-- Close Annotation Overlay -->
      <button
        type="button"
        on:click={() => dispatch('close')}
        class="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-slate-800 transition cursor-pointer"
        title="Exit Annotation Mode"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>
{/if}
