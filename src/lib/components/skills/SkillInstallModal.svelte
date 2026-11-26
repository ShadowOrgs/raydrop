<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { X, FolderOpen, ShieldAlert, CheckCircle2, AlertTriangle, Download, Loader2 } from "lucide-svelte";
  import {
    skillsApi,
    type SkillAgentRecord,
    type SkillInstallPreviewResponse,
    type SkillInstallTargetResult,
  } from "../../utils/tauri-ipc";

  export let skillId: string;
  export let agents: SkillAgentRecord[] = [];

  const dispatch = createEventDispatcher<{
    close: void;
    installed: SkillInstallTargetResult[];
  }>();

  let selectedAgentIds: string[] = [];
  let scope: "global" | "project" = "global";
  let projectPath = "";
  let conflictPolicy: "backup_and_replace" | "update" | "keep_existing" = "backup_and_replace";
  let confirmHighRisk = false;

  let preview: SkillInstallPreviewResponse | null = null;
  let loadingPreview = false;
  let installing = false;
  let errorMsg = "";
  let results: SkillInstallTargetResult[] = [];

  onMount(() => {
    const detected = agents
      .filter((a) => a.detected_status === "Detected" || a.detected_status === "ManualConfiguration")
      .map((a) => a.id);
    selectedAgentIds = detected.length > 0 ? detected : agents.slice(0, 2).map((a) => a.id);
    refreshPreview();
  });

  async function pickProjectFolder() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Select Target Project Directory",
      });
      if (typeof picked === "string") {
        projectPath = picked;
        refreshPreview();
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function toggleAgent(id: string) {
    if (selectedAgentIds.includes(id)) {
      selectedAgentIds = selectedAgentIds.filter((x) => x !== id);
    } else {
      selectedAgentIds = [...selectedAgentIds, id];
    }
    refreshPreview();
  }

  async function refreshPreview() {
    if (selectedAgentIds.length === 0) {
      preview = null;
      return;
    }
    if (scope === "project" && !projectPath.trim()) {
      preview = null;
      return;
    }
    loadingPreview = true;
    errorMsg = "";
    try {
      preview = await skillsApi.previewInstall(
        skillId,
        selectedAgentIds,
        scope,
        scope === "project" ? projectPath.trim() : null
      );
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loadingPreview = false;
    }
  }

  $: hasRisk =
    preview?.security_report.highest_severity === "High" ||
    preview?.security_report.highest_severity === "Critical";

  async function runInstall() {
    if (selectedAgentIds.length === 0) return;
    installing = true;
    errorMsg = "";
    try {
      results = await skillsApi.install(
        skillId,
        selectedAgentIds,
        scope,
        scope === "project" ? projectPath.trim() : null,
        conflictPolicy,
        confirmHighRisk
      );
      dispatch("installed", results);
    } catch (e) {
      errorMsg = String(e);
    } finally {
      installing = false;
    }
  }
</script>

<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-3 sm:p-6">
  <div class="w-full max-w-2xl max-h-[90vh] rounded-2xl bg-slate-900 border border-slate-700/80 shadow-2xl flex flex-col overflow-hidden">
    <div class="px-5 py-4 border-b border-slate-800 flex items-center justify-between">
      <div>
        <h3 class="text-base font-bold text-white">Install Skill</h3>
        <p class="text-xs text-slate-400">{preview?.skill_name || skillId}</p>
      </div>
      <button
        type="button"
        on:click={() => dispatch("close")}
        class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-5 space-y-4">
      <!-- Scope -->
      <div>
        <div class="text-xs font-bold text-slate-200 mb-2">1. Install Scope</div>
        <div class="grid grid-cols-2 gap-2">
          <button
            type="button"
            on:click={() => {
              scope = "global";
              refreshPreview();
            }}
            class="p-2.5 rounded-xl border text-left text-xs cursor-pointer transition {scope === 'global' ? 'bg-cyan-500/15 border-cyan-500/50 text-white' : 'bg-slate-800/60 border-slate-700 text-slate-300'}"
          >
            <div class="font-bold">Global (User)</div>
            <div class="text-[11px] text-slate-400">Available across all projects</div>
          </button>
          <button
            type="button"
            on:click={() => {
              scope = "project";
              refreshPreview();
            }}
            class="p-2.5 rounded-xl border text-left text-xs cursor-pointer transition {scope === 'project' ? 'bg-cyan-500/15 border-cyan-500/50 text-white' : 'bg-slate-800/60 border-slate-700 text-slate-300'}"
          >
            <div class="font-bold">Project Directory</div>
            <div class="text-[11px] text-slate-400">Installed into a specific repo</div>
          </button>
        </div>

        {#if scope === "project"}
          <div class="mt-2 flex items-center gap-2">
            <input
              type="text"
              bind:value={projectPath}
              on:change={refreshPreview}
              placeholder="Select project folder..."
              class="flex-1 px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white font-mono"
            />
            <button
              type="button"
              on:click={pickProjectFolder}
              class="px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-white flex items-center gap-1.5 cursor-pointer"
            >
              <FolderOpen class="w-3.5 h-3.5 text-cyan-400" />
              Browse
            </button>
          </div>
        {/if}
      </div>

      <!-- Agents -->
      <div>
        <div class="text-xs font-bold text-slate-200 mb-2">2. Target Coding Agents</div>
        <div class="grid grid-cols-2 sm:grid-cols-3 gap-2">
          {#each agents as agent}
            {@const checked = selectedAgentIds.includes(agent.id)}
            <button
              type="button"
              on:click={() => toggleAgent(agent.id)}
              class="p-2.5 rounded-xl border text-left text-xs cursor-pointer transition {checked ? 'bg-emerald-500/15 border-emerald-500/50 text-white' : 'bg-slate-800/50 border-slate-700/70 text-slate-300'}"
            >
              <div class="flex items-center justify-between gap-1">
                <span class="font-semibold truncate">{agent.display_name}</span>
                <span class="text-[10px] px-1.5 py-0.2 rounded bg-slate-950/60 text-slate-300">
                  {agent.detected_status === "Detected" || agent.detected_status === "ManualConfiguration" ? "Ready" : "Path"}
                </span>
              </div>
              <div class="text-[10px] text-slate-400 mt-0.5 truncate">{agent.compatibility_level}</div>
            </button>
          {/each}
        </div>
      </div>

      <!-- Conflict policy -->
      <div>
        <div class="text-xs font-bold text-slate-200 mb-2">3. Conflict Policy</div>
        <select
          bind:value={conflictPolicy}
          class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white"
        >
          <option value="backup_and_replace">Backup existing &amp; Replace</option>
          <option value="update">Update (with automatic backup)</option>
          <option value="keep_existing">Keep existing (skip conflicts)</option>
        </select>
      </div>

      <!-- Destination Preview -->
      {#if loadingPreview}
        <div class="text-xs text-slate-400 py-2">Resolving destination paths...</div>
      {:else if preview}
        <div class="space-y-2">
          <div class="text-xs font-bold text-slate-200">4. Destination Preview</div>
          {#each preview.targets as t}
            <div class="rounded-xl bg-slate-950/70 border border-slate-800 p-2.5 text-xs">
              <div class="flex items-center justify-between gap-2">
                <span class="font-semibold text-white">{t.agent_name}</span>
                {#if !t.allowed}
                  <span class="text-rose-400 text-[11px]">{t.block_reason}</span>
                {:else if t.conflict_exists}
                  <span class="text-amber-300 text-[11px] flex items-center gap-1">
                    <AlertTriangle class="w-3 h-3" />
                    {t.conflict_same_hash ? "Already identical" : `Conflict (${t.existing_files_count} files)`}
                  </span>
                {:else}
                  <span class="text-emerald-400 text-[11px]">Ready</span>
                {/if}
              </div>
              {#if t.destination_path}
                <div class="text-[10px] font-mono text-slate-400 mt-1 break-all">{t.destination_path}</div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      {#if hasRisk}
        <label class="flex items-start gap-2.5 p-3 rounded-xl bg-rose-500/15 border border-rose-500/40 text-xs text-rose-200 cursor-pointer">
          <input type="checkbox" bind:checked={confirmHighRisk} class="mt-0.5" />
          <span>
            <strong class="inline-flex items-center gap-1"><ShieldAlert class="w-3.5 h-3.5" /> High/Critical findings detected:</strong>
            I have reviewed the skill files and security findings and want to proceed with installation.
          </span>
        </label>
      {/if}

      {#if errorMsg}
        <div class="p-3 rounded-xl bg-rose-500/15 border border-rose-500/40 text-xs text-rose-200">
          {errorMsg}
        </div>
      {/if}

      {#if results.length > 0}
        <div class="space-y-1.5">
          <div class="text-xs font-bold text-emerald-300">Installation Results</div>
          {#each results as r}
            <div class="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-xs flex items-center justify-between gap-2">
              <span class="font-semibold text-white flex items-center gap-1.5">
                {#if r.success}
                  <CheckCircle2 class="w-3.5 h-3.5 text-emerald-400" />
                {:else}
                  <AlertTriangle class="w-3.5 h-3.5 text-rose-400" />
                {/if}
                {r.agent_name}
              </span>
              <span class="text-[11px] {r.success ? 'text-emerald-300' : 'text-rose-300'}">
                {r.success ? r.action_taken : r.error}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="px-5 py-3.5 border-t border-slate-800 flex items-center justify-end gap-2">
      <button
        type="button"
        on:click={() => dispatch("close")}
        class="px-4 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 cursor-pointer"
      >
        Close
      </button>
      <button
        type="button"
        disabled={installing || selectedAgentIds.length === 0 || (scope === "project" && !projectPath.trim()) || (hasRisk && !confirmHighRisk)}
        on:click={runInstall}
        class="px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 disabled:opacity-40 text-slate-950 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
      >
        {#if installing}
          <Loader2 class="w-3.5 h-3.5 animate-spin" />
          <span>Installing...</span>
        {:else}
          <Download class="w-3.5 h-3.5" />
          <span>Confirm Install</span>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  button {
    transition: transform 0.12s ease, background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }
  button:active:not(:disabled) {
    transform: scale(0.94);
  }
</style>
