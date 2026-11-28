<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    Bot,
    Check,
    Copy,
    Eye,
    EyeOff,
    Image as ImageIcon,
    Loader2,
    Play,
    RefreshCw,
    Server,
    Terminal,
    Wrench,
    Globe,
    Trash2,
  } from "lucide-svelte";
  import { networkInfo, cloudMyDevices, appSettings } from "../stores/state";
  import { contacts } from "../stores/contacts";

  interface LlmBridgeProvider {
    id: string;
    name: string;
    cli_command: string;
    enabled: boolean;
    port: number;
    api_key: string;
    default_model: string;
    available_models: string[];
    supports_image_gen: boolean;
    is_public: boolean;
    execution_mode?: string;
    extra_args?: string;
    cli_installed: boolean;
    running: boolean;
    last_error: string | null;
  }

  interface LlmBridgeLogEvent {
    provider_id: string;
    provider_name: string;
    method: string;
    path: string;
    model: string;
    status: number;
    latency_ms: number;
    message: string;
    timestamp: number;
  }

  let providers: LlmBridgeProvider[] = [];
  let loading = true;
  let savingId = "";
  let refreshingId = "";
  let testingId = "";
  let copiedId = "";
  let showKeys: Record<string, boolean> = {};
  let testResults: Record<string, { ok: boolean; latency_ms: number; text: string }> = {};
  let logs: LlmBridgeLogEvent[] = [];
  let unlistenLog: UnlistenFn | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let tailscaleIp: string | null = null;
  let localIpNet: string = "";
  let wanHttpBase: string = "https://stun.raydrop.web.id";
  let myDeviceId: string = "";

  interface LlmPeerProxyInfo {
    id: string;
    peer_device_id: string;
    peer_alias: string;
    provider_id: string;
    local_port: number;
    remote_api_key: string;
    running: boolean;
  }

  let peerProxies: LlmPeerProxyInfo[] = [];
  let selectedPeerId = "";
  let selectedPeerProvider = "agy";
  let peerLocalPort = 48851;
  let peerRemoteApiKey = "";
  let peerConnecting = false;
  let peerError = "";
  let peerScanning = false;
  let discoveredPeerBridges: Array<{
    id: string;
    name: string;
    port: number;
    default_model: string;
    supports_image_gen: boolean;
    is_public: boolean;
    api_key: string | null;
  }> = [];
  let activeRemotePeers: Array<{
    id: string;
    name: string;
    bridges: typeof discoveredPeerBridges;
  }> = [];
  let initialPeerScanDone = false;

  $: if (!initialPeerScanDone && authorizedPeers.some((p) => p.online)) {
    initialPeerScanDone = true;
    scanAllActivePeerBridges();
  }

  async function scanAllActivePeerBridges() {
    const onlinePeers = authorizedPeers.filter((ap) => ap.online);
    if (onlinePeers.length === 0) {
      activeRemotePeers = [];
      discoveredPeerBridges = [];
      return;
    }
    peerScanning = true;
    peerError = "";
    try {
      const results = await Promise.allSettled(
        onlinePeers.map(async (ap) => {
          const bridges = await invoke<typeof discoveredPeerBridges>(
            "discover_peer_llm_bridges",
            { peerDeviceId: ap.id },
          );
          return { id: ap.id, name: ap.name, bridges };
        }),
      );
      activeRemotePeers = results
        .filter(
          (r): r is PromiseFulfilledResult<{ id: string; name: string; bridges: typeof discoveredPeerBridges }> =>
            r.status === "fulfilled" && Array.isArray(r.value.bridges) && r.value.bridges.length > 0,
        )
        .map((r) => r.value);

      if (selectedPeerId) {
        const found = activeRemotePeers.find((p) => p.id === selectedPeerId);
        if (found) {
          discoveredPeerBridges = found.bridges;
        } else {
          selectedPeerId = "";
          discoveredPeerBridges = [];
        }
      }
    } finally {
      peerScanning = false;
    }
  }

  async function scanPeerBridges() {
    if (!selectedPeerId) {
      discoveredPeerBridges = [];
      return;
    }
    const cached = activeRemotePeers.find((p) => p.id === selectedPeerId);
    if (cached) {
      discoveredPeerBridges = cached.bridges;
      if (cached.bridges.length > 0) {
        selectedPeerProvider = cached.bridges[0].id;
        peerRemoteApiKey = cached.bridges[0].api_key || "";
      }
      return;
    }
    peerScanning = true;
    peerError = "";
    try {
      discoveredPeerBridges = await invoke("discover_peer_llm_bridges", {
        peerDeviceId: selectedPeerId,
      });
    } catch (err: any) {
      discoveredPeerBridges = [];
      peerError = String(err);
    } finally {
      peerScanning = false;
    }
  }

  $: authorizedPeers = [
    ...$cloudMyDevices
      .filter((d) => d.device_id !== $appSettings.device_id)
      .map((d) => ({ id: d.device_id, name: `${d.device_name} (My Device)`, online: d.is_online })),
    ...$contacts
      .filter((c) => !c.is_own_device)
      .map((c) => ({
        id: c.device_id,
        name: `${c.custom_name || c.alias_name} (${c.trust_level === "trusted_auto_accept" ? "Auto-Accept" : "Contact • Public Only"})`,
        online: c.is_online,
      })),
  ];

  async function loadProviders() {
    loading = true;
    try {
      providers = await invoke<LlmBridgeProvider[]>("get_llm_bridge_status");
      const net = await invoke<{
        local_ip: string;
        tailscale_ip: string | null;
        device_id?: string;
        wan_http_base?: string;
      }>("get_llm_bridge_network_info");
      localIpNet = net.local_ip || $networkInfo.localIp || "127.0.0.1";
      tailscaleIp = net.tailscale_ip;
      if (net.device_id) myDeviceId = net.device_id;
      if (net.wan_http_base) wanHttpBase = net.wan_http_base;
      peerProxies = await invoke<LlmPeerProxyInfo[]>("get_peer_llm_proxies");
      scanAllActivePeerBridges();
    } catch (err) {
      console.warn("Failed to load LLM Bridge status:", err);
    } finally {
      loading = false;
    }
  }

  async function connectPeerProxy() {
    if (!selectedPeerId) return;
    peerConnecting = true;
    peerError = "";
    try {
      const target = authorizedPeers.find((p) => p.id === selectedPeerId);
      peerProxies = await invoke<LlmPeerProxyInfo[]>("start_peer_llm_proxy", {
        peerDeviceId: selectedPeerId,
        peerAlias: target?.name || selectedPeerId,
        providerId: selectedPeerProvider,
        localPort: Number(peerLocalPort),
        remoteApiKey: peerRemoteApiKey.trim(),
      });
    } catch (err: any) {
      peerError = String(err);
    } finally {
      peerConnecting = false;
    }
  }

  async function disconnectPeerProxy(proxyId: string) {
    try {
      peerProxies = await invoke<LlmPeerProxyInfo[]>("stop_peer_llm_proxy", { proxyId });
    } catch (_) {}
  }

  onMount(async () => {
    await loadProviders();
    unlistenLog = await listen<LlmBridgeLogEvent>("llm_bridge_log", (ev) => {
      if (ev.payload) {
        logs = [ev.payload, ...logs.slice(0, 49)];
      }
    });
    unlistenStatus = await listen<LlmBridgeProvider[]>("llm_bridge_status_changed", (ev) => {
      if (Array.isArray(ev.payload)) {
        providers = ev.payload;
      }
    });
  });

  onDestroy(() => {
    if (unlistenLog) unlistenLog();
    if (unlistenStatus) unlistenStatus();
  });

  async function saveProvider(p: LlmBridgeProvider, toggleEnabled?: boolean, togglePublic?: boolean) {
    savingId = p.id;
    const nextEnabled = toggleEnabled !== undefined ? toggleEnabled : p.enabled;
    const nextPublic = togglePublic !== undefined ? togglePublic : p.is_public;
    try {
      providers = await invoke<LlmBridgeProvider[]>("update_llm_bridge_provider", {
        input: {
          id: p.id,
          enabled: nextEnabled,
          port: Number(p.port),
          api_key: p.api_key,
          default_model: p.default_model,
          cli_command: p.cli_command,
          is_public: nextPublic,
          execution_mode: p.execution_mode || "yolo",
          extra_args: p.extra_args || "",
        },
      });
    } catch (err: any) {
      testResults = {
        ...testResults,
        [p.id]: { ok: false, latency_ms: 0, text: String(err) },
      };
    } finally {
      savingId = "";
    }
  }

  async function refreshModels(p: LlmBridgeProvider) {
    refreshingId = p.id;
    try {
      providers = await invoke<LlmBridgeProvider[]>("refresh_llm_bridge_models", {
        providerId: p.id,
      });
    } catch (err: any) {
      testResults = {
        ...testResults,
        [p.id]: { ok: false, latency_ms: 0, text: String(err) },
      };
    } finally {
      refreshingId = "";
    }
  }

  async function testProvider(p: LlmBridgeProvider) {
    testingId = p.id;
    try {
      const res = await invoke<{ ok: boolean; latency_ms: number; reply?: string; error?: string }>(
        "test_llm_bridge_provider",
        { providerId: p.id },
      );
      testResults = {
        ...testResults,
        [p.id]: {
          ok: res.ok,
          latency_ms: res.latency_ms,
          text: res.ok ? `OK (${res.latency_ms}ms): ${res.reply || ""}` : res.error || "Failed",
        },
      };
    } catch (err: any) {
      testResults = {
        ...testResults,
        [p.id]: { ok: false, latency_ms: 0, text: String(err) },
      };
    } finally {
      testingId = "";
    }
  }

  function regenerateKey(p: LlmBridgeProvider) {
    const randHex = Array.from(crypto.getRandomValues(new Uint8Array(8)))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
    p.api_key = `sk-${p.id}-${randHex}`;
    providers = [...providers];
  }

  function copyBaseUrl(p: LlmBridgeProvider) {
    const hostIp = $networkInfo.localIp || "127.0.0.1";
    const url = `http://${hostIp}:${p.port}/v1`;
    navigator.clipboard.writeText(url);
    copiedId = p.id;
    setTimeout(() => (copiedId = ""), 2000);
  }
</script>

<div class="h-full w-full flex flex-col gap-4 p-4 sm:p-5 overflow-y-auto text-slate-100">
  <!-- Header -->
  <div class="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-white/10">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-500/25 to-indigo-500/25 border border-cyan-400/30 flex items-center justify-center">
        <Bot class="w-5 h-5 text-cyan-400" />
      </div>
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-base font-extrabold tracking-tight text-white">AI CLI LLM Bridge (OpenAI-Compatible)</h2>
          <span class="px-2 py-0.5 rounded-full text-[10px] font-bold bg-cyan-500/15 text-cyan-300 border border-cyan-500/30">
            /v1/chat • /v1/models • /v1/images
          </span>
        </div>
        <p class="text-xs text-slate-400">
          Expose AGY, Claude Code, Codex &amp; Gemini CLI sebagai endpoint OpenAI-compatible dengan port, API key, Tool Calling &amp; Image Gen masing-masing.
        </p>
      </div>
    </div>

    <button
      type="button"
      disabled={loading}
      on:click={loadProviders}
      class="px-3 py-1.5 rounded-xl bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
    >
      <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
      <span>Refresh Status</span>
    </button>
  </div>

  <!-- Providers Grid -->
  {#if loading && providers.length === 0}
    <div class="flex items-center justify-center py-12 text-xs text-slate-400 gap-2">
      <Loader2 class="w-4 h-4 animate-spin text-cyan-400" />
      <span>Loading LLM Bridge providers...</span>
    </div>
  {:else}
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
      {#each providers as p (p.id)}
        {@const baseUrl = `http://${$networkInfo.localIp || '127.0.0.1'}:${p.port}/v1`}
        <div class="rounded-2xl bg-slate-900/75 border {p.running ? 'border-emerald-500/40' : 'border-white/10'} p-4 space-y-3">
          <!-- Card Header -->
          <div class="flex items-start justify-between gap-3">
            <div class="space-y-1">
              <div class="flex flex-wrap items-center gap-2">
                <h3 class="text-sm font-bold text-white">{p.name}</h3>
                <span class="px-2 py-0.5 rounded-full text-[10px] font-bold {p.cli_installed
                  ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                  : 'bg-rose-500/15 text-rose-300 border border-rose-500/30'}">
                  {p.cli_installed ? `CLI: ${p.cli_command} Ready` : `CLI '${p.cli_command}' Missing`}
                </span>
                {#if p.supports_image_gen}
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-bold bg-indigo-500/20 text-indigo-300 border border-indigo-400/30 flex items-center gap-1">
                    <ImageIcon class="w-3 h-3" /> Gen Image
                  </span>
                {/if}
                <span class="px-2 py-0.5 rounded-full text-[10px] font-bold bg-cyan-500/15 text-cyan-300 border border-cyan-400/30 flex items-center gap-1">
                  <Wrench class="w-3 h-3" /> Tool Call
                </span>
              </div>
              <div class="flex flex-wrap items-center gap-2 text-[11px] font-mono text-cyan-300">
                <Server class="w-3.5 h-3.5 text-slate-400 shrink-0" />
                <span>LAN: {`http://${localIpNet || $networkInfo.localIp || '127.0.0.1'}:${p.port}/v1`}</span>
                <button
                  type="button"
                  on:click={() => copyBaseUrl(p)}
                  class="p-1 rounded hover:bg-white/10 text-slate-400 hover:text-white cursor-pointer"
                  title="Copy LAN Base URL"
                >
                  {#if copiedId === p.id}
                    <Check class="w-3 h-3 text-emerald-400" />
                  {:else}
                    <Copy class="w-3 h-3" />
                  {/if}
                </button>
              </div>
              {#if tailscaleIp}
                <div class="flex items-center gap-2 text-[11px] font-mono text-indigo-300">
                  <Globe class="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                  <span>Tailscale: {`http://${tailscaleIp}:${p.port}/v1`}</span>
                  <button
                    type="button"
                    on:click={() => {
                      navigator.clipboard.writeText(`http://${tailscaleIp}:${p.port}/v1`);
                      copiedId = `${p.id}-ts`;
                      setTimeout(() => (copiedId = ""), 2000);
                    }}
                    class="p-1 rounded hover:bg-white/10 text-slate-400 hover:text-white cursor-pointer"
                    title="Copy Tailscale Base URL"
                  >
                    {#if copiedId === `${p.id}-ts`}
                      <Check class="w-3 h-3 text-emerald-400" />
                    {:else}
                      <Copy class="w-3 h-3" />
                    {/if}
                  </button>
                </div>
              {/if}
              {#if myDeviceId}
                <div class="flex items-center gap-2 text-[11px] font-mono text-emerald-300">
                  <Globe class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                  <span class="truncate">WAN Relay: {`${wanHttpBase}/llm/${myDeviceId}/${p.id}/v1`}</span>
                  <button
                    type="button"
                    on:click={() => {
                      navigator.clipboard.writeText(`${wanHttpBase}/llm/${myDeviceId}/${p.id}/v1`);
                      copiedId = `${p.id}-wan`;
                      setTimeout(() => (copiedId = ""), 2000);
                    }}
                    class="p-1 rounded hover:bg-white/10 text-slate-400 hover:text-white transition active:scale-95 cursor-pointer shrink-0"
                    title="Copy WAN Relay Base URL"
                  >
                    {#if copiedId === `${p.id}-wan`}
                      <Check class="w-3 h-3 text-emerald-400" />
                    {:else}
                      <Copy class="w-3 h-3" />
                    {/if}
                  </button>
                </div>
              {/if}
            </div>

            <!-- Public/Private + ON/OFF Controls -->
            <div class="flex items-center gap-1.5 shrink-0">
              <button
                type="button"
                disabled={savingId === p.id}
                on:click={() => saveProvider(p, undefined, !p.is_public)}
                class="px-2.5 py-1.5 rounded-xl text-[11px] font-bold border transition cursor-pointer {p.is_public
                  ? 'bg-amber-500/20 text-amber-300 border-amber-500/40'
                  : 'bg-slate-800 text-slate-300 border-slate-700 hover:text-white'}"
                title={p.is_public
                  ? "Public: Semua kontak di Raydrop bisa akses (tetap pakai API Key)"
                  : "Private: Hanya device milik sendiri & kontak auto-accept"}
              >
                {p.is_public ? "Public" : "Private"}
              </button>

              <button
                type="button"
                disabled={savingId === p.id}
                on:click={() => saveProvider(p, !p.enabled)}
                class="px-3 py-1.5 rounded-xl text-xs font-bold border transition cursor-pointer flex items-center gap-1.5 {p.running
                  ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                  : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-white'}"
              >
                {#if savingId === p.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin" />
                {:else}
                  <span class="w-2 h-2 rounded-full {p.running ? 'bg-emerald-400 animate-pulse' : 'bg-slate-500'}"></span>
                {/if}
                <span>{p.running ? "ON" : "OFF"}</span>
              </button>
            </div>
          </div>

          {#if p.last_error}
            <div class="p-2 rounded-lg bg-rose-500/15 border border-rose-500/30 text-[11px] text-rose-300">
              {p.last_error}
            </div>
          {/if}

          <!-- Config Inputs -->
          <div class="grid grid-cols-1 sm:grid-cols-12 gap-2.5 text-xs">
            <div class="sm:col-span-3">
              <div class="flex items-center justify-between mb-1">
                <label class="text-[10px] font-semibold text-slate-400 uppercase">Port</label>
                <button
                  type="button"
                  on:click={() => saveProvider({ ...p, port: 0 })}
                  class="text-[10px] text-cyan-400 hover:text-cyan-300 cursor-pointer"
                  title="Assign free dynamic port automatically"
                >
                  Auto
                </button>
              </div>
              <input
                type="number"
                min="0"
                max="65535"
                bind:value={p.port}
                class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
              />
            </div>

            <div class="sm:col-span-3">
              <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">CLI Binary</label>
              <input
                type="text"
                bind:value={p.cli_command}
                class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
              />
            </div>

            <div class="sm:col-span-6">
              <div class="flex items-center justify-between mb-1">
                <label class="text-[10px] font-semibold text-slate-400 uppercase">Default Model</label>
                <button
                  type="button"
                  disabled={refreshingId === p.id}
                  on:click={() => refreshModels(p)}
                  class="text-[10px] text-cyan-400 hover:text-cyan-300 flex items-center gap-1 cursor-pointer"
                >
                  <RefreshCw class="w-2.5 h-2.5 {refreshingId === p.id ? 'animate-spin' : ''}" />
                  <span>Fetch Models ({p.available_models.length})</span>
                </button>
              </div>
              <div class="flex gap-1.5">
                <select
                  bind:value={p.default_model}
                  class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
                >
                  {#each p.available_models as m}
                    <option value={m}>{m}</option>
                  {/each}
                  {#if p.default_model && !p.available_models.includes(p.default_model)}
                    <option value={p.default_model}>{p.default_model}</option>
                  {/if}
                </select>
              </div>
            </div>

            <div class="sm:col-span-5">
              <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">
                Execution Mode
              </label>
              <select
                value={p.execution_mode || "yolo"}
                on:change={(e) => {
                  p.execution_mode = e.currentTarget.value;
                }}
                class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white text-xs outline-none focus:border-cyan-400"
              >
                <option value="yolo">YOLO (Auto-Approve / Headless)</option>
                <option value="auto">Auto-Edit (Workspace Safe)</option>
                <option value="safe">Safe (Read-Only / No Bypass)</option>
              </select>
            </div>

            <div class="sm:col-span-7">
              <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">
                Extra CLI Flags (Opsional)
              </label>
              <input
                type="text"
                bind:value={p.extra_args}
                placeholder="misal: --print-timeout 60s"
                class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
              />
            </div>

            <div class="sm:col-span-12">
              <div class="flex items-center justify-between mb-1">
                <label class="text-[10px] font-semibold text-slate-400 uppercase">API Key (Bearer Token)</label>
                <button
                  type="button"
                  on:click={() => regenerateKey(p)}
                  class="text-[10px] text-amber-400 hover:text-amber-300 cursor-pointer"
                >
                  Regenerate Key
                </button>
              </div>
              <div class="flex gap-1.5">
                {#if showKeys[p.id]}
                  <input
                    type="text"
                    bind:value={p.api_key}
                    class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
                  />
                {:else}
                  <input
                    type="password"
                    bind:value={p.api_key}
                    class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none focus:border-cyan-400"
                  />
                {/if}
                <button
                  type="button"
                  on:click={() => (showKeys = { ...showKeys, [p.id]: !showKeys[p.id] })}
                  class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700 cursor-pointer"
                  title="Show/Hide Key"
                >
                  {#if showKeys[p.id]}
                    <EyeOff class="w-3.5 h-3.5" />
                  {:else}
                    <Eye class="w-3.5 h-3.5" />
                  {/if}
                </button>
              </div>
            </div>
          </div>

          <!-- Actions & Test Result -->
          <div class="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-white/5">
            <div class="text-[11px] font-mono truncate max-w-xs">
              {#if testResults[p.id]}
                <span class={testResults[p.id].ok ? "text-emerald-400" : "text-rose-400"}>
                  {testResults[p.id].text}
                </span>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                disabled={testingId === p.id}
                on:click={() => testProvider(p)}
                class="px-3 py-1.5 rounded-lg bg-white/5 hover:bg-white/10 border border-white/10 text-xs font-semibold flex items-center gap-1.5 cursor-pointer"
              >
                {#if testingId === p.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin text-cyan-400" />
                  <span>Testing...</span>
                {:else}
                  <Play class="w-3.5 h-3.5 text-emerald-400" />
                  <span>Test CLI</span>
                {/if}
              </button>
              <button
                type="button"
                disabled={savingId === p.id}
                on:click={() => saveProvider(p)}
                class="px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
              >
                {#if savingId === p.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin" />
                {/if}
                <span>Save &amp; Apply</span>
              </button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Remote Peer LLM Bridges (Raydrop WAN / LAN Relay without Tailscale) -->
  <div class="rounded-2xl bg-slate-900/75 border border-indigo-500/25 p-4 space-y-3">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h3 class="text-xs font-bold uppercase tracking-wider text-white flex items-center gap-2">
          <Globe class="w-4 h-4 text-indigo-400" />
          <span>Remote Peer LLM Bridges (Raydrop WAN / LAN Relay)</span>
        </h3>
        <p class="text-[11px] text-slate-400 mt-0.5">
          Hanya menampilkan device online yang LLM Bridge-nya sedang ON dan diizinkan (Public atau Private milik sendiri / Auto-Accept).
        </p>
      </div>
      <button
        type="button"
        disabled={peerScanning}
        on:click={scanAllActivePeerBridges}
        class="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-50 text-slate-200 text-xs font-semibold flex items-center gap-1.5 transition active:scale-[0.97] cursor-pointer"
      >
        <RefreshCw class="w-3.5 h-3.5 {peerScanning ? 'animate-spin' : ''}" />
        <span>{peerScanning ? "Scanning..." : "Scan Active"}</span>
      </button>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-12 gap-2 text-xs">
      <div class="sm:col-span-4">
        <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">Peer Device</label>
        <select
          bind:value={selectedPeerId}
          on:change={scanPeerBridges}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white text-xs outline-none focus:border-indigo-400"
        >
          <option value="">
            {peerScanning
              ? "Scanning active bridges..."
              : activeRemotePeers.length === 0
              ? "-- Tidak ada bridge aktif --"
              : `-- Pilih Device (${activeRemotePeers.length} aktif) --`}
          </option>
          {#each activeRemotePeers as ap}
            <option value={ap.id}>{ap.name} • {ap.bridges.length} ON</option>
          {/each}
        </select>
      </div>
      <div class="sm:col-span-2">
        <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">CLI Target</label>
        <select
          bind:value={selectedPeerProvider}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none"
        >
          <option value="agy">AGY</option>
          <option value="claude">Claude</option>
          <option value="codex">Codex</option>
          <option value="gemini">Gemini</option>
          <option value="opencode">OpenCode</option>
        </select>
      </div>
      <div class="sm:col-span-2">
        <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">Local Port</label>
        <input
          type="number"
          min="1024"
          max="65535"
          bind:value={peerLocalPort}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none"
        />
      </div>
      <div class="sm:col-span-4">
        <label class="block text-[10px] font-semibold text-slate-400 uppercase mb-1">Host API Key (Opsional)</label>
        <div class="flex gap-1.5">
          <input
            type="password"
            bind:value={peerRemoteApiKey}
            placeholder="sk-..."
            class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-white font-mono text-xs outline-none"
          />
          <button
            type="button"
            disabled={peerConnecting || !selectedPeerId}
            on:click={connectPeerProxy}
            class="px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 disabled:opacity-45 disabled:cursor-not-allowed text-slate-950 font-bold text-xs flex items-center gap-1 transition active:scale-[0.97] cursor-pointer shrink-0"
          >
            {#if peerConnecting}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
            {/if}
            <span>{peerConnecting ? "Connecting..." : "Connect"}</span>
          </button>
        </div>
      </div>
    </div>

    {#if peerScanning}
      <div class="text-xs text-cyan-300 flex items-center gap-1.5">
        <Loader2 class="w-3.5 h-3.5 animate-spin" />
        <span>Scanning active LLM Bridges on peer...</span>
      </div>
    {:else if discoveredPeerBridges.length > 0}
      <div class="flex flex-wrap gap-2">
        {#each discoveredPeerBridges as db}
          <button
            type="button"
            on:click={() => {
              selectedPeerProvider = db.id;
              if (db.api_key) peerRemoteApiKey = db.api_key;
              connectPeerProxy();
            }}
            class="px-2.5 py-1.5 rounded-lg bg-slate-950/90 hover:bg-indigo-500/20 border border-indigo-500/35 text-xs flex items-center gap-2 cursor-pointer"
          >
            <span class="w-2 h-2 rounded-full bg-emerald-400"></span>
            <span class="font-bold text-white">{db.name}</span>
            <span class="font-mono text-[10px] text-cyan-300">{db.default_model}</span>
            <span class="px-1.5 py-0.2 rounded text-[9px] font-bold {db.is_public ? 'bg-amber-500/20 text-amber-300' : 'bg-slate-800 text-slate-300'}">
              {db.is_public ? "Public" : "Private"}
            </span>
          </button>
        {/each}
      </div>
    {/if}

    {#if peerError}
      <div class="text-xs text-rose-400">{peerError}</div>
    {/if}

    {#if peerProxies.length > 0}
      <div class="space-y-1.5">
        {#each peerProxies as px}
          <div class="flex flex-wrap items-center justify-between gap-2 px-3 py-2 rounded-xl bg-slate-950/70 border border-emerald-500/30 text-xs font-mono">
            <div class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full bg-emerald-400 animate-pulse"></span>
              <span class="text-white font-sans font-semibold">{px.peer_alias}</span>
              <span class="text-indigo-300">[{px.provider_id}]</span>
              <span class="text-cyan-300">http://127.0.0.1:{px.local_port}/v1</span>
            </div>
            <button
              type="button"
              on:click={() => disconnectPeerProxy(px.id)}
              class="p-1 rounded hover:bg-rose-500/20 text-rose-400 cursor-pointer"
              title="Disconnect Peer Bridge"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Live Bridge Request & Error Log -->
  <div class="rounded-2xl bg-slate-900/70 border border-white/10 p-4">
    <div class="flex items-center justify-between mb-2">
      <div class="flex items-center gap-2">
        <Terminal class="w-4 h-4 text-cyan-400" />
        <h3 class="text-xs font-bold uppercase tracking-wider text-white">Live API Request &amp; Error Logs ({logs.length})</h3>
      </div>
      {#if logs.length > 0}
        <button
          type="button"
          on:click={() => (logs = [])}
          class="text-[11px] text-slate-400 hover:text-white cursor-pointer"
        >
          Clear
        </button>
      {/if}
    </div>
    {#if logs.length === 0}
      <div class="text-xs text-slate-500 py-3">
        Belum ada request masuk. Arahkan client OpenAI kamu ke Base URL &amp; API Key provider yang aktif (ON).
      </div>
    {:else}
      <div class="space-y-1.5 max-h-56 overflow-y-auto font-mono text-[11px]">
        {#each logs as l}
          <div class="flex flex-wrap items-center gap-2 px-2.5 py-1.5 rounded-lg bg-slate-950/70 border border-white/5">
            <span class="font-bold {l.status >= 400 ? 'text-rose-400' : 'text-emerald-400'}">{l.status}</span>
            <span class="text-cyan-300">[{l.provider_id}]</span>
            <span class="text-slate-300">{l.method} {l.path}</span>
            <span class="text-indigo-300">({l.model})</span>
            <span class="text-slate-400">{l.latency_ms}ms</span>
            <span class="text-slate-200 truncate flex-1">{l.message}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
