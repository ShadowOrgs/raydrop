<script lang="ts">
  import {
    appSettings,
    deviceCryptoMeta,
    cloudMyDevices,
    cloudContacts,
    authModalOpen,
  } from "../stores/state";
  import { contacts } from "../stores/contacts";
  import { transfers } from "../stores/transfers";
  import { fetchUnreadCounts, fetchChatGroups, chatStore } from "../stores/chat";
  import { raydropApi, cloudApi, getApiBaseUrl } from "../utils/tauri-ipc";
  import {
    getAccountDeviceName,
    saveAccountDeviceName,
  } from "../utils/accountDeviceNames";
  import {
    Mail,
    Lock,
    User,
    Laptop,
    X,
    ArrowRight,
    ShieldCheck,
  } from "lucide-svelte";

  let mode: "login" | "register" = "login";
  let email = "";
  let name = "";
  let password = "";
  let deviceName = "";
  let loading = false;
  let errorMsg = "";

  $: if ($authModalOpen && !deviceName) {
    deviceName = $appSettings.alias_name;
  }

  async function handleSubmit() {
    errorMsg = "";
    if (!email.trim() || !password.trim()) {
      errorMsg = "Email and password are required.";
      return;
    }
    if (mode === "register" && !name.trim()) {
      errorMsg = "Full name is required for registration.";
      return;
    }

    loading = true;
    try {
      // Preserve current signed-in account's device name before switching accounts
      if ($appSettings.user_email && $appSettings.alias_name) {
        saveAccountDeviceName($appSettings.user_email, $appSettings.alias_name);
      }

      const baseUrl = getApiBaseUrl($appSettings.signaling_url);
      const rememberedName = getAccountDeviceName(email);
      const lockedDeviceName =
        mode === "login"
          ? (rememberedName || $appSettings.alias_name)
          : (deviceName || $appSettings.alias_name).trim();

      const payload = {
        email: email.trim(),
        name: name.trim(),
        password,
        device_id: $appSettings.device_id,
        device_name: lockedDeviceName,
        os_type: $deviceCryptoMeta.osType,
        ed25519_pubkey: $deviceCryptoMeta.ed25519Pubkey,
      };

      const res =
        mode === "register"
          ? await cloudApi.register(baseUrl, payload)
          : await cloudApi.login(baseUrl, payload);

      // Lock device name to remembered per-account name or existing server record when re-logging in
      const existingServerDev = (res.my_devices || []).find(
        (d: any) => d.device_id === $appSettings.device_id,
      );
      const finalDeviceName =
        mode === "login"
          ? (rememberedName || existingServerDev?.device_name || payload.device_name)
          : payload.device_name;

      saveAccountDeviceName(res.user.email, finalDeviceName);

      const updatedSettings = await raydropApi.saveCloudSession({
        userId: res.user.id,
        userEmail: res.user.email,
        userName: res.user.name,
        authToken: res.token || "",
        deviceName: finalDeviceName,
      });

      appSettings.set(updatedSettings);
      cloudMyDevices.set(res.my_devices);
      cloudContacts.set(res.contacts);

      const syncedLocal = await raydropApi.syncCloudDataToLocal(
        res.my_devices,
        res.contacts,
      );
      contacts.set(syncedLocal);
      chatStore.set({});
      transfers.set(await raydropApi.getTransferHistory().catch(() => []));
      await fetchUnreadCounts().catch(() => {});
      await fetchChatGroups().catch(() => {});
      authModalOpen.set(false);
      password = "";
    } catch (e: any) {
      errorMsg = e?.message || String(e);
    } finally {
      loading = false;
    }
  }
</script>

{#if $authModalOpen}
  <div
    class="fixed inset-0 z-50 bg-slate-950/85 backdrop-blur-md flex items-center justify-center p-4"
  >
    <div
      class="w-full max-w-md max-h-[90vh] overflow-y-auto bg-slate-900 border border-slate-800 rounded-2xl p-6 shadow-2xl"
    >
      <div
        class="flex items-center justify-between pb-4 border-b border-slate-800"
      >
        <div class="flex items-center gap-3">
          <img
            src="/icon.png"
            alt="Raydrop"
            class="w-12 h-12 object-contain rounded-xl bg-slate-950/60 p-0.5 border border-slate-800"
          />
          <div>
            <h3 class="text-base font-bold text-white">
              {mode === "login"
                ? "Sign In to Raydrop Mesh"
                : "Create Raydrop Account"}
            </h3>
            <p class="text-xs text-slate-400">
              Manage your devices like Tailscale & share files by Email
            </p>
          </div>
        </div>
        {#if $appSettings.auth_token}
          <button
            on:click={() => authModalOpen.set(false)}
            class="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800"
          >
            <X class="w-4 h-4" />
          </button>
        {/if}
      </div>

      <!-- Mode Switcher -->
      <div
        class="grid grid-cols-2 gap-1.5 bg-slate-950 p-1 rounded-xl border border-slate-800 mt-4"
      >
        <button
          on:click={() => {
            mode = "login";
            errorMsg = "";
          }}
          class="py-1.5 rounded-lg text-xs font-semibold transition-all {mode ===
          'login'
            ? 'bg-cyan-500 text-slate-950'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          Sign In
        </button>
        <button
          on:click={() => {
            mode = "register";
            errorMsg = "";
          }}
          class="py-1.5 rounded-lg text-xs font-semibold transition-all {mode ===
          'register'
            ? 'bg-cyan-500 text-slate-950'
            : 'text-slate-400 hover:text-slate-200'}"
        >
          Register Account
        </button>
      </div>

      <div class="space-y-3.5 mt-4">
        <div>
          <label
            for="auth-email"
            class="block text-xs font-medium text-slate-300 mb-1"
          >
            Email Address (Your Unique Raydrop Identity)
          </label>
          <div class="relative">
            <Mail class="w-4 h-4 text-slate-500 absolute left-3 top-2.5" />
            <input
              id="auth-email"
              type="email"
              bind:value={email}
              placeholder="you@example.com"
              class="w-full pl-9 pr-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        {#if mode === "register"}
          <div>
            <label
              for="auth-name"
              class="block text-xs font-medium text-slate-300 mb-1"
            >
              Full Name / Display Name
            </label>
            <div class="relative">
              <User class="w-4 h-4 text-slate-500 absolute left-3 top-2.5" />
              <input
                id="auth-name"
                type="text"
                bind:value={name}
                placeholder="Adens"
                class="w-full pl-9 pr-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
              />
            </div>
          </div>
        {/if}

        <div>
          <label
            for="auth-password"
            class="block text-xs font-medium text-slate-300 mb-1"
          >
            Password
          </label>
          <div class="relative">
            <Lock class="w-4 h-4 text-slate-500 absolute left-3 top-2.5" />
            <input
              id="auth-password"
              type="password"
              bind:value={password}
              placeholder="••••••••"
              class="w-full pl-9 pr-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500"
            />
          </div>
        </div>

        <div>
          <label
            for="auth-device-name"
            class="block text-xs font-medium text-slate-300 mb-1 flex items-center justify-between"
          >
            <span>This Device's Name (Shown in My Devices)</span>
            {#if mode === "login"}
              <span class="text-[10px] font-mono text-slate-400"
                >Locked to Device ID</span
              >
            {/if}
          </label>
          <div class="relative">
            <Laptop class="w-4 h-4 text-slate-500 absolute left-3 top-2.5" />
            <input
              id="auth-device-name"
              type="text"
              bind:value={deviceName}
              disabled={mode === "login"}
              placeholder="My-Unique-Device"
              class="w-full pl-9 pr-3 py-2 rounded-xl bg-slate-950 border border-slate-800 text-xs text-white focus:outline-none focus:border-cyan-500 disabled:opacity-60 disabled:cursor-not-allowed"
            />
          </div>
        </div>

        {#if errorMsg}
          <p
            class="text-xs text-rose-400 bg-rose-500/10 border border-rose-500/30 px-3 py-2 rounded-xl"
          >
            {errorMsg}
          </p>
        {/if}
      </div>

      <div class="mt-6 flex items-center justify-between gap-3">
        <button
          on:click={() => authModalOpen.set(false)}
          class="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs font-medium text-slate-300"
        >
          LAN Only Mode
        </button>

        <button
          on:click={handleSubmit}
          disabled={loading}
          class="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-50 text-xs font-bold text-slate-950 shadow-lg shadow-cyan-500/20"
        >
          <span>
            {loading
              ? "Connecting to Raydrop Cloud..."
              : mode === "login"
                ? "Sign In & Sync Devices"
                : "Create Account & Link Device"}
          </span>
          <ArrowRight class="w-4 h-4" />
        </button>
      </div>
    </div>
  </div>
{/if}
