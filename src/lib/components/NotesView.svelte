<script lang="ts">
  import { onMount } from "svelte";
  import {
    StickyNote,
    CheckSquare,
    Square,
    Pin,
    PinOff,
    Archive,
    ArchiveRestore,
    Trash2,
    Plus,
    Image as ImageIcon,
    Palette,
    Search,
    X,
    Tag,
    Clipboard,
  } from "lucide-svelte";
  import {
    raydropApi,
    type NoteItem,
    type NoteChecklistEntry,
  } from "../utils/tauri-ipc";

  export let active = false;

  const NOTE_COLORS: { id: string; label: string; cardClass: string; dot: string }[] = [
    { id: "default", label: "Default", cardClass: "bg-slate-900/85 border-slate-800/90", dot: "#334155" },
    { id: "cyan", label: "Cyan", cardClass: "bg-cyan-950/35 border-cyan-500/35", dot: "#06b6d4" },
    { id: "indigo", label: "Indigo", cardClass: "bg-indigo-950/35 border-indigo-500/35", dot: "#6366f1" },
    { id: "emerald", label: "Emerald", cardClass: "bg-emerald-950/35 border-emerald-500/35", dot: "#10b981" },
    { id: "amber", label: "Amber", cardClass: "bg-amber-950/35 border-amber-500/35", dot: "#f59e0b" },
    { id: "rose", label: "Rose", cardClass: "bg-rose-950/35 border-rose-500/35", dot: "#f43f5e" },
    { id: "purple", label: "Purple", cardClass: "bg-purple-950/35 border-purple-500/35", dot: "#a855f7" },
  ];

  function colorClass(id: string): string {
    return NOTE_COLORS.find((c) => c.id === id)?.cardClass || NOTE_COLORS[0].cardClass;
  }

  let notes: NoteItem[] = [];
  let loading = true;
  let searchQuery = "";
  let viewFilter: "active" | "pinned" | "checklists" | "archived" = "active";

  // Quick composer state (Google Keep style)
  let composerOpen = false;
  let compTitle = "";
  let compContent = "";
  let compType: "note" | "checklist" = "note";
  let compChecklist: NoteChecklistEntry[] = [];
  let compNewTaskText = "";
  let compImages: string[] = [];
  let compColor = "default";
  let compLabels: string[] = [];
  let compLabelInput = "";
  let compPinned = false;
  let compColorPickerOpen = false;
  let compFileInput: HTMLInputElement;

  // Edit Modal state
  let editingNote: NoteItem | null = null;
  let editNewTaskText = "";
  let editLabelInput = "";
  let editColorPickerOpen = false;
  let editFileInput: HTMLInputElement;
  let lightboxImage: string | null = null;

  async function loadNotes() {
    try {
      notes = await raydropApi.listNotes(true);
    } catch (err) {
      console.warn("Failed to load notes:", err);
    } finally {
      loading = false;
    }
  }

  $: if (active) {
    loadNotes();
  }

  onMount(() => {
    loadNotes();
  });

  function makeId() {
    return `${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  }

  function resetComposer() {
    composerOpen = false;
    compTitle = "";
    compContent = "";
    compType = "note";
    compChecklist = [];
    compNewTaskText = "";
    compImages = [];
    compColor = "default";
    compLabels = [];
    compLabelInput = "";
    compPinned = false;
    compColorPickerOpen = false;
  }

  function addComposerChecklistItem() {
    const text = compNewTaskText.trim();
    if (!text) return;
    compChecklist = [...compChecklist, { id: makeId(), text, checked: false }];
    compNewTaskText = "";
  }

  function addComposerLabel() {
    const lbl = compLabelInput.trim();
    if (!lbl || compLabels.includes(lbl)) return;
    compLabels = [...compLabels, lbl];
    compLabelInput = "";
  }

  async function saveComposerNote() {
    const hasBody =
      compTitle.trim() ||
      compContent.trim() ||
      compChecklist.length > 0 ||
      compImages.length > 0;
    if (!hasBody) {
      resetComposer();
      return;
    }
    try {
      const saved = await raydropApi.saveNote({
        id: "",
        title: compTitle.trim(),
        content: compContent,
        note_type: compType,
        checklist: compChecklist,
        images: compImages,
        color: compColor,
        labels: compLabels,
        pinned: compPinned,
        archived: false,
        created_at: 0,
        updated_at: 0,
      });
      notes = [saved, ...notes];
      resetComposer();
    } catch (err) {
      console.warn("Save note failed:", err);
    }
  }

  function readFileAsDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || ""));
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  async function handlePasteInComposer(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of Array.from(items)) {
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const file = item.getAsFile();
        if (file) {
          const dataUrl = await readFileAsDataUrl(file);
          if (dataUrl) {
            composerOpen = true;
            compImages = [...compImages, dataUrl];
          }
        }
      }
    }
  }

  async function pasteOsImageToComposer() {
    try {
      const content = await raydropApi.readSystemClipboard();
      if (content && content.startsWith("data:image/")) {
        composerOpen = true;
        compImages = [...compImages, content];
      }
    } catch (_) {}
  }

  async function handleFilePickComposer(e: Event) {
    const input = e.target as HTMLInputElement;
    const files = input.files;
    if (!files) return;
    for (const f of Array.from(files)) {
      if (f.type.startsWith("image/")) {
        const url = await readFileAsDataUrl(f);
        if (url) compImages = [...compImages, url];
      }
    }
    input.value = "";
  }

  async function handlePasteInEditor(e: ClipboardEvent) {
    if (!editingNote) return;
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of Array.from(items)) {
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const file = item.getAsFile();
        if (file) {
          const dataUrl = await readFileAsDataUrl(file);
          if (dataUrl && editingNote) {
            editingNote = {
              ...editingNote,
              images: [...editingNote.images, dataUrl],
            };
          }
        }
      }
    }
  }

  async function pasteOsImageToEditor() {
    if (!editingNote) return;
    try {
      const content = await raydropApi.readSystemClipboard();
      if (content && content.startsWith("data:image/") && editingNote) {
        editingNote = {
          ...editingNote,
          images: [...editingNote.images, content],
        };
      }
    } catch (_) {}
  }

  async function handleFilePickEditor(e: Event) {
    if (!editingNote) return;
    const input = e.target as HTMLInputElement;
    const files = input.files;
    if (!files) return;
    const added: string[] = [];
    for (const f of Array.from(files)) {
      if (f.type.startsWith("image/")) {
        const url = await readFileAsDataUrl(f);
        if (url) added.push(url);
      }
    }
    if (added.length > 0 && editingNote) {
      editingNote = { ...editingNote, images: [...editingNote.images, ...added] };
    }
    input.value = "";
  }

  async function toggleCardChecklistItem(note: NoteItem, entryId: string) {
    const updatedChecklist = note.checklist.map((c) =>
      c.id === entryId ? { ...c, checked: !c.checked } : c,
    );
    const updatedNote = { ...note, checklist: updatedChecklist };
    notes = notes.map((n) => (n.id === note.id ? updatedNote : n));
    try {
      const saved = await raydropApi.saveNote(updatedNote);
      notes = notes.map((n) => (n.id === saved.id ? saved : n));
    } catch (_) {}
  }

  async function handleTogglePin(note: NoteItem) {
    try {
      await raydropApi.togglePinNote(note.id);
      await loadNotes();
    } catch (_) {}
  }

  async function handleToggleArchive(note: NoteItem) {
    try {
      await raydropApi.toggleArchiveNote(note.id);
      if (editingNote?.id === note.id) editingNote = null;
      await loadNotes();
    } catch (_) {}
  }

  async function handleDeleteNote(noteId: string) {
    try {
      await raydropApi.deleteNote(noteId);
      notes = notes.filter((n) => n.id !== noteId);
      if (editingNote?.id === noteId) editingNote = null;
    } catch (_) {}
  }

  async function saveEditingNote() {
    if (!editingNote) return;
    try {
      const saved = await raydropApi.saveNote(editingNote);
      notes = notes.map((n) => (n.id === saved.id ? saved : n));
      editingNote = null;
      editColorPickerOpen = false;
    } catch (err) {
      console.warn("Failed to save note:", err);
    }
  }

  function addEditorTask() {
    if (!editingNote) return;
    const text = editNewTaskText.trim();
    if (!text) return;
    editingNote = {
      ...editingNote,
      note_type: "checklist",
      checklist: [...editingNote.checklist, { id: makeId(), text, checked: false }],
    };
    editNewTaskText = "";
  }

  function addEditorLabel() {
    if (!editingNote) return;
    const lbl = editLabelInput.trim();
    if (!lbl || editingNote.labels.includes(lbl)) return;
    editingNote = { ...editingNote, labels: [...editingNote.labels, lbl] };
    editLabelInput = "";
  }

  $: filteredNotes = notes.filter((n) => {
    if (viewFilter === "archived") {
      if (!n.archived) return false;
    } else {
      if (n.archived) return false;
      if (viewFilter === "pinned" && !n.pinned) return false;
      if (viewFilter === "checklists" && n.note_type !== "checklist" && n.checklist.length === 0)
        return false;
    }
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const inTitle = n.title.toLowerCase().includes(q);
      const inContent = n.content.toLowerCase().includes(q);
      const inChecklist = n.checklist.some((c) => c.text.toLowerCase().includes(q));
      const inLabels = n.labels.some((l) => l.toLowerCase().includes(q));
      return inTitle || inContent || inChecklist || inLabels;
    }
    return true;
  });

  $: pinnedNotes = filteredNotes.filter((n) => n.pinned);
  $: otherNotes = filteredNotes.filter((n) => !n.pinned);
</script>

<div class="h-full w-full flex flex-col gap-4 overflow-hidden">
  <!-- Top Bar: Search & Filters -->
  <div class="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-2.5 shrink-0">
    <div class="flex items-center gap-2.5">
      <div
        class="w-9 h-9 rounded-xl bg-amber-500/15 border border-amber-500/30 flex items-center justify-center text-amber-400 shrink-0"
      >
        <StickyNote class="w-4.5 h-4.5" />
      </div>
      <div>
        <h2 class="text-sm font-bold text-white">Notes & Task Lists</h2>
        <p class="text-[11px] text-slate-400">
          Google Keep style — paste images (Ctrl+V), checklists & labels.
        </p>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div
        class="flex items-center gap-2 px-3 py-1.5 rounded-xl bg-slate-900/90 border border-slate-800 focus-within:border-cyan-500/60 w-full sm:w-56"
      >
        <Search class="w-3.5 h-3.5 text-slate-500 shrink-0" />
        <input
          type="text"
          bind:value={searchQuery}
          placeholder="Search notes..."
          class="flex-1 bg-transparent text-xs text-white placeholder-slate-500 focus:outline-none"
        />
        {#if searchQuery}
          <button type="button" on:click={() => (searchQuery = "")} class="text-slate-400 hover:text-white">
            <X class="w-3 h-3" />
          </button>
        {/if}
      </div>

      <div class="flex items-center gap-1 bg-slate-900/90 p-1 rounded-xl border border-slate-800">
        {#each [
          { id: "active", label: "Notes" },
          { id: "pinned", label: "Pinned" },
          { id: "checklists", label: "Tasks" },
          { id: "archived", label: "Archive" },
        ] as tab}
          <button
            type="button"
            on:click={() => (viewFilter = tab.id)}
            class="px-2.5 py-1 rounded-lg text-[11px] font-semibold transition-all cursor-pointer {viewFilter ===
            tab.id
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
              : 'text-slate-400 hover:text-slate-200'}"
          >
            {tab.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- Quick "Take a note..." Composer (Google Keep style) -->
  {#if viewFilter !== "archived"}
    <div
      class="max-w-2xl w-full mx-auto rounded-2xl border transition-all shadow-lg shrink-0 {colorClass(
        compColor,
      )}"
      on:paste={handlePasteInComposer}
    >
      {#if !composerOpen}
        <div class="flex items-center justify-between px-4 py-2.5 gap-2">
          <button
            type="button"
            on:click={() => {
              compType = "note";
              composerOpen = true;
            }}
            class="flex-1 text-left text-xs text-slate-400 hover:text-slate-200 cursor-text py-1"
          >
            Take a note... (or press Ctrl+V to paste an image)
          </button>
          <div class="flex items-center gap-1">
            <button
              type="button"
              on:click={() => {
                compType = "checklist";
                composerOpen = true;
              }}
              class="p-1.5 rounded-lg text-slate-400 hover:text-cyan-300 hover:bg-slate-800/80 cursor-pointer"
              title="New checklist"
            >
              <CheckSquare class="w-4 h-4" />
            </button>
            <button
              type="button"
              on:click={() => {
                composerOpen = true;
                compFileInput?.click();
              }}
              class="p-1.5 rounded-lg text-slate-400 hover:text-cyan-300 hover:bg-slate-800/80 cursor-pointer"
              title="Add image"
            >
              <ImageIcon class="w-4 h-4" />
            </button>
          </div>
        </div>
      {:else}
        <div class="p-4 space-y-3">
          <!-- Images Preview -->
          {#if compImages.length > 0}
            <div class="grid grid-cols-2 sm:grid-cols-3 gap-2">
              {#each compImages as img, idx}
                <div class="relative group rounded-xl overflow-hidden bg-black/40 border border-slate-700/60">
                  <img src={img} alt="Note attachment" class="w-full h-28 object-cover" />
                  <button
                    type="button"
                    on:click={() => (compImages = compImages.filter((_, i) => i !== idx))}
                    class="absolute top-1.5 right-1.5 p-1 rounded-full bg-black/75 text-white opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
                  >
                    <X class="w-3 h-3" />
                  </button>
                </div>
              {/each}
            </div>
          {/if}

          <div class="flex items-center gap-2">
            <input
              type="text"
              bind:value={compTitle}
              placeholder="Title"
              class="flex-1 bg-transparent text-sm font-bold text-white placeholder-slate-500 focus:outline-none"
            />
            <button
              type="button"
              on:click={() => (compPinned = !compPinned)}
              class="p-1.5 rounded-lg text-slate-400 hover:text-amber-300 cursor-pointer"
              title={compPinned ? "Unpin note" : "Pin note"}
            >
              {#if compPinned}
                <Pin class="w-4 h-4 text-amber-400" />
              {:else}
                <Pin class="w-4 h-4" />
              {/if}
            </button>
          </div>

          {#if compType === "note"}
            <textarea
              bind:value={compContent}
              rows={3}
              placeholder="Take a note... (Ctrl+V works for images too)"
              class="w-full bg-transparent text-xs text-slate-200 placeholder-slate-500 focus:outline-none resize-none leading-relaxed"
            ></textarea>
          {:else}
            <div class="space-y-1.5">
              {#each compChecklist as entry (entry.id)}
                <div class="flex items-center gap-2 text-xs">
                  <button
                    type="button"
                    on:click={() =>
                      (compChecklist = compChecklist.map((c) =>
                        c.id === entry.id ? { ...c, checked: !c.checked } : c,
                      ))}
                    class="text-cyan-400 cursor-pointer"
                  >
                    {#if entry.checked}
                      <CheckSquare class="w-4 h-4 text-emerald-400" />
                    {:else}
                      <Square class="w-4 h-4 text-slate-400" />
                    {/if}
                  </button>
                  <input
                    type="text"
                    bind:value={entry.text}
                    class="flex-1 bg-transparent text-slate-200 focus:outline-none {entry.checked
                      ? 'line-through text-slate-500'
                      : ''}"
                  />
                  <button
                    type="button"
                    on:click={() =>
                      (compChecklist = compChecklist.filter((c) => c.id !== entry.id))}
                    class="text-slate-500 hover:text-rose-400 cursor-pointer"
                  >
                    <X class="w-3.5 h-3.5" />
                  </button>
                </div>
              {/each}
              <div class="flex items-center gap-2 pt-1">
                <Plus class="w-4 h-4 text-slate-500" />
                <input
                  type="text"
                  bind:value={compNewTaskText}
                  on:keydown={(e) => e.key === "Enter" && (e.preventDefault(), addComposerChecklistItem())}
                  placeholder="List item (press Enter to add)..."
                  class="flex-1 bg-transparent text-xs text-slate-200 placeholder-slate-500 focus:outline-none"
                />
                <button
                  type="button"
                  on:click={addComposerChecklistItem}
                  class="text-[11px] text-cyan-400 hover:underline font-semibold cursor-pointer"
                >
                  Add
                </button>
              </div>
            </div>
          {/if}

          <!-- Labels -->
          {#if compLabels.length > 0}
            <div class="flex flex-wrap gap-1.5">
              {#each compLabels as lbl}
                <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] bg-slate-800 text-slate-300 border border-slate-700">
                  #{lbl}
                  <button type="button" on:click={() => (compLabels = compLabels.filter((l) => l !== lbl))}>
                    <X class="w-2.5 h-2.5" />
                  </button>
                </span>
              {/each}
            </div>
          {/if}

          <!-- Composer Footer Toolbar -->
          <div class="flex items-center justify-between pt-2 border-t border-slate-800/70 gap-2 flex-wrap">
            <div class="flex items-center gap-1 relative">
              <button
                type="button"
                on:click={() => (compType = compType === "note" ? "checklist" : "note")}
                class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
                title={compType === "note" ? "Switch to checklist" : "Switch to text note"}
              >
                <CheckSquare class="w-4 h-4" />
              </button>
              <button
                type="button"
                on:click={() => compFileInput?.click()}
                class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
                title="Upload image"
              >
                <ImageIcon class="w-4 h-4" />
              </button>
              <button
                type="button"
                on:click={pasteOsImageToComposer}
                class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
                title="Paste image from OS clipboard"
              >
                <Clipboard class="w-4 h-4" />
              </button>
              <button
                type="button"
                on:click={() => (compColorPickerOpen = !compColorPickerOpen)}
                class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
                title="Background color"
              >
                <Palette class="w-4 h-4" />
              </button>

              {#if compColorPickerOpen}
                <div class="absolute bottom-9 left-0 z-30 flex items-center gap-1.5 p-2 rounded-xl bg-slate-900 border border-slate-700 shadow-xl">
                  {#each NOTE_COLORS as c}
                    <button
                      type="button"
                      on:click={() => {
                        compColor = c.id;
                        compColorPickerOpen = false;
                      }}
                      class="w-5 h-5 rounded-full border-2 cursor-pointer {compColor === c.id
                        ? 'border-white scale-110'
                        : 'border-slate-600'}"
                      style="background: {c.dot}"
                      title={c.label}
                    ></button>
                  {/each}
                </div>
              {/if}

              <div class="flex items-center gap-1 ml-1 bg-slate-950/60 border border-slate-800 rounded-lg px-2 py-0.5">
                <Tag class="w-3 h-3 text-slate-500" />
                <input
                  type="text"
                  bind:value={compLabelInput}
                  on:keydown={(e) => e.key === "Enter" && (e.preventDefault(), addComposerLabel())}
                  placeholder="Add label..."
                  class="w-20 bg-transparent text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none"
                />
              </div>
            </div>

            <div class="flex items-center gap-2">
              <button
                type="button"
                on:click={resetComposer}
                class="px-3 py-1 rounded-lg text-xs text-slate-400 hover:text-white cursor-pointer"
              >
                Cancel
              </button>
              <button
                type="button"
                on:click={saveComposerNote}
                class="px-3.5 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
              >
                Save Note
              </button>
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <input
    type="file"
    accept="image/*"
    multiple
    bind:this={compFileInput}
    on:change={handleFilePickComposer}
    class="hidden"
  />
  <input
    type="file"
    accept="image/*"
    multiple
    bind:this={editFileInput}
    on:change={handleFilePickEditor}
    class="hidden"
  />

  <!-- Notes Grid -->
  <div class="flex-1 overflow-y-auto pr-1 space-y-5 min-h-0">
    {#if loading}
      <div class="h-40 flex items-center justify-center text-xs text-slate-400">
        Loading notes...
      </div>
    {:else if filteredNotes.length === 0}
      <div
        class="h-56 flex flex-col items-center justify-center text-center bg-slate-900/40 border border-dashed border-slate-800 rounded-2xl p-6"
      >
        <StickyNote class="w-8 h-8 text-slate-600 mb-2" />
        <p class="text-sm font-semibold text-slate-300">No notes here</p>
        <p class="text-xs text-slate-500 mt-1">
          Create a note, paste a screenshot (Ctrl+V), or build a task checklist above.
        </p>
      </div>
    {:else}
      {#if pinnedNotes.length > 0 && viewFilter !== "archived"}
        <div class="space-y-2">
          <div class="text-[10px] font-bold uppercase tracking-wider text-slate-400 px-1">
            Pinned
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {#each pinnedNotes as note (note.id)}
              <!-- svelte-ignore a11y-click-events-have-key-events -->
              <!-- svelte-ignore a11y-no-static-element-interactions -->
              <div
                on:click={() => (editingNote = JSON.parse(JSON.stringify(note)))}
                class="group rounded-2xl border p-3.5 flex flex-col justify-between gap-2.5 cursor-pointer hover:shadow-lg transition-all {colorClass(
                  note.color,
                )}"
              >
                <div class="space-y-2">
                  {#if note.images.length > 0}
                    <div class="grid {note.images.length > 1 ? 'grid-cols-2' : 'grid-cols-1'} gap-1.5">
                      {#each note.images.slice(0, 4) as img}
                        <img
                          src={img}
                          alt="Note snippet"
                          on:click|stopPropagation={() => (lightboxImage = img)}
                          class="w-full h-28 object-cover rounded-xl bg-black/40 border border-slate-800/60"
                        />
                      {/each}
                    </div>
                  {/if}
                  {#if note.title}
                    <h3 class="text-sm font-bold text-white leading-snug">{note.title}</h3>
                  {/if}
                  {#if note.content}
                    <p class="text-xs text-slate-300 whitespace-pre-wrap break-words line-clamp-6 leading-relaxed">
                      {note.content}
                    </p>
                  {/if}
                  {#if note.checklist.length > 0}
                    <div class="space-y-1">
                      {#each note.checklist.slice(0, 6) as item (item.id)}
                        <div
                          on:click|stopPropagation={() => toggleCardChecklistItem(note, item.id)}
                          class="flex items-start gap-2 text-xs hover:bg-white/5 rounded-md px-1 py-0.5"
                        >
                          {#if item.checked}
                            <CheckSquare class="w-3.5 h-3.5 text-emerald-400 shrink-0 mt-0.5" />
                            <span class="line-through text-slate-500 break-words">{item.text}</span>
                          {:else}
                            <Square class="w-3.5 h-3.5 text-slate-400 shrink-0 mt-0.5" />
                            <span class="text-slate-200 break-words">{item.text}</span>
                          {/if}
                        </div>
                      {/each}
                      {#if note.checklist.length > 6}
                        <p class="text-[10px] text-slate-400 pl-1">
                          +{note.checklist.length - 6} more items
                        </p>
                      {/if}
                    </div>
                  {/if}
                  {#if note.labels.length > 0}
                    <div class="flex flex-wrap gap-1 pt-1">
                      {#each note.labels as lbl}
                        <span class="px-2 py-0.5 rounded-full text-[10px] bg-slate-950/60 text-slate-300 border border-slate-700/70">
                          #{lbl}
                        </span>
                      {/each}
                    </div>
                  {/if}
                </div>

                <div class="flex items-center justify-between pt-2 border-t border-white/5 opacity-80 group-hover:opacity-100">
                  <span class="text-[10px] text-slate-500 font-mono">
                    {#if note.checklist.length > 0}
                      {note.checklist.filter((c) => c.checked).length}/{note.checklist.length} done
                    {:else}
                      Note
                    {/if}
                  </span>
                  <div class="flex items-center gap-1" on:click|stopPropagation>
                    <button
                      type="button"
                      on:click={() => handleTogglePin(note)}
                      class="p-1 rounded-lg text-amber-400 hover:bg-amber-500/15 cursor-pointer"
                      title="Unpin"
                    >
                      <PinOff class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      on:click={() => handleToggleArchive(note)}
                      class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 cursor-pointer"
                      title="Archive"
                    >
                      <Archive class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      on:click={() => handleDeleteNote(note.id)}
                      class="p-1 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 cursor-pointer"
                      title="Delete"
                    >
                      <Trash2 class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if otherNotes.length > 0}
        <div class="space-y-2">
          {#if pinnedNotes.length > 0 && viewFilter !== "archived"}
            <div class="text-[10px] font-bold uppercase tracking-wider text-slate-400 px-1">
              Others
            </div>
          {/if}
          <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
            {#each otherNotes as note (note.id)}
              <!-- svelte-ignore a11y-click-events-have-key-events -->
              <!-- svelte-ignore a11y-no-static-element-interactions -->
              <div
                on:click={() => (editingNote = JSON.parse(JSON.stringify(note)))}
                class="group rounded-2xl border p-3.5 flex flex-col justify-between gap-2.5 cursor-pointer hover:shadow-lg transition-all {colorClass(
                  note.color,
                )}"
              >
                <div class="space-y-2">
                  {#if note.images.length > 0}
                    <div class="grid {note.images.length > 1 ? 'grid-cols-2' : 'grid-cols-1'} gap-1.5">
                      {#each note.images.slice(0, 4) as img}
                        <img
                          src={img}
                          alt="Note snippet"
                          on:click|stopPropagation={() => (lightboxImage = img)}
                          class="w-full h-28 object-cover rounded-xl bg-black/40 border border-slate-800/60"
                        />
                      {/each}
                    </div>
                  {/if}
                  {#if note.title}
                    <h3 class="text-sm font-bold text-white leading-snug">{note.title}</h3>
                  {/if}
                  {#if note.content}
                    <p class="text-xs text-slate-300 whitespace-pre-wrap break-words line-clamp-6 leading-relaxed">
                      {note.content}
                    </p>
                  {/if}
                  {#if note.checklist.length > 0}
                    <div class="space-y-1">
                      {#each note.checklist.slice(0, 6) as item (item.id)}
                        <div
                          on:click|stopPropagation={() => toggleCardChecklistItem(note, item.id)}
                          class="flex items-start gap-2 text-xs hover:bg-white/5 rounded-md px-1 py-0.5"
                        >
                          {#if item.checked}
                            <CheckSquare class="w-3.5 h-3.5 text-emerald-400 shrink-0 mt-0.5" />
                            <span class="line-through text-slate-500 break-words">{item.text}</span>
                          {:else}
                            <Square class="w-3.5 h-3.5 text-slate-400 shrink-0 mt-0.5" />
                            <span class="text-slate-200 break-words">{item.text}</span>
                          {/if}
                        </div>
                      {/each}
                      {#if note.checklist.length > 6}
                        <p class="text-[10px] text-slate-400 pl-1">
                          +{note.checklist.length - 6} more items
                        </p>
                      {/if}
                    </div>
                  {/if}
                  {#if note.labels.length > 0}
                    <div class="flex flex-wrap gap-1 pt-1">
                      {#each note.labels as lbl}
                        <span class="px-2 py-0.5 rounded-full text-[10px] bg-slate-950/60 text-slate-300 border border-slate-700/70">
                          #{lbl}
                        </span>
                      {/each}
                    </div>
                  {/if}
                </div>

                <div class="flex items-center justify-between pt-2 border-t border-white/5 opacity-80 group-hover:opacity-100">
                  <span class="text-[10px] text-slate-500 font-mono">
                    {#if note.checklist.length > 0}
                      {note.checklist.filter((c) => c.checked).length}/{note.checklist.length} done
                    {:else}
                      Note
                    {/if}
                  </span>
                  <div class="flex items-center gap-1" on:click|stopPropagation>
                    {#if !note.archived}
                      <button
                        type="button"
                        on:click={() => handleTogglePin(note)}
                        class="p-1 rounded-lg text-slate-400 hover:text-amber-300 hover:bg-amber-500/15 cursor-pointer"
                        title="Pin"
                      >
                        <Pin class="w-3.5 h-3.5" />
                      </button>
                    {/if}
                    <button
                      type="button"
                      on:click={() => handleToggleArchive(note)}
                      class="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 cursor-pointer"
                      title={note.archived ? "Unarchive" : "Archive"}
                    >
                      {#if note.archived}
                        <ArchiveRestore class="w-3.5 h-3.5" />
                      {:else}
                        <Archive class="w-3.5 h-3.5" />
                      {/if}
                    </button>
                    <button
                      type="button"
                      on:click={() => handleDeleteNote(note.id)}
                      class="p-1 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 cursor-pointer"
                      title="Delete"
                    >
                      <Trash2 class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<!-- Note Editor Modal -->
{#if editingNote}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 z-50 bg-black/75 backdrop-blur-sm flex items-center justify-center p-4"
    on:click={saveEditingNote}
  >
    <div
      class="w-full max-w-xl max-h-[90vh] overflow-y-auto rounded-2xl border p-5 space-y-4 shadow-2xl {colorClass(
        editingNote.color,
      )}"
      on:click|stopPropagation
      on:paste={handlePasteInEditor}
    >
      {#if editingNote.images.length > 0}
        <div class="grid grid-cols-2 gap-2">
          {#each editingNote.images as img, idx}
            <div class="relative group rounded-xl overflow-hidden bg-black/40 border border-slate-700/60">
              <img
                src={img}
                alt="Note attachment"
                on:click={() => (lightboxImage = img)}
                class="w-full max-h-48 object-cover cursor-pointer"
              />
              <button
                type="button"
                on:click={() => {
                  if (!editingNote) return;
                  editingNote = {
                    ...editingNote,
                    images: editingNote.images.filter((_, i) => i !== idx),
                  };
                }}
                class="absolute top-1.5 right-1.5 p-1 rounded-full bg-black/75 text-white opacity-0 group-hover:opacity-100 cursor-pointer"
              >
                <X class="w-3.5 h-3.5" />
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="flex items-center gap-2">
        <input
          type="text"
          bind:value={editingNote.title}
          placeholder="Title"
          class="flex-1 bg-transparent text-base font-bold text-white placeholder-slate-500 focus:outline-none"
        />
        <button
          type="button"
          on:click={() => {
            if (editingNote) editingNote = { ...editingNote, pinned: !editingNote.pinned };
          }}
          class="p-1.5 rounded-lg text-slate-400 hover:text-amber-300 cursor-pointer"
          title={editingNote.pinned ? "Unpin" : "Pin"}
        >
          {#if editingNote.pinned}
            <Pin class="w-4 h-4 text-amber-400" />
          {:else}
            <Pin class="w-4 h-4" />
          {/if}
        </button>
      </div>

      <textarea
        bind:value={editingNote.content}
        rows={4}
        placeholder="Note text... (Ctrl+V to paste image)"
        class="w-full bg-transparent text-xs text-slate-200 placeholder-slate-500 focus:outline-none resize-y leading-relaxed"
      ></textarea>

      <!-- Checklist Editor -->
      <div class="space-y-1.5 border-t border-white/10 pt-3">
        {#each editingNote.checklist as entry (entry.id)}
          <div class="flex items-center gap-2 text-xs">
            <button
              type="button"
              on:click={() => {
                if (!editingNote) return;
                editingNote = {
                  ...editingNote,
                  checklist: editingNote.checklist.map((c) =>
                    c.id === entry.id ? { ...c, checked: !c.checked } : c,
                  ),
                };
              }}
              class="cursor-pointer"
            >
              {#if entry.checked}
                <CheckSquare class="w-4 h-4 text-emerald-400" />
              {:else}
                <Square class="w-4 h-4 text-slate-400" />
              {/if}
            </button>
            <input
              type="text"
              bind:value={entry.text}
              class="flex-1 bg-transparent text-slate-200 focus:outline-none {entry.checked
                ? 'line-through text-slate-500'
                : ''}"
            />
            <button
              type="button"
              on:click={() => {
                if (!editingNote) return;
                editingNote = {
                  ...editingNote,
                  checklist: editingNote.checklist.filter((c) => c.id !== entry.id),
                };
              }}
              class="text-slate-500 hover:text-rose-400 cursor-pointer"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
        {/each}
        <div class="flex items-center gap-2 pt-1">
          <Plus class="w-4 h-4 text-slate-500" />
          <input
            type="text"
            bind:value={editNewTaskText}
            on:keydown={(e) => e.key === "Enter" && (e.preventDefault(), addEditorTask())}
            placeholder="Add checklist item..."
            class="flex-1 bg-transparent text-xs text-slate-200 placeholder-slate-500 focus:outline-none"
          />
          <button
            type="button"
            on:click={addEditorTask}
            class="text-[11px] text-cyan-400 hover:underline font-semibold cursor-pointer"
          >
            Add
          </button>
        </div>
      </div>

      <!-- Labels -->
      {#if editingNote.labels.length > 0}
        <div class="flex flex-wrap gap-1.5">
          {#each editingNote.labels as lbl}
            <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] bg-slate-950/70 text-slate-300 border border-slate-700">
              #{lbl}
              <button
                type="button"
                on:click={() => {
                  if (editingNote) {
                    editingNote = {
                      ...editingNote,
                      labels: editingNote.labels.filter((l) => l !== lbl),
                    };
                  }
                }}
              >
                <X class="w-2.5 h-2.5" />
              </button>
            </span>
          {/each}
        </div>
      {/if}

      <div class="flex items-center justify-between pt-3 border-t border-white/10 gap-2 flex-wrap">
        <div class="flex items-center gap-1 relative">
          <button
            type="button"
            on:click={() => editFileInput?.click()}
            class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
            title="Upload image"
          >
            <ImageIcon class="w-4 h-4" />
          </button>
          <button
            type="button"
            on:click={pasteOsImageToEditor}
            class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
            title="Paste image from OS clipboard"
          >
            <Clipboard class="w-4 h-4" />
          </button>
          <button
            type="button"
            on:click={() => (editColorPickerOpen = !editColorPickerOpen)}
            class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/80 cursor-pointer"
            title="Color"
          >
            <Palette class="w-4 h-4" />
          </button>
          {#if editColorPickerOpen}
            <div class="absolute bottom-9 left-0 z-30 flex items-center gap-1.5 p-2 rounded-xl bg-slate-900 border border-slate-700 shadow-xl">
              {#each NOTE_COLORS as c}
                <button
                  type="button"
                  on:click={() => {
                    if (editingNote) editingNote = { ...editingNote, color: c.id };
                    editColorPickerOpen = false;
                  }}
                  class="w-5 h-5 rounded-full border-2 cursor-pointer {editingNote.color === c.id
                    ? 'border-white scale-110'
                    : 'border-slate-600'}"
                  style="background: {c.dot}"
                  title={c.label}
                ></button>
              {/each}
            </div>
          {/if}
          <div class="flex items-center gap-1 ml-1 bg-slate-950/60 border border-slate-800 rounded-lg px-2 py-0.5">
            <Tag class="w-3 h-3 text-slate-500" />
            <input
              type="text"
              bind:value={editLabelInput}
              on:keydown={(e) => e.key === "Enter" && (e.preventDefault(), addEditorLabel())}
              placeholder="Add label..."
              class="w-20 bg-transparent text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none"
            />
          </div>
        </div>

        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={() => editingNote && handleDeleteNote(editingNote.id)}
            class="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-500/15 cursor-pointer"
            title="Delete note"
          >
            <Trash2 class="w-4 h-4" />
          </button>
          <button
            type="button"
            on:click={saveEditingNote}
            class="px-4 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold cursor-pointer"
          >
            Done & Save
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Image Lightbox -->
{#if lightboxImage}
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div
    class="fixed inset-0 z-[60] bg-black/90 backdrop-blur-md flex items-center justify-center p-4"
    on:click={() => (lightboxImage = null)}
  >
    <img src={lightboxImage} alt="Preview" class="max-w-full max-h-[90vh] object-contain rounded-xl" />
  </div>
{/if}
