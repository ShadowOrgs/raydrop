<script lang="ts">
  export let score: number = 0;
  export let label: string = "Score";
  export let size: number = 88;
  export let strokeWidth: number = 7;

  $: clamped = Math.max(0, Math.min(100, Math.round(score)));
  $: radius = (size - strokeWidth) / 2;
  $: circumference = 2 * Math.PI * radius;
  $: dashOffset = circumference - (clamped / 100) * circumference;
  $: color =
    clamped >= 90
      ? "#10b981"
      : clamped >= 70
        ? "#06b6d4"
        : clamped >= 50
          ? "#f59e0b"
          : "#f43f5e";
</script>

<div class="flex flex-col items-center gap-1.5 select-none">
  <div class="relative flex items-center justify-center" style="width: {size}px; height: {size}px;">
    <svg width={size} height={size} class="-rotate-90">
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="transparent"
        stroke="rgba(255,255,255,0.08)"
        stroke-width={strokeWidth}
      />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="transparent"
        stroke={color}
        stroke-width={strokeWidth}
        stroke-linecap="round"
        stroke-dasharray={circumference}
        stroke-dashoffset={dashOffset}
        class="transition-all duration-500 ease-out"
      />
    </svg>
    <span
      class="absolute font-extrabold font-mono text-white"
      style="font-size: {Math.round(size * 0.24)}px; color: {color};"
    >
      {clamped}
    </span>
  </div>
  <span class="text-[11px] font-semibold text-slate-300 text-center leading-tight">{label}</span>
</div>
