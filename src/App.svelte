<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { get } from "svelte/store";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import Header from "./lib/components/Header.svelte";
  import FloatingRail from "./lib/components/FloatingRail.svelte";
  import ContactList from "./lib/components/ContactList.svelte";
  import ActiveTransfers from "./lib/components/ActiveTransfers.svelte";
  import ChatView from "./lib/components/ChatView.svelte";
  import TransferModal from "./lib/components/TransferModal.svelte";
  import TraySettings from "./lib/components/TraySettings.svelte";
  import AuthModal from "./lib/components/AuthModal.svelte";
  import UpdateModal from "./lib/components/UpdateModal.svelte";
  import MagnetDownloadModal from "./lib/components/MagnetDownloadModal.svelte";
  import BrowserExtensionModal from "./lib/components/BrowserExtensionModal.svelte";
  import MeetingView from "./lib/components/MeetingView.svelte";
  import MeetingInviteModal from "./lib/components/MeetingInviteModal.svelte";
  import WinampPlayer from "./lib/components/WinampPlayer.svelte";
  import RemoteDesktopView from "./lib/components/RemoteDesktopView.svelte";
  import DashboardView from "./lib/components/DashboardView.svelte";
  import ClipboardHistoryView from "./lib/components/ClipboardHistoryView.svelte";
  import NotesView from "./lib/components/NotesView.svelte";
  import ConverterView from "./lib/components/ConverterView.svelte";
  import WebDebuggerView from "./lib/components/WebDebuggerView.svelte";
  import SkillsLibraryView from "./lib/components/SkillsLibraryView.svelte";
  import { initWebDebuggerListeners, extensionUpdateInfo } from "./lib/stores/webDebugger";
  import { browserExtensionModalOpen } from "./lib/stores/torrents";
  import WolModal from "./lib/components/WolModal.svelte";
  import { setupMeetingSignalListener, isInMeeting } from "./lib/stores/meeting";
  import {
    initRemoteDesktopListener,
    loadRemoteDesktopSettings,
    activeRemoteSession,
    remoteConnectionStatus,
  } from "./lib/stores/remote_desktop";
  import { initTorrentStore } from "./lib/stores/torrents";
  import { refreshLanStatuses } from "./lib/stores/contacts";
  import {
    initPresenceNotifications,
    notifyIncomingTransferRequest,
    notifyTransferState,
    playAppSound,
    presenceToasts,
    removePresenceToast,
  } from "./lib/stores/presence";
  import { initAvatarStore, peerAvatars } from "./lib/stores/avatars";
  import { applyTheme, type RaydropTheme } from "./lib/stores/theme";
  import {
    Upload,
    X,
    Zap,
    MessageSquare,
    Wifi,
    Users,
    Video,
    Monitor,
    Laptop,
    Activity,
    ClipboardList,
    StickyNote,
    Wand2,
    LayoutGrid,
  } from "lucide-svelte";
  import {
    appSettings,
    deviceCryptoMeta,
    networkInfo,
    cloudMyDevices,
    cloudContacts,
    incomingRequestQueue,
    cliQueuedFiles,
    cliConvertFiles,
    authModalOpen,
    activeMainTab,
    activeChatPeerId,
    isDraggingFiles,
    myRankExp,
  } from "./lib/stores/state";
  import {
    fetchUnreadCounts,
    handleIncomingChatMessage,
    markChatRead,
    chatToasts,
    addChatToast,
    removeChatToast,
    totalUnreadCount,
    chatGroups,
    fetchChatGroups,
  } from "./lib/stores/chat";
  import { contacts, lanContacts } from "./lib/stores/contacts";
  import {
    transfers,
    activeTransfers,
    applyTransferProgress,
    upsertTransferRecord,
  } from "./lib/stores/transfers";
  import { updateProgress } from "./lib/stores/update";
  import { checkForUpdates } from "./lib/utils/updater";
  import {
    raydropApi,
    cloudApi,
    getApiBaseUrl,
    type IncomingTransferEvent,
    type TransferProgressEvent,
    type TransferRecord,
    type UpdateProgressPayload,
    type ChatMessage,
    type ClipboardSyncedPayload,
  } from "./lib/utils/tauri-ipc";

  async function refreshCloudState(sigUrl: string, token: string | null) {
    if (!token) return;
    try {
      const baseUrl = getApiBaseUrl(sigUrl);
      const cloudState = await cloudApi.getState(baseUrl, token);
      cloudMyDevices.set(cloudState.my_devices);
      cloudContacts.set(cloudState.contacts);
      const synced = await raydropApi.syncCloudDataToLocal(
        cloudState.my_devices,
        cloudState.contacts,
      );
      contacts.set(synced);
    } catch (e) {
      console.warn("Cloud state sync failed:", e);
    }
  }

  let statusSyncTimer: any = null;
  let isAppReady = false;
  let initStatusText = "Menyiapkan komponen & layanan Raydrop...";
  let pendingMeshRequest: { device_id: string; alias_name: string } | null = null;

  const BOTTOM_TABS_STORAGE_KEY = "raydrop_bottom_bar_tabs_v1";
  const ALL_BOTTOM_MENUS = [
    { id: "dashboard", label: "Dashboard" },
    { id: "transfers", label: "Transfers" },
    { id: "chat", label: "Messages" },
    { id: "meeting", label: "Meeting" },
    { id: "remote", label: "Remote" },
    { id: "lan", label: "LAN Radar" },
    { id: "devices", label: "My Devices" },
    { id: "contacts", label: "Contacts" },
    { id: "clipboard", label: "Copy History" },
    { id: "notes", label: "Notes" },
    { id: "converter", label: "Converter" },
    { id: "skills", label: "AI Skills" },
  ] as const;
  type BottomMenuId = (typeof ALL_BOTTOM_MENUS)[number]["id"];
  const DEFAULT_BOTTOM_TAB_IDS: BottomMenuId[] = [
    "dashboard",
    "transfers",
    "chat",
    "meeting",
    "remote",
  ];

  function loadBottomBarTabIds(): BottomMenuId[] {
    try {
      const raw = localStorage.getItem(BOTTOM_TABS_STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed)) {
          const valid = parsed.filter((id): id is BottomMenuId =>
            ALL_BOTTOM_MENUS.some((m) => m.id === id),
          );
          if (valid.length > 0) return valid;
        }
      }
    } catch (_) {}
    return [...DEFAULT_BOTTOM_TAB_IDS];
  }

  let bottomBarTabIds: BottomMenuId[] = loadBottomBarTabIds();
  let isBottomMenuPickerOpen = false;

  function saveBottomBarTabIds(ids: BottomMenuId[]) {
    bottomBarTabIds = ids;
    try {
      localStorage.setItem(BOTTOM_TABS_STORAGE_KEY, JSON.stringify(ids));
    } catch (_) {}
  }

  function toggleBottomBarTab(id: BottomMenuId) {
    if (bottomBarTabIds.includes(id)) {
      if (bottomBarTabIds.length <= 1) return; // Keep at least 1 tab on the bar
      saveBottomBarTabIds(bottomBarTabIds.filter((t) => t !== id));
    } else {
      const next = ALL_BOTTOM_MENUS.map((m) => m.id).filter(
        (mId) => bottomBarTabIds.includes(mId) || mId === id,
      );
      saveBottomBarTabIds(next);
    }
  }

  function resetBottomBarTabs() {
    saveBottomBarTabIds([...DEFAULT_BOTTOM_TAB_IDS]);
  }

  onDestroy(() => {
    if (statusSyncTimer) {
      clearInterval(statusSyncTimer);
    }
  });

  onMount(async () => {
    if (typeof window !== "undefined" && window.innerWidth < 768) {
      activeMainTab.set("home");
    }
    const readyFailSafe = setTimeout(() => {
      isAppReady = true;
    }, 8000);

    try {
      initStatusText = "Menghubungkan listener & sistem P2P...";
      // 0. Initialize WebRTC P2P Voice & Online Meeting Signaling
      setupMeetingSignalListener();
      initWebDebuggerListeners();

    // 1. Immediately subscribe to all real-time backend events FIRST to prevent startup race conditions
    await raydropApi.listenEvent<boolean>(
      "signaling_status_changed",
      async (connected) => {
        networkInfo.update((n) => ({ ...n, signalingConnected: connected }));
        if (connected && $appSettings.auth_token) {
          await refreshCloudState(
            $appSettings.signaling_url,
            $appSettings.auth_token,
          );
        }
      },
    );

    await raydropApi.listenEvent("peer_discovered", async () => {
      contacts.set(await raydropApi.getContacts());
      refreshLanStatuses();
    });

    await raydropApi.listenEvent<{
      device_id: string;
      alias_name: string;
    }>("mesh_request_received", (ev) => {
      playAppSound("mesh_request");
      pendingMeshRequest = ev;
    });

    let contactsUpdateTimer: any = null;
    await raydropApi.listenEvent("contacts_updated", () => {
      if (contactsUpdateTimer) clearTimeout(contactsUpdateTimer);
      contactsUpdateTimer = setTimeout(async () => {
        try {
          contacts.set(await raydropApi.getContacts());
          refreshLanStatuses();
        } catch (_) {}
      }, 300);
    });

    refreshLanStatuses();
    const lanStatusPollTimer = setInterval(() => {
      refreshLanStatuses();
    }, 5000);
    initPresenceNotifications();

    await raydropApi.listenEvent("cloud_state_changed", async () => {
      await refreshCloudState(
        $appSettings.signaling_url,
        $appSettings.auth_token,
      );
    });

    await raydropApi.listenEvent<boolean>(
      "discoverability_changed",
      (enabled) => {
        appSettings.update((s) => ({ ...s, discoverable: enabled }));
      },
    );

    await raydropApi.listenEvent<boolean>("autostart_changed", (enabled) => {
      appSettings.update((s) => ({ ...s, autostart: enabled }));
    });

    initStatusText = "Memuat status perangkat & tema...";
    // 2. Fetch fresh app state & populate stores
    const snap = await raydropApi.getAppState();
    appSettings.set(snap.settings);
    if (typeof snap.my_rank_exp === "number") {
      myRankExp.set(snap.my_rank_exp);
    }
    if (
      snap.settings.theme &&
      ["azure", "slate", "oled", "cyberpunk", "light"].includes(snap.settings.theme)
    ) {
      applyTheme(snap.settings.theme as RaydropTheme, false);
    }
    deviceCryptoMeta.set({
      ed25519Pubkey: snap.ed25519_pubkey,
      osType: snap.os_type,
    });
    networkInfo.set({
      localIp: snap.local_ip,
      listenPort: snap.listen_port,
      wanReflexiveAddr: snap.wan_reflexive_addr,
      signalingConnected: snap.signaling_connected,
    });

    if (snap.queued_cli_files.length > 0) {
      cliQueuedFiles.set(snap.queued_cli_files);
      try {
        const win = getCurrentWindow();
        await win.setAlwaysOnTop(true);
        await win.show();
        await win.unminimize();
        await win.setFocus();
        await win.requestUserAttention(UserAttentionType.Critical);
        setTimeout(() => {
          win.setAlwaysOnTop(false).catch(() => {});
        }, 1800);
      } catch (err) {
        console.warn("Window focus error on init cli:", err);
      }
    }

    if (snap.queued_convert_files && snap.queued_convert_files.length > 0) {
      cliConvertFiles.set(snap.queued_convert_files);
      activeMainTab.set("converter");
      try {
        const win = getCurrentWindow();
        await win.show();
        await win.unminimize();
        await win.setFocus();
      } catch (_) {}
    }

    initStatusText = "Memuat kontak & riwayat transfer...";
    contacts.set(await raydropApi.getContacts());
    transfers.set(await raydropApi.getTransferHistory());
    await fetchUnreadCounts();

    if (!snap.settings.auth_token) {
      authModalOpen.set(true);
    } else {
      initStatusText = "Sinkronisasi perangkat & cloud...";
      await Promise.race([
        refreshCloudState(snap.settings.signaling_url, snap.settings.auth_token),
        new Promise((r) => setTimeout(r, 2500)),
      ]);
    }

    // 3. Periodic synchronization timer (every 4s) to ensure signaling & network status never get stuck
    statusSyncTimer = setInterval(async () => {
      try {
        const activeFeatures: string[] = [];
        if (get(remoteConnectionStatus) === "connected") {
          activeFeatures.push("remote_desktop");
        }
        if (get(isInMeeting)) {
          activeFeatures.push("meeting");
        }
        const updatedExp = await raydropApi.recordActivityTick(activeFeatures, 4);
        if (typeof updatedExp === "number") {
          myRankExp.set(updatedExp);
        }

        const fresh = await raydropApi.getAppState();
        if (typeof fresh.my_rank_exp === "number") {
          myRankExp.set(fresh.my_rank_exp);
        }
        networkInfo.update((n) => ({
          ...n,
          signalingConnected: fresh.signaling_connected,
          wanReflexiveAddr: fresh.wan_reflexive_addr || n.wanReflexiveAddr,
        }));
      } catch (_) {}
    }, 4000);

    await raydropApi.listenEvent<IncomingTransferEvent>(
      "incoming_transfer_request",
      async (evt) => {
        incomingRequestQueue.update((q) => [...q, evt]);
        notifyIncomingTransferRequest(evt);
        try {
          const win = getCurrentWindow();
          await win.setAlwaysOnTop(true);
          await win.show();
          await win.unminimize();
          await win.setFocus();
          await win.requestUserAttention(UserAttentionType.Critical);
          setTimeout(() => {
            win.setAlwaysOnTop(false).catch(() => {});
          }, 1800);
        } catch (err) {
          console.warn("Window auto-focus fallback error:", err);
        }
      },
    );

    await raydropApi.listenEvent<TransferProgressEvent>(
      "transfer_progress",
      (evt) => {
        applyTransferProgress(evt);
      },
    );

    await raydropApi.listenEvent<TransferRecord>(
      "transfer_state_changed",
      (rec) => {
        upsertTransferRecord(rec);
        notifyTransferState(rec);
      },
    );

    await raydropApi.listenEvent<string[]>("cli_send_files", async (paths) => {
      cliQueuedFiles.update((prev) => [...prev, ...paths]);
      try {
        const win = getCurrentWindow();
        await win.setAlwaysOnTop(true);
        await win.show();
        await win.unminimize();
        await win.setFocus();
        await win.requestUserAttention(UserAttentionType.Critical);
        setTimeout(() => {
          win.setAlwaysOnTop(false).catch(() => {});
        }, 1800);
      } catch (err) {
        console.warn("Window focus error on cli send:", err);
      }
    });

    await raydropApi.listenEvent<string[]>("cli_convert_files", async (paths) => {
      cliConvertFiles.update((prev) => [...prev, ...paths]);
      activeMainTab.set("converter");
      try {
        const win = getCurrentWindow();
        await win.show();
        await win.unminimize();
        await win.setFocus();
      } catch (_) {}
    });

    await raydropApi.listenEvent<UpdateProgressPayload>(
      "app_update_progress",
      (payload) => {
        updateProgress.set(payload);
      },
    );

    await raydropApi.listenEvent<ChatMessage>(
      "chat_message_received",
      async (msg) => {
        handleIncomingChatMessage(msg);
        if (
          $activeMainTab === "chat" &&
          $activeChatPeerId === msg.peer_device_id
        ) {
          await markChatRead(msg.peer_device_id);
        } else {
          const contactList = get(contacts);
          const peer = contactList.find(
            (c) => c.device_id === msg.peer_device_id,
          );
          const peerName =
            peer?.custom_name ||
            peer?.alias_name ||
            peer?.user_name ||
            msg.peer_device_id.slice(0, 14);
          addChatToast(msg, peerName);

          try {
            const win = getCurrentWindow();
            await win.requestUserAttention(UserAttentionType.Informational);
          } catch (_) {}
        }
      },
    );

    // Listen for group chat updates & removals
    await raydropApi.listenEvent<any>("chat_group_updated", (group) => {
      chatGroups.update((groups) => {
        const idx = groups.findIndex((g) => g.id === group.id);
        if (idx >= 0) {
          groups[idx] = group;
          return [...groups];
        } else {
          return [group, ...groups];
        }
      });
    });

    await raydropApi.listenEvent<string>("chat_group_removed", (groupId) => {
      chatGroups.update((groups) => groups.filter((g) => g.id !== groupId));
      if (get(activeChatPeerId) === groupId) {
        activeChatPeerId.set(null);
      }
    });

    await fetchChatGroups();

    await raydropApi.listenEvent<ClipboardSyncedPayload>(
      "clipboard_synced",
      async (payload) => {
        const isImg =
          payload.content.startsWith("data:image/") ||
          (payload as any).is_image;
        const preview = isImg
          ? "🖼️ [Gambar Clipboard - Siap di-Paste (Ctrl+V)]"
          : payload.content.length > 50
            ? payload.content.slice(0, 50) + "..."
            : payload.content;

        // Ensure system clipboard also receives text if supported on Web / WebView
        if (!isImg && navigator.clipboard && navigator.clipboard.writeText) {
          try {
            await navigator.clipboard.writeText(payload.content);
          } catch (_) {}
        }

        addChatToast(
          {
            id: "clip-" + Date.now(),
            peer_device_id: payload.from_device_id,
            direction: "incoming",
            content: payload.auto_copied
              ? `📋 [Auto-Copied] ${preview}`
              : `📋 [Shared] ${preview}`,
            msg_type: "clipboard",
            is_read: true,
            created_at: Math.floor(Date.now() / 1000),
          },
          payload.sender_alias,
        );
      },
    );

    await raydropApi.listenEvent<string>("open_file_requested", (filePath) => {
      addChatToast(
        {
          id: "open-file-" + Date.now(),
          peer_device_id: "local",
          direction: "incoming",
          content: `📁 File saved at: ${filePath}. Open your Files or Gallery app to access it.`,
          msg_type: "text",
          is_read: true,
          created_at: Math.floor(Date.now() / 1000),
        },
        "Raydrop Storage",
      );
    });

    await raydropApi.listenEvent<string>(
      "open_folder_requested",
      (folderPath) => {
        addChatToast(
          {
            id: "open-folder-" + Date.now(),
            peer_device_id: "local",
            direction: "incoming",
            content: `📁 Files saved in Downloads folder (${folderPath}). Open your Files app to access them.`,
            msg_type: "text",
            is_read: true,
            created_at: Math.floor(Date.now() / 1000),
          },
          "Raydrop Storage",
        );
      },
    );

    // Prevent WebView/Chromium from showing the prohibited/block drag cursor (⊘)
    const preventBrowserDragDefault = (e: DragEvent) => {
      e.preventDefault();
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = "copy";
      }
    };
    window.addEventListener("dragenter", preventBrowserDragDefault);
    window.addEventListener("dragover", preventBrowserDragDefault);
    window.addEventListener("drop", preventBrowserDragDefault);

    // Native OS Drag and Drop multiple files/folders listener
    try {
      const webview = getCurrentWebview();
      await webview.onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          isDraggingFiles.set(true);
        } else if (event.payload.type === "drop") {
          isDraggingFiles.set(false);
          const droppedPaths = event.payload.paths;
          if (droppedPaths && droppedPaths.length > 0) {
            const currentTab = get(activeMainTab);
            const rSession = get(activeRemoteSession);
            const rStatus = get(remoteConnectionStatus);
            if (
              currentTab === "remote" &&
              rStatus === "connected" &&
              rSession?.isClient
            ) {
              raydropApi
                .initiateTransfer(rSession.peerDeviceId, droppedPaths)
                .catch((err) => {
                  console.warn("Direct remote drop transfer error:", err);
                  cliQueuedFiles.update((prev) => [...prev, ...droppedPaths]);
                });
            } else {
              cliQueuedFiles.update((prev) => [...prev, ...droppedPaths]);
            }
          }
        } else if (event.payload.type === "leave") {
          isDraggingFiles.set(false);
        }
      });
    } catch (err) {
      console.warn("Native Drag-Drop event setup error:", err);
    }

    // Initialize Torrent Engine & listeners
    initStatusText = "Menyiapkan modul transfer & remote...";
    await Promise.allSettled([
      initTorrentStore(),
      initAvatarStore(),
      loadRemoteDesktopSettings(),
    ]);
    initRemoteDesktopListener();
    } catch (err) {
      console.warn("App initialization warning:", err);
    } finally {
      clearTimeout(readyFailSafe);
      setTimeout(() => {
        isAppReady = true;
      }, 150);
    }

    // Auto-check for software updates silently on startup after brief initialization
    setTimeout(() => {
      checkForUpdates(false).catch(() => {});
    }, 2500);

    // Periodically re-check for updates every 30 minutes while running in background/tray
    const updateInterval = setInterval(
      () => {
        checkForUpdates(false).catch(() => {});
      },
      30 * 60 * 1000,
    );

    // Re-check whenever the user unhides or focuses the Raydrop window from tray (with 60s cooldown)
    let lastFocusUpdateCheck = Date.now();
    const handleWindowFocus = () => {
      const now = Date.now();
      if (now - lastFocusUpdateCheck < 60_000) return;
      lastFocusUpdateCheck = now;
      checkForUpdates(false).catch(() => {});
    };
    window.addEventListener("focus", handleWindowFocus);

    return () => {
      clearInterval(updateInterval);
      window.removeEventListener("focus", handleWindowFocus);
      window.removeEventListener("dragenter", preventBrowserDragDefault);
      window.removeEventListener("dragover", preventBrowserDragDefault);
      window.removeEventListener("drop", preventBrowserDragDefault);
    };
  });
</script>

<div
  class="h-screen w-screen flex flex-col bg-slate-950 text-slate-100 select-none overflow-hidden relative"
>
  <!-- Ambient floating blobs (matching landing page aesthetic) -->
  <div
    class="ambient-blob w-[450px] h-[450px] bg-cyan-500/[0.07] -top-24 -left-24"
    aria-hidden="true"
  ></div>
  <div
    class="ambient-blob w-[450px] h-[450px] bg-indigo-500/[0.07] top-1/2 -right-24"
    aria-hidden="true"
  ></div>
  <div
    class="ambient-blob w-[380px] h-[380px] bg-emerald-500/[0.06] -bottom-24 left-1/3"
    aria-hidden="true"
  ></div>

  <Header />

  <!-- Workspace: Spatial Floating Left Pill Dock + Translucent Spatial Window -->
  <div class="flex-1 flex min-h-0 w-full overflow-hidden gap-2 sm:gap-3 px-2 sm:px-3 pb-2 sm:pb-3 pt-1">
    <FloatingRail />

    <div class="flex-1 flex flex-col min-w-0 min-h-0">
      <main
        class="spatial-window flex-1 w-full max-w-full p-2.5 sm:p-4 overflow-x-hidden overflow-y-hidden relative flex flex-col min-h-0 rounded-[26px] bg-white/[0.04] backdrop-blur-2xl border border-white/[0.14] shadow-[0_28px_70px_-15px_rgba(0,0,0,0.6)]"
      >
    <!-- Mobile Windows-Phone / Metro Tile Home Menu (md:hidden) -->
    {#if $activeMainTab === "home"}
      <div class="md:hidden h-full w-full overflow-y-auto overflow-x-hidden pb-6 space-y-3">
        <div class="flex items-center justify-between px-1 pt-1">
          <div class="flex items-center gap-2">
            <LayoutGrid class="w-4 h-4 text-cyan-400" />
            <h2 class="text-sm font-extrabold tracking-tight text-white">Raydrop Hub</h2>
          </div>
          <span class="text-[10px] font-mono text-slate-400">Tap a tile to open</span>
        </div>

        <div class="grid grid-cols-2 gap-2.5">
          <!-- Wide Featured Tile: Transfers -->
          <button
            type="button"
            on:click={() => activeMainTab.set("transfers")}
            class="col-span-2 relative p-4 rounded-2xl bg-gradient-to-br from-cyan-600/30 via-sky-600/20 to-slate-900 border border-cyan-500/40 text-left flex items-center justify-between shadow-lg active:scale-[0.98] transition cursor-pointer"
          >
            <div class="flex items-center gap-3 min-w-0">
              <div class="w-11 h-11 rounded-xl bg-cyan-500/20 border border-cyan-400/40 flex items-center justify-center text-cyan-300 shrink-0">
                <Zap class="w-6 h-6" />
              </div>
              <div class="min-w-0">
                <div class="text-sm font-bold text-white">File Transfers</div>
                <div class="text-[11px] text-cyan-200/80 truncate">Send, receive, HTTP &amp; Torrent</div>
              </div>
            </div>
            {#if $activeTransfers.length > 0}
              <span class="px-2 py-0.5 rounded-full bg-cyan-400 text-slate-950 text-xs font-extrabold shrink-0">
                {$activeTransfers.length}
              </span>
            {/if}
          </button>

          <!-- Encrypted Chat -->
          <button
            type="button"
            on:click={() => activeMainTab.set("chat")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-indigo-600/30 to-slate-900 border border-indigo-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="flex items-start justify-between">
              <div class="w-9 h-9 rounded-xl bg-indigo-500/20 border border-indigo-400/40 flex items-center justify-center text-indigo-300">
                <MessageSquare class="w-5 h-5" />
              </div>
              {#if $totalUnreadCount > 0}
                <span class="px-1.5 py-0.5 rounded-full bg-rose-500 text-white text-[10px] font-bold">
                  {$totalUnreadCount}
                </span>
              {/if}
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Encrypted Chat</div>
              <div class="text-[10px] text-indigo-200/75 truncate">P2P &amp; Group chat</div>
            </div>
          </button>

          <!-- Remote Desktop -->
          <button
            type="button"
            on:click={() => activeMainTab.set("remote")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-emerald-600/30 to-slate-900 border border-emerald-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-emerald-500/20 border border-emerald-400/40 flex items-center justify-center text-emerald-300">
              <Monitor class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Remote Desktop</div>
              <div class="text-[10px] text-emerald-200/75 truncate">Control PC &amp; Extend</div>
            </div>
          </button>

          <!-- Online Meeting -->
          <button
            type="button"
            on:click={() => activeMainTab.set("meeting")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-violet-600/30 to-slate-900 border border-violet-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-violet-500/20 border border-violet-400/40 flex items-center justify-center text-violet-300">
              <Video class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Online Meeting</div>
              <div class="text-[10px] text-violet-200/75 truncate">Voice &amp; video rooms</div>
            </div>
          </button>

          <!-- LAN Radar -->
          <button
            type="button"
            on:click={() => activeMainTab.set("lan")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-sky-600/30 to-slate-900 border border-sky-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="flex items-start justify-between">
              <div class="w-9 h-9 rounded-xl bg-sky-500/20 border border-sky-400/40 flex items-center justify-center text-sky-300">
                <Wifi class="w-5 h-5" />
              </div>
              {#if $lanContacts.length > 0}
                <span class="px-1.5 py-0.5 rounded-full bg-emerald-400 text-slate-950 text-[10px] font-bold">
                  {$lanContacts.length}
                </span>
              {/if}
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">LAN Radar</div>
              <div class="text-[10px] text-sky-200/75 truncate">Nearby Wi-Fi peers</div>
            </div>
          </button>

          <!-- My Devices -->
          <button
            type="button"
            on:click={() => activeMainTab.set("devices")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-blue-600/30 to-slate-900 border border-blue-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-blue-500/20 border border-blue-400/40 flex items-center justify-center text-blue-300">
              <Laptop class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">My Devices</div>
              <div class="text-[10px] text-blue-200/75 truncate">Linked &amp; Wake-on-LAN</div>
            </div>
          </button>

          <!-- Contacts / Peers -->
          <button
            type="button"
            on:click={() => activeMainTab.set("contacts")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-teal-600/30 to-slate-900 border border-teal-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-teal-500/20 border border-teal-400/40 flex items-center justify-center text-teal-300">
              <Users class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Contacts</div>
              <div class="text-[10px] text-teal-200/75 truncate">Trusted peers list</div>
            </div>
          </button>

          <!-- File Converter -->
          <button
            type="button"
            on:click={() => activeMainTab.set("converter")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-amber-600/30 to-slate-900 border border-amber-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-amber-500/20 border border-amber-400/40 flex items-center justify-center text-amber-300">
              <Wand2 class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">File Converter</div>
              <div class="text-[10px] text-amber-200/75 truncate">Image, Icon ZIP &amp; PDF</div>
            </div>
          </button>

          <!-- Copy History -->
          <button
            type="button"
            on:click={() => activeMainTab.set("clipboard")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-slate-700/50 to-slate-900 border border-slate-600/50 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-slate-700/60 border border-slate-500/40 flex items-center justify-center text-cyan-300">
              <ClipboardList class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Copy History</div>
              <div class="text-[10px] text-slate-300/75 truncate">Clipboard snippets</div>
            </div>
          </button>

          <!-- Notes & Checklists -->
          <button
            type="button"
            on:click={() => activeMainTab.set("notes")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-yellow-600/25 to-slate-900 border border-yellow-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-yellow-500/20 border border-yellow-400/40 flex items-center justify-center text-yellow-300">
              <StickyNote class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Notes &amp; Lists</div>
              <div class="text-[10px] text-yellow-200/75 truncate">Quick notes &amp; tasks</div>
            </div>
          </button>

          <!-- System Analytics -->
          <button
            type="button"
            on:click={() => activeMainTab.set("dashboard")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-rose-600/25 to-slate-900 border border-rose-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-rose-500/20 border border-rose-400/40 flex items-center justify-center text-rose-300">
              <Activity class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">Analytics</div>
              <div class="text-[10px] text-rose-200/75 truncate">Stats &amp; system monitor</div>
            </div>
          </button>

          <!-- AI Skills Library -->
          <button
            type="button"
            on:click={() => activeMainTab.set("skills")}
            class="relative p-3.5 rounded-2xl bg-gradient-to-br from-cyan-600/25 to-slate-900 border border-cyan-500/40 text-left flex flex-col justify-between min-h-[100px] shadow-md active:scale-[0.98] transition cursor-pointer"
          >
            <div class="w-9 h-9 rounded-xl bg-cyan-500/20 border border-cyan-400/40 flex items-center justify-center text-cyan-300">
              <Zap class="w-5 h-5" />
            </div>
            <div class="mt-2">
              <div class="text-xs font-bold text-white">AI Skills</div>
              <div class="text-[10px] text-cyan-200/75 truncate">Coding agent skills</div>
            </div>
          </button>
        </div>
      </div>
    {/if}

    <div
      class="h-full w-full {$activeMainTab === 'transfers'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <ActiveTransfers />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'chat'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <ChatView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'meeting'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <MeetingView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'remote'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <RemoteDesktopView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'dashboard'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <DashboardView active={$activeMainTab === 'dashboard'} />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'clipboard'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <ClipboardHistoryView active={$activeMainTab === 'clipboard'} />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'notes'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <NotesView active={$activeMainTab === 'notes'} />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'converter'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <ConverterView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'webdebugger'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <WebDebuggerView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'skills'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <SkillsLibraryView />
    </div>
    <div
      class="h-full w-full {$activeMainTab === 'lan' ||
      $activeMainTab === 'devices' ||
      $activeMainTab === 'contacts'
        ? 'flex flex-col'
        : 'hidden'}"
    >
      <ContactList />
    </div>
  </main>

      <!-- Bottom Spatial Pill Bar + Customizable Menu Picker -->
      <div class="hidden md:flex flex-col items-center gap-1.5 pt-2 shrink-0 select-none relative">
        {#if isBottomMenuPickerOpen}
          <!-- svelte-ignore a11y-click-events-have-key-events -->
          <!-- svelte-ignore a11y-no-static-element-interactions -->
          <div
            class="fixed inset-0 z-30"
            on:click={() => (isBottomMenuPickerOpen = false)}
          ></div>
          <div
            class="absolute bottom-full mb-2.5 z-40 w-72 rounded-2xl bg-slate-900/95 backdrop-blur-2xl border border-white/15 shadow-2xl p-3 space-y-2.5"
          >
            <div class="flex items-center justify-between px-1">
              <div>
                <div class="text-xs font-bold text-white">Customize Bottom Menu</div>
                <div class="text-[10px] text-slate-400">
                  Click a menu to open, or pin/unpin it below
                </div>
              </div>
              <button
                type="button"
                on:click={resetBottomBarTabs}
                class="text-[10px] font-semibold text-sky-400 hover:text-sky-300 px-2 py-0.5 rounded-md hover:bg-white/5 transition cursor-pointer"
                title="Reset to default tabs"
              >
                Reset
              </button>
            </div>

            <div class="grid grid-cols-1 gap-1 max-h-64 overflow-y-auto pr-0.5">
              {#each ALL_BOTTOM_MENUS as menu}
                {@const isPinned = bottomBarTabIds.includes(menu.id)}
                {@const isCurrent = $activeMainTab === menu.id}
                <div
                  class="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-xl border transition {isCurrent
                    ? 'bg-sky-500/15 border-sky-500/35 text-white'
                    : 'bg-white/[0.03] border-white/5 hover:bg-white/[0.08] text-slate-200'}"
                >
                  <button
                    type="button"
                    on:click={() => {
                      activeMainTab.set(menu.id);
                      isBottomMenuPickerOpen = false;
                    }}
                    class="flex-1 text-left text-xs font-medium truncate cursor-pointer"
                  >
                    {menu.label}
                  </button>
                  <button
                    type="button"
                    on:click={() => toggleBottomBarTab(menu.id)}
                    disabled={isPinned && bottomBarTabIds.length <= 1}
                    class="px-2 py-0.5 rounded-full text-[10px] font-semibold border transition cursor-pointer disabled:opacity-40 {isPinned
                      ? 'bg-sky-400/20 text-sky-300 border-sky-400/40 hover:bg-rose-500/20 hover:text-rose-300 hover:border-rose-400/40'
                      : 'bg-white/5 text-slate-400 border-white/10 hover:bg-sky-400/20 hover:text-sky-300 hover:border-sky-400/40'}"
                    title={isPinned ? "Remove from bottom bar" : "Pin to bottom bar"}
                  >
                    {isPinned ? "Pinned" : "+ Pin"}
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div class="spatial-glass-pill relative z-40 inline-flex items-center gap-1 px-2 py-1 rounded-full bg-white/[0.08] backdrop-blur-2xl border border-white/15 shadow-xl max-w-[92vw] overflow-x-auto">
          {#each ALL_BOTTOM_MENUS.filter((m) => bottomBarTabIds.includes(m.id)) as tab (tab.id)}
            <button
              type="button"
              on:click={() => {
                activeMainTab.set(tab.id);
                isBottomMenuPickerOpen = false;
              }}
              class="px-3.5 py-1 rounded-full text-[11px] font-semibold transition cursor-pointer shrink-0 {$activeMainTab === tab.id
                ? 'bg-white/20 text-white shadow-sm'
                : 'text-white/65 hover:text-white hover:bg-white/10'}"
            >
              {tab.label}
            </button>
          {/each}
          <div class="w-px h-4 bg-white/15 mx-0.5 shrink-0"></div>
          <button
            type="button"
            on:click={() => (isBottomMenuPickerOpen = !isBottomMenuPickerOpen)}
            class="w-7 h-7 rounded-full bg-sky-400 hover:bg-sky-300 text-slate-950 font-bold flex items-center justify-center transition-transform duration-200 cursor-pointer shrink-0 {isBottomMenuPickerOpen
              ? 'rotate-45'
              : ''}"
            title="Quick Switch & Customize Bottom Menu"
          >
            +
          </button>
        </div>
        <div class="w-24 h-1 rounded-full bg-white/30"></div>
      </div>
    </div>
  </div>

  <!-- Fullscreen Drag & Drop Overlay Indicator -->
  {#if $isDraggingFiles}
    <div
      class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex flex-col items-center justify-center pointer-events-none transition-all"
    >
      <div
        class="p-8 rounded-3xl border-2 border-dashed border-cyan-400 bg-slate-900/95 shadow-2xl shadow-cyan-500/25 flex flex-col items-center text-center max-w-md mx-4 transform scale-105 transition-transform"
      >
        <div
          class="w-20 h-20 rounded-2xl bg-gradient-to-tr from-cyan-500/25 to-indigo-500/25 border border-cyan-400/50 flex items-center justify-center text-cyan-300 mb-4 animate-bounce"
        >
          <Upload class="w-10 h-10 stroke-[2.2]" />
        </div>
        <h3 class="text-xl font-extrabold text-white font-display">
          {#if $activeMainTab === "remote" && $remoteConnectionStatus === "connected" && $activeRemoteSession?.isClient}
            Drop to Send Directly to {$activeRemoteSession.peerAlias}
          {:else}
            Drop Files to Transfer
          {/if}
        </h3>
        <p class="text-xs text-slate-300 mt-2">
          {#if $activeMainTab === "remote" && $remoteConnectionStatus === "connected" && $activeRemoteSession?.isClient}
            Files dropped onto the Remote Desktop screen are transferred
            directly to the host device immediately.
          {:else}
            Drop any multiple files or folders here to send directly via
            encrypted P2P.
          {/if}
        </p>
        <div
          class="flex items-center gap-2 mt-4 text-[11px] font-mono text-cyan-400 bg-cyan-500/10 px-3 py-1.5 rounded-full border border-cyan-500/30"
        >
          <span>⚡ Unlimited Size • Resumable • Zero Cloud Storage</span>
        </div>
      </div>
    </div>
  {/if}

  <!-- In-app Floating Presence (Online/Offline) Toast Notifications -->
  {#if $presenceToasts.length > 0}
    <div
      class="fixed top-16 right-5 z-50 flex flex-col gap-2 max-w-xs pointer-events-auto"
    >
      {#each $presenceToasts as pt (pt.id)}
        <div
          class="px-3.5 py-2.5 rounded-xl bg-slate-900/95 border shadow-xl backdrop-blur-xl flex items-center justify-between gap-3 animate-in fade-in slide-in-from-right duration-200 {pt.tone === 'rose'
            ? 'border-rose-500/50 shadow-rose-500/15'
            : pt.tone === 'amber'
              ? 'border-amber-500/50 shadow-amber-500/15'
              : pt.tone === 'cyan'
                ? 'border-cyan-500/50 shadow-cyan-500/15'
                : pt.tone === 'slate' || !pt.isOnline
                  ? 'border-slate-700/80 shadow-black/40'
                  : 'border-emerald-500/50 shadow-emerald-500/15'}"
        >
          <div class="flex items-center gap-2.5 min-w-0">
            <span
              class="w-2.5 h-2.5 rounded-full shrink-0 {pt.tone === 'rose'
                ? 'bg-rose-400 shadow-[0_0_8px_rgba(251,113,133,0.7)]'
                : pt.tone === 'amber'
                  ? 'bg-amber-400 shadow-[0_0_8px_rgba(251,191,36,0.7)]'
                  : pt.tone === 'cyan'
                    ? 'bg-cyan-400 shadow-[0_0_8px_rgba(34,211,238,0.7)]'
                    : pt.tone === 'slate' || !pt.isOnline
                      ? 'bg-slate-500'
                      : 'bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.7)]'}"
            ></span>
            <div class="min-w-0">
              <div class="text-xs font-bold text-white truncate">{pt.displayName}</div>
              <div
                class="text-[11px] font-medium truncate {pt.tone === 'rose'
                  ? 'text-rose-300'
                  : pt.tone === 'amber'
                    ? 'text-amber-300'
                    : pt.tone === 'cyan'
                      ? 'text-cyan-300'
                      : pt.tone === 'slate' || !pt.isOnline
                        ? 'text-slate-400'
                        : 'text-emerald-300'}"
              >
                {pt.subtitle || (pt.isOnline ? "Sekarang Online" : "Baru saja Offline")}
              </div>
            </div>
          </div>
          <button
            type="button"
            on:click={() => removePresenceToast(pt.id)}
            class="p-1 text-slate-400 hover:text-white rounded-lg cursor-pointer"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <!-- In-app Floating Chat Toast Notifications (Bottom-Right) -->
  {#if $chatToasts.length > 0}
    <div
      class="fixed bottom-6 right-6 z-50 flex flex-col gap-2.5 max-w-sm pointer-events-auto"
    >
      {#each $chatToasts as toast (toast.id)}
        <div
          role="button"
          tabindex="0"
          on:click={() => {
            activeMainTab.set("chat");
            activeChatPeerId.set(toast.peerDeviceId);
            removeChatToast(toast.id);
          }}
          on:keydown={(e) => {
            if (e.key === "Enter") {
              activeMainTab.set("chat");
              activeChatPeerId.set(toast.peerDeviceId);
              removeChatToast(toast.id);
            }
          }}
          class="p-4 rounded-2xl bg-slate-900/95 border border-cyan-500/50 shadow-2xl shadow-cyan-500/20 backdrop-blur-xl flex items-start justify-between gap-3 cursor-pointer hover:border-cyan-400 hover:scale-[1.02] transition-all transform animate-in slide-in-from-bottom duration-200"
        >
          <div class="flex items-start gap-3 min-w-0">
            {#if $peerAvatars[toast.peerDeviceId]}
              <img
                src={$peerAvatars[toast.peerDeviceId]}
                alt={toast.peerName}
                class="w-9 h-9 rounded-xl object-cover border border-cyan-500/40 shrink-0 mt-0.5 shadow-sm"
              />
            {:else}
              <div
                class="w-9 h-9 rounded-xl bg-gradient-to-tr from-cyan-500/20 to-indigo-500/20 border border-cyan-500/40 flex items-center justify-center text-cyan-300 font-bold text-xs shrink-0 mt-0.5"
              >
                {toast.peerName.slice(0, 2).toUpperCase()}
              </div>
            {/if}
            <div class="min-w-0">
              <div class="flex items-center gap-1.5">
                <span class="text-xs font-bold text-white truncate"
                  >{toast.peerName}</span
                >
                <span class="text-[10px] text-cyan-400 font-mono"
                  >New Message</span
                >
              </div>
              <p class="text-xs text-slate-300 truncate mt-0.5 max-w-[220px]">
                {toast.content}
              </p>
              <span class="text-[10px] text-cyan-400/80 font-mono mt-1 block">
                Click to open chat &rarr;
              </span>
            </div>
          </div>
          <button
            type="button"
            on:click|stopPropagation={() => removeChatToast(toast.id)}
            class="text-slate-400 hover:text-white p-1 rounded-lg hover:bg-slate-800 transition shrink-0"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <TransferModal />
  <TraySettings />
  <AuthModal />
  <UpdateModal />
  <MagnetDownloadModal />
  <BrowserExtensionModal />
  <MeetingInviteModal />
  <WolModal />
  <WinampPlayer />

  {#if pendingMeshRequest}
    <div
      class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
    >
      <div
        class="w-full max-w-sm bg-slate-900 border border-sky-500/40 rounded-2xl p-5 shadow-2xl space-y-4"
      >
        <div>
          <h3 class="text-sm font-bold text-white">Direct Mesh (Virtual LAN) Request</h3>
          <p class="text-xs text-slate-300 mt-1.5 leading-relaxed">
            <span class="font-semibold text-sky-300">{pendingMeshRequest.alias_name}</span>
            wants to link with this device via Raydrop Direct Mesh (persistent P2P tunnel for faster file transfers & remote control).
          </p>
        </div>
        <div class="flex items-center gap-2 pt-1">
          <button
            type="button"
            on:click={async () => {
              if (!pendingMeshRequest) return;
              const req = pendingMeshRequest;
              pendingMeshRequest = null;
              await raydropApi.respondDirectMesh(req.device_id, false).catch(() => {});
            }}
            class="flex-1 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-300 border border-slate-700 transition cursor-pointer"
          >
            Decline
          </button>
          <button
            type="button"
            on:click={async () => {
              if (!pendingMeshRequest) return;
              const req = pendingMeshRequest;
              pendingMeshRequest = null;
              await raydropApi.respondDirectMesh(req.device_id, true).catch(() => {});
            }}
            class="flex-1 py-2 rounded-xl bg-sky-500 hover:bg-sky-400 text-xs font-bold text-slate-950 transition cursor-pointer"
          >
            Approve Mesh
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if $extensionUpdateInfo}
    <div class="fixed bottom-4 right-4 z-[85] max-w-sm rounded-2xl bg-slate-900/95 backdrop-blur-xl border border-amber-500/40 shadow-2xl p-3.5 flex items-center gap-3">
      <div class="flex-1 text-xs">
        <div class="font-bold text-amber-300">
          Chrome Extension Update (v{$extensionUpdateInfo.installedVersion} → v{$extensionUpdateInfo.latestVersion})
        </div>
        <div class="text-[11px] text-slate-300 mt-0.5">
          Versi ekstensi terbaru tersedia. Klik Update untuk memperbarui folder &amp; reload di Chrome.
        </div>
      </div>
      <div class="flex items-center gap-1.5 shrink-0">
        <button
          type="button"
          on:click={() => ($browserExtensionModalOpen = true)}
          class="px-2.5 py-1.5 rounded-lg bg-amber-500 hover:bg-amber-400 text-slate-950 font-bold text-[11px] cursor-pointer"
        >
          Update
        </button>
        <button
          type="button"
          on:click={() => extensionUpdateInfo.set(null)}
          class="px-2 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 text-slate-400 text-[11px] cursor-pointer"
        >
          ✕
        </button>
      </div>
    </div>
  {/if}

  {#if !isAppReady}
    <div
      class="fixed inset-0 z-[100] bg-slate-950/95 backdrop-blur-xl flex flex-col items-center justify-center p-6 select-none"
      aria-busy="true"
      aria-label="Loading Raydrop"
    >
      <div class="flex flex-col items-center gap-4 max-w-xs w-full text-center">
        <div class="relative w-16 h-16 flex items-center justify-center">
          <div class="absolute inset-0 rounded-2xl border-2 border-cyan-500/30 animate-ping"></div>
          <div class="w-16 h-16 rounded-2xl bg-slate-900 border border-cyan-500/40 shadow-2xl shadow-cyan-500/20 flex items-center justify-center overflow-hidden">
            <img src="/icon.png" alt="Raydrop" class="w-14 h-14 object-contain rounded-xl" />
          </div>
        </div>
        <div class="space-y-1">
          <h2 class="text-base font-bold text-white tracking-tight">Raydrop</h2>
          <p class="text-xs text-slate-400">{initStatusText}</p>
        </div>
        <div class="w-48 h-1 rounded-full bg-slate-800 overflow-hidden">
          <div class="h-full w-1/2 rounded-full bg-gradient-to-r from-cyan-500 to-indigo-500 animate-pulse"></div>
        </div>
      </div>
    </div>
  {/if}

</div>
