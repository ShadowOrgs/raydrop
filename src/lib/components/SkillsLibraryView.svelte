<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    Sparkles,
    Search,
    FolderOpen,
    FileArchive,
    Github,
    Star,
    RefreshCw,
    Download,
    Trash2,
    Cpu,
    Globe,
    Layers,
    ShieldAlert,
    ShieldCheck,
    RotateCcw,
    Plus,
    CheckCircle2,
    AlertTriangle,
    Settings,
    History,
    Loader2,
    Share2,
  } from "lucide-svelte";
  import {
    skillsApi,
    type SkillRecord,
    type SkillDetailResponse,
    type SkillCompatibilityEntry,
    type SkillAgentRecord,
    type SkillSourceRecord,
    type SkillInstallationRecord,
    type SkillInstallHistoryRecord,
    type SkillUpdateCheckItem,
    type SkillPresetRecord,
    type SkillsSettings,
    type DiscoveredRepoSkill,
  } from "../utils/tauri-ipc";
  import SkillDetailModal from "./skills/SkillDetailModal.svelte";
  import SkillInstallModal from "./skills/SkillInstallModal.svelte";
  import SkillShareModal from "./skills/SkillShareModal.svelte";

  type SubTab = "discover" | "installed" | "agents" | "sources" | "presets";
  let activeSubTab: SubTab = "discover";

  let loading = false;
  let busyKey: string | null = null;
  let statusBanner = "";
  let errorBanner = "";

  // Discover state
  let skills: SkillRecord[] = [];
  let searchQuery = "";
  let selectedSourceId = "";
  let selectedCategory = "";
  let favoritesOnly = false;
  let sortBy: "updated" | "popularity" | "alpha" = "updated";

  // GitHub import bar
  let githubUrl = "";
  let discoveringGh = false;
  let importingGh = false;
  let ghDiscovered: DiscoveredRepoSkill[] = [];
  let ghBranch = "";
  let ghCommit = "";

  // Detail, Install & Share modals
  let activeDetail: SkillDetailResponse | null = null;
  let activeCompatibility: SkillCompatibilityEntry[] = [];
  let installSkillId: string | null = null;
  let shareSkill: SkillRecord | null = null;

  // Installed / Updates / History
  let installations: SkillInstallationRecord[] = [];
  let scannedProjects: string[] = [];
  let updateChecks: SkillUpdateCheckItem[] = [];
  let installHistory: SkillInstallHistoryRecord[] = [];
  let showHistoryPanel = false;
  let filterAgentId = "";
  let filterScope: "" | "global" | "project" = "";
  let filterProjectPath = "";

  // Agents
  let agents: SkillAgentRecord[] = [];
  let detectingAgents = false;

  // Sources
  let sources: SkillSourceRecord[] = [];
  let newSourceName = "";
  let newSourceUrl = "";
  let newSourceType = "custom_registry";

  // Presets & Settings
  let presets: SkillPresetRecord[] = [];
  let skillsSettings: SkillsSettings = {
    backup_retention_count: 5,
    require_confirm_high_risk: true,
    block_critical_by_default: false,
    default_scope: "global",
    auto_sync_curated: true,
  };
  let newPresetName = "";
  let newPresetDesc = "";
  let newPresetSkillIds: string[] = [];

  onMount(async () => {
    loading = true;
    try {
      await skillsApi.initLibrary();
      await Promise.all([
        loadSkills(),
        loadAgents(),
        loadSources(),
        loadInstalled(),
        loadPresets(),
        loadSettings(),
      ]);
    } catch (e) {
      errorBanner = String(e);
    } finally {
      loading = false;
    }
  });

  async function loadSkills() {
    skills = await skillsApi.search({
      query: searchQuery || undefined,
      source_id: selectedSourceId || undefined,
      category: selectedCategory || undefined,
      favorites_only: favoritesOnly,
      sort_by: sortBy,
    });
  }

  async function loadAgents() {
    agents = await skillsApi.getAgents();
  }

  async function loadSources() {
    sources = await skillsApi.listSources();
  }

  async function loadInstalled() {
    [installations, installHistory, scannedProjects] = await Promise.all([
      skillsApi.listInstalled(),
      skillsApi.getInstallHistory(),
      skillsApi.listScannedProjects(),
    ]);
  }

  async function handleScanProjectFolder() {
    errorBanner = "";
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Select Project Folder to Scan for Installed Skills",
      });
      if (typeof picked === "string") {
        installations = await skillsApi.scanProjectFolder(picked);
        scannedProjects = await skillsApi.listScannedProjects();
        statusBanner = `Scanned project folder: ${picked}`;
      }
    } catch (e) {
      errorBanner = String(e);
    }
  }

  async function loadPresets() {
    presets = await skillsApi.listPresets();
  }

  async function loadSettings() {
    skillsSettings = await skillsApi.getSettings();
  }

  $: categories = Array.from(
    new Set(skills.flatMap((s) => s.categories || []))
  ).sort();

  $: installedProjects = Array.from(
    new Set(
      installations
        .map((i) => i.project_path)
        .filter((p): p is string => Boolean(p))
    )
  ).sort();

  $: filteredInstallations = installations.filter((i) => {
    if (filterAgentId && i.agent_id !== filterAgentId) return false;
    if (filterScope && i.scope !== filterScope) return false;
    if (filterProjectPath && i.project_path !== filterProjectPath) return false;
    return true;
  });

  async function handleImportFolder() {
    errorBanner = "";
    statusBanner = "";
    busyKey = "import-folder";
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Select Skill Folder (containing SKILL.md)",
      });
      if (typeof picked === "string") {
        const rec = await skillsApi.importLocalFolder(picked);
        statusBanner = `Imported '${rec.name}' from local folder.`;
        await loadSkills();
      }
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleImportZip() {
    errorBanner = "";
    statusBanner = "";
    busyKey = "import-zip";
    try {
      const picked = await open({
        directory: false,
        multiple: false,
        filters: [{ name: "ZIP Archives", extensions: ["zip"] }],
        title: "Select Skill ZIP Archive",
      });
      if (typeof picked === "string") {
        const rec = await skillsApi.importZip(picked);
        statusBanner = `Imported '${rec.name}' from ZIP archive.`;
        await loadSkills();
      }
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleDiscoverGithub() {
    if (!githubUrl.trim()) return;
    errorBanner = "";
    statusBanner = "";
    discoveringGh = true;
    ghDiscovered = [];
    try {
      const [branch, sha, list] = await skillsApi.discoverGithub(githubUrl.trim());
      ghBranch = branch;
      ghCommit = sha;
      ghDiscovered = list;
      if (list.length === 0) {
        statusBanner = "No SKILL.md directories found in repository.";
      }
    } catch (e) {
      errorBanner = String(e);
    } finally {
      discoveringGh = false;
    }
  }

  async function handleImportGithub(url: string, subpath?: string | null, keyOverride?: string) {
    errorBanner = "";
    statusBanner = "";
    importingGh = true;
    if (keyOverride) busyKey = keyOverride;
    try {
      const rec = await skillsApi.importGithub(url, subpath);
      statusBanner = `Imported '${rec.name}' (pinned ${rec.commit_sha?.slice(0, 8) || ""}).`;
      ghDiscovered = [];
      await loadSkills();
      if (activeDetail && activeDetail.skill.id === rec.id) {
        await openDetail(rec.id);
      }
    } catch (e) {
      errorBanner = String(e);
    } finally {
      importingGh = false;
      if (keyOverride && busyKey === keyOverride) busyKey = null;
    }
  }

  async function openDetail(skillId: string) {
    errorBanner = "";
    busyKey = `inspect:${skillId}`;
    try {
      activeDetail = await skillsApi.getDetail(skillId);
      activeCompatibility = await skillsApi.getCompatibility(skillId);
    } catch (e) {
      errorBanner = String(e);
    } finally {
      if (busyKey === `inspect:${skillId}`) busyKey = null;
    }
  }

  async function handleToggleFavorite(skillId: string) {
    try {
      await skillsApi.toggleFavorite(skillId);
      await loadSkills();
      if (activeDetail?.skill.id === skillId) {
        activeDetail = await skillsApi.getDetail(skillId);
      }
    } catch (e) {
      errorBanner = String(e);
    }
  }

  async function handleDeleteSkill(skillId: string) {
    try {
      await skillsApi.deleteSkill(skillId);
      activeDetail = null;
      statusBanner = "Skill removed from managed library.";
      await loadSkills();
    } catch (e) {
      errorBanner = String(e);
    }
  }

  async function handleRedetectAgents() {
    detectingAgents = true;
    try {
      agents = await skillsApi.detectAgents();
      statusBanner = "Agent detection refreshed.";
    } catch (e) {
      errorBanner = String(e);
    } finally {
      detectingAgents = false;
    }
  }

  async function handleConfigureAgentPath(agentId: string) {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Select Custom Agent Skills Directory",
      });
      if (typeof picked === "string") {
        await skillsApi.configureAgentPath(agentId, picked);
        await loadAgents();
        statusBanner = `Configured custom path for ${agentId}.`;
      }
    } catch (e) {
      errorBanner = String(e);
    }
  }

  async function handleClearAgentPath(agentId: string) {
    try {
      await skillsApi.configureAgentPath(agentId, null);
      await loadAgents();
    } catch (e) {
      errorBanner = String(e);
    }
  }

  async function handleUninstall(instId: string, force = false) {
    errorBanner = "";
    busyKey = `uninstall:${instId}`;
    try {
      await skillsApi.uninstall(instId, force);
      statusBanner = "Skill uninstalled cleanly.";
      await Promise.all([loadInstalled(), loadSkills()]);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("MODIFIED_FILES:") && !force) {
        if (confirm(`Modified files detected:\n${msg}\n\nProceed with uninstall anyway?`)) {
          await handleUninstall(instId, true);
        }
      } else {
        errorBanner = msg;
      }
    } finally {
      if (busyKey === `uninstall:${instId}`) busyKey = null;
    }
  }

  async function handleCheckUpdates() {
    busyKey = "check-updates";
    try {
      updateChecks = await skillsApi.checkUpdates();
      const count = updateChecks.filter((u) => u.has_update).length;
      statusBanner = `Update check complete: ${count} update(s) available.`;
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleRollback(historyId: string) {
    busyKey = `rollback:${historyId}`;
    try {
      const dest = await skillsApi.rollback(historyId);
      statusBanner = `Rolled back backup to ${dest}.`;
      await loadInstalled();
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleAddSource() {
    if (!newSourceName.trim() || !newSourceUrl.trim()) return;
    busyKey = "add-source";
    try {
      await skillsApi.addSource(newSourceName.trim(), newSourceType, newSourceUrl.trim());
      newSourceName = "";
      newSourceUrl = "";
      await loadSources();
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleRefreshSource(id: string) {
    errorBanner = "";
    busyKey = `sync:${id}`;
    try {
      const n = await skillsApi.refreshSource(id);
      statusBanner = `Source synced (${n} entries).`;
      await Promise.all([loadSources(), loadSkills()]);
    } catch (e) {
      errorBanner = String(e);
      await loadSources();
    } finally {
      busyKey = null;
    }
  }

  async function handleSavePreset() {
    if (!newPresetName.trim()) return;
    busyKey = "save-preset";
    try {
      await skillsApi.savePreset(
        null,
        newPresetName.trim(),
        newPresetDesc.trim(),
        "custom",
        newPresetSkillIds
      );
      newPresetName = "";
      newPresetDesc = "";
      newPresetSkillIds = [];
      await loadPresets();
      statusBanner = "Preset saved.";
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }

  async function handleSaveSettings() {
    busyKey = "save-settings";
    try {
      skillsSettings = await skillsApi.updateSettings(skillsSettings);
      statusBanner = "Skills Library settings saved.";
    } catch (e) {
      errorBanner = String(e);
    } finally {
      busyKey = null;
    }
  }
</script>

<div class="h-full w-full flex flex-col gap-3 overflow-hidden">
  <!-- Top Bar -->
  <div class="flex flex-wrap items-center justify-between gap-3 px-4 py-3 rounded-2xl bg-slate-900/75 border border-slate-800/80 shrink-0">
    <div class="flex items-center gap-2.5">
      <div class="w-9 h-9 rounded-xl bg-gradient-to-br from-cyan-500/25 to-indigo-500/25 border border-cyan-500/40 flex items-center justify-center text-cyan-300">
        <Sparkles class="w-5 h-5" />
      </div>
      <div>
        <h2 class="text-sm font-bold text-white">AI Skills Library</h2>
        <p class="text-[11px] text-slate-400">
          Local-first coding-agent skills manager (Codex, Claude Code, Cursor, Gemini CLI &amp; more)
        </p>
      </div>
    </div>

    <div class="flex items-center gap-1.5 bg-slate-950/70 p-1 rounded-xl border border-slate-800">
      <button
        type="button"
        on:click={() => (activeSubTab = "discover")}
        class="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition {activeSubTab === 'discover' ? 'bg-cyan-500 text-slate-950' : 'text-slate-300 hover:text-white'}"
      >
        Discover ({skills.length})
      </button>
      <button
        type="button"
        on:click={() => {
          activeSubTab = "installed";
          loadInstalled();
        }}
        class="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition {activeSubTab === 'installed' ? 'bg-cyan-500 text-slate-950' : 'text-slate-300 hover:text-white'}"
      >
        Installed ({installations.length})
      </button>
      <button
        type="button"
        on:click={() => (activeSubTab = "agents")}
        class="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition {activeSubTab === 'agents' ? 'bg-cyan-500 text-slate-950' : 'text-slate-300 hover:text-white'}"
      >
        Agents ({agents.filter((a) => a.detected_status === 'Detected' || a.detected_status === 'ManualConfiguration').length}/{agents.length})
      </button>
      <button
        type="button"
        on:click={() => (activeSubTab = "sources")}
        class="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition {activeSubTab === 'sources' ? 'bg-cyan-500 text-slate-950' : 'text-slate-300 hover:text-white'}"
      >
        Sources
      </button>
      <button
        type="button"
        on:click={() => (activeSubTab = "presets")}
        class="px-3 py-1.5 rounded-lg text-xs font-semibold cursor-pointer transition {activeSubTab === 'presets' ? 'bg-cyan-500 text-slate-950' : 'text-slate-300 hover:text-white'}"
      >
        Presets &amp; Policy
      </button>
    </div>
  </div>

  {#if statusBanner}
    <div class="px-4 py-2 rounded-xl bg-emerald-500/15 border border-emerald-500/35 text-xs text-emerald-200 flex items-center justify-between shrink-0">
      <span>{statusBanner}</span>
      <button type="button" on:click={() => (statusBanner = "")} class="text-emerald-300 hover:text-white cursor-pointer">✕</button>
    </div>
  {/if}
  {#if errorBanner}
    <div class="px-4 py-2 rounded-xl bg-rose-500/15 border border-rose-500/40 text-xs text-rose-200 flex items-center justify-between shrink-0">
      <span>{errorBanner}</span>
      <button type="button" on:click={() => (errorBanner = "")} class="text-rose-300 hover:text-white cursor-pointer">✕</button>
    </div>
  {/if}

  <!-- Tab Content -->
  <div class="flex-1 overflow-y-auto pr-1 space-y-4">
    {#if activeSubTab === "discover"}
      <!-- Import Bar -->
      <div class="p-4 rounded-2xl bg-slate-900/70 border border-slate-800/80 space-y-3">
        <div class="flex flex-wrap items-center gap-2">
          <div class="flex-1 min-w-[220px] relative">
            <Github class="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              bind:value={githubUrl}
              placeholder="Paste public GitHub repo or skill folder URL (e.g. https://github.com/anthropics/skills)"
              class="w-full pl-9 pr-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-cyan-500"
            />
          </div>
          <button
            type="button"
            disabled={discoveringGh || !githubUrl.trim()}
            on:click={handleDiscoverGithub}
            class="px-3.5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-40 text-slate-950 font-bold text-xs cursor-pointer"
          >
            {discoveringGh ? "Scanning..." : "Scan GitHub"}
          </button>
          <button
            type="button"
            disabled={importingGh || !githubUrl.trim()}
            on:click={() => handleImportGithub(githubUrl.trim(), null)}
            class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-40 text-white font-semibold text-xs border border-slate-700 cursor-pointer"
          >
            {importingGh ? "Importing..." : "Direct Import"}
          </button>
          <button
            type="button"
            on:click={handleImportFolder}
            disabled={busyKey === "import-folder"}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold border border-slate-700 flex items-center gap-1.5 cursor-pointer"
          >
            {#if busyKey === "import-folder"}
              <Loader2 class="w-3.5 h-3.5 text-amber-400 animate-spin" />
            {:else}
              <FolderOpen class="w-3.5 h-3.5 text-amber-400" />
            {/if}
            <span>Local Folder</span>
          </button>
          <button
            type="button"
            on:click={handleImportZip}
            disabled={busyKey === "import-zip"}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold border border-slate-700 flex items-center gap-1.5 cursor-pointer"
          >
            {#if busyKey === "import-zip"}
              <Loader2 class="w-3.5 h-3.5 text-indigo-400 animate-spin" />
            {:else}
              <FileArchive class="w-3.5 h-3.5 text-indigo-400" />
            {/if}
            <span>ZIP Archive</span>
          </button>
        </div>

        {#if ghDiscovered.length > 0}
          <div class="pt-2 border-t border-slate-800 space-y-2">
            <div class="text-xs font-semibold text-cyan-300">
              Discovered {ghDiscovered.length} skill(s) on {ghBranch} ({ghCommit.slice(0, 8)}):
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
              {#each ghDiscovered as d}
                <div class="p-3 rounded-xl bg-slate-950/80 border border-slate-800 flex flex-col justify-between gap-2">
                  <div>
                    <div class="text-xs font-bold text-white">{d.name}</div>
                    <div class="text-[10px] font-mono text-slate-400">{d.relative_path || "(root)"}</div>
                    <p class="text-[11px] text-slate-300 mt-1 line-clamp-2">{d.description}</p>
                  </div>
                  <button
                    type="button"
                    disabled={importingGh}
                    on:click={() => handleImportGithub(githubUrl.trim(), d.relative_path || null)}
                    class="px-2.5 py-1.5 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold text-[11px] cursor-pointer"
                  >
                    Import Skill
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Search & Filters -->
      <div class="flex flex-wrap items-center gap-2">
        <div class="flex-1 min-w-[200px] relative">
          <Search class="w-3.5 h-3.5 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            bind:value={searchQuery}
            on:input={loadSkills}
            placeholder="Search skills by name, tag, author, description..."
            class="w-full pl-8 pr-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
          />
        </div>
        <select
          bind:value={selectedSourceId}
          on:change={loadSkills}
          class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200"
        >
          <option value="">All Sources</option>
          {#each sources as src}
            <option value={src.id}>{src.name}</option>
          {/each}
        </select>
        <select
          bind:value={selectedCategory}
          on:change={loadSkills}
          class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200"
        >
          <option value="">All Categories</option>
          {#each categories as cat}
            <option value={cat}>{cat}</option>
          {/each}
        </select>
        <select
          bind:value={sortBy}
          on:change={loadSkills}
          class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200"
        >
          <option value="updated">Sort: Recently Updated</option>
          <option value="popularity">Sort: Popularity</option>
          <option value="alpha">Sort: A-Z</option>
        </select>
        <button
          type="button"
          on:click={() => {
            favoritesOnly = !favoritesOnly;
            loadSkills();
          }}
          class="px-3 py-2 rounded-xl border text-xs font-semibold flex items-center gap-1.5 cursor-pointer {favoritesOnly ? 'bg-amber-500/20 border-amber-500/50 text-amber-300' : 'bg-slate-900 border-slate-800 text-slate-300'}"
        >
          <Star class="w-3.5 h-3.5 {favoritesOnly ? 'fill-amber-400' : ''}" />
          Favorites
        </button>
      </div>

      <!-- Skills Grid -->
      {#if loading}
        <div class="py-12 text-center text-xs text-slate-400">Loading Skills Library...</div>
      {:else if skills.length === 0}
        <div class="p-10 rounded-2xl bg-slate-900/50 border border-slate-800 text-center text-xs text-slate-400">
          No skills match your filters. Import a local folder, ZIP archive, or GitHub URL above.
        </div>
      {:else}
        <div class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {#each skills as skill}
            <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800/80 hover:border-cyan-500/40 transition flex flex-col justify-between gap-3">
              <div>
                <div class="flex items-start justify-between gap-2">
                  <button
                    type="button"
                    on:click={() => openDetail(skill.id)}
                    class="text-left text-sm font-bold text-white hover:text-cyan-300 cursor-pointer truncate"
                  >
                    {skill.name}
                  </button>
                  <button
                    type="button"
                    on:click={() => handleToggleFavorite(skill.id)}
                    class="text-slate-400 hover:text-amber-400 cursor-pointer shrink-0"
                  >
                    <Star class="w-4 h-4 {skill.is_favorite ? 'fill-amber-400 text-amber-400' : ''}" />
                  </button>
                </div>
                <p class="text-xs text-slate-400 mt-1 line-clamp-2">{skill.description}</p>
                <div class="flex flex-wrap gap-1 mt-2">
                  {#each skill.categories.slice(0, 2) as c}
                    <span class="px-2 py-0.5 rounded-full text-[10px] bg-indigo-500/15 text-indigo-300 border border-indigo-500/30">{c}</span>
                  {/each}
                  {#each skill.tags.slice(0, 3) as t}
                    <span class="px-2 py-0.5 rounded-full text-[10px] bg-slate-800 text-slate-300">#{t}</span>
                  {/each}
                </div>
              </div>

              <div class="pt-2 border-t border-slate-800/80 flex items-center justify-between gap-2 text-[11px]">
                <div class="flex items-center gap-2 text-slate-400">
                  {#if skill.security_status === "high_risk"}
                    <span class="inline-flex items-center gap-1 text-rose-300 font-semibold">
                      <ShieldAlert class="w-3.5 h-3.5" /> High Risk
                    </span>
                  {:else if skill.security_status === "warnings"}
                    <span class="inline-flex items-center gap-1 text-amber-300">
                      <ShieldAlert class="w-3.5 h-3.5" /> Warnings
                    </span>
                  {:else if skill.security_status === "clean"}
                    <span class="inline-flex items-center gap-1 text-emerald-300">
                      <ShieldCheck class="w-3.5 h-3.5" /> Scanned
                    </span>
                  {:else}
                    <span class="text-slate-400">Catalog Ref</span>
                  {/if}
                  {#if skill.installed_agents.length > 0}
                    <span class="px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-300 text-[10px]">
                      {skill.installed_agents.length} installed
                    </span>
                  {/if}
                </div>

                <div class="flex items-center gap-1.5">
                  <button
                    type="button"
                    disabled={busyKey === `inspect:${skill.id}`}
                    on:click={() => openDetail(skill.id)}
                    class="px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 font-semibold flex items-center gap-1 cursor-pointer"
                  >
                    {#if busyKey === `inspect:${skill.id}`}
                      <Loader2 class="w-3 h-3 animate-spin" />
                    {/if}
                    <span>Inspect</span>
                  </button>
                  {#if skill.local_package_path}
                    <button
                      type="button"
                      on:click={() => (shareSkill = skill)}
                      class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-cyan-300 cursor-pointer"
                      title="Share Skill via P2P"
                    >
                      <Share2 class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      on:click={() => (installSkillId = skill.id)}
                      class="px-2.5 py-1 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold cursor-pointer"
                    >
                      Install
                    </button>
                  {:else}
                    <button
                      type="button"
                      disabled={importingGh}
                      on:click={() => handleImportGithub(skill.source_url, skill.source_path ?? null, `fetch:${skill.id}`)}
                      class="px-2.5 py-1 rounded-lg bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-slate-950 font-bold flex items-center gap-1 cursor-pointer"
                    >
                      {#if busyKey === `fetch:${skill.id}`}
                        <Loader2 class="w-3 h-3 animate-spin" />
                        <span>Fetching...</span>
                      {:else}
                        <span>Fetch</span>
                      {/if}
                    </button>
                  {/if}
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}

    {:else if activeSubTab === "installed"}
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-wrap items-center gap-2">
          <select
            bind:value={filterAgentId}
            class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200"
          >
            <option value="">All Agents</option>
            {#each agents as a}
              <option value={a.id}>{a.display_name}</option>
            {/each}
          </select>
          <select
            bind:value={filterScope}
            class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200"
          >
            <option value="">All Scopes</option>
            <option value="global">Global Only</option>
            <option value="project">Project Only</option>
          </select>
          {#if installedProjects.length > 0}
            <select
              bind:value={filterProjectPath}
              class="px-3 py-2 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-200 max-w-[220px]"
            >
              <option value="">All Projects ({installedProjects.length})</option>
              {#each installedProjects as proj}
                <option value={proj}>{proj}</option>
              {/each}
            </select>
          {/if}
          <button
            type="button"
            on:click={handleScanProjectFolder}
            class="px-3 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-xs font-bold text-slate-950 flex items-center gap-1.5 cursor-pointer"
          >
            <FolderOpen class="w-3.5 h-3.5" />
            Scan Project Folder
          </button>
          <button
            type="button"
            disabled={busyKey === "check-updates"}
            on:click={handleCheckUpdates}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-xs font-semibold text-white flex items-center gap-1.5 cursor-pointer"
          >
            <RefreshCw class="w-3.5 h-3.5 text-cyan-400 {busyKey === 'check-updates' ? 'animate-spin' : ''}" />
            <span>{busyKey === "check-updates" ? "Checking..." : "Check Updates"}</span>
          </button>
          <button
            type="button"
            on:click={() => (showHistoryPanel = !showHistoryPanel)}
            class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white flex items-center gap-1.5 cursor-pointer"
          >
            <History class="w-3.5 h-3.5 text-amber-400" />
            {showHistoryPanel ? "Hide History" : `History & Rollback (${installHistory.length})`}
          </button>
        </div>
      </div>

      {#if updateChecks.some((u) => u.has_update || u.locally_modified_files.length > 0)}
        <div class="p-3 rounded-xl bg-amber-500/10 border border-amber-500/30 space-y-1.5 text-xs">
          <div class="font-bold text-amber-300">Update &amp; Diff Summary</div>
          {#each updateChecks.filter((u) => u.has_update || u.locally_modified_files.length > 0) as u}
            <div class="flex items-center justify-between gap-2 text-slate-200">
              <span>
                <strong>{u.skill_name}</strong> ({u.agent_id}) —
                {u.changed_files.length} changed, {u.added_files.length} added, {u.removed_files.length} removed
                {#if u.locally_modified_files.length > 0}
                  <span class="text-amber-300"> • Local edits: {u.locally_modified_files.join(", ")}</span>
                {/if}
              </span>
              <button
                type="button"
                on:click={() => (installSkillId = u.skill_id)}
                class="px-2.5 py-1 rounded-lg bg-amber-500 text-slate-950 font-bold text-[11px] cursor-pointer"
              >
                Update
              </button>
            </div>
          {/each}
        </div>
      {/if}

      {#if showHistoryPanel}
        <div class="rounded-2xl bg-slate-900/75 border border-slate-800 p-4 space-y-2">
          <div class="text-xs font-bold text-white">Install &amp; Backup History</div>
          <div class="space-y-1.5 max-h-64 overflow-y-auto">
            {#each installHistory as h}
              <div class="p-2.5 rounded-xl bg-slate-950/70 border border-slate-800 text-xs flex items-center justify-between gap-2">
                <div>
                  <span class="font-semibold text-white">{h.skill_name}</span>
                  <span class="text-slate-400"> • {h.agent_id} • </span>
                  <span class="uppercase font-mono text-[10px] text-cyan-300">{h.action}</span>
                </div>
                {#if h.backup_path}
                  <button
                    type="button"
                    on:click={() => handleRollback(h.id)}
                    class="px-2.5 py-1 rounded-lg bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 border border-amber-500/40 text-[11px] font-semibold flex items-center gap-1 cursor-pointer"
                  >
                    <RotateCcw class="w-3 h-3" />
                    Rollback
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if filteredInstallations.length === 0}
        <div class="p-10 rounded-2xl bg-slate-900/50 border border-slate-800 text-center text-xs text-slate-400">
          No managed skill installations yet. Install a skill from the Discover tab.
        </div>
      {:else}
        <div class="space-y-2">
          {#each filteredInstallations as inst}
            <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 flex flex-wrap items-center justify-between gap-3">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="text-sm font-bold text-white">{inst.skill_name}</span>
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-cyan-500/15 text-cyan-300">{inst.agent_id}</span>
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-semibold {inst.scope === 'global' ? 'bg-indigo-500/15 text-indigo-300' : 'bg-amber-500/15 text-amber-300'}">
                    {inst.scope === "global" ? "Global" : "Project"}
                  </span>
                  <span class="px-2 py-0.5 rounded-full text-[10px] {inst.status === 'installed' ? 'bg-emerald-500/15 text-emerald-300' : inst.status === 'external' ? 'bg-sky-500/15 text-sky-300' : 'bg-amber-500/15 text-amber-300'}">
                    {inst.status === "external" ? "Pre-existing" : inst.status}
                  </span>
                </div>
                {#if inst.project_path}
                  <div class="text-[11px] text-amber-200/80 mt-1 truncate">Project: <span class="font-mono">{inst.project_path}</span></div>
                {/if}
                <div class="text-[11px] font-mono text-slate-400 mt-0.5 truncate">{inst.install_path}</div>
              </div>
              <div class="flex items-center gap-2">
                {#if !inst.skill_id.startsWith("ext_")}
                  <button
                    type="button"
                    on:click={() => openDetail(inst.skill_id)}
                    class="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white cursor-pointer"
                  >
                    Details
                  </button>
                {/if}
                <button
                  type="button"
                  disabled={busyKey === `uninstall:${inst.id}`}
                  on:click={() => handleUninstall(inst.id, false)}
                  class="px-3 py-1.5 rounded-xl bg-rose-500/15 hover:bg-rose-500/25 disabled:opacity-50 text-rose-300 border border-rose-500/30 text-xs font-semibold flex items-center gap-1 cursor-pointer"
                >
                  {#if busyKey === `uninstall:${inst.id}`}
                    <Loader2 class="w-3.5 h-3.5 animate-spin" />
                    <span>Removing...</span>
                  {:else}
                    <Trash2 class="w-3.5 h-3.5" />
                    <span>Uninstall</span>
                  {/if}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}

    {:else if activeSubTab === "agents"}
      <div class="flex items-center justify-between">
        <div class="text-xs text-slate-400">
          Detection checks PATH executables, config folders, and existing skills directories.
        </div>
        <button
          type="button"
          disabled={detectingAgents}
          on:click={handleRedetectAgents}
          class="px-3.5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs flex items-center gap-1.5 cursor-pointer"
        >
          <RefreshCw class="w-3.5 h-3.5 {detectingAgents ? 'animate-spin' : ''}" />
          Re-detect Agents
        </button>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        {#each agents as agent}
          <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 space-y-2">
            <div class="flex items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <Cpu class="w-4 h-4 text-cyan-400" />
                <span class="text-sm font-bold text-white">{agent.display_name}</span>
              </div>
              <span class="px-2 py-0.5 rounded-full text-[10px] font-semibold {agent.detected_status === 'Detected' || agent.detected_status === 'ManualConfiguration' ? 'bg-emerald-500/15 text-emerald-300' : agent.detected_status === 'PathInvalid' ? 'bg-rose-500/15 text-rose-300' : 'bg-slate-800 text-slate-400'}">
                {agent.detected_status}
              </span>
            </div>
            <p class="text-[11px] text-slate-400">{agent.detection_reason}</p>
            <div class="text-[11px] font-mono text-slate-300 bg-slate-950/70 p-2 rounded-lg border border-slate-800 space-y-0.5">
              <div class="truncate">Global: {agent.configured_path || agent.global_skill_path}</div>
              <div class="truncate text-slate-400">Project: {agent.project_skill_path}</div>
            </div>
            <div class="flex items-center gap-2 pt-1">
              <button
                type="button"
                on:click={() => handleConfigureAgentPath(agent.id)}
                class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white cursor-pointer"
              >
                Configure Path
              </button>
              {#if agent.configured_path}
                <button
                  type="button"
                  on:click={() => handleClearAgentPath(agent.id)}
                  class="px-3 py-1.5 rounded-lg bg-slate-800/60 hover:bg-slate-700 text-xs text-slate-300 cursor-pointer"
                >
                  Reset Default
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>

    {:else if activeSubTab === "sources"}
      <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 space-y-3">
        <div class="text-xs font-bold text-white">Add Custom Registry / Source</div>
        <div class="flex flex-wrap items-center gap-2">
          <input
            type="text"
            bind:value={newSourceName}
            placeholder="Source Name (e.g. Team Registry)"
            class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white"
          />
          <input
            type="text"
            bind:value={newSourceUrl}
            placeholder="https://example.com/skills-registry.json"
            class="flex-1 min-w-[220px] px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white"
          />
          <button
            type="button"
            on:click={handleAddSource}
            class="px-3.5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs flex items-center gap-1 cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" />
            Add Source
          </button>
        </div>
      </div>

      <div class="space-y-2">
        {#each sources as src}
          <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 flex flex-wrap items-center justify-between gap-3">
            <div>
              <div class="flex items-center gap-2">
                <Globe class="w-4 h-4 text-cyan-400" />
                <span class="text-sm font-bold text-white">{src.name}</span>
                <span class="px-2 py-0.5 rounded-full text-[10px] font-mono bg-slate-800 text-slate-300">{src.provider_type}</span>
              </div>
              <div class="text-[11px] font-mono text-slate-400 mt-1">{src.source_url}</div>
              {#if src.last_error}
                <div class="text-[11px] text-rose-300 mt-1">Error: {src.last_error}</div>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                disabled={busyKey === `sync:${src.id}`}
                on:click={() => handleRefreshSource(src.id)}
                class="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-xs font-semibold text-white flex items-center gap-1 cursor-pointer"
              >
                <RefreshCw class="w-3.5 h-3.5 text-cyan-400 {busyKey === `sync:${src.id}` ? 'animate-spin' : ''}" />
                <span>{busyKey === `sync:${src.id}` ? "Syncing..." : "Sync"}</span>
              </button>
            </div>
          </div>
        {/each}
      </div>

    {:else if activeSubTab === "presets"}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div class="space-y-3">
          <div class="text-xs font-bold text-white flex items-center gap-1.5">
            <Layers class="w-4 h-4 text-cyan-400" />
            <span>Role Presets &amp; Custom Bundles</span>
          </div>
          {#each presets as p}
            <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 space-y-1.5">
              <div class="flex items-center justify-between">
                <span class="text-sm font-bold text-white">{p.name}</span>
                <span class="px-2 py-0.5 rounded-full text-[10px] bg-indigo-500/15 text-indigo-300">
                  {p.is_builtin ? "Built-in" : "Custom"} ({p.skill_ids.length} skills)
                </span>
              </div>
              <p class="text-xs text-slate-400">{p.description}</p>
            </div>
          {/each}

          <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 space-y-2">
            <div class="text-xs font-bold text-white">Create Custom Bundle</div>
            <input
              type="text"
              bind:value={newPresetName}
              placeholder="Preset Name"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white"
            />
            <input
              type="text"
              bind:value={newPresetDesc}
              placeholder="Short description"
              class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white"
            />
            <button
              type="button"
              on:click={handleSavePreset}
              class="px-3.5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-xs cursor-pointer"
            >
              Save Preset
            </button>
          </div>
        </div>

        <div class="p-4 rounded-2xl bg-slate-900/75 border border-slate-800 space-y-3 h-fit">
          <div class="text-xs font-bold text-white flex items-center gap-1.5">
            <Settings class="w-4 h-4 text-cyan-400" />
            <span>Security &amp; Backup Policy</span>
          </div>
          <label class="flex items-center justify-between text-xs text-slate-200">
            <span>Require explicit confirmation for High/Critical findings</span>
            <input type="checkbox" bind:checked={skillsSettings.require_confirm_high_risk} />
          </label>
          <label class="flex items-center justify-between text-xs text-slate-200">
            <span>Block Critical security findings by default</span>
            <input type="checkbox" bind:checked={skillsSettings.block_critical_by_default} />
          </label>
          <label class="flex items-center justify-between text-xs text-slate-200">
            <span>Backup retention count per skill/agent</span>
            <input
              type="number"
              min="1"
              max="20"
              bind:value={skillsSettings.backup_retention_count}
              class="w-20 px-2 py-1 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white text-right"
            />
          </label>
          <button
            type="button"
            disabled={busyKey === "save-settings"}
            on:click={handleSaveSettings}
            class="px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 disabled:opacity-50 text-slate-950 font-bold text-xs flex items-center gap-1.5 cursor-pointer"
          >
            {#if busyKey === "save-settings"}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
              <span>Saving...</span>
            {:else}
              <CheckCircle2 class="w-3.5 h-3.5" />
              <span>Save Policy</span>
            {/if}
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if activeDetail}
  <SkillDetailModal
    detail={activeDetail}
    compatibility={activeCompatibility}
    on:close={() => (activeDetail = null)}
    on:install={(e) => {
      activeDetail = null;
      installSkillId = e.detail;
    }}
    on:share={(e) => {
      activeDetail = null;
      shareSkill = e.detail;
    }}
    on:toggleFavorite={(e) => handleToggleFavorite(e.detail)}
    on:deleteSkill={(e) => handleDeleteSkill(e.detail)}
    on:fetchCatalogSkill={(e) => handleImportGithub(e.detail.url, e.detail.subpath)}
  />
{/if}

{#if installSkillId}
  <SkillInstallModal
    skillId={installSkillId}
    {agents}
    on:close={() => (installSkillId = null)}
    on:installed={async () => {
      await Promise.all([loadInstalled(), loadSkills()]);
    }}
  />
{/if}

{#if shareSkill}
  <SkillShareModal
    skill={shareSkill}
    on:close={() => (shareSkill = null)}
  />
{/if}

<style>
  button {
    transition: transform 0.12s ease, background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  button:active:not(:disabled) {
    transform: scale(0.94);
  }
</style>
