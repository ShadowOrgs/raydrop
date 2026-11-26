<script lang="ts">
  import {
    Zap,
    MessageSquare,
    Video,
    Monitor,
    Users,
    Activity,
    ClipboardList,
    StickyNote,
    Wand2,
    Globe,
    Sparkles,
  } from "lucide-svelte";
  import {
    activeMainTab,
    appSettings,
    cloudContacts,
    cloudMyDevices,
    type MainTab,
  } from "../stores/state";
  import { totalUnreadCount } from "../stores/chat";
  import { lanContacts } from "../stores/contacts";
  import { activeTransfers } from "../stores/transfers";
  import { myAvatar } from "../stores/avatars";

  interface RailItem {
    id: MainTab;
    label: string;
    icon: any;
  }

  const RAIL_ITEMS: RailItem[] = [
    { id: "dashboard", label: "Dashboard", icon: Activity },
    { id: "transfers", label: "File Transfers", icon: Zap },
    { id: "chat", label: "Messages", icon: MessageSquare },
    { id: "meeting", label: "Online Meeting", icon: Video },
    { id: "remote", label: "Remote Desktop", icon: Monitor },
    { id: "lan", label: "Network & Contacts", icon: Users },
    { id: "clipboard", label: "Copy History", icon: ClipboardList },
    { id: "notes", label: "Notes & Lists", icon: StickyNote },
    { id: "converter", label: "File Converter", icon: Wand2 },
    { id: "webdebugger", label: "Web Debugger", icon: Globe },
    { id: "skills", label: "AI Skills Library", icon: Sparkles },
  ];

  let hoveredTooltip: { label: string; top: number; left: number } | null = null;

  function showTooltip(e: MouseEvent, label: string) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hoveredTooltip = {
      label,
      top: rect.top + rect.height / 2,
      left: rect.right + 12,
    };
  }

  function hideTooltip() {
    hoveredTooltip = null;
  }

  function isItemActive(id: MainTab, current: MainTab): boolean {
    if (id === "lan") {
      return current === "lan" || current === "devices" || current === "contacts";
    }
    return current === id;
  }

  function getBadgeCount(
    id: MainTab,
    transfersCount: number,
    unreadCount: number,
    lanCount: number,
    devicesCount: number,
    contactsCount: number,
  ): number {
    if (id === "transfers") return transfersCount;
    if (id === "chat") return unreadCount;
    if (id === "lan") return lanCount + devicesCount + contactsCount;
    return 0;
  }
</script>

<!-- Spatial Floating Left Pill Dock (visionOS style) -->
<aside
  aria-label="Spatial Navigation Dock"
  class="hidden md:flex flex-col items-center justify-center z-30 shrink-0 select-none pl-2 sm:pl-3 max-h-full py-1"
>
  <div
    class="spatial-glass-pill rail-scroll flex flex-col items-center gap-1 p-1.5 rounded-full bg-white/[0.08] backdrop-blur-2xl border border-white/15 shadow-[0_16px_40px_-8px_rgba(0,0,0,0.55)] max-h-full overflow-y-auto overflow-x-hidden"
  >
    {#each RAIL_ITEMS as item}
      {@const isDisabled = item.id === "remote" && !$appSettings.discoverable}
      {@const isActive = isItemActive(item.id, $activeMainTab)}
      {@const badgeCount = getBadgeCount(
        item.id,
        $activeTransfers.length,
        $totalUnreadCount,
        $lanContacts.length,
        $cloudMyDevices.length,
        $cloudContacts.length,
      )}

      <button
        type="button"
        disabled={isDisabled}
        on:click={() => {
          if (!isDisabled) activeMainTab.set(item.id);
        }}
        on:mouseenter={(e) =>
          showTooltip(
            e,
            isDisabled
              ? "Remote Desktop disabled while Hidden (Offline)"
              : item.label,
          )}
        on:mouseleave={hideTooltip}
        title={isDisabled
          ? "Remote Desktop disabled while Hidden (Offline)"
          : item.label}
        class="group relative w-8 h-8 rounded-full flex items-center justify-center shrink-0 transition-all duration-200 {isDisabled
          ? 'opacity-30 cursor-not-allowed text-white/40'
          : isActive
            ? 'bg-white text-slate-950 shadow-lg scale-105 cursor-pointer'
            : 'text-white/75 hover:text-white hover:bg-white/15 cursor-pointer'}"
      >
        <svelte:component this={item.icon} class="w-4 h-4" />

        {#if badgeCount > 0}
          <span
            class="absolute -top-0.5 -right-0.5 min-w-[14px] h-[14px] px-1 rounded-full bg-sky-400 text-slate-950 text-[9px] font-extrabold font-mono flex items-center justify-center shadow"
          >
            {badgeCount}
          </span>
        {/if}
      </button>
    {/each}

    <div class="w-5 h-px bg-white/15 my-0.5 shrink-0"></div>

    <!-- Bottom Avatar Orb -->
    <button
      type="button"
      on:click={() => activeMainTab.set("dashboard")}
      on:mouseenter={(e) =>
        showTooltip(e, $appSettings.alias_name || "Raydrop Node")}
      on:mouseleave={hideTooltip}
      class="w-8 h-8 rounded-full overflow-hidden border border-white/25 bg-white/10 flex items-center justify-center shrink-0 hover:scale-105 transition cursor-pointer"
      title={$appSettings.alias_name || "Raydrop Node"}
    >
      {#if $myAvatar}
        <img src={$myAvatar} alt="Avatar" class="w-full h-full object-cover" />
      {:else}
        <span class="text-[10px] font-extrabold text-white">
          {($appSettings.alias_name || "RD").slice(0, 2).toUpperCase()}
        </span>
      {/if}
    </button>
  </div>
</aside>

{#if hoveredTooltip}
  <div
    class="pointer-events-none fixed -translate-y-1/2 px-3 py-1 rounded-full bg-slate-950/95 backdrop-blur-xl border border-white/15 text-[11px] font-semibold text-white whitespace-nowrap shadow-xl z-50"
    style="top: {hoveredTooltip.top}px; left: {hoveredTooltip.left}px;"
  >
    {hoveredTooltip.label}
  </div>
{/if}

<style>
  .rail-scroll {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  .rail-scroll::-webkit-scrollbar {
    display: none;
  }
</style>
