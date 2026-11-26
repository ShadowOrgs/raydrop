<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    X,
    Star,
    Download,
    ShieldAlert,
    ShieldCheck,
    FileText,
    FolderTree,
    ExternalLink,
    Trash2,
    Cpu,
    GitCommit,
    Loader2,
    Share2,
  } from "lucide-svelte";
  import {
    skillsApi,
    type SkillRecord,
    type SkillDetailResponse,
    type SkillFilePreview,
    type SkillCompatibilityEntry,
  } from "../../utils/tauri-ipc";

  export let detail: SkillDetailResponse;
  export let compatibility: SkillCompatibilityEntry[] = [];

  const dispatch = createEventDispatcher<{
    close: void;
    install: string;
    share: SkillRecord;
    toggleFavorite: string;
    deleteSkill: string;
    fetchCatalogSkill: { url: string; subpath?: string | null };
  }>();

  let selectedFilePath: string | null = null;
  let filePreview: SkillFilePreview | null = null;
  let loadingFile = false;
  let fetchingPkg = false;

  $: if (detail && !selectedFilePath && detail.files.length > 0) {
    const md =
      detail.files.find((f) => f.relative_path.toUpperCase() === "SKILL.MD") ||
      detail.files[0];
    if (md) loadPreview(md.relative_path);
  }

  async function loadPreview(relPath: string) {
    selectedFilePath = relPath;
    loadingFile = true;
    try {
      filePreview = await skillsApi.previewFile(detail.skill.id, relPath);
    } catch (e) {
      filePreview = null;
    } finally {
      loadingFile = false;
    }
  }

  function severityBadge(sev: string) {
    switch (sev) {
      case "Critical":
        return "bg-rose-500/20 text-rose-300 border-rose-500/40";
      case "High":
        return "bg-orange-500/20 text-orange-300 border-orange-500/40";
      case "Medium":
        return "bg-amber-500/20 text-amber-300 border-amber-500/40";
      case "Low":
        return "bg-sky-500/20 text-sky-300 border-sky-500/40";
      default:
        return "bg-slate-700/60 text-slate-300 border-slate-600/50";
    }
  }

  function compatBadge(level: string) {
    switch (level) {
      case "Native":
        return "bg-emerald-500/15 text-emerald-300 border-emerald-500/30";
      case "Compatible":
        return "bg-cyan-500/15 text-cyan-300 border-cyan-500/30";
      case "Adapted":
        return "bg-amber-500/15 text-amber-300 border-amber-500/30";
      default:
        return "bg-rose-500/15 text-rose-300 border-rose-500/30";
    }
  }
</script>

<div class="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-md flex items-center justify-center p-3 sm:p-6">
  <div class="w-full max-w-5xl max-h-[90vh] rounded-2xl bg-slate-900 border border-slate-700/80 shadow-2xl flex flex-col overflow-hidden">
    <!-- Header -->
    <div class="px-5 py-4 border-b border-slate-800 flex items-start justify-between gap-4 shrink-0">
      <div class="min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <h3 class="text-lg font-bold text-white truncate">{detail.skill.name}</h3>
          <span class="px-2 py-0.5 rounded-full text-[10px] font-mono bg-slate-800 text-slate-300 border border-slate-700">
            {detail.skill.version || "unversioned"}
          </span>
          <span class="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
            License: {detail.skill.license || "Unknown"}
          </span>
        </div>
        <p class="text-xs text-slate-400 mt-1">{detail.skill.description}</p>
        <div class="flex items-center gap-3 mt-2 text-[11px] text-slate-400 flex-wrap">
          {#if detail.skill.author}
            <span>Author: <strong class="text-slate-200">{detail.skill.author}</strong></span>
          {/if}
          {#if detail.skill.commit_sha}
            <span class="inline-flex items-center gap-1 font-mono text-emerald-300">
              <GitCommit class="w-3 h-3" />
              {detail.skill.commit_sha.slice(0, 10)}
            </span>
          {/if}
          {#if detail.skill.repository_url}
            <a
              href={detail.skill.repository_url}
              target="_blank"
              rel="noreferrer"
              class="inline-flex items-center gap-1 text-cyan-400 hover:underline"
            >
              <ExternalLink class="w-3 h-3" />
              Upstream Repo
            </a>
          {/if}
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <button
          type="button"
          on:click={() => dispatch("toggleFavorite", detail.skill.id)}
          class="p-2 rounded-xl border border-slate-700 bg-slate-800 hover:bg-slate-700 transition cursor-pointer {detail.skill.is_favorite ? 'text-amber-400' : 'text-slate-400'}"
          title="Toggle Favorite"
        >
          <Star class="w-4 h-4 {detail.skill.is_favorite ? 'fill-amber-400' : ''}" />
        </button>

        {#if detail.files.length === 0 && detail.skill.source_url.startsWith("http")}
          <button
            type="button"
            disabled={fetchingPkg}
            on:click={() => {
              fetchingPkg = true;
              dispatch("fetchCatalogSkill", {
                url: detail.skill.source_url,
                subpath: detail.skill.source_path ?? null,
              });
            }}
            class="px-3.5 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-slate-950 font-bold text-xs flex items-center gap-1.5 cursor-pointer"
          >
            {#if fetchingPkg}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
              <span>Fetching...</span>
            {:else}
              <Download class="w-3.5 h-3.5" />
              <span>Fetch Package from GitHub</span>
            {/if}
          </button>
        {:else}
          <button
            type="button"
            on:click={() => dispatch("share", detail.skill)}
            class="p-2 rounded-xl border border-cyan-500/30 bg-cyan-500/10 hover:bg-cyan-500/20 text-cyan-300 cursor-pointer"
            title="Share Skill to Contacts / Devices via P2P"
          >
            <Share2 class="w-4 h-4" />
          </button>
          <button
            type="button"
            on:click={() => dispatch("install", detail.skill.id)}
            class="px-3.5 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold text-xs flex items-center gap-1.5 cursor-pointer"
          >
            <Download class="w-3.5 h-3.5" />
            Install to Agents
          </button>
        {/if}

        <button
          type="button"
          on:click={() => dispatch("deleteSkill", detail.skill.id)}
          class="p-2 rounded-xl border border-rose-500/30 bg-rose-500/10 hover:bg-rose-500/20 text-rose-300 cursor-pointer"
          title="Remove from Managed Library"
        >
          <Trash2 class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={() => dispatch("close")}
          class="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
        >
          <X class="w-4 h-4" />
        </button>
      </div>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto p-5 space-y-5">
      <!-- Security Scan Summary -->
      <div class="rounded-xl border p-4 {detail.skill.security_status === 'high_risk' ? 'bg-rose-500/10 border-rose-500/40' : detail.skill.security_status === 'warnings' ? 'bg-amber-500/10 border-amber-500/40' : 'bg-emerald-500/10 border-emerald-500/30'}">
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-2 font-bold text-xs text-white">
            {#if detail.skill.security_report.findings.length > 0}
              <ShieldAlert class="w-4 h-4 text-amber-400" />
            {:else}
              <ShieldCheck class="w-4 h-4 text-emerald-400" />
            {/if}
            <span>Static Security Scan ({detail.skill.security_status})</span>
          </div>
          <span class="text-[11px] text-slate-300">
            {detail.skill.security_report.scanned_files} file(s) scanned • {detail.skill.security_report.executable_files} script(s)
          </span>
        </div>
        <p class="text-xs text-slate-200 mt-1.5">
          {detail.skill.security_report.summary || "Package has not been downloaded/scanned yet."}
        </p>

        {#if detail.skill.security_report.findings.length > 0}
          <div class="mt-3 space-y-2 max-h-48 overflow-y-auto pr-1">
            {#each detail.skill.security_report.findings as f}
              <div class="rounded-lg bg-slate-950/70 border border-slate-800 p-2.5 text-xs">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="px-1.5 py-0.5 rounded border text-[10px] font-bold {severityBadge(f.severity)}">
                    {f.severity}
                  </span>
                  <span class="font-semibold text-white">{f.title}</span>
                  <span class="font-mono text-[10px] text-slate-400 ml-auto">
                    {f.file_path}{f.line_number ? `:${f.line_number}` : ""}
                  </span>
                </div>
                <p class="text-slate-300 mt-1">{f.explanation}</p>
                <p class="text-[11px] text-slate-400 mt-0.5">
                  <strong>Why:</strong> {f.why_it_matters} • <strong>Action:</strong> {f.recommendation}
                </p>
                {#if f.snippet}
                  <pre class="mt-1.5 p-1.5 rounded bg-black/60 text-[10px] text-amber-200 overflow-x-auto">{f.snippet}</pre>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Agent Compatibility -->
      {#if compatibility.length > 0}
        <div>
          <div class="flex items-center gap-1.5 text-xs font-bold text-slate-200 mb-2">
            <Cpu class="w-4 h-4 text-cyan-400" />
            <span>Agent Compatibility</span>
          </div>
          <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 gap-2">
            {#each compatibility as c}
              <div class="rounded-xl bg-slate-800/60 border border-slate-700/70 p-2.5" title={c.note}>
                <div class="text-xs font-semibold text-white truncate">{c.agent_name}</div>
                <span class="inline-block mt-1 px-1.5 py-0.5 rounded border text-[10px] font-semibold {compatBadge(c.level)}">
                  {c.level}
                </span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- File Tree & Preview -->
      {#if detail.files.length > 0}
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3 min-h-[260px]">
          <div class="rounded-xl bg-slate-950/70 border border-slate-800 p-3 flex flex-col">
            <div class="flex items-center gap-1.5 text-xs font-bold text-slate-300 mb-2">
              <FolderTree class="w-3.5 h-3.5 text-cyan-400" />
              <span>Package Files ({detail.files.length})</span>
            </div>
            <div class="space-y-1 overflow-y-auto max-h-72 pr-1">
              {#each detail.files as f}
                <button
                  type="button"
                  on:click={() => loadPreview(f.relative_path)}
                  class="w-full text-left px-2.5 py-1.5 rounded-lg text-xs font-mono flex items-center justify-between gap-2 cursor-pointer transition {selectedFilePath === f.relative_path ? 'bg-cyan-500/20 text-cyan-200 border border-cyan-500/40' : 'text-slate-300 hover:bg-slate-800'}"
                >
                  <span class="truncate flex items-center gap-1.5">
                    <FileText class="w-3 h-3 shrink-0" />
                    {f.relative_path}
                  </span>
                  <span class="text-[10px] text-slate-500 shrink-0">{f.size}B</span>
                </button>
              {/each}
            </div>
          </div>

          <div class="md:col-span-2 rounded-xl bg-slate-950/70 border border-slate-800 p-3 flex flex-col">
            <div class="flex items-center justify-between text-xs font-mono text-slate-400 pb-2 border-b border-slate-800">
              <span>{selectedFilePath || "Select a file"}</span>
              {#if filePreview}
                <span class="text-[10px]">SHA256: {filePreview.content_hash.slice(0, 12)}</span>
              {/if}
            </div>
            {#if loadingFile}
              <div class="py-10 text-center text-xs text-slate-400">Loading preview...</div>
            {:else if filePreview?.content}
              <pre class="mt-2 text-xs text-slate-200 font-mono whitespace-pre-wrap break-words overflow-y-auto max-h-72">{filePreview.content}</pre>
              {#if filePreview.truncated}
                <div class="text-[10px] text-amber-400 mt-1">Preview truncated at 256 KB.</div>
              {/if}
            {:else if filePreview}
              <div class="py-10 text-center text-xs text-slate-400">Binary file ({filePreview.size} bytes) — preview unavailable.</div>
            {:else}
              <div class="py-10 text-center text-xs text-slate-500">Select a file to inspect its contents before installing.</div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="rounded-xl bg-slate-950/60 border border-slate-800 p-6 text-center text-xs text-slate-400">
          This is a catalog reference entry. Click <strong class="text-cyan-300">Fetch Package from GitHub</strong> above to download, scan, and inspect all files locally.
        </div>
      {/if}
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
