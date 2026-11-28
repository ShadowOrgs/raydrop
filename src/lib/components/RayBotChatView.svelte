<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { marked } from "marked";
  import { raydropApi, type AiMcpConfig } from "../utils/tauri-ipc";
  import {
    Bot,
    Plus,
    Send,
    Paperclip,
    Image as ImageIcon,
    Mic,
    Square,
    Trash2,
    Edit3,
    Check,
    X,
    Loader2,
    Search,
    Music,
    Sparkles,
    FileText,
    MessageSquare,
    FolderOpen,
    RefreshCw,
    FolderGit2,
  } from "lucide-svelte";

  interface RaybotAttachment {
    kind: "image" | "file" | "voice";
    name: string;
    path: string;
    mime: string;
    data_url: string;
    size_bytes: number;
  }

  interface RaybotSession {
    id: string;
    title: string;
    created_at: number;
    updated_at: number;
    last_message: string;
  }

  interface RaybotMessage {
    id: string;
    session_id: string;
    role: "user" | "assistant";
    content: string;
    attachments: RaybotAttachment[];
    created_at: number;
  }

  interface RaybotSendResult {
    user_message: RaybotMessage;
    assistant_message: RaybotMessage;
    session: RaybotSession;
  }

  interface ProjectEntry {
    name: string;
    path: string;
    tech_stack: string;
    summary: string;
    pending_tasks: string[];
    is_active: boolean;
    updated_at: number;
  }

  interface ProjectsOverview {
    root_dir: string;
    active_project: string | null;
    projects: ProjectEntry[];
    relations: Array<{
      source_project: string;
      target_project: string;
      relation_type: string;
      description: string;
    }>;
  }

  let projectsOverview: ProjectsOverview | null = null;
  let scanningProjects = false;

  async function loadProjectsOverview() {
    try {
      projectsOverview = await invoke<ProjectsOverview>("raybot_get_projects_overview");
    } catch (e) {
      console.error("Failed to load projects overview:", e);
    }
  }

  async function pickProjectsRoot() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: projectsOverview?.root_dir || undefined,
      });
      if (typeof selected === "string" && selected.trim()) {
        scanningProjects = true;
        projectsOverview = await invoke<ProjectsOverview>("raybot_set_projects_root", {
          rootDir: selected,
        });
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      scanningProjects = false;
    }
  }

  async function scanProjects() {
    scanningProjects = true;
    try {
      projectsOverview = await invoke<ProjectsOverview>("raybot_scan_projects");
    } catch (e) {
      errorMsg = String(e);
    } finally {
      scanningProjects = false;
    }
  }

  async function selectActiveProject(name: string) {
    if (!name) return;
    try {
      projectsOverview = await invoke<ProjectsOverview>("raybot_select_project", { name });
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function handleProjectSelectChange(e: Event) {
    const target = e.currentTarget as HTMLSelectElement | null;
    if (target?.value) {
      void selectActiveProject(target.value);
    }
  }

  let sessions: RaybotSession[] = [];
  let activeSessionId: string | null = null;
  let messages: RaybotMessage[] = [];
  let searchQuery = "";
  let loadingSessions = true;
  let loadingMessages = false;
  let sending = false;
  let errorMsg = "";

  let personaName = "RayBot";
  let personaAvatar = "";
  let personaModel = "";

  function applyAiConfig(cfg: Partial<AiMcpConfig> | null | undefined) {
    if (!cfg) return;
    personaName = (cfg.persona_name || "RayBot").trim() || "RayBot";
    personaAvatar = cfg.persona_avatar || "";
    personaModel = cfg.llm_model || "";
  }

  let inputText = "";
  let pendingAttachments: RaybotAttachment[] = [];
  let chatScrollEl: HTMLDivElement | null = null;
  let lightboxImage: string | null = null;

  // Rename state
  let renamingId: string | null = null;
  let renameValue = "";

  // Voice recording state
  let recording = false;
  let mediaRecorder: MediaRecorder | null = null;
  let voiceChunks: Blob[] = [];
  let speechRec: any = null;
  let recordSecs = 0;
  let recordTimer: any = null;

  const QUICK_PROMPTS = [
    "Putar lagu lo-fi chill di RayAmp",
    "Buat playlist 'Coding' dan tambahkan 3 lagu instrumental",
    "Tambahkan lagu ke queue dan tampilkan isi queue",
    "Cek status CPU, RAM & device yang online",
  ];

  async function loadSessions(selectFirstIfNone = true) {
    if (!projectsOverview) {
      void loadProjectsOverview();
    }
    loadingSessions = true;
    try {
      sessions = await invoke<RaybotSession[]>("raybot_list_sessions");
      if (selectFirstIfNone && !activeSessionId) {
        if (sessions.length > 0) {
          await selectSession(sessions[0].id);
        } else {
          await createNewChat();
        }
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loadingSessions = false;
    }
  }

  onMount(() => {
    loadSessions(true);
    raydropApi.getAiMcpConfig().then(applyAiConfig).catch(() => {});
    const onCfgUpdated = (e: Event) => {
      applyAiConfig((e as CustomEvent<AiMcpConfig>).detail);
    };
    window.addEventListener("raybot-config-updated", onCfgUpdated);
    return () => {
      window.removeEventListener("raybot-config-updated", onCfgUpdated);
      if (recordTimer) clearInterval(recordTimer);
    };
  });

  async function selectSession(id: string) {
    activeSessionId = id;
    loadingMessages = true;
    errorMsg = "";
    try {
      messages = await invoke<RaybotMessage[]>("raybot_get_messages", {
        sessionId: id,
      });
      await scrollToBottom();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loadingMessages = false;
    }
  }

  async function createNewChat() {
    errorMsg = "";
    try {
      const s = await invoke<RaybotSession>("raybot_create_session", {
        title: "New Chat",
      });
      sessions = [s, ...sessions];
      activeSessionId = s.id;
      messages = [];
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function deleteChat(id: string) {
    try {
      await invoke("raybot_delete_session", { sessionId: id });
      sessions = sessions.filter((s) => s.id !== id);
      if (activeSessionId === id) {
        activeSessionId = null;
        messages = [];
        if (sessions.length > 0) {
          await selectSession(sessions[0].id);
        } else {
          await createNewChat();
        }
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function startRename(s: RaybotSession) {
    renamingId = s.id;
    renameValue = s.title;
  }

  async function submitRename(id: string) {
    if (!renameValue.trim()) {
      renamingId = null;
      return;
    }
    try {
      await invoke("raybot_rename_session", {
        sessionId: id,
        title: renameValue.trim(),
      });
      sessions = sessions.map((s) =>
        s.id === id ? { ...s, title: renameValue.trim() } : s,
      );
    } catch (e) {
      errorMsg = String(e);
    } finally {
      renamingId = null;
    }
  }

  async function scrollToBottom() {
    await tick();
    if (chatScrollEl) {
      chatScrollEl.scrollTop = chatScrollEl.scrollHeight;
    }
  }

  async function handleAttachFile() {
    try {
      const picked = await open({
        multiple: true,
        title: "Attach file(s) for RayBot",
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      for (const p of paths) {
        const name = p.split(/[\\/]/).pop() || "file";
        const lower = name.toLowerCase();
        const isImg =
          lower.endsWith(".png") ||
          lower.endsWith(".jpg") ||
          lower.endsWith(".jpeg") ||
          lower.endsWith(".webp") ||
          lower.endsWith(".gif");
        pendingAttachments = [
          ...pendingAttachments,
          {
            kind: isImg ? "image" : "file",
            name,
            path: p,
            mime: isImg ? "image/*" : "application/octet-stream",
            data_url: "",
            size_bytes: 0,
          },
        ];
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function readBlobAsDataUrl(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || ""));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
  }

  async function handleImageInputChange(e: Event) {
    const input = e.target as HTMLInputElement;
    if (!input.files) return;
    for (const file of Array.from(input.files)) {
      const dataUrl = await readBlobAsDataUrl(file);
      pendingAttachments = [
        ...pendingAttachments,
        {
          kind: "image",
          name: file.name,
          path: "",
          mime: file.type || "image/png",
          data_url: dataUrl,
          size_bytes: file.size,
        },
      ];
    }
    input.value = "";
  }

  async function handlePaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of Array.from(items)) {
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const blob = item.getAsFile();
        if (!blob) continue;
        const dataUrl = await readBlobAsDataUrl(blob);
        pendingAttachments = [
          ...pendingAttachments,
          {
            kind: "image",
            name: `pasted-${Date.now()}.png`,
            path: "",
            mime: blob.type || "image/png",
            data_url: dataUrl,
            size_bytes: blob.size,
          },
        ];
      }
    }
  }

  async function toggleVoiceRecord() {
    if (recording) {
      mediaRecorder?.stop();
      try {
        speechRec?.stop();
      } catch {}
      recording = false;
      if (recordTimer) {
        clearInterval(recordTimer);
        recordTimer = null;
      }
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      voiceChunks = [];
      mediaRecorder = new MediaRecorder(stream);
      mediaRecorder.ondataavailable = (ev) => {
        if (ev.data.size > 0) voiceChunks.push(ev.data);
      };
      mediaRecorder.onstop = async () => {
        stream.getTracks().forEach((t) => t.stop());
        const blob = new Blob(voiceChunks, { type: "audio/webm" });
        const dataUrl = await readBlobAsDataUrl(blob);
        pendingAttachments = [
          ...pendingAttachments,
          {
            kind: "voice",
            name: `voice-${recordSecs}s.webm`,
            path: "",
            mime: "audio/webm",
            data_url: dataUrl,
            size_bytes: blob.size,
          },
        ];
        recordSecs = 0;
      };
      mediaRecorder.start();
      recording = true;
      recordSecs = 0;
      recordTimer = setInterval(() => {
        recordSecs += 1;
      }, 1000);

      // Optional live speech-to-text if supported by WebView
      const SpeechRecognitionCtor =
        (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
      if (SpeechRecognitionCtor) {
        speechRec = new SpeechRecognitionCtor();
        speechRec.continuous = true;
        speechRec.interimResults = false;
        speechRec.lang = "id-ID";
        speechRec.onresult = (ev: any) => {
          for (let i = ev.resultIndex; i < ev.results.length; i++) {
            if (ev.results[i].isFinal) {
              const transcript = ev.results[i][0].transcript.trim();
              if (transcript) {
                inputText = inputText ? `${inputText} ${transcript}` : transcript;
              }
            }
          }
        };
        try {
          speechRec.start();
        } catch {}
      }
    } catch (e) {
      errorMsg = `Microphone unavailable: ${e}`;
    }
  }

  function removeAttachment(idx: number) {
    pendingAttachments = pendingAttachments.filter((_, i) => i !== idx);
  }

  async function sendMessage(overrideText?: string) {
    const text = (overrideText ?? inputText).trim();
    if (!text && pendingAttachments.length === 0) return;
    if (sending) return;

    if (!activeSessionId) {
      await createNewChat();
    }
    if (!activeSessionId) return;

    const atts = [...pendingAttachments];
    if (!overrideText) {
      inputText = "";
      pendingAttachments = [];
    }

    // Optimistic user bubble
    const tempId = `temp-${Date.now()}`;
    messages = [
      ...messages,
      {
        id: tempId,
        session_id: activeSessionId,
        role: "user",
        content: text,
        attachments: atts,
        created_at: Math.floor(Date.now() / 1000),
      },
    ];
    await scrollToBottom();

    sending = true;
    errorMsg = "";
    try {
      const res = await invoke<RaybotSendResult>("raybot_send_message", {
        sessionId: activeSessionId,
        text,
        attachments: atts,
      });
      messages = [
        ...messages.filter((m) => m.id !== tempId),
        res.user_message,
        res.assistant_message,
      ];
      sessions = [
        res.session,
        ...sessions.filter((s) => s.id !== res.session.id),
      ];
      await scrollToBottom();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      sending = false;
    }
  }

  function renderMarkdown(md: string): string {
    try {
      return marked.parse(md || "", { async: false }) as string;
    } catch {
      return md || "";
    }
  }

  function formatTime(ts: number): string {
    if (!ts) return "";
    return new Date(ts * 1000).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  $: filteredSessions = sessions.filter((s) => {
    const q = searchQuery.trim().toLowerCase();
    return (
      !q ||
      s.title.toLowerCase().includes(q) ||
      s.last_message.toLowerCase().includes(q)
    );
  });

  $: activeSession = sessions.find((s) => s.id === activeSessionId) || null;
</script>

<div class="h-full flex bg-slate-950 text-slate-100 overflow-hidden">
  <!-- Left Sidebar: Persistent Chat List -->
  <div class="w-64 sm:w-72 border-r border-slate-800/80 bg-slate-900/40 flex flex-col shrink-0">
    <div class="p-3.5 border-b border-slate-800/80 space-y-2.5">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <div class="w-8 h-8 rounded-full overflow-hidden bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shrink-0">
            {#if personaAvatar}
              <img src={personaAvatar} alt={personaName} class="w-full h-full object-cover" />
            {:else}
              <Bot class="w-4 h-4" />
            {/if}
          </div>
          <div class="min-w-0">
            <div class="text-xs font-bold text-white truncate">{personaName}</div>
            <div class="text-[10px] text-slate-400 truncate">
              {personaModel ? personaModel : "MCP + RayAmp + Vision"}
            </div>
          </div>
        </div>
        <button
          type="button"
          on:click={createNewChat}
          title="New Chat"
          class="px-2.5 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center gap-1 transition active:scale-[0.97] cursor-pointer"
        >
          <Plus class="w-3.5 h-3.5" />
          New
        </button>
      </div>
      <div class="relative">
        <Search class="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
        <input
          type="text"
          bind:value={searchQuery}
          placeholder="Search chats..."
          class="w-full pl-8 pr-3 py-1.5 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
        />
      </div>
    </div>

    <div class="flex-1 overflow-y-auto p-2 space-y-1">
      {#if loadingSessions}
        <div class="flex items-center justify-center py-8 text-xs text-slate-500 gap-2">
          <Loader2 class="w-4 h-4 animate-spin" />
          Loading...
        </div>
      {:else}
        {#each filteredSessions as s (s.id)}
          <div
            class="group rounded-xl p-2.5 transition border {activeSessionId === s.id
              ? 'bg-cyan-500/15 border-cyan-500/35'
              : 'bg-transparent border-transparent hover:bg-slate-800/50'}"
          >
            {#if renamingId === s.id}
              <div class="flex items-center gap-1">
                <input
                  type="text"
                  bind:value={renameValue}
                  on:keydown={(e) => e.key === "Enter" && submitRename(s.id)}
                  class="flex-1 px-2 py-1 rounded bg-slate-950 border border-cyan-500 text-xs text-white"
                />
                <button
                  type="button"
                  on:click={() => submitRename(s.id)}
                  class="p-1 text-emerald-400 hover:text-emerald-300 cursor-pointer"
                >
                  <Check class="w-3.5 h-3.5" />
                </button>
              </div>
            {:else}
              <div class="flex items-start justify-between gap-1.5">
                <button
                  type="button"
                  on:click={() => selectSession(s.id)}
                  class="flex-1 text-left min-w-0 cursor-pointer"
                >
                  <div class="text-xs font-semibold text-white truncate flex items-center gap-1.5">
                    <MessageSquare class="w-3 h-3 text-cyan-400 shrink-0" />
                    <span class="truncate">{s.title}</span>
                  </div>
                  {#if s.last_message}
                    <div class="text-[11px] text-slate-400 truncate mt-0.5">{s.last_message}</div>
                  {/if}
                </button>
                <div class="hidden group-hover:flex items-center gap-0.5 shrink-0">
                  <button
                    type="button"
                    on:click={() => startRename(s)}
                    class="p-1 rounded hover:bg-slate-700 text-slate-400 hover:text-white transition active:scale-95 cursor-pointer"
                    title="Rename"
                  >
                    <Edit3 class="w-3 h-3" />
                  </button>
                  <button
                    type="button"
                    on:click={() => deleteChat(s.id)}
                    class="p-1 rounded hover:bg-rose-500/20 text-slate-400 hover:text-rose-300 transition active:scale-95 cursor-pointer"
                    title="Delete"
                  >
                    <Trash2 class="w-3 h-3" />
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- Main Chat Area -->
  <div class="flex-1 flex flex-col min-w-0">
    <!-- Top Header -->
    <div class="px-5 py-3 border-b border-slate-800/80 bg-slate-900/30 flex flex-wrap items-center justify-between gap-3">
      <div class="min-w-0">
        <h2 class="text-sm font-bold text-white truncate">
          {activeSession ? activeSession.title : "RayBot Assistant"}
        </h2>
        <div class="text-[11px] text-slate-400 flex items-center gap-2">
          <span class="inline-flex items-center gap-1 text-emerald-400">
            <Music class="w-3 h-3" /> RayAmp Queue &amp; Playlists Ready
          </span>
          <span>•</span>
          <span>Attach files, paste images (Ctrl+V), or record voice</span>
        </div>
      </div>

      <div class="flex items-center gap-1.5 flex-wrap">
        <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-slate-950/80 border border-slate-800 text-xs">
          <FolderGit2 class="w-3.5 h-3.5 text-cyan-400 shrink-0" />
          <select
            value={projectsOverview?.active_project || ""}
            on:change={handleProjectSelectChange}
            class="bg-transparent text-slate-200 text-xs outline-none cursor-pointer max-w-[160px]"
            title={projectsOverview ? `Root: ${projectsOverview.root_dir}` : "Coding Project"}
          >
            <option value="" class="bg-slate-900 text-slate-400">
              {projectsOverview?.projects.length ? "Select project..." : "No projects scanned"}
            </option>
            {#each projectsOverview?.projects || [] as p}
              <option value={p.name} class="bg-slate-900 text-slate-100">
                {p.name}{p.tech_stack ? ` (${p.tech_stack})` : ""}
              </option>
            {/each}
          </select>
        </div>

        <button
          type="button"
          on:click={pickProjectsRoot}
          disabled={scanningProjects}
          class="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-800/80 hover:bg-slate-700 text-slate-200 text-xs transition active:scale-[0.97] disabled:opacity-50 cursor-pointer"
          title={projectsOverview ? `Projects Root: ${projectsOverview.root_dir}` : "Set Projects Root Folder"}
        >
          <FolderOpen class="w-3.5 h-3.5 text-amber-400" />
          <span>Root</span>
        </button>

        <button
          type="button"
          on:click={scanProjects}
          disabled={scanningProjects}
          class="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-800/80 hover:bg-slate-700 text-slate-200 text-xs transition active:scale-[0.97] disabled:opacity-50 cursor-pointer"
          title="Scan Projects Root"
        >
          {#if scanningProjects}
            <Loader2 class="w-3.5 h-3.5 animate-spin text-cyan-400" />
          {:else}
            <RefreshCw class="w-3.5 h-3.5 text-cyan-400" />
          {/if}
          <span>Scan</span>
        </button>
      </div>
    </div>

    {#if errorMsg}
      <div class="mx-5 mt-3 px-3.5 py-2 rounded-xl bg-rose-500/15 border border-rose-500/30 text-rose-300 text-xs flex items-center justify-between">
        <span>{errorMsg}</span>
        <button type="button" on:click={() => (errorMsg = "")} class="text-rose-200 hover:text-white cursor-pointer">
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    {/if}

    <!-- Messages Feed -->
    <div bind:this={chatScrollEl} class="flex-1 overflow-y-auto p-5 space-y-4">
      {#if loadingMessages}
        <div class="flex items-center justify-center py-20 text-slate-400 text-xs gap-2">
          <Loader2 class="w-4 h-4 animate-spin text-cyan-400" />
          Loading conversation...
        </div>
      {:else if messages.length === 0}
        <div class="max-w-xl mx-auto mt-10 text-center space-y-4">
          <div class="w-14 h-14 mx-auto rounded-full overflow-hidden bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400">
            {#if personaAvatar}
              <img src={personaAvatar} alt={personaName} class="w-full h-full object-cover" />
            {:else}
              <Sparkles class="w-6 h-6" />
            {/if}
          </div>
          <div>
            <h3 class="text-base font-bold text-white">Chat with {personaName}</h3>
            <p class="text-xs text-slate-400 mt-1">
              Control RayAmp music queues &amp; playlists, inspect attached files/images, or use any MCP server you installed.
            </p>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-2 text-left">
            {#each QUICK_PROMPTS as qp}
              <button
                type="button"
                on:click={() => sendMessage(qp)}
                class="p-3 rounded-xl bg-slate-900/70 hover:bg-slate-800 border border-slate-800 text-xs text-slate-200 transition active:scale-[0.98] cursor-pointer"
              >
                {qp}
              </button>
            {/each}
          </div>
        </div>
      {:else}
        {#each messages as msg (msg.id)}
          <div class="flex {msg.role === 'user' ? 'justify-end' : 'justify-start items-start gap-2.5'}">
            {#if msg.role === "assistant"}
              <div class="w-7 h-7 rounded-full overflow-hidden bg-cyan-500/15 border border-cyan-500/30 flex items-center justify-center text-cyan-400 shrink-0 mt-0.5">
                {#if personaAvatar}
                  <img src={personaAvatar} alt={personaName} class="w-full h-full object-cover" />
                {:else}
                  <Bot class="w-3.5 h-3.5" />
                {/if}
              </div>
            {/if}
            <div
              class="max-w-[80%] rounded-2xl px-4 py-3 text-xs space-y-2 {msg.role === 'user'
                ? 'bg-cyan-600/25 border border-cyan-500/35 text-white'
                : 'bg-slate-900/90 border border-slate-800 text-slate-100'}"
            >
              {#if msg.role === "assistant"}
                <div class="text-[11px] font-bold text-cyan-300">{personaName}</div>
              {/if}
              {#if msg.attachments && msg.attachments.length > 0}
                <div class="space-y-2">
                  {#each msg.attachments as att}
                    {#if att.kind === "image" && att.data_url}
                      <button
                        type="button"
                        on:click={() => (lightboxImage = att.data_url)}
                        class="block rounded-xl overflow-hidden border border-slate-700 max-w-xs cursor-pointer"
                      >
                        <img src={att.data_url} alt={att.name} class="max-h-56 object-contain" />
                      </button>
                    {:else if att.kind === "voice" && att.data_url}
                      <div class="p-2 rounded-xl bg-slate-950/70 border border-slate-800">
                        <div class="text-[10px] text-cyan-300 mb-1">{att.name}</div>
                        <audio controls src={att.data_url} class="h-8 w-56" />
                      </div>
                    {:else}
                      <div class="px-2.5 py-1.5 rounded-lg bg-slate-950/70 border border-slate-800 inline-flex items-center gap-1.5 text-[11px] text-slate-300">
                        <FileText class="w-3.5 h-3.5 text-cyan-400" />
                        <span>{att.name}</span>
                      </div>
                    {/if}
                  {/each}
                </div>
              {/if}

              {#if msg.content}
                {#if msg.role === "assistant"}
                  <div class="prose prose-invert prose-xs max-w-none leading-relaxed break-words">
                    {@html renderMarkdown(msg.content)}
                  </div>
                {:else}
                  <div class="whitespace-pre-wrap leading-relaxed break-words">{msg.content}</div>
                {/if}
              {/if}
              <div class="text-[10px] text-slate-400/80 text-right">{formatTime(msg.created_at)}</div>
            </div>
          </div>
        {/each}

        {#if sending}
          <div class="flex justify-start">
            <div class="px-4 py-3 rounded-2xl bg-slate-900/90 border border-slate-800 text-xs text-cyan-300 flex items-center gap-2">
              <Loader2 class="w-4 h-4 animate-spin" />
              <span>RayBot is thinking &amp; running tools...</span>
            </div>
          </div>
        {/if}
      {/if}
    </div>

    <!-- Composer -->
    <div class="p-4 border-t border-slate-800/80 bg-slate-900/40 space-y-2.5">
      {#if pendingAttachments.length > 0}
        <div class="flex flex-wrap gap-2">
          {#each pendingAttachments as att, idx}
            <div class="relative px-2.5 py-1.5 rounded-xl bg-slate-950 border border-slate-700 flex items-center gap-2 text-[11px]">
              {#if att.kind === "image" && att.data_url}
                <img src={att.data_url} alt={att.name} class="w-8 h-8 rounded object-cover" />
              {:else if att.kind === "voice"}
                <Mic class="w-3.5 h-3.5 text-rose-400" />
              {:else}
                <FileText class="w-3.5 h-3.5 text-cyan-400" />
              {/if}
              <span class="max-w-[140px] truncate text-slate-200">{att.name}</span>
              <button
                type="button"
                on:click={() => removeAttachment(idx)}
                class="text-slate-400 hover:text-rose-400 cursor-pointer"
              >
                <X class="w-3.5 h-3.5" />
              </button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="flex items-end gap-2">
        <button
          type="button"
          on:click={handleAttachFile}
          title="Attach File"
          class="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 transition active:scale-95 cursor-pointer"
        >
          <Paperclip class="w-4 h-4" />
        </button>
        <label
          title="Attach Image"
          class="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 transition active:scale-95 cursor-pointer"
        >
          <ImageIcon class="w-4 h-4" />
          <input type="file" accept="image/*" multiple class="hidden" on:change={handleImageInputChange} />
        </label>
        <button
          type="button"
          on:click={toggleVoiceRecord}
          title={recording ? "Stop Voice Recording" : "Record Voice / Dictate"}
          class="p-2.5 rounded-xl transition active:scale-95 flex items-center gap-1 cursor-pointer {recording
            ? 'bg-rose-600 text-white animate-pulse'
            : 'bg-slate-800 hover:bg-slate-700 text-slate-300'}"
        >
          {#if recording}
            <Square class="w-4 h-4" />
            <span class="text-[11px] font-mono">{recordSecs}s</span>
          {:else}
            <Mic class="w-4 h-4" />
          {/if}
        </button>

        <textarea
          bind:value={inputText}
          rows="1"
          on:paste={handlePaste}
          on:keydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              sendMessage();
            }
          }}
          placeholder="Message RayBot... (Ctrl+V to paste image, Enter to send)"
          class="flex-1 px-3.5 py-2.5 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white resize-none max-h-32 focus:outline-none focus:border-cyan-500"
        />

        <button
          type="button"
          disabled={sending || (!inputText.trim() && pendingAttachments.length === 0)}
          on:click={() => sendMessage()}
          class="px-4 py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-slate-950 text-xs font-bold flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
        >
          {#if sending}
            <Loader2 class="w-4 h-4 animate-spin" />
          {:else}
            <Send class="w-4 h-4" />
          {/if}
          <span>Send</span>
        </button>
      </div>
    </div>
  </div>
</div>

{#if lightboxImage}
  <button
    type="button"
    on:click={() => (lightboxImage = null)}
    class="fixed inset-0 z-50 bg-black/85 flex items-center justify-center p-6 cursor-pointer"
  >
    <img src={lightboxImage} alt="Preview" class="max-w-full max-h-full rounded-xl shadow-2xl" />
  </button>
{/if}
