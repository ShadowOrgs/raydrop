<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    ClipboardList,
    Copy,
    Check,
    Pin,
    PinOff,
    Trash2,
    Search,
    Image as ImageIcon,
    FileText,
    Sparkles,
    X,
    Pencil,
  } from "lucide-svelte";
  import { raydropApi, type ClipboardHistoryItem } from "../utils/tauri-ipc";
  import SnippingEditorModal from "./SnippingEditorModal.svelte";

  export let active = false;

  let items: ClipboardHistoryItem[] = [];
  let loading = true;
  let searchQuery = "";
  let filterMode: "all" | "text" | "image" | "pinned" = "all";
  let copiedId: string | null = null;
  let expandedItem: ClipboardHistoryItem | null = null;
  let editingItem: ClipboardHistoryItem | null = null;
  let savingEdited = false;
  let unlisten: (() => void) | null = null;

  async function loadHistory() {
    try {
      items = await raydropApi.listClipboardHistory();
    } catch (err) {
      console.warn("Failed to load clipboard history:", err);
    } finally {
      loading = false;
    }
  }

  $: if (active) {
    loadHistory();
  }

  onMount(async () => {
    await loadHistory();
    try {
      unlisten = await raydropApi.listenEvent<void>(
        "clipboard_history_updated",
        () => {
          loadHistory();
        },
      );
    } catch (_) {}
  });

  onDestroy(() => {
    if (unlisten) unlisten();
  });

  async function handleCopy(item: ClipboardHistoryItem) {
    try {
      await raydropApi.copyClipboardHistoryItem(item.id);
      copiedId = item.id;
      setTimeout(() => {
        if (copiedId === item.id) copiedId = null;
      }, 1500);
    } catch (err) {
      console.warn("Copy item failed:", err);
    }
  }

  async function handleTogglePin(item: ClipboardHistoryItem) {
    try {
      await raydropApi.togglePinClipboardItem(item.id);
      await loadHistory();
    } catch (err) {
      console.warn("Pin toggle failed:", err);
    }
  }

  async function handleDelete(item: ClipboardHistoryItem) {
    try {
      await raydropApi.deleteClipboardHistoryItem(item.id);
      items = items.filter((i) => i.id !== item.id);
    } catch (err) {
      console.warn("Delete failed:", err);
    }
  }

  async function handleSaveEditedImage(e: CustomEvent<{ dataUrl: string; copyToClipboard: boolean }>) {
    if (!editingItem) return;
    savingEdited = true;
    try {
      await invoke("update_clipboard_history_image", {
        id: editingItem.id,
        dataUrl: e.detail.dataUrl,
        copyToClipboard: e.detail.copyToClipboard,
      });
      await loadHistory();
      editingItem = null;
    } catch (err) {
      console.warn("Save edited image failed:", err);
    } finally {
      savingEdited = false;
    }
  }

  async function handleClearUnpinned() {
    try {
      await raydropApi.clearClipboardHistory();
      await loadHistory();
    } catch (err) {
      console.warn("Clear failed:", err);
    }
  }

  function formatRelativeTime(unixSecs: number): string {
    const diff = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
    if (diff < 10) return "Just now";
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  }

  $: filteredItems = items.filter((item) => {
    if (filterMode === "text" && item.content_type !== "text") return false;
    if (filterMode === "image" && item.content_type !== "image") return false;
    if (filterMode === "pinned" && !item.pinned) return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      if (item.content_type === "image") {
        return "image snapshot".includes(q);
      }
      return item.content.toLowerCase().includes(q);
    }
    return true;
  });
</script>

<div class="h-full w-full flex flex-col gap-4 overflow-hidden">
  <!-- Header -->
  <div
    class="bg-slate-900/80 border border-slate-800/80 rounded-2xl p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-3 shrink-0"
  >
    <div class="flex items-center gap-3">
      <div
        class="w-10 h-10 rounded-xl bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shrink-0"
      >
        <ClipboardList class="w-5 h-5" />
      </div>
      <div>
        <div class="flex items-center gap-2 flex-wrap">
          <h2 class="text-base font-bold text-white">Copy History (Win+V)</h2>
          <span
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/15 text-emerald-300 border border-emerald-500/30"
          >
            <span class="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse"></span>
            Auto-Capturing
          </span>
        </div>
        <p class="text-xs text-slate-400 mt-0.5">
          Cross-platform clipboard manager for Windows, macOS & Linux — click any item to copy again.
        </p>
      </div>
    </div>

    <div class="flex items-center gap-2 shrink-0">
      <span class="text-[11px] font-mono text-slate-400 px-2" title="Keeps up to 30 unpinned items; pinned items are never auto-deleted">
        {items.length} saved (max 30 unpinned)
      </span>
      <button
        type="button"
        on:click={handleClearUnpinned}
        disabled={items.every((i) => i.pinned)}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-rose-500/20 text-slate-300 hover:text-rose-300 border border-slate-700/80 hover:border-rose-500/30 text-xs font-semibold transition-all disabled:opacity-40 cursor-pointer"
        title="Clear all unpinned clipboard items"
      >
        <Trash2 class="w-3.5 h-3.5" />
        <span>Clear Unpinned</span>
      </button>
    </div>
  </div>

  <!-- Search & Filters -->
  <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2.5 shrink-0">
    <div
      class="flex-1 flex items-center gap-2 px-3 py-2 rounded-xl bg-slate-900/90 border border-slate-800 focus-within:border-cyan-500/60"
    >
      <Search class="w-4 h-4 text-slate-500 shrink-0" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="Search copied text..."
        class="flex-1 bg-transparent text-xs text-white placeholder-slate-500 focus:outline-none"
      />
      {#if searchQuery}
        <button
          type="button"
          on:click={() => (searchQuery = "")}
          class="text-slate-400 hover:text-white"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-1 bg-slate-900/90 p-1 rounded-xl border border-slate-800 shrink-0">
      {#each [
        { id: "all", label: "All" },
        { id: "text", label: "Text" },
        { id: "image", label: "Images" },
        { id: "pinned", label: "Pinned" },
      ] as tab}
        <button
          type="button"
          on:click={() => (filterMode = tab.id)}
          class="px-3 py-1.5 rounded-lg text-xs font-semibold transition-all cursor-pointer {filterMode ===
          tab.id
            ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <!-- History Grid / List -->
  <div class="flex-1 overflow-y-auto pr-1 space-y-2.5 min-h-0">
    {#if loading}
      <div class="h-48 flex items-center justify-center text-xs text-slate-400">
        Loading clipboard history...
      </div>
    {:else if filteredItems.length === 0}
      <div
        class="h-64 flex flex-col items-center justify-center text-center bg-slate-900/40 border border-dashed border-slate-800 rounded-2xl p-6"
      >
        <Sparkles class="w-8 h-8 text-slate-600 mb-2" />
        <p class="text-sm font-semibold text-slate-300">
          {items.length === 0 ? "No clipboard items yet" : "No matching clipboard items"}
        </p>
        <p class="text-xs text-slate-500 mt-1 max-w-sm">
          Copy any text or image anywhere on your system (Ctrl+C / Cmd+C) and it will automatically appear here.
        </p>
      </div>
    {:else}
      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        {#each filteredItems as item (item.id)}
          <div
            class="group bg-slate-900/80 hover:bg-slate-900 border {item.pinned
              ? 'border-amber-500/40'
              : 'border-slate-800/90 hover:border-cyan-500/40'} rounded-2xl p-3.5 flex flex-col justify-between gap-3 transition-all shadow-sm"
          >
            <!-- Card Top Meta -->
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <span
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-semibold {item.content_type ===
                  'image'
                    ? 'bg-indigo-500/15 text-indigo-300 border border-indigo-500/30'
                    : 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/30'}"
                >
                  {#if item.content_type === "image"}
                    <ImageIcon class="w-3 h-3" />
                    <span>Image</span>
                  {:else}
                    <FileText class="w-3 h-3" />
                    <span>Text</span>
                  {/if}
                </span>
                {#if item.pinned}
                  <span
                    class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-semibold bg-amber-500/15 text-amber-300 border border-amber-500/30"
                  >
                    <Pin class="w-2.5 h-2.5" />
                    <span>Pinned</span>
                  </span>
                {/if}
                <span class="text-[11px] text-slate-500 font-mono">
                  {formatRelativeTime(item.created_at)}
                </span>
              </div>

              <div class="flex items-center gap-1">
                {#if item.content_type === "image"}
                  <button
                    type="button"
                    on:click={() => (editingItem = item)}
                    class="flex items-center gap-1 px-2.5 py-1 rounded-lg text-[11px] font-semibold transition-all cursor-pointer bg-indigo-500/15 hover:bg-indigo-500/30 text-indigo-300 border border-indigo-500/30"
                    title="Edit image (Snipping Tool)"
                  >
                    <Pencil class="w-3 h-3" />
                    <span>Edit</span>
                  </button>
                {/if}
                <button
                  type="button"
                  on:click={() => handleCopy(item)}
                  class="flex items-center gap-1 px-2.5 py-1 rounded-lg text-[11px] font-semibold transition-all cursor-pointer {copiedId ===
                  item.id
                    ? 'bg-emerald-500 text-slate-950'
                    : 'bg-cyan-500/15 hover:bg-cyan-500/30 text-cyan-300 border border-cyan-500/30'}"
                  title="Copy to OS clipboard"
                >
                  {#if copiedId === item.id}
                    <Check class="w-3 h-3" />
                    <span>Copied!</span>
                  {:else}
                    <Copy class="w-3 h-3" />
                    <span>Copy</span>
                  {/if}
                </button>

                <button
                  type="button"
                  on:click={() => handleTogglePin(item)}
                  class="p-1.5 rounded-lg text-slate-400 hover:text-amber-300 hover:bg-amber-500/15 transition-colors cursor-pointer"
                  title={item.pinned ? "Unpin item" : "Pin item"}
                >
                  {#if item.pinned}
                    <PinOff class="w-3.5 h-3.5 text-amber-400" />
                  {:else}
                    <Pin class="w-3.5 h-3.5" />
                  {/if}
                </button>

                <button
                  type="button"
                  on:click={() => handleDelete(item)}
                  class="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 transition-colors cursor-pointer"
                  title="Delete item"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <!-- Card Content -->
            <!-- svelte-ignore a11y-click-events-have-key-events -->
            <!-- svelte-ignore a11y-no-static-element-interactions -->
            <div
              on:click={() => handleCopy(item)}
              class="cursor-pointer rounded-xl bg-slate-950/70 border border-slate-800/80 p-2.5 hover:border-slate-700 transition-colors"
              title="Click to copy to clipboard"
            >
              {#if item.content_type === "image"}
                <div class="flex items-center justify-center bg-black/40 rounded-lg overflow-hidden max-h-44">
                  <img
                    src={item.content}
                    alt="Copied snippet"
                    class="max-h-44 object-contain"
                  />
                </div>
              {:else}
                <pre
                  class="text-xs text-slate-200 font-sans whitespace-pre-wrap break-words line-clamp-4 leading-relaxed"
                >{item.content}</pre>
                {#if item.content.length > 220}
                  <button
                    type="button"
                    on:click|stopPropagation={() => (expandedItem = item)}
                    class="mt-1.5 text-[11px] text-cyan-400 hover:underline font-medium"
                  >
                    View full text ({item.content.length.toLocaleString()} chars)
                  </button>
                {/if}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Full Text Preview Modal -->
{#if expandedItem}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
    on:click={() => (expandedItem = null)}
  >
    <div
      class="bg-slate-900 border border-slate-700/80 rounded-2xl max-w-2xl w-full max-h-[85vh] flex flex-col p-5 gap-3 shadow-2xl"
      on:click|stopPropagation
    >
      <div class="flex items-center justify-between border-b border-slate-800 pb-3">
        <span class="text-sm font-bold text-white">Full Clipboard Text</span>
        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={() => expandedItem && handleCopy(expandedItem)}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
          >
            <Copy class="w-3.5 h-3.5" />
            <span>Copy All</span>
          </button>
          <button
            type="button"
            on:click={() => (expandedItem = null)}
            class="p-1.5 rounded-lg bg-slate-800 text-slate-300 hover:text-white cursor-pointer"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>
      <div class="overflow-y-auto flex-1 bg-slate-950/80 border border-slate-800 rounded-xl p-3">
        <pre class="text-xs text-slate-200 font-sans whitespace-pre-wrap break-words leading-relaxed">{expandedItem.content}</pre>
      </div>
    </div>
  </div>
{/if}

{#if editingItem}
  <SnippingEditorModal
    imageSrc={editingItem.content}
    saving={savingEdited}
    on:close={() => (editingItem = null)}
    on:save={handleSaveEditedImage}
  />
{/if}
