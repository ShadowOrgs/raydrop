<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    appSettings,
    networkInfo,
    settingsModalOpen,
    authModalOpen,
    cloudMyDevices,
    cloudContacts,
    activeMainTab,
    cliQueuedFiles,
    myRankExp,
  } from "../stores/state";
  import { getRankInfo } from "../utils/rank";
  import { contacts, lanContacts } from "../stores/contacts";
  import { activeTransfers, transfers } from "../stores/transfers";
  import { totalUnreadCount, chatStore } from "../stores/chat";
  import { raydropApi } from "../utils/tauri-ipc";
  import { saveAccountDeviceName } from "../utils/accountDeviceNames";
  import {
    updateInfo,
    updateModalOpen,
    updateChecking,
  } from "../stores/update";
  import { checkForUpdates } from "../utils/updater";
  import { myAvatar } from "../stores/avatars";
  import { presenceSoundEnabled, togglePresenceSound } from "../stores/presence";
  import { RAYDROP_THEMES, currentTheme, applyTheme } from "../stores/theme";
  import {
    Copy,
    Check,
    Globe,
    Wifi,
    Settings,
    LogIn,
    LogOut,
    Mail,
    Sparkles,
    RefreshCw,
    Activity,
    ClipboardList,
    StickyNote,
    Wand2,
    Laptop,
    Users,
    Zap,
    Plus,
    LayoutGrid,
    ArrowLeft,
    MessageSquare,
    Camera,
    Video,
    Monitor,
    Palette,
    Volume2,
    VolumeX,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";

  let appVersion = "1.2.61";
  let copied = false;
  let themeMenuOpen = false;
  let profileMenuOpen = false;

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch (_) {}
  });

  async function toggleDiscoverable() {
    const next = !$appSettings.discoverable;
    const res = await raydropApi.toggleDiscoverability(next);
    appSettings.update((s) => ({ ...s, discoverable: res }));
  }

  async function enableAutostartQuick() {
    try {
      const res = await raydropApi.toggleAutostart(true);
      appSettings.update((s) => ({ ...s, autostart: res }));
    } catch (err) {
      console.warn("Failed to enable autostart:", err);
    }
  }

  async function copyIdentity() {
    const text = $appSettings.user_email || $appSettings.device_id;
    await navigator.clipboard.writeText(text);
    copied = true;
    setTimeout(() => (copied = false), 1800);
  }

  async function handleLogout() {
    saveAccountDeviceName($appSettings.user_email, $appSettings.alias_name);
    const updated = await raydropApi.logoutCloudSession();
    appSettings.set(updated);
    cloudMyDevices.set([]);
    cloudContacts.set([]);
    transfers.set([]);
    chatStore.set({});
    authModalOpen.set(true);
  }

  async function handleUploadAvatar() {
    try {
      const picked = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Images",
            extensions: ["png", "jpg", "jpeg", "webp", "bmp", "gif"],
          },
        ],
        title: "Choose Profile Avatar",
      });
      if (typeof picked === "string") {
        const b64 = await raydropApi.setMyAvatar(picked);
        myAvatar.set(b64);
      }
    } catch (err) {
      console.warn("Failed to upload avatar:", err);
    }
  }
</script>

<svelte:window
  on:click={() => {
    themeMenuOpen = false;
    profileMenuOpen = false;
  }}
/>

<header
  class="border-b border-slate-800/80 bg-slate-900/80 backdrop-blur-xl px-3 sm:px-5 pt-8 pb-2.5 sm:py-3 flex items-center justify-between gap-2 sm:gap-4 z-20 shrink-0"
>
  <!-- Brand & Identity (Left) -->
  <div class="flex items-center gap-2 sm:gap-3 shrink-0 min-w-0">
    {#if $activeMainTab !== "home"}
      <button
        type="button"
        on:click={() => activeMainTab.set("home")}
        class="md:hidden inline-flex items-center gap-1 px-2 py-1.5 rounded-xl bg-slate-800/90 hover:bg-slate-700 text-cyan-300 border border-slate-700/80 text-xs font-semibold shrink-0 cursor-pointer"
        title="Back to All Menus"
      >
        <ArrowLeft class="w-3.5 h-3.5" />
        <LayoutGrid class="w-3.5 h-3.5" />
      </button>
    {/if}
    <img
      src="/icon.png"
      alt="Raydrop Icon"
      class="w-7 h-7 sm:w-9 sm:h-9 rounded-xl object-contain shrink-0"
    />
    <div>
      <div class="flex items-center gap-1.5 sm:gap-2">
        <h1
          class="text-sm sm:text-base font-extrabold tracking-tight text-white font-display whitespace-nowrap"
        >
          Raydrop
        </h1>

        <!-- Online/Signaling Status Dot for Mobile -->
        <span
          class="md:hidden w-2 h-2 rounded-full shrink-0 {$networkInfo.signalingConnected
            ? 'bg-emerald-400'
            : 'bg-amber-400'}"
          title={$networkInfo.signalingConnected
            ? "Online (Connected to Signaling)"
            : "Standby"}
        ></span>

        <button
          type="button"
          on:click={() => checkForUpdates(true)}
          class="hidden sm:inline-flex text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white border border-slate-700 transition-colors items-center gap-1 cursor-pointer shrink-0"
          title="Click to check for software updates"
        >
          {#if $updateChecking}
            <RefreshCw class="w-2.5 h-2.5 animate-spin text-cyan-400" />
          {/if}
          <span>v{appVersion}</span>
        </button>

        {#if $updateInfo?.has_update}
          <button
            type="button"
            on:click={() => updateModalOpen.set(true)}
            class="hidden sm:inline-flex items-center gap-1 text-[10px] font-mono font-bold px-2 py-0.5 rounded-full bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/40 shadow-sm shadow-cyan-500/20 animate-pulse transition-all cursor-pointer shrink-0"
            title="Click to view update details and install"
          >
            <Sparkles class="w-3 h-3 text-cyan-400" />
            <span>Update {$updateInfo.latest_version}</span>
          </button>
        {/if}
      </div>

      <!-- Identity line (Desktop only to prevent mobile header overflow) -->
      <div class="hidden sm:flex items-center gap-2 mt-0.5">
        {#if $appSettings.user_email}
          <Mail class="w-3 h-3 text-cyan-400 shrink-0" />
          <span
            class="text-xs font-mono text-cyan-300 truncate max-w-[130px] lg:max-w-[170px]"
            title={$appSettings.user_email}
          >
            {$appSettings.user_email}
          </span>
        {:else}
          <span
            class="text-xs font-mono text-slate-400 truncate max-w-[130px] lg:max-w-[170px]"
            >{$appSettings.device_id}</span
          >
        {/if}
        <button
          on:click={copyIdentity}
          class="text-slate-400 hover:text-cyan-400 transition-colors p-0.5 rounded"
          title="Copy Email / Device ID"
        >
          {#if copied}
            <Check class="w-3 h-3 text-emerald-400" />
          {:else}
            <Copy class="w-3 h-3" />
          {/if}
        </button>
      </div>
    </div>
  </div>

  <!-- Navigation moved to FloatingRail on desktop for a clean editorial workspace -->

  <!-- Status Badges & Quick Action Controls (Right - Compact & Profile Always Visible) -->
  <div class="flex items-center gap-1.5 sm:gap-2 shrink-0">
    <!-- Auto-Start OFF Warning Pill (clickable to turn ON) -->
    {#if !$appSettings.autostart}
      <button
        type="button"
        on:click={enableAutostartQuick}
        class="hidden lg:flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl bg-slate-800/90 hover:bg-slate-700 text-slate-300 text-[11px] font-semibold transition cursor-pointer"
        title="Auto-Start at Boot is OFF — click to enable so Raydrop starts automatically after restart"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-rose-400"></span>
        <span>Autostart OFF</span>
      </button>
    {/if}

    <!-- WAN STUN Status Pill -->
    <div
      class="flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-xl text-[11px] font-semibold {$networkInfo.signalingConnected
        ? 'bg-emerald-500/15 text-emerald-300'
        : 'bg-slate-800/90 text-slate-400'}"
      title={$networkInfo.signalingConnected
        ? `WAN Online (NAT: ${$networkInfo.wanReflexiveAddr || "Connected"})`
        : "WAN Signaling Offline — LAN Only"}
    >
      <Globe
        class="w-3.5 h-3.5 {$networkInfo.signalingConnected
          ? 'text-emerald-400'
          : 'text-slate-400'}"
      />
      <span class="hidden lg:inline">
        {$networkInfo.signalingConnected ? "WAN Online" : "LAN Only"}
      </span>
    </div>

    <!-- Discoverability Toggle Pill -->
    <button
      type="button"
      on:click={toggleDiscoverable}
      class="flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-xl text-[11px] font-semibold transition-all cursor-pointer {$appSettings.discoverable
        ? 'bg-cyan-500/15 text-cyan-300 hover:bg-cyan-500/25'
        : 'bg-slate-800/90 text-slate-400 hover:bg-slate-700'}"
      title={$appSettings.discoverable
        ? "Visible & Online to other users (Click to go Hidden & Offline)"
        : "Hidden & Offline Mode — other users see you as offline (Click to go Visible)"}
    >
      <span
        class="w-2 h-2 rounded-full {$appSettings.discoverable
          ? 'bg-cyan-400 animate-pulse'
          : 'bg-slate-500'}"
      ></span>
      <span class="hidden lg:inline">
        {$appSettings.discoverable ? "Visible" : "Hidden • Offline"}
      </span>
    </button>

    <!-- Quick Theme Switcher Dropdown -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div class="relative" on:click|stopPropagation>
      <button
        type="button"
        on:click={() => {
          themeMenuOpen = !themeMenuOpen;
          profileMenuOpen = false;
        }}
        class="p-2 rounded-xl bg-slate-800/70 hover:bg-slate-700 text-slate-300 hover:text-white transition-colors cursor-pointer flex items-center gap-1.5"
        title="Switch Theme ({RAYDROP_THEMES.find((t) => t.id === $currentTheme)
          ?.name || 'Raydrop Obsidian'})"
      >
        <Palette class="w-4 h-4 text-cyan-400" />
      </button>

      {#if themeMenuOpen}
        <div
          class="absolute right-0 mt-2 w-64 rounded-2xl bg-slate-900/95 backdrop-blur-xl border border-slate-800 shadow-2xl p-2 z-50 space-y-1"
        >
          <div
            class="px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider text-slate-500"
          >
            Appearance
          </div>
          {#each RAYDROP_THEMES as themeOpt}
            <button
              type="button"
              on:click={() => {
                applyTheme(themeOpt.id);
                themeMenuOpen = false;
              }}
              class="w-full flex items-center justify-between gap-2.5 px-2.5 py-2 rounded-xl text-left transition cursor-pointer {$currentTheme ===
              themeOpt.id
                ? 'bg-cyan-500/15 text-white ring-1 ring-cyan-500/35'
                : 'text-slate-300 hover:bg-slate-800/80'}"
            >
              <div class="flex items-center gap-2.5 min-w-0">
                <span
                  class="w-6 h-6 rounded-lg shrink-0 flex items-center justify-center border border-white/15 shadow-inner"
                  style="background: {themeOpt.previewBg}"
                >
                  <span
                    class="w-2.5 h-2.5 rounded-full shadow-sm"
                    style="background: {themeOpt.previewAccent}"
                  ></span>
                </span>
                <div class="min-w-0">
                  <div class="text-xs font-semibold truncate">
                    {themeOpt.name}
                  </div>
                  <div class="text-[10px] text-slate-400 truncate">
                    {themeOpt.description}
                  </div>
                </div>
              </div>
              {#if $currentTheme === themeOpt.id}
                <Check class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Profile Avatar & Context Menu (Change Avatar, Settings, Sign In/Out) -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div class="relative shrink-0" on:click|stopPropagation>
      <button
        type="button"
        on:click={() => {
          profileMenuOpen = !profileMenuOpen;
          themeMenuOpen = false;
        }}
        class="relative group w-8 h-8 rounded-xl overflow-hidden bg-slate-800 flex items-center justify-center shrink-0 transition cursor-pointer shadow-sm"
        title="Profile Menu ({$appSettings.alias_name || 'Raydrop'})"
      >
        {#if $myAvatar}
          <img
            src={$myAvatar}
            alt="My Avatar"
            class="w-full h-full object-cover"
          />
        {:else}
          <span class="text-[11px] font-bold text-cyan-300">
            {($appSettings.alias_name || "RD").slice(0, 2).toUpperCase()}
          </span>
        {/if}
      </button>

      {#if profileMenuOpen}
        {@const myRank = getRankInfo($myRankExp)}
        <div
          class="absolute right-0 mt-2 w-56 rounded-2xl bg-slate-900 border border-slate-800 shadow-2xl p-2 z-50 space-y-1"
        >
          <div class="px-2.5 py-2 border-b border-slate-800 mb-1 space-y-2">
            <div class="flex items-center justify-between gap-2">
              <div class="min-w-0">
                <div class="text-xs font-bold text-white truncate">
                  {$appSettings.alias_name || "Raydrop User"}
                </div>
                <div class="text-[11px] text-slate-400 truncate">
                  {$appSettings.user_email || $appSettings.device_id}
                </div>
              </div>
              <span
                class="text-[10px] font-bold px-2 py-0.5 rounded-full shrink-0 whitespace-nowrap"
                style={myRank.badgeStyle}
              >
                {myRank.icon}
                {myRank.tierName} Lv.{myRank.level}
              </span>
            </div>
            <div class="space-y-1">
              <div
                class="flex items-center justify-between text-[10px] font-mono text-slate-400"
              >
                <span>{myRank.exp.toLocaleString()} EXP</span>
                <span>Next: {myRank.nextLevelExp.toLocaleString()}</span>
              </div>
              <div
                class="w-full h-1.5 rounded-full bg-slate-800 overflow-hidden"
              >
                <div
                  class="h-full rounded-full transition-all"
                  style="width: {myRank.progressPercent}%; background: {myRank.barColor}"
                ></div>
              </div>
            </div>
          </div>

          <button
            type="button"
            on:click={() => {
              profileMenuOpen = false;
              handleUploadAvatar();
            }}
            class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left text-xs text-slate-300 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            <Camera class="w-4 h-4 text-cyan-400 shrink-0" />
            <span>Change Avatar</span>
          </button>

          <button
            type="button"
            on:click={() => togglePresenceSound()}
            class="w-full flex items-center justify-between gap-2.5 px-2.5 py-2 rounded-xl text-left text-xs text-slate-300 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            <div class="flex items-center gap-2.5">
              {#if $presenceSoundEnabled}
                <Volume2 class="w-4 h-4 text-emerald-400 shrink-0" />
                <span>Notification Sounds</span>
              {:else}
                <VolumeX class="w-4 h-4 text-slate-500 shrink-0" />
                <span>Notification Sounds</span>
              {/if}
            </div>
            <span class="text-[10px] font-bold px-1.5 py-0.5 rounded {$presenceSoundEnabled ? 'bg-emerald-500/15 text-emerald-300' : 'bg-slate-800 text-slate-500'}">
              {$presenceSoundEnabled ? "ON" : "MUTED"}
            </span>
          </button>

          <button
            type="button"
            on:click={() => {
              profileMenuOpen = false;
              settingsModalOpen.set(true);
            }}
            class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left text-xs text-slate-300 hover:text-white hover:bg-slate-800 transition cursor-pointer"
          >
            <Settings class="w-4 h-4 text-cyan-400 shrink-0" />
            <span>Settings</span>
          </button>

          <div class="border-t border-slate-800 my-1"></div>

          {#if $appSettings.auth_token}
            <button
              type="button"
              on:click={() => {
                profileMenuOpen = false;
                handleLogout();
              }}
              class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left text-xs text-rose-400 hover:text-rose-300 hover:bg-rose-500/15 transition cursor-pointer"
            >
              <LogOut class="w-4 h-4 shrink-0" />
              <span>Sign Out</span>
            </button>
          {:else}
            <button
              type="button"
              on:click={() => {
                profileMenuOpen = false;
                authModalOpen.set(true);
              }}
              class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left text-xs text-cyan-300 hover:text-white hover:bg-cyan-500/20 transition cursor-pointer font-semibold"
            >
              <LogIn class="w-4 h-4 shrink-0" />
              <span>Sign In to Sync Devices</span>
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</header>
