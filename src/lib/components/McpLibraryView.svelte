<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    Plug,
    Plus,
    Sparkles,
    CheckCircle2,
    AlertCircle,
    Loader2,
    Trash2,
    Bot,
    KeyRound,
    Wrench,
    RefreshCw,
    Eye,
    EyeOff,
    Terminal,
    Globe,
    Edit3,
    X,
  } from "lucide-svelte";

  interface McpAuthField {
    key: string;
    label: string;
    target: string;
    header_prefix: string;
    required: boolean;
    placeholder: string;
  }

  interface McpToolInfo {
    name: string;
    description: string;
    input_schema: any;
  }

  interface McpAgentInstallInfo {
    agent_id: string;
    agent_name: string;
    config_path: string;
    installed: boolean;
    installed_at: number | null;
  }

  interface McpLibraryItem {
    id: string;
    name: string;
    description: string;
    source_url: string;
    transport: string;
    url: string;
    command: string;
    args: string[];
    headers: Record<string, string>;
    env: Record<string, string>;
    auth_fields: McpAuthField[];
    enabled_for_bot: boolean;
    cached_tools: McpToolInfo[];
    last_test_status: string;
    last_test_error: string;
    installations: McpAgentInstallInfo[];
    created_at: number;
    updated_at: number;
  }

  interface McpInspectResult {
    item: McpLibraryItem;
    detected_via: string;
    probe_notes: string[];
  }

  interface McpTestResult {
    ok: boolean;
    message: string;
    tools: McpToolInfo[];
  }

  interface McpSupportedAgent {
    id: string;
    display_name: string;
    config_path: string;
    detected: boolean;
  }

  interface McpPopularPreset {
    category: string;
    requires_auth: boolean;
    item: McpLibraryItem;
  }

  let items: McpLibraryItem[] = [];
  let supportedAgents: McpSupportedAgent[] = [];
  let popularPresets: McpPopularPreset[] = [];
  let activeSubTab: "library" | "popular" = "library";
  let popularSearch = "";
  let popularCategory = "All";
  let loading = true;
  let errorMsg = "";

  // Add/Edit modal state
  let showModal = false;
  let sourceInput = "";
  let inspecting = false;
  let probeNotes: string[] = [];
  let detectedVia = "";
  let draft: McpLibraryItem | null = null;
  let draftArgsText = "";
  let draftAuthValues: Record<string, string> = {};
  let customHeaderKey = "";
  let customHeaderVal = "";
  let showSecrets = false;

  let testing = false;
  let testResult: McpTestResult | null = null;
  let saving = false;

  // Per-item action busy states
  let busyBot: Record<string, boolean> = {};
  let busyAgent: Record<string, boolean> = {};
  let busyDelete: Record<string, boolean> = {};
  let expandedTools: Record<string, boolean> = {};

  async function loadAll() {
    loading = true;
    errorMsg = "";
    try {
      const [list, agents, presets] = await Promise.all([
        invoke<McpLibraryItem[]>("mcp_library_list"),
        invoke<McpSupportedAgent[]>("mcp_library_supported_agents"),
        invoke<McpPopularPreset[]>("mcp_library_popular_presets"),
      ]);
      items = list;
      supportedAgents = agents;
      popularPresets = presets;
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadAll();
  });

  function openAddModal() {
    showModal = true;
    sourceInput = "";
    probeNotes = [];
    detectedVia = "";
    draft = null;
    draftArgsText = "";
    draftAuthValues = {};
    testResult = null;
    showSecrets = false;
  }

  function openEditModal(item: McpLibraryItem) {
    showModal = true;
    sourceInput = item.source_url || item.url || "";
    probeNotes = [];
    detectedVia = "";
    draft = JSON.parse(JSON.stringify(item));
    draftArgsText = (item.args || []).join(" ");
    draftAuthValues = {};
    for (const f of item.auth_fields || []) {
      const existing =
        f.target === "env" ? item.env?.[f.key] || "" : item.headers?.[f.key] || "";
      draftAuthValues[f.key] =
        f.header_prefix && existing.startsWith(f.header_prefix)
          ? existing.slice(f.header_prefix.length)
          : existing;
    }
    testResult = null;
    showSecrets = false;
  }

  function openPresetModal(preset: McpPopularPreset) {
    openEditModal(preset.item);
    probeNotes = [
      `Loaded popular preset (${preset.category}). ${preset.requires_auth ? "Fill in the required credentials below and click Test Connection." : "Click Test Connection or Save to Library."}`,
    ];
  }

  $: popularCategories = ["All", ...Array.from(new Set(popularPresets.map((p) => p.category)))];
  $: filteredPopular = popularPresets.filter((p) => {
    const catOk = popularCategory === "All" || p.category === popularCategory;
    const q = popularSearch.trim().toLowerCase();
    const qOk =
      !q ||
      p.item.name.toLowerCase().includes(q) ||
      p.item.description.toLowerCase().includes(q) ||
      p.item.id.toLowerCase().includes(q);
    return catOk && qOk;
  });

  async function inspectSource() {
    if (!sourceInput.trim()) return;
    inspecting = true;
    errorMsg = "";
    testResult = null;
    try {
      const res = await invoke<McpInspectResult>("mcp_library_inspect", {
        input: sourceInput.trim(),
      });
      draft = res.item;
      draftArgsText = (res.item.args || []).join(" ");
      probeNotes = res.probe_notes || [];
      detectedVia = res.detected_via || "";
      draftAuthValues = {};
      for (const f of res.item.auth_fields || []) {
        draftAuthValues[f.key] = "";
      }
    } catch (e) {
      errorMsg = String(e);
    } finally {
      inspecting = false;
    }
  }

  function syncDraftAuthAndArgs(): McpLibraryItem | null {
    if (!draft) return null;
    const copy: McpLibraryItem = JSON.parse(JSON.stringify(draft));
    if (copy.transport === "stdio") {
      copy.args = draftArgsText
        .split(/\s+/)
        .map((s) => s.trim())
        .filter(Boolean);
    }
    for (const f of copy.auth_fields || []) {
      const rawVal = (draftAuthValues[f.key] || "").trim();
      if (!rawVal) {
        if (f.target === "env") {
          delete copy.env[f.key];
        } else {
          delete copy.headers[f.key];
        }
        continue;
      }
      if (f.target === "env") {
        copy.env[f.key] = rawVal;
      } else {
        const finalVal =
          f.header_prefix && !rawVal.startsWith(f.header_prefix)
            ? `${f.header_prefix}${rawVal}`
            : rawVal;
        copy.headers[f.key] = finalVal;
      }
    }
    return copy;
  }

  function addCustomHeaderOrEnv() {
    if (!draft || !customHeaderKey.trim()) return;
    if (draft.transport === "stdio") {
      draft.env = { ...draft.env, [customHeaderKey.trim()]: customHeaderVal };
    } else {
      draft.headers = { ...draft.headers, [customHeaderKey.trim()]: customHeaderVal };
    }
    customHeaderKey = "";
    customHeaderVal = "";
  }

  function removeHeaderOrEnv(key: string) {
    if (!draft) return;
    if (draft.transport === "stdio") {
      const next = { ...draft.env };
      delete next[key];
      draft.env = next;
    } else {
      const next = { ...draft.headers };
      delete next[key];
      draft.headers = next;
    }
  }

  async function testDraftConnection() {
    const payload = syncDraftAuthAndArgs();
    if (!payload) return;
    testing = true;
    testResult = null;
    try {
      const res = await invoke<McpTestResult>("mcp_library_test", { item: payload });
      testResult = res;
      if (res.ok && draft) {
        draft.cached_tools = res.tools;
        draft.last_test_status = "ok";
        draft.last_test_error = "";
      }
    } catch (e) {
      testResult = { ok: false, message: String(e), tools: [] };
    } finally {
      testing = false;
    }
  }

  async function saveDraft() {
    const payload = syncDraftAuthAndArgs();
    if (!payload) return;
    saving = true;
    try {
      await invoke<McpLibraryItem>("mcp_library_save", { item: payload });
      showModal = false;
      activeSubTab = "library";
      await loadAll();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      saving = false;
    }
  }

  async function toggleBot(item: McpLibraryItem) {
    busyBot = { ...busyBot, [item.id]: true };
    try {
      await invoke("mcp_library_toggle_bot", {
        id: item.id,
        enabled: !item.enabled_for_bot,
      });
      await loadAll();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      busyBot = { ...busyBot, [item.id]: false };
    }
  }

  async function toggleAgentInstall(item: McpLibraryItem, ag: McpAgentInstallInfo) {
    const k = `${item.id}:${ag.agent_id}`;
    busyAgent = { ...busyAgent, [k]: true };
    try {
      if (ag.installed) {
        await invoke("mcp_library_uninstall_agent", {
          mcpId: item.id,
          agentId: ag.agent_id,
        });
      } else {
        await invoke("mcp_library_install_agent", {
          mcpId: item.id,
          agentId: ag.agent_id,
        });
      }
      await loadAll();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      busyAgent = { ...busyAgent, [k]: false };
    }
  }

  async function deleteItem(id: string) {
    busyDelete = { ...busyDelete, [id]: true };
    try {
      await invoke("mcp_library_delete", { id });
      await loadAll();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      busyDelete = { ...busyDelete, [id]: false };
    }
  }

  function isAgentDetected(agentId: string): boolean {
    return supportedAgents.some((a) => a.id === agentId && a.detected);
  }
</script>

<div class="h-full flex flex-col bg-slate-950 text-slate-100 overflow-hidden">
  <!-- Header -->
  <div class="px-6 py-4 border-b border-slate-800/80 flex items-center justify-between gap-4 bg-slate-900/40">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
        <Plug class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-lg font-bold tracking-tight text-white">MCP Library</h1>
        <p class="text-xs text-slate-400">
          Paste any MCP link or GitHub repo — AI configures it for your Coding Agents &amp; Raydrop Bot
        </p>
      </div>
    </div>
    <div class="flex items-center gap-2">
      <button
        type="button"
        disabled={loading}
        on:click={loadAll}
        class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-60 text-slate-200 text-xs font-medium flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
      >
        <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
        {loading ? "Refreshing..." : "Refresh"}
      </button>
      <button
        type="button"
        on:click={openAddModal}
        class="px-4 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold flex items-center gap-1.5 shadow-lg shadow-emerald-900/30 transition active:scale-[0.97] cursor-pointer"
      >
        <Plus class="w-4 h-4" />
        Add MCP
      </button>
    </div>
  </div>

  {#if errorMsg}
    <div class="mx-6 mt-4 px-4 py-2.5 rounded-xl bg-rose-500/15 border border-rose-500/30 text-rose-300 text-xs flex items-center justify-between">
      <span>{errorMsg}</span>
      <button type="button" on:click={() => (errorMsg = "")} class="text-rose-200 hover:text-white cursor-pointer">
        <X class="w-3.5 h-3.5" />
      </button>
    </div>
  {/if}

  <!-- Sub-Tabs -->
  <div class="px-6 pt-3 pb-2 border-b border-slate-800/60 flex items-center gap-2 bg-slate-900/20">
    <button
      type="button"
      on:click={() => (activeSubTab = "library")}
      class="px-3.5 py-1.5 rounded-xl text-xs font-semibold transition active:scale-[0.97] cursor-pointer {activeSubTab === 'library'
        ? 'bg-emerald-600/20 text-emerald-300 border border-emerald-500/40'
        : 'bg-slate-900 text-slate-400 border border-slate-800 hover:text-slate-200'}"
    >
      My MCP Library ({items.length})
    </button>
    <button
      type="button"
      on:click={() => (activeSubTab = "popular")}
      class="px-3.5 py-1.5 rounded-xl text-xs font-semibold transition active:scale-[0.97] flex items-center gap-1.5 cursor-pointer {activeSubTab === 'popular'
        ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
        : 'bg-slate-900 text-slate-400 border border-slate-800 hover:text-slate-200'}"
    >
      <Sparkles class="w-3.5 h-3.5" />
      Popular MCPs ({popularPresets.length})
    </button>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-6 space-y-4">
    {#if activeSubTab === "popular"}
      <div class="flex flex-wrap items-center justify-between gap-3">
        <input
          type="text"
          bind:value={popularSearch}
          placeholder="Search popular MCPs (e.g. mysql, github, playwright, context7)..."
          class="flex-1 min-w-[240px] px-3.5 py-2 rounded-xl bg-slate-900 border border-slate-700 text-xs text-white focus:outline-none focus:border-cyan-500"
        />
        <div class="flex flex-wrap items-center gap-1.5">
          {#each popularCategories as cat}
            <button
              type="button"
              on:click={() => (popularCategory = cat)}
              class="px-3 py-1.5 rounded-xl text-[11px] font-semibold transition active:scale-[0.97] cursor-pointer {popularCategory === cat
                ? 'bg-cyan-500 text-slate-950'
                : 'bg-slate-900 border border-slate-800 text-slate-300 hover:border-slate-700'}"
            >
              {cat}
            </button>
          {/each}
        </div>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        {#each filteredPopular as preset (preset.item.id)}
          {@const alreadyAdded = items.some((i) => i.id === preset.item.id)}
          <div class="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 flex flex-col justify-between gap-3">
            <div>
              <div class="flex items-center justify-between gap-2">
                <h3 class="text-sm font-bold text-white truncate">{preset.item.name}</h3>
                <span class="px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase bg-slate-800 text-slate-300 border border-slate-700 shrink-0">
                  {preset.item.transport}
                </span>
              </div>
              <div class="flex items-center gap-1.5 mt-1.5 flex-wrap">
                <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
                  {preset.category}
                </span>
                {#if preset.requires_auth}
                  <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-amber-500/15 text-amber-300 border border-amber-500/30 flex items-center gap-1">
                    <KeyRound class="w-2.5 h-2.5" />
                    API Key / Auth
                  </span>
                {:else}
                  <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                    No Key Required
                  </span>
                {/if}
              </div>
              <p class="text-xs text-slate-300/80 mt-2 leading-relaxed">{preset.item.description}</p>
              <div class="text-[10px] text-slate-500 mt-2 font-mono truncate">
                {preset.item.transport === "stdio"
                  ? `${preset.item.command} ${preset.item.args.join(" ")}`
                  : preset.item.url}
              </div>
            </div>
            <button
              type="button"
              on:click={() => openPresetModal(preset)}
              class="w-full py-2 rounded-xl text-xs font-semibold transition active:scale-[0.97] flex items-center justify-center gap-1.5 cursor-pointer {alreadyAdded
                ? 'bg-slate-800 hover:bg-slate-700 text-emerald-300 border border-emerald-500/30'
                : 'bg-emerald-600 hover:bg-emerald-500 text-white'}"
            >
              {#if alreadyAdded}
                <CheckCircle2 class="w-3.5 h-3.5" />
                In Library (Reconfigure)
              {:else}
                <Plus class="w-3.5 h-3.5" />
                Configure &amp; Add
              {/if}
            </button>
          </div>
        {/each}
      </div>
    {:else if loading}
      <div class="flex items-center justify-center py-20 text-slate-400 text-sm gap-2">
        <Loader2 class="w-5 h-5 animate-spin text-emerald-400" />
        Loading MCP Library...
      </div>
    {:else if items.length === 0}
      <div class="max-w-xl mx-auto mt-12 p-8 rounded-2xl border border-dashed border-slate-800 bg-slate-900/40 text-center space-y-3">
        <div class="w-12 h-12 mx-auto rounded-2xl bg-emerald-500/15 border border-emerald-500/30 flex items-center justify-center text-emerald-400">
          <Sparkles class="w-6 h-6" />
        </div>
        <h2 class="text-base font-semibold text-white">No MCP servers added yet</h2>
        <p class="text-xs text-slate-400 leading-relaxed">
          Paste a remote MCP URL (like <code class="text-emerald-300">https://mcp.sehatierp.com/</code>), a GitHub repo, an <code class="text-emerald-300">npx</code> command, or JSON config. Raydrop will inspect it, ask for any API key if needed, and install it to your Coding Agents &amp; RayBot.
        </p>
        <div class="flex items-center justify-center gap-2 pt-2">
          <button
            type="button"
            on:click={() => (activeSubTab = "popular")}
            class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-semibold inline-flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
          >
            <Sparkles class="w-4 h-4" />
            Browse Popular MCPs ({popularPresets.length})
          </button>
          <button
            type="button"
            on:click={openAddModal}
            class="px-4 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold inline-flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
          >
            <Plus class="w-4 h-4" />
            Add Custom MCP
          </button>
        </div>
      </div>
    {:else}
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-4">
        {#each items as item (item.id)}
          <div class="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 flex flex-col gap-3">
            <!-- Top Row -->
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <h3 class="text-sm font-bold text-white truncate">{item.name}</h3>
                  <span class="px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase bg-slate-800 text-slate-300 border border-slate-700">
                    {item.transport}
                  </span>
                  {#if item.last_test_status === "ok"}
                    <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 flex items-center gap-1">
                      <CheckCircle2 class="w-3 h-3" />
                      {item.cached_tools.length} tools
                    </span>
                  {:else if item.last_test_status === "error"}
                    <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-rose-500/15 text-rose-300 border border-rose-500/30 flex items-center gap-1" title={item.last_test_error}>
                      <AlertCircle class="w-3 h-3" />
                      Error
                    </span>
                  {/if}
                </div>
                <div class="text-[11px] text-slate-400 mt-1 truncate font-mono">
                  {item.transport === "stdio" ? `${item.command} ${item.args.join(" ")}` : item.url}
                </div>
                {#if item.description}
                  <p class="text-xs text-slate-300/80 mt-1.5 line-clamp-2">{item.description}</p>
                {/if}
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  on:click={() => openEditModal(item)}
                  title="Edit / Configure API Key"
                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 transition active:scale-95 cursor-pointer"
                >
                  <Edit3 class="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  disabled={busyDelete[item.id]}
                  on:click={() => deleteItem(item.id)}
                  title="Delete MCP"
                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-rose-600/30 disabled:opacity-50 text-slate-400 hover:text-rose-300 transition active:scale-95 cursor-pointer"
                >
                  {#if busyDelete[item.id]}
                    <Loader2 class="w-3.5 h-3.5 animate-spin text-rose-300" />
                  {:else}
                    <Trash2 class="w-3.5 h-3.5" />
                  {/if}
                </button>
              </div>
            </div>

            <!-- Raydrop Bot Toggle -->
            <div class="flex items-center justify-between px-3 py-2 rounded-xl bg-slate-950/60 border border-slate-800/80">
              <div class="flex items-center gap-2 text-xs">
                <Bot class="w-4 h-4 {item.enabled_for_bot ? 'text-emerald-400' : 'text-slate-500'}" />
                <span class="font-medium text-slate-200">Enable for Raydrop Bot &amp; Telegram</span>
              </div>
              <button
                type="button"
                disabled={busyBot[item.id]}
                on:click={() => toggleBot(item)}
                class="px-3 py-1 rounded-lg text-[11px] font-semibold transition active:scale-[0.97] disabled:opacity-60 flex items-center gap-1.5 cursor-pointer {item.enabled_for_bot
                  ? 'bg-emerald-600 text-white'
                  : 'bg-slate-800 text-slate-300 hover:bg-slate-700'}"
              >
                {#if busyBot[item.id]}
                  <Loader2 class="w-3 h-3 animate-spin" />
                  Updating...
                {:else}
                  {item.enabled_for_bot ? "Enabled" : "Disabled"}
                {/if}
              </button>
            </div>

            <!-- Agent Installations -->
            <div>
              <div class="text-[11px] font-semibold text-slate-400 mb-1.5">Install to Coding Agents</div>
              <div class="flex flex-wrap gap-1.5">
                {#each item.installations as ag (ag.agent_id)}
                  {@const busy = busyAgent[`${item.id}:${ag.agent_id}`]}
                  {@const detected = isAgentDetected(ag.agent_id)}
                  <button
                    type="button"
                    disabled={busy}
                    on:click={() => toggleAgentInstall(item, ag)}
                    title={ag.config_path}
                    class="px-2.5 py-1 rounded-lg text-[11px] font-medium border transition active:scale-[0.97] disabled:opacity-60 flex items-center gap-1 cursor-pointer {ag.installed
                      ? 'bg-cyan-500/20 border-cyan-500/40 text-cyan-200'
                      : detected
                      ? 'bg-slate-800/90 border-slate-700 text-slate-300 hover:border-slate-500'
                      : 'bg-slate-900/60 border-slate-800 text-slate-500 hover:text-slate-300'}"
                  >
                    {#if busy}
                      <Loader2 class="w-3 h-3 animate-spin" />
                    {:else if ag.installed}
                      <CheckCircle2 class="w-3 h-3 text-cyan-400" />
                    {/if}
                    {ag.agent_name}
                  </button>
                {/each}
              </div>
            </div>

            <!-- Expandable Tools Preview -->
            {#if item.cached_tools.length > 0}
              <div>
                <button
                  type="button"
                  on:click={() => (expandedTools = { ...expandedTools, [item.id]: !expandedTools[item.id] })}
                  class="text-[11px] text-emerald-400 hover:text-emerald-300 flex items-center gap-1 cursor-pointer"
                >
                  <Wrench class="w-3 h-3" />
                  {expandedTools[item.id] ? "Hide" : "Show"} {item.cached_tools.length} tools
                </button>
                {#if expandedTools[item.id]}
                  <div class="mt-2 max-h-36 overflow-y-auto space-y-1 pr-1">
                    {#each item.cached_tools as t}
                      <div class="px-2.5 py-1.5 rounded-lg bg-slate-950/80 border border-slate-800/80 text-[11px]">
                        <span class="font-mono font-semibold text-emerald-300">{t.name}</span>
                        {#if t.description}
                          <span class="text-slate-400 ml-1.5">— {t.description}</span>
                        {/if}
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<!-- Add / Edit MCP Modal -->
{#if showModal}
  <div class="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
    <div class="w-full max-w-2xl max-h-[90vh] flex flex-col rounded-2xl bg-slate-900 border border-slate-700 shadow-2xl overflow-hidden">
      <div class="px-5 py-3.5 border-b border-slate-800 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <Sparkles class="w-4 h-4 text-emerald-400" />
          <h2 class="text-sm font-bold text-white">
            {draft ? `Configure ${draft.name}` : "Add MCP by URL, GitHub Repo, or JSON"}
          </h2>
        </div>
        <button type="button" on:click={() => (showModal = false)} class="text-slate-400 hover:text-white cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-5 space-y-4">
        <!-- Source URL / AI Inspect -->
        <div class="space-y-2">
          <label class="text-xs font-semibold text-slate-300">MCP URL, GitHub Repo, CLI Command, or JSON</label>
          <div class="flex gap-2">
            <input
              type="text"
              bind:value={sourceInput}
              placeholder="https://mcp.sehatierp.com/ or https://github.com/... or npx -y ..."
              class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white focus:outline-none focus:border-emerald-500"
              on:keydown={(e) => e.key === "Enter" && inspectSource()}
            />
            <button
              type="button"
              disabled={inspecting || !sourceInput.trim()}
              on:click={inspectSource}
              class="px-4 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-semibold flex items-center gap-1.5 shrink-0 transition active:scale-[0.97] cursor-pointer"
            >
              {#if inspecting}
                <Loader2 class="w-3.5 h-3.5 animate-spin" />
                Analyzing...
              {:else}
                <Sparkles class="w-3.5 h-3.5" />
                Analyze
              {/if}
            </button>
          </div>
          {#if probeNotes.length > 0}
            <div class="p-2.5 rounded-xl bg-slate-950/80 border border-slate-800 text-[11px] text-slate-400 space-y-0.5">
              {#each probeNotes as note}
                <div>• {note}</div>
              {/each}
            </div>
          {/if}
        </div>

        {#if draft}
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="text-[11px] font-medium text-slate-400">Display Name</label>
              <input
                type="text"
                bind:value={draft.name}
                class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
              />
            </div>
            <div>
              <label class="text-[11px] font-medium text-slate-400">Server ID (slug)</label>
              <input
                type="text"
                bind:value={draft.id}
                class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white font-mono"
              />
            </div>
          </div>

          <div class="grid grid-cols-3 gap-3">
            <div>
              <label class="text-[11px] font-medium text-slate-400">Transport</label>
              <select
                bind:value={draft.transport}
                class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
              >
                <option value="http">Remote HTTP (Streamable)</option>
                <option value="sse">Remote SSE</option>
                <option value="stdio">Local Stdio (npx / uvx)</option>
              </select>
            </div>
            {#if draft.transport === "stdio"}
              <div>
                <label class="text-[11px] font-medium text-slate-400">Command</label>
                <input
                  type="text"
                  bind:value={draft.command}
                  placeholder="npx"
                  class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white font-mono"
                />
              </div>
              <div>
                <label class="text-[11px] font-medium text-slate-400">Arguments</label>
                <input
                  type="text"
                  bind:value={draftArgsText}
                  placeholder="-y @org/mcp-server"
                  class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white font-mono"
                />
              </div>
            {:else}
              <div class="col-span-2">
                <label class="text-[11px] font-medium text-slate-400">Endpoint URL</label>
                <input
                  type="text"
                  bind:value={draft.url}
                  placeholder="https://mcp.sehatierp.com/mcp"
                  class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white font-mono"
                />
              </div>
            {/if}
          </div>

          <!-- API Keys / Headers / Env -->
          <div class="p-3.5 rounded-xl bg-slate-950/70 border border-slate-800 space-y-3">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-1.5 text-xs font-semibold text-amber-300">
                <KeyRound class="w-3.5 h-3.5" />
                Authentication / {draft.transport === "stdio" ? "Environment Variables" : "HTTP Headers"}
              </div>
              <button
                type="button"
                on:click={() => (showSecrets = !showSecrets)}
                class="text-[11px] text-slate-400 hover:text-white flex items-center gap-1 cursor-pointer"
              >
                {#if showSecrets}
                  <EyeOff class="w-3 h-3" /> Hide
                {:else}
                  <Eye class="w-3 h-3" /> Show
                {/if}
              </button>
            </div>

            {#each draft.auth_fields as field}
              <div>
                <label class="text-[11px] text-slate-300 flex items-center gap-1">
                  <span>{field.label}</span>
                  <span class="text-slate-500 font-mono">({field.key})</span>
                  {#if field.required}<span class="text-rose-400">*</span>{/if}
                </label>
                {#if showSecrets}
                  <input
                    type="text"
                    bind:value={draftAuthValues[field.key]}
                    placeholder={field.placeholder || "Enter API key / token"}
                    class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white font-mono"
                  />
                {:else}
                  <input
                    type="password"
                    bind:value={draftAuthValues[field.key]}
                    placeholder={field.placeholder || "Enter API key / token"}
                    class="mt-1 w-full px-3 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white font-mono"
                  />
                {/if}
              </div>
            {/each}

            <!-- Existing custom headers/env (excluding keys already shown in auth_fields) -->
            {#each Object.entries(draft.transport === "stdio" ? draft.env : draft.headers).filter(([k]) => !(draft?.auth_fields || []).some((f) => f.key === k)) as [k, v]}
              <div class="flex items-center gap-2 text-xs">
                <span class="px-2 py-1 rounded bg-slate-900 border border-slate-800 font-mono text-slate-300">{k}</span>
                <span class="flex-1 px-2 py-1 rounded bg-slate-900 border border-slate-800 font-mono text-slate-400 truncate">
                  {showSecrets ? v : "••••••••••••"}
                </span>
                <button
                  type="button"
                  on:click={() => removeHeaderOrEnv(k)}
                  class="text-slate-500 hover:text-rose-400 cursor-pointer"
                >
                  <X class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}

            <!-- Add custom header / env -->
            <div class="flex gap-2 pt-1">
              <input
                type="text"
                bind:value={customHeaderKey}
                placeholder={draft.transport === "stdio" ? "ENV_VAR_NAME" : "Header (e.g. Authorization or X-API-Key)"}
                class="w-1/2 px-2.5 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white font-mono"
              />
              {#if showSecrets}
                <input
                  type="text"
                  bind:value={customHeaderVal}
                  placeholder="Value (e.g. Bearer sk-...)"
                  class="w-1/2 px-2.5 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white font-mono"
                />
              {:else}
                <input
                  type="password"
                  bind:value={customHeaderVal}
                  placeholder="Value (e.g. Bearer sk-...)"
                  class="w-1/2 px-2.5 py-1.5 rounded-lg bg-slate-900 border border-slate-700 text-xs text-white font-mono"
                />
              {/if}
              <button
                type="button"
                on:click={addCustomHeaderOrEnv}
                class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-white cursor-pointer"
              >
                Add
              </button>
            </div>
          </div>

          <!-- Test Result -->
          {#if testResult}
            <div
              class="p-3 rounded-xl border text-xs {testResult.ok
                ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300'
                : 'bg-rose-500/10 border-rose-500/30 text-rose-300'}"
            >
              <div class="font-semibold flex items-center gap-1.5">
                {#if testResult.ok}
                  <CheckCircle2 class="w-4 h-4" />
                {:else}
                  <AlertCircle class="w-4 h-4" />
                {/if}
                {testResult.message}
              </div>
              {#if testResult.tools.length > 0}
                <div class="mt-2 flex flex-wrap gap-1">
                  {#each testResult.tools.slice(0, 12) as t}
                    <span class="px-2 py-0.5 rounded bg-slate-950/70 border border-emerald-500/20 font-mono text-[10px]">
                      {t.name}
                    </span>
                  {/each}
                  {#if testResult.tools.length > 12}
                    <span class="text-[10px] text-slate-400">+{testResult.tools.length - 12} more</span>
                  {/if}
                </div>
              {/if}
            </div>
          {/if}
        {/if}
      </div>

      {#if draft}
        <div class="px-5 py-3.5 border-t border-slate-800 flex items-center justify-between bg-slate-950/50">
          <button
            type="button"
            disabled={testing}
            on:click={testDraftConnection}
            class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
          >
            {#if testing}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
              Testing...
            {:else}
              <Plug class="w-3.5 h-3.5" />
              Test Connection
            {/if}
          </button>
          <button
            type="button"
            disabled={saving || !draft.name.trim()}
            on:click={saveDraft}
            class="px-4 py-2 rounded-xl bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-semibold flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
          >
            {#if saving}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
              Saving...
            {:else}
              Save to Library
            {/if}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
