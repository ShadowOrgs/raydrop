<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    Bot,
    Server,
    Send,
    Sparkles,
    Eye,
    EyeOff,
    Plus,
    Trash2,
    Save,
    CheckCircle2,
    AlertCircle,
    Copy,
    Check,
    RefreshCw,
    RotateCcw,
  } from "lucide-svelte";
  import {
    raydropApi,
    type AiMcpConfig,
    type AiMcpRuntimeStatus,
  } from "../utils/tauri-ipc";

  const PROVIDER_PRESETS: Record<
    string,
    { label: string; baseUrl: string; defaultModel: string }
  > = {
    openai: {
      label: "OpenAI",
      baseUrl: "https://api.openai.com/v1",
      defaultModel: "gpt-4o-mini",
    },
    gemini: {
      label: "Google Gemini",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
      defaultModel: "gemini-2.5-flash",
    },
    anthropic: {
      label: "Anthropic Claude",
      baseUrl: "https://api.anthropic.com/v1",
      defaultModel: "claude-3-5-sonnet-latest",
    },
    deepseek: {
      label: "DeepSeek",
      baseUrl: "https://api.deepseek.com/v1",
      defaultModel: "deepseek-chat",
    },
    groq: {
      label: "Groq",
      baseUrl: "https://api.groq.com/openai/v1",
      defaultModel: "llama-3.3-70b-versatile",
    },
    openrouter: {
      label: "OpenRouter",
      baseUrl: "https://openrouter.ai/api/v1",
      defaultModel: "openai/gpt-4o-mini",
    },
    ollama: {
      label: "Ollama (Local)",
      baseUrl: "http://localhost:11434/v1",
      defaultModel: "llama3.2",
    },
    custom: {
      label: "Custom OpenAI-Compatible",
      baseUrl: "",
      defaultModel: "",
    },
  };

  const DEFAULT_PERSONA_PROMPT =
    "You are RayBot, a helpful AI assistant controlling the user's RayDrop PC, RayAmp media player, Wake-on-LAN, notes, clipboard, and system telemetry via MCP tools. Always confirm actions clearly and concisely.";

  let config: AiMcpConfig = {
    mcp_enabled: true,
    mcp_port: 48765,
    mcp_auth_token: "",
    llm_provider: "openai",
    llm_base_url: "https://api.openai.com/v1",
    llm_api_key: "",
    llm_model: "gpt-4o-mini",
    llm_temperature: 0.7,
    llm_max_tokens: 2048,
    llm_prompt_tool_mode: false,
    persona_name: "RayBot",
    persona_preset: "assistant",
    persona_language: "auto",
    persona_system_prompt: DEFAULT_PERSONA_PROMPT,
    telegram_enabled: false,
    telegram_bot_token: "",
    telegram_allowed_users: [],
  };

  let runtime: AiMcpRuntimeStatus | null = null;
  let showApiKey = false;
  let showBotToken = false;
  let newTgUserId = "";
  let newTgLabel = "";
  let saving = false;
  let testingLlm = false;
  let testingTg = false;
  let feedbackMsg = "";
  let errorMsg = "";
  let copiedMcpJson = false;
  let localBridges: Array<{
    id: string;
    name: string;
    port: number;
    api_key: string;
    default_model: string;
    running: boolean;
  }> = [];
  let peerProxies: Array<{
    id: string;
    peer_alias: string;
    provider_id: string;
    local_port: number;
    remote_api_key: string;
  }> = [];

  onMount(async () => {
    try {
      config = await raydropApi.getAiMcpConfig();
      runtime = await raydropApi.getAiMcpRuntimeStatus();
    } catch (_) {}
    try {
      localBridges = await invoke("get_llm_bridge_status");
      peerProxies = await invoke("get_peer_llm_proxies");
    } catch (_) {}
  });

  function handleProviderChange(providerKey: string) {
    config.llm_provider = providerKey;
    if (providerKey.startsWith("bridge:")) {
      const bid = providerKey.slice(7);
      const found = localBridges.find((b) => b.id === bid);
      if (found) {
        config.llm_base_url = `http://127.0.0.1:${found.port}/v1`;
        config.llm_api_key = found.api_key;
        config.llm_model = found.default_model;
      }
      return;
    }
    if (providerKey.startsWith("peer:")) {
      const pid = providerKey.slice(5);
      const found = peerProxies.find((p) => p.id === pid);
      if (found) {
        config.llm_base_url = `http://127.0.0.1:${found.local_port}/v1`;
        config.llm_api_key = found.remote_api_key;
        config.llm_model = found.provider_id;
      }
      return;
    }
    const preset = PROVIDER_PRESETS[providerKey];
    if (preset && providerKey !== "custom") {
      config.llm_base_url = preset.baseUrl;
      config.llm_model = preset.defaultModel;
    }
  }

  function addTelegramUser() {
    errorMsg = "";
    const parsed = Number(newTgUserId.trim());
    if (!Number.isInteger(parsed) || parsed <= 0) {
      errorMsg = "Telegram User ID harus berupa angka valid (contoh: 123456789).";
      return;
    }
    if (config.telegram_allowed_users.some((u) => u.user_id === parsed)) {
      errorMsg = "Telegram User ID tersebut sudah ada di daftar whitelist.";
      return;
    }
    config.telegram_allowed_users = [
      ...config.telegram_allowed_users,
      {
        user_id: parsed,
        label: newTgLabel.trim() || `User ${parsed}`,
      },
    ];
    newTgUserId = "";
    newTgLabel = "";
  }

  function removeTelegramUser(uid: number) {
    config.telegram_allowed_users = config.telegram_allowed_users.filter(
      (u) => u.user_id !== uid,
    );
  }

  async function handleSave() {
    saving = true;
    errorMsg = "";
    feedbackMsg = "";
    try {
      config = await raydropApi.saveAiMcpConfig(config);
      setTimeout(async () => {
        runtime = await raydropApi.getAiMcpRuntimeStatus().catch(() => runtime);
      }, 450);
      feedbackMsg = "MCP, LLM Provider, Persona & Telegram Bot config saved!";
    } catch (err: any) {
      errorMsg = err?.message || String(err);
    } finally {
      saving = false;
    }
  }

  async function handleTestLlm() {
    testingLlm = true;
    errorMsg = "";
    feedbackMsg = "";
    try {
      const res = await raydropApi.testLlmConnection(config);
      feedbackMsg = res;
    } catch (err: any) {
      errorMsg = err?.message || String(err);
    } finally {
      testingLlm = false;
    }
  }

  async function handleVerifyTelegram() {
    testingTg = true;
    errorMsg = "";
    feedbackMsg = "";
    try {
      const uname = await raydropApi.verifyTelegramToken(config.telegram_bot_token);
      feedbackMsg = `Telegram Bot valid: ${uname}`;
    } catch (err: any) {
      errorMsg = err?.message || String(err);
    } finally {
      testingTg = false;
    }
  }

  function copyMcpClientConfig() {
    const activePort = runtime?.mcp.running ? runtime.mcp.port : config.mcp_port || 48765;
    const snippet = JSON.stringify(
      {
        mcpServers: {
          raydrop: {
            url: `http://127.0.0.1:${activePort}/mcp`,
            headers: config.mcp_auth_token.trim()
              ? { Authorization: `Bearer ${config.mcp_auth_token.trim()}` }
              : undefined,
          },
        },
      },
      null,
      2,
    );
    navigator.clipboard?.writeText(snippet);
    copiedMcpJson = true;
    setTimeout(() => (copiedMcpJson = false), 2000);
  }
</script>

<div class="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 space-y-4">
  <div class="flex items-center justify-between gap-2">
    <div>
      <p class="text-xs font-semibold text-slate-200 flex items-center gap-1.5">
        <Bot class="w-3.5 h-3.5 text-cyan-400" />
        <span>AI Remote Control (Full MCP, LLM Provider, Persona &amp; Telegram Bot)</span>
      </p>
      <p class="text-[11px] text-slate-400 mt-0.5">
        Expose RayDrop &amp; RayAmp via MCP and connect your own Telegram AI Bot.
      </p>
    </div>
  </div>

  <!-- 1. MCP Server -->
  <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800 space-y-2.5">
    <div class="flex items-center justify-between">
      <span class="text-xs font-semibold text-cyan-300 flex items-center gap-1.5">
        <Server class="w-3.5 h-3.5" />
        <span>1. RayDrop MCP Server (HTTP / JSON-RPC)</span>
      </span>
      <label class="flex items-center gap-1.5 text-[11px] text-slate-300 cursor-pointer">
        <input type="checkbox" bind:checked={config.mcp_enabled} class="accent-cyan-500" />
        <span>Enabled</span>
      </label>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="mcp-port">Port</label>
        <input
          id="mcp-port"
          type="number"
          min="1024"
          max="65535"
          bind:value={config.mcp_port}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
        />
      </div>
      <div class="sm:col-span-2">
        <label class="block text-[11px] text-slate-400 mb-1" for="mcp-token">
          Bearer Auth Token (Optional)
        </label>
        <input
          id="mcp-token"
          type="text"
          bind:value={config.mcp_auth_token}
          placeholder="Leave empty for local trusted access"
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
        />
      </div>
    </div>

    <div class="flex items-center justify-between gap-2 text-[11px]">
      <span class="text-slate-400 font-mono truncate">
        {#if runtime?.mcp.running}
          <span class="text-emerald-400">● Running</span> at {runtime.mcp.url}
        {:else if runtime?.mcp.last_error}
          <span class="text-rose-400">● {runtime.mcp.last_error}</span>
        {:else}
          <span class="text-slate-500">● Stopped</span>
        {/if}
      </span>
      <button
        type="button"
        on:click={copyMcpClientConfig}
        class="px-2 py-1 rounded-md bg-slate-800 hover:bg-slate-700 text-slate-200 flex items-center gap-1 shrink-0 cursor-pointer"
      >
        {#if copiedMcpJson}
          <Check class="w-3 h-3 text-emerald-400" />
          <span class="text-emerald-300">Copied JSON</span>
        {:else}
          <Copy class="w-3 h-3" />
          <span>Copy MCP JSON</span>
        {/if}
      </button>
    </div>
  </div>

  <!-- 2. LLM Provider -->
  <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800 space-y-2.5">
    <div class="flex items-center justify-between">
      <span class="text-xs font-semibold text-amber-300 flex items-center gap-1.5">
        <Sparkles class="w-3.5 h-3.5" />
        <span>2. LLM Provider (Common &amp; Custom OpenAI-Compatible)</span>
      </span>
      <button
        type="button"
        disabled={testingLlm}
        on:click={handleTestLlm}
        class="px-2.5 py-1 rounded-lg bg-amber-500/15 hover:bg-amber-500/25 border border-amber-500/40 text-amber-300 text-[11px] font-semibold flex items-center gap-1 cursor-pointer disabled:opacity-50"
      >
        <RefreshCw class="w-3 h-3 {testingLlm ? 'animate-spin' : ''}" />
        <span>{testingLlm ? "Testing..." : "Test LLM"}</span>
      </button>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="llm-provider">Provider</label>
        <select
          id="llm-provider"
          bind:value={config.llm_provider}
          on:change={() => handleProviderChange(config.llm_provider)}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
        >
          {#each Object.entries(PROVIDER_PRESETS) as [key, p]}
            <option value={key}>{p.label}</option>
          {/each}
          {#if localBridges.length > 0 || peerProxies.length > 0}
            <optgroup label="Raydrop LLM Bridges (Local & Peer)">
              {#each localBridges as b}
                <option value={`bridge:${b.id}`}>
                  Raydrop Bridge: {b.name} [:{b.port}] {b.running ? "(ON)" : "(OFF)"}
                </option>
              {/each}
              {#each peerProxies as px}
                <option value={`peer:${px.id}`}>
                  Peer Bridge: {px.peer_alias} ({px.provider_id}) [:{px.local_port}]
                </option>
              {/each}
            </optgroup>
          {/if}
        </select>
      </div>
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="llm-model">Model Name</label>
        <input
          id="llm-model"
          type="text"
          bind:value={config.llm_model}
          placeholder="gpt-4o-mini / gemini-2.5-flash / custom"
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
        />
      </div>
    </div>

    <div>
      <label class="block text-[11px] text-slate-400 mb-1" for="llm-baseurl">
        API Base URL (OpenAI-Compatible `/v1` or Anthropic)
      </label>
      <input
        id="llm-baseurl"
        type="text"
        bind:value={config.llm_base_url}
        placeholder="https://api.openai.com/v1"
        class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
      />
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
      <div class="sm:col-span-2">
        <label class="block text-[11px] text-slate-400 mb-1" for="llm-apikey">API Key</label>
        <div class="flex items-center gap-1">
          {#if showApiKey}
            <input
              id="llm-apikey"
              type="text"
              bind:value={config.llm_api_key}
              placeholder="sk-... / AIza... (optional for Ollama)"
              class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
            />
          {:else}
            <input
              id="llm-apikey"
              type="password"
              bind:value={config.llm_api_key}
              placeholder="sk-... / AIza... (optional for Ollama)"
              class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
            />
          {/if}
          <button
            type="button"
            on:click={() => (showApiKey = !showApiKey)}
            class="p-1.5 rounded-lg bg-slate-800 text-slate-400 hover:text-white cursor-pointer"
          >
            {#if showApiKey}
              <EyeOff class="w-3.5 h-3.5" />
            {:else}
              <Eye class="w-3.5 h-3.5" />
            {/if}
          </button>
        </div>
      </div>
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="llm-temp">Temperature</label>
        <input
          id="llm-temp"
          type="number"
          min="0"
          max="2"
          step="0.1"
          bind:value={config.llm_temperature}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
        />
      </div>
    </div>

    <label class="flex items-center gap-2 text-[11px] text-slate-300 cursor-pointer pt-0.5">
      <input type="checkbox" bind:checked={config.llm_prompt_tool_mode} class="accent-amber-500" />
      <span>Prompt-Based Tool Mode (recommended for custom CLI/proxy endpoints without native OpenAI <code>tools</code>)</span>
    </label>
  </div>

  <!-- 3. Bot Persona -->
  <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800 space-y-2.5">
    <div class="flex items-center justify-between">
      <span class="text-xs font-semibold text-purple-300 flex items-center gap-1.5">
        <Bot class="w-3.5 h-3.5" />
        <span>3. Bot Persona &amp; Behavior</span>
      </span>
      <button
        type="button"
        on:click={() => (config.persona_system_prompt = DEFAULT_PERSONA_PROMPT)}
        class="text-[11px] text-slate-400 hover:text-white flex items-center gap-1 cursor-pointer"
      >
        <RotateCcw class="w-3 h-3" />
        <span>Reset Default</span>
      </button>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="persona-name">Bot Name</label>
        <input
          id="persona-name"
          type="text"
          bind:value={config.persona_name}
          placeholder="RayBot"
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
        />
      </div>
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="persona-preset">Style</label>
        <select
          id="persona-preset"
          bind:value={config.persona_preset}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
        >
          <option value="assistant">Professional Assistant</option>
          <option value="friendly">Friendly &amp; Casual</option>
          <option value="concise">Concise Tech</option>
          <option value="custom">Custom Only</option>
        </select>
      </div>
      <div>
        <label class="block text-[11px] text-slate-400 mb-1" for="persona-lang">Language</label>
        <select
          id="persona-lang"
          bind:value={config.persona_language}
          class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
        >
          <option value="auto">Auto (Match User)</option>
          <option value="id">Indonesian</option>
          <option value="en">English</option>
        </select>
      </div>
    </div>

    <div>
      <label class="block text-[11px] text-slate-400 mb-1" for="persona-prompt">
        Persona / System Instructions
      </label>
      <textarea
        id="persona-prompt"
        rows="3"
        bind:value={config.persona_system_prompt}
        placeholder="Describe how the bot should behave, rules, or personality..."
        class="w-full px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white resize-y"
      ></textarea>
    </div>
  </div>

  <!-- 4. Telegram Bot & Multiple User IDs -->
  <div class="p-3 rounded-xl bg-slate-900/70 border border-slate-800 space-y-2.5">
    <div class="flex items-center justify-between">
      <span class="text-xs font-semibold text-sky-300 flex items-center gap-1.5">
        <Send class="w-3.5 h-3.5" />
        <span>4. Telegram AI Bot (Full RayDrop MCP)</span>
      </span>
      <label class="flex items-center gap-1.5 text-[11px] text-slate-300 cursor-pointer">
        <input type="checkbox" bind:checked={config.telegram_enabled} class="accent-sky-500" />
        <span>Enable Bot</span>
      </label>
    </div>

    <div>
      <div class="flex items-center justify-between mb-1">
        <label class="block text-[11px] text-slate-400" for="tg-token">
          Bot Token (from @BotFather)
        </label>
        {#if runtime?.telegram.running}
          <span class="text-[10px] text-emerald-400 font-mono">
            ● Running ({runtime.telegram.bot_username})
          </span>
        {:else if runtime?.telegram.last_error}
          <span class="text-[10px] text-rose-400 truncate max-w-[200px]">
            ● {runtime.telegram.last_error}
          </span>
        {/if}
      </div>
      <div class="flex items-center gap-1.5">
        {#if showBotToken}
          <input
            id="tg-token"
            type="text"
            bind:value={config.telegram_bot_token}
            placeholder="123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
            class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
          />
        {:else}
          <input
            id="tg-token"
            type="password"
            bind:value={config.telegram_bot_token}
            placeholder="123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
            class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
          />
        {/if}
        <button
          type="button"
          on:click={() => (showBotToken = !showBotToken)}
          class="p-1.5 rounded-lg bg-slate-800 text-slate-400 hover:text-white cursor-pointer"
        >
          {#if showBotToken}
            <EyeOff class="w-3.5 h-3.5" />
          {:else}
            <Eye class="w-3.5 h-3.5" />
          {/if}
        </button>
        <button
          type="button"
          disabled={testingTg}
          on:click={handleVerifyTelegram}
          class="px-2.5 py-1.5 rounded-lg bg-sky-500/20 hover:bg-sky-500/30 border border-sky-500/40 text-sky-300 text-[11px] font-semibold cursor-pointer disabled:opacity-50"
        >
          {testingTg ? "..." : "Verify"}
        </button>
      </div>
    </div>

    <div class="space-y-1.5">
      <p class="text-[11px] text-slate-400">
        Allowed Telegram User IDs (Multiple Whitelist — check your numeric ID via <code class="text-slate-200">@userinfobot</code>):
      </p>
      <div class="flex items-center gap-1.5">
        <input
          type="text"
          bind:value={newTgUserId}
          placeholder="Numeric User ID (e.g. 123456789)"
          class="flex-1 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs font-mono text-white"
        />
        <input
          type="text"
          bind:value={newTgLabel}
          placeholder="Label (e.g. HP Utama)"
          class="w-36 px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-700 text-xs text-white"
        />
        <button
          type="button"
          on:click={addTelegramUser}
          class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 border border-slate-700 text-xs font-semibold text-white flex items-center gap-1 cursor-pointer"
        >
          <Plus class="w-3.5 h-3.5 text-sky-400" />
          <span>Add</span>
        </button>
      </div>

      {#if config.telegram_allowed_users.length === 0}
        <p class="text-[11px] text-amber-400/90 italic">
          Belum ada User ID di whitelist. Tambahkan minimal 1 User ID agar bot bisa digunakan.
        </p>
      {:else}
        <div class="space-y-1">
          {#each config.telegram_allowed_users as user (user.user_id)}
            <div class="flex items-center justify-between px-2.5 py-1.5 rounded-lg bg-slate-950 border border-slate-800 text-xs">
              <div class="flex items-center gap-2">
                <span class="font-mono text-sky-300">{user.user_id}</span>
                <span class="text-slate-400">• {user.label}</span>
              </div>
              <button
                type="button"
                on:click={() => removeTelegramUser(user.user_id)}
                class="p-1 text-slate-500 hover:text-rose-400 cursor-pointer"
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if errorMsg}
    <div class="flex items-center gap-2 text-xs text-rose-300 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl">
      <AlertCircle class="w-4 h-4 shrink-0" />
      <span>{errorMsg}</span>
    </div>
  {/if}

  {#if feedbackMsg}
    <div class="flex items-center gap-2 text-xs text-emerald-300 bg-emerald-500/10 border border-emerald-500/30 px-3 py-2 rounded-xl">
      <CheckCircle2 class="w-4 h-4 shrink-0" />
      <span>{feedbackMsg}</span>
    </div>
  {/if}

  <div class="flex justify-end">
    <button
      type="button"
      disabled={saving}
      on:click={handleSave}
      class="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center gap-1.5 shadow-md shadow-cyan-500/20 disabled:opacity-50 cursor-pointer"
    >
      <Save class="w-3.5 h-3.5" />
      <span>{saving ? "Saving & Restarting..." : "Save AI / MCP / Telegram Config"}</span>
    </button>
  </div>
</div>
