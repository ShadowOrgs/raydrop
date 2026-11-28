<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import { Terminal as XTerm } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import "@xterm/xterm/css/xterm.css";
  import { notifyToast } from "../stores/presence";
  import { appSettings } from "../stores/state";
  import {
    Terminal,
    Database,
    Activity,
    Plus,
    Trash2,
    Edit3,
    Play,
    Square,
    Copy,
    ClipboardPaste,
    RefreshCw,
    Download,
    Upload,
    Clock,
    HeartPulse,
    Server,
    FolderPlus,
    ExternalLink,
    Share2,
    Loader2,
    X,
  } from "lucide-svelte";

  let activeSubTab: "ssh" | "db" | "uptime" = "ssh";

  // ===================== STAGE 1: SSH =====================
  interface SshGroup {
    id: string;
    name: string;
    color: string;
  }
  interface SshServer {
    id: string;
    group_id: string;
    name: string;
    host: string;
    port: number;
    username: string;
    auth_type: string;
    password: string;
    private_key_path: string;
    notes: string;
  }
  interface SshServerHealth {
    server_id: string;
    kernel: string;
    uptime: string;
    cpu_cores: number;
    load_1m: number;
    load_5m: number;
    ram_used_mb: number;
    ram_total_mb: number;
    disk_used_mb: number;
    disk_total_mb: number;
  }

  let sshGroups: SshGroup[] = [];
  let sshServers: SshServer[] = [];
  let selectedSshGroupId = "";
  let sshHealth: SshServerHealth | null = null;
  let checkingSshHealthId = "";

  // Terminal tabs
  interface SshTermTab {
    sessionId: string;
    server: SshServer;
    status: "connected" | "disconnected" | "connecting";
  }
  let termTabs: SshTermTab[] = [];
  let activeTermId = "";
  const termInstances = new Map<string, { term: XTerm; fit: FitAddon }>();
  let connectingSshId = "";
  let testingSsh = false;
  let sshTestResult: { ok: boolean; msg: string } | null = null;
  let unlistenSshOut: UnlistenFn | null = null;
  let unlistenSshStatus: UnlistenFn | null = null;

  // Modals for SSH
  let showGroupModal = false;
  let groupForm = { id: "", name: "", color: "#06b6d4" };
  let showServerModal = false;
  let serverForm = {
    id: "",
    group_id: "",
    name: "",
    host: "",
    port: 22,
    username: "root",
    auth_type: "password",
    password: "",
    private_key_path: "",
    notes: "",
  };

  async function loadSshWorkspace() {
    try {
      const res = await invoke<{ groups: SshGroup[]; servers: SshServer[] }>("ssh_list_workspace");
      sshGroups = res.groups;
      sshServers = res.servers;
    } catch (e) {
      notifyToast("SSH Error", String(e), "rose");
    }
  }

  async function saveSshGroup() {
    try {
      await invoke("ssh_save_group", {
        id: groupForm.id || null,
        name: groupForm.name,
        color: groupForm.color,
      });
      showGroupModal = false;
      await loadSshWorkspace();
      notifyToast("SSH Group", "Group berhasil disimpan", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  async function saveSshServer() {
    try {
      await invoke("ssh_save_server", {
        input: {
          ...serverForm,
          id: serverForm.id || null,
          port: Number(serverForm.port) || 22,
        },
      });
      showServerModal = false;
      await loadSshWorkspace();
      notifyToast("SSH Server", "Server config berhasil disimpan", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  async function deleteSshServer(id: string) {
    await invoke("ssh_delete_server", { id });
    await loadSshWorkspace();
  }

  async function testSshConnection() {
    testingSsh = true;
    sshTestResult = null;
    try {
      const msg = await invoke<string>("ssh_test_connection", {
        input: {
          ...serverForm,
          id: serverForm.id || null,
          port: Number(serverForm.port) || 22,
        },
      });
      sshTestResult = { ok: true, msg };
      notifyToast("SSH Test OK", msg, "emerald");
    } catch (e) {
      sshTestResult = { ok: false, msg: String(e) };
      notifyToast("SSH Test Failed", String(e), "rose", 5000);
    } finally {
      testingSsh = false;
    }
  }

  async function connectSshServer(srv: SshServer) {
    connectingSshId = srv.id;
    try {
      const sessionId = await invoke<string>("ssh_connect", {
        serverId: srv.id,
        cols: 100,
        rows: 30,
      });
      termTabs = [...termTabs, { sessionId, server: srv, status: "connected" }];
      activeTermId = sessionId;
      await tick();
      fitActiveTerminal();
      notifyToast("SSH Connected", `${srv.username}@${srv.host}`, "emerald");
    } catch (e) {
      notifyToast("SSH Connect Failed", String(e), "rose", 6000);
    } finally {
      connectingSshId = "";
    }
  }

  function fitActiveTerminal() {
    if (!activeTermId) return;
    const entry = termInstances.get(activeTermId);
    if (!entry) return;
    setTimeout(() => {
      try {
        entry.fit.fit();
        entry.term.focus();
      } catch (_) {}
    }, 30);
  }

  function attachXterm(node: HTMLDivElement, sessionId: string) {
    let entry = termInstances.get(sessionId);
    if (!entry) {
      const term = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: "Consolas, 'Cascadia Mono', monospace",
        theme: {
          background: "#050912",
          foreground: "#e2e8f0",
          cursor: "#22d3ee",
        },
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown" && ev.ctrlKey && ev.shiftKey && ev.code === "KeyC") {
          copyTerminalSelection();
          return false;
        }
        if (ev.type === "keydown" && ev.ctrlKey && ev.shiftKey && ev.code === "KeyV") {
          pasteToTerminal();
          return false;
        }
        return true;
      });
      term.onData((data) => {
        const bytes = new TextEncoder().encode(data);
        let bin = "";
        for (const b of bytes) bin += String.fromCharCode(b);
        const b64 = btoa(bin);
        invoke("ssh_send_input", { sessionId, dataB64: b64 }).catch(() => {});
      });
      term.onResize(({ cols, rows }) => {
        invoke("ssh_resize", { sessionId, cols, rows }).catch(() => {});
      });
      entry = { term, fit };
      termInstances.set(sessionId, entry);
      term.open(node);
      setTimeout(() => {
        try {
          fit.fit();
          term.focus();
        } catch (_) {}
      }, 40);
    }
    return {
      destroy() {
        // Disposed explicitly in closeTermTab / onDestroy
      },
    };
  }

  async function selectTermTab(sessionId: string) {
    activeTermId = sessionId;
    await tick();
    fitActiveTerminal();
  }

  async function closeTermTab(sessionId: string) {
    await invoke("ssh_disconnect", { sessionId }).catch(() => {});
    const inst = termInstances.get(sessionId);
    inst?.term.dispose();
    termInstances.delete(sessionId);
    termTabs = termTabs.filter((t) => t.sessionId !== sessionId);
    if (activeTermId === sessionId) {
      activeTermId = termTabs[0]?.sessionId || "";
      if (activeTermId) {
        await tick();
        fitActiveTerminal();
      }
    }
  }

  async function copyTerminalSelection() {
    const inst = termInstances.get(activeTermId);
    const sel = inst?.term.getSelection() || "";
    if (!sel) {
      notifyToast("Terminal", "Pilih teks di terminal terlebih dahulu", "amber");
      return;
    }
    await navigator.clipboard.writeText(sel);
    notifyToast("Copied", "Teks terminal disalin ke clipboard", "cyan");
  }

  async function pasteToTerminal() {
    if (!activeTermId) return;
    const text = await navigator.clipboard.readText();
    if (!text) return;
    const bytes = new TextEncoder().encode(text);
    let bin = "";
    for (const b of bytes) bin += String.fromCharCode(b);
    await invoke("ssh_send_input", { sessionId: activeTermId, dataB64: btoa(bin) });
  }

  async function checkSshHealth(srv: SshServer) {
    checkingSshHealthId = srv.id;
    try {
      sshHealth = await invoke<SshServerHealth>("ssh_check_server_health", { serverId: srv.id });
    } catch (e) {
      notifyToast("Health Check Failed", String(e), "rose");
    } finally {
      checkingSshHealthId = "";
    }
  }

  // ===================== STAGE 2: DATABASE MANAGER =====================
  interface DbConnection {
    id: string;
    name: string;
    db_type: string;
    host: string;
    port: number;
    username: string;
    password: string;
    database_name: string;
    ssh_server_id: string;
    admin_tool: string;
    backup_dir: string;
  }
  interface DbSchedule {
    id: string;
    connection_id: string;
    interval_minutes: number;
    keep_last: number;
    enabled: boolean;
    last_run_at: number;
    last_status: string;
  }
  interface DbBackupRecord {
    id: string;
    connection_id: string;
    file_path: string;
    size_bytes: number;
    status: string;
    created_at: number;
  }
  interface DbToolsStatus {
    tools_dir: string;
    mysql_client: string | null;
    mysqldump_client: string | null;
    psql_client: string | null;
    pg_dump_client: string | null;
    sqlite_client: string | null;
    php_bin: string | null;
    adminer_installed: boolean;
    phpmyadmin_installed: boolean;
    pgweb_bin: string | null;
    running_admins: Array<{ connection_id: string; tool: string; port: number; url: string }>;
  }
  interface DbHealth {
    connection_id: string;
    ok: boolean;
    latency_ms: number;
    version: string;
    uptime_secs: number;
    active_connections: number;
    table_count: number;
    size_mb: number;
    message: string;
  }

  let dbConns: DbConnection[] = [];
  let dbSchedules: DbSchedule[] = [];
  let dbHistory: DbBackupRecord[] = [];
  let dbTools: DbToolsStatus | null = null;
  let installingTool = "";
  let dbHealthMap: Record<string, DbHealth> = {};
  let checkingDbId = "";
  let backingUpDbId = "";
  let launchingDbId = "";
  let restoringDbId = "";

  let showDbModal = false;
  let dbForm = {
    id: "",
    name: "",
    db_type: "mysql",
    host: "127.0.0.1",
    port: 3306,
    username: "root",
    password: "",
    database_name: "",
    ssh_server_id: "",
    admin_tool: "adminer",
    backup_dir: "",
  };

  let showSchedModal = false;
  let schedForm = { connection_id: "", interval_minutes: 1440, keep_last: 7, enabled: true };

  async function loadDbOverview() {
    try {
      const ov = await invoke<{
        connections: DbConnection[];
        schedules: DbSchedule[];
        history: DbBackupRecord[];
        tools: DbToolsStatus;
      }>("dbm_list_overview");
      dbConns = ov.connections;
      dbSchedules = ov.schedules;
      dbHistory = ov.history;
      dbTools = ov.tools;
    } catch (e) {
      notifyToast("DB Error", String(e), "rose");
    }
  }

  async function installDbTool(toolId: string) {
    installingTool = toolId;
    try {
      dbTools = await invoke<DbToolsStatus>("dbm_install_tool", { toolId });
      notifyToast("Tool Installed", `${toolId} berhasil diinstall`, "emerald");
    } catch (e) {
      notifyToast("Install Failed", String(e), "rose", 6000);
    } finally {
      installingTool = "";
    }
  }

  async function saveDbConn() {
    try {
      await invoke("dbm_save_connection", {
        input: { ...dbForm, id: dbForm.id || null, port: Number(dbForm.port) || 3306 },
      });
      showDbModal = false;
      await loadDbOverview();
      notifyToast("Database", "Config database berhasil disimpan", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  async function launchDbAdmin(conn: DbConnection) {
    launchingDbId = conn.id;
    try {
      const res = await invoke<{ url: string; port: number; tool: string }>("dbm_launch_admin", {
        connectionId: conn.id,
      });
      await loadDbOverview();
      notifyToast("DB Admin Ready", `${res.tool} auto-login di port ${res.port}`, "cyan");
      try {
        await openExternal(res.url);
      } catch (_) {
        window.open(res.url, "_blank");
      }
    } catch (e) {
      notifyToast("Launch Admin Failed", String(e), "rose", 5000);
    } finally {
      launchingDbId = "";
    }
  }

  async function stopDbAdmin(connId: string) {
    await invoke("dbm_stop_admin", { connectionId: connId });
    await loadDbOverview();
  }

  async function checkDbHealth(connId: string) {
    checkingDbId = connId;
    try {
      const h = await invoke<DbHealth>("dbm_check_health", { connectionId: connId });
      dbHealthMap = { ...dbHealthMap, [connId]: h };
      notifyToast(
        h.ok ? "DB Healthy" : "DB Unreachable",
        h.ok ? `${h.version} • ${h.latency_ms}ms` : h.message,
        h.ok ? "emerald" : "rose"
      );
    } catch (e) {
      notifyToast("DB Health Error", String(e), "rose");
    } finally {
      checkingDbId = "";
    }
  }

  async function backupDbNow(connId: string) {
    backingUpDbId = connId;
    try {
      const rec = await invoke<DbBackupRecord>("dbm_backup_now", { connectionId: connId });
      await loadDbOverview();
      notifyToast("Backup Complete", `Saved .sql.gz (${Math.round(rec.size_bytes / 1024)} KB)`, "emerald");
    } catch (e) {
      notifyToast("Backup Failed", String(e), "rose", 6000);
    } finally {
      backingUpDbId = "";
    }
  }

  async function restoreSpecificBackup(conn: DbConnection, filePath: string) {
    restoringDbId = conn.id;
    try {
      const msg = await invoke<string>("dbm_restore_backup", {
        connectionId: conn.id,
        filePath,
        allowNonEmpty: false,
      });
      notifyToast("Restore Complete", msg, "emerald");
    } catch (e: any) {
      const errStr = String(e);
      if (errStr.includes("SAFE_RESTORE_BLOCKED")) {
        notifyToast("Safe Restore Blocked", errStr, "amber", 8000);
      } else {
        notifyToast("Restore Failed", errStr, "rose", 6000);
      }
    } finally {
      restoringDbId = "";
    }
  }

  async function restoreDbFile(conn: DbConnection) {
    const picked = await openDialog({
      multiple: false,
      title: `Restore Backup to ${conn.database_name}`,
      filters: [{ name: "SQL / GZ Backup", extensions: ["gz", "sql"] }],
    });
    if (typeof picked !== "string" || !picked) return;
    await restoreSpecificBackup(conn, picked);
  }

  async function saveDbSchedule() {
    try {
      await invoke("dbm_save_schedule", {
        id: null,
        connectionId: schedForm.connection_id,
        intervalMinutes: Number(schedForm.interval_minutes) || 1440,
        keepLast: Number(schedForm.keep_last) || 7,
        enabled: schedForm.enabled,
      });
      showSchedModal = false;
      await loadDbOverview();
      notifyToast("Schedule Saved", "Auto-backup schedule aktif", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  // ===================== STAGE 3: UPTIME MONITOR =====================
  interface UptimeGroup {
    id: string;
    name: string;
    color: string;
  }
  interface UptimeMonitor {
    id: string;
    group_id: string;
    name: string;
    monitor_type: string;
    target: string;
    interval_minutes: number;
    timeout_secs: number;
    expected_status: number;
    enabled: boolean;
    current_status: string;
    last_latency_ms: number;
    last_checked_at: number;
    last_error: string;
  }
  interface UptimeCheckRecord {
    id: number;
    monitor_id: string;
    status: string;
    latency_ms: number;
  }

  let uptimeGroups: UptimeGroup[] = [];
  let uptimeMonitors: UptimeMonitor[] = [];
  let uptimeChecks: UptimeCheckRecord[] = [];
  let selectedUptimeGroupId = "";
  let checkingMonitorId = "";

  let showUptimeGroupModal = false;
  let uptimeGroupForm = { id: "", name: "", color: "#10b981" };
  let showMonitorModal = false;
  let monitorForm = {
    id: "",
    group_id: "",
    name: "",
    monitor_type: "http",
    target: "https://",
    interval_minutes: 5,
    timeout_secs: 10,
    expected_status: 200,
    enabled: true,
  };

  async function loadUptimeOverview() {
    try {
      const ov = await invoke<{
        groups: UptimeGroup[];
        monitors: UptimeMonitor[];
        recent_checks: UptimeCheckRecord[];
      }>("uptime_list_overview");
      uptimeGroups = ov.groups;
      uptimeMonitors = ov.monitors;
      uptimeChecks = ov.recent_checks;
    } catch (e) {
      notifyToast("Uptime Error", String(e), "rose");
    }
  }

  async function saveUptimeGroup() {
    try {
      await invoke("uptime_save_group", {
        id: uptimeGroupForm.id || null,
        name: uptimeGroupForm.name,
        color: uptimeGroupForm.color,
      });
      showUptimeGroupModal = false;
      await loadUptimeOverview();
      notifyToast("Uptime Group", "Group disimpan", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  async function saveMonitor() {
    try {
      await invoke("uptime_save_monitor", {
        input: {
          ...monitorForm,
          id: monitorForm.id || null,
          interval_minutes: Number(monitorForm.interval_minutes) || 5,
          timeout_secs: Number(monitorForm.timeout_secs) || 10,
          expected_status: Number(monitorForm.expected_status) || 200,
        },
      });
      showMonitorModal = false;
      await loadUptimeOverview();
      notifyToast("Uptime Monitor", "Monitor berhasil disimpan", "emerald");
    } catch (e) {
      notifyToast("Error", String(e), "rose");
    }
  }

  async function checkMonitorNow(id: string) {
    checkingMonitorId = id;
    try {
      await invoke("uptime_check_now", { monitorId: id });
      await loadUptimeOverview();
    } catch (e) {
      notifyToast("Check Failed", String(e), "rose");
    } finally {
      checkingMonitorId = "";
    }
  }

  let publishingStatus = false;
  let sharedStatusUrl = "";

  function getServerHttpBase(): string {
    const raw = $appSettings.signaling_url || "wss://stun.raydrop.web.id/ws";
    return raw
      .replace(/^wss:\/\//i, "https://")
      .replace(/^ws:\/\//i, "http://")
      .replace(/\/ws\/?$/i, "")
      .replace(/\/+$/, "");
  }

  async function shareGroupStatusPage() {
    publishingStatus = true;
    try {
      const serverBaseUrl = getServerHttpBase();
      const url = await invoke<string>("uptime_publish_status_page", {
        groupId: selectedUptimeGroupId || null,
        serverBaseUrl,
      });
      sharedStatusUrl = url;
      await navigator.clipboard.writeText(url).catch(() => {});
      notifyToast("Public Status URL Ready", `Disalin ke clipboard: ${url}`, "emerald", 6000);
    } catch (e) {
      notifyToast("Share Status Failed", String(e), "rose", 5000);
    } finally {
      publishingStatus = false;
    }
  }

  onMount(async () => {
    await Promise.all([loadSshWorkspace(), loadDbOverview(), loadUptimeOverview()]);
    unlistenSshOut = await listen<{ session_id: string; data_b64: string }>("ssh_output", (ev) => {
      const inst = termInstances.get(ev.payload.session_id);
      if (!inst) return;
      const bin = atob(ev.payload.data_b64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      inst.term.write(bytes);
    });
    unlistenSshStatus = await listen<{ session_id: string; status: string }>("ssh_status", (ev) => {
      termTabs = termTabs.map((t) =>
        t.sessionId === ev.payload.session_id
          ? { ...t, status: (ev.payload.status as any) || "disconnected" }
          : t
      );
    });
  });

  onDestroy(() => {
    if (unlistenSshOut) unlistenSshOut();
    if (unlistenSshStatus) unlistenSshStatus();
    for (const [, inst] of termInstances) {
      inst.term.dispose();
    }
    termInstances.clear();
  });
</script>

<svelte:window on:resize={fitActiveTerminal} />

<div class="h-full flex flex-col gap-3 overflow-hidden">
  <!-- Top Sub-Navigation -->
  <div class="bg-slate-900/80 border border-slate-800 rounded-2xl p-2 flex flex-wrap items-center justify-between gap-2 shrink-0">
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        on:click={() => (activeSubTab = "ssh")}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {activeSubTab === 'ssh'
          ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
          : 'text-slate-400 hover:text-white'}"
      >
        <Terminal class="w-3.5 h-3.5" />
        <span>SSH Terminal</span>
      </button>
      <button
        type="button"
        on:click={() => {
          activeSubTab = "db";
          loadDbOverview();
        }}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {activeSubTab === 'db'
          ? 'bg-indigo-500/20 text-indigo-300 border border-indigo-500/40'
          : 'text-slate-400 hover:text-white'}"
      >
        <Database class="w-3.5 h-3.5" />
        <span>Database Manager</span>
      </button>
      <button
        type="button"
        on:click={() => {
          activeSubTab = "uptime";
          loadUptimeOverview();
        }}
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {activeSubTab === 'uptime'
          ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40'
          : 'text-slate-400 hover:text-white'}"
      >
        <Activity class="w-3.5 h-3.5" />
        <span>Uptime Monitor</span>
      </button>
    </div>

    <div class="text-[11px] text-slate-400 px-2">
      Keepalive SSH • On-Demand DB Tools • .sql.gz Auto-Backup • Uptime Alerts
    </div>
  </div>

  <!-- TAB 1: SSH TERMINAL & SERVERS -->
  {#if activeSubTab === "ssh"}
    <div class="flex-1 min-h-0 grid grid-cols-1 lg:grid-cols-12 gap-3 overflow-hidden">
      <!-- Left Server & Group Sidebar -->
      <div class="lg:col-span-4 bg-slate-900/75 border border-slate-800 rounded-2xl p-3 flex flex-col gap-2.5 overflow-hidden">
        <div class="flex items-center justify-between gap-2">
          <span class="text-xs font-bold text-white">SSH Servers &amp; Groups</span>
          <div class="flex items-center gap-1">
            <button
              type="button"
              on:click={() => {
                groupForm = { id: "", name: "", color: "#06b6d4" };
                showGroupModal = true;
              }}
              class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 text-[11px] flex items-center gap-1 cursor-pointer"
            >
              <FolderPlus class="w-3 h-3" /> Group
            </button>
            <button
              type="button"
              on:click={() => {
                serverForm = {
                  id: "",
                  group_id: selectedSshGroupId,
                  name: "",
                  host: "",
                  port: 22,
                  username: "root",
                  auth_type: "password",
                  password: "",
                  private_key_path: "",
                  notes: "",
                };
                showServerModal = true;
              }}
              class="px-2 py-1 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-bold text-[11px] flex items-center gap-1 cursor-pointer"
            >
              <Plus class="w-3 h-3" /> Server
            </button>
          </div>
        </div>

        <!-- Group Pills -->
        <div class="flex flex-wrap gap-1">
          <button
            type="button"
            on:click={() => (selectedSshGroupId = "")}
            class="px-2 py-0.5 rounded-full text-[10px] font-semibold cursor-pointer {selectedSshGroupId === ''
              ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
              : 'bg-slate-800 text-slate-400'}"
          >
            All ({sshServers.length})
          </button>
          {#each sshGroups as g}
            <button
              type="button"
              on:click={() => (selectedSshGroupId = g.id)}
              class="px-2 py-0.5 rounded-full text-[10px] font-semibold cursor-pointer border {selectedSshGroupId === g.id
                ? 'bg-slate-800 text-white border-cyan-400'
                : 'bg-slate-900 text-slate-400 border-slate-700'}"
              style="border-left-color: {g.color}; border-left-width: 3px;"
            >
              {g.name}
            </button>
          {/each}
        </div>

        <!-- Server List -->
        <div class="flex-1 overflow-y-auto space-y-2 pr-1">
          {#each sshServers.filter((s) => !selectedSshGroupId || s.group_id === selectedSshGroupId) as srv (srv.id)}
            <div class="p-2.5 rounded-xl bg-slate-950/75 border border-slate-800 space-y-2">
              <div class="flex items-start justify-between gap-2">
                <div class="min-w-0">
                  <div class="text-xs font-bold text-white truncate">{srv.name}</div>
                  <div class="text-[11px] font-mono text-slate-400 truncate">
                    {srv.username}@{srv.host}:{srv.port}
                  </div>
                </div>
                <div class="flex items-center gap-1 shrink-0">
                  <button
                    type="button"
                    disabled={connectingSshId === srv.id}
                    on:click={() => connectSshServer(srv)}
                    class="px-2 py-1 rounded-lg bg-emerald-500/20 hover:bg-emerald-500 text-emerald-300 hover:text-slate-950 text-[11px] font-bold cursor-pointer transition active:scale-95 disabled:opacity-60 flex items-center gap-1"
                  >
                    {#if connectingSshId === srv.id}
                      <Loader2 class="w-3 h-3 animate-spin" />
                      <span>Connecting...</span>
                    {:else}
                      <span>Connect</span>
                    {/if}
                  </button>
                  <button
                    type="button"
                    on:click={() => checkSshHealth(srv)}
                    disabled={checkingSshHealthId === srv.id}
                    class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-cyan-300 cursor-pointer transition active:scale-95 disabled:opacity-60"
                    title="Check Remote Server Health"
                  >
                    {#if checkingSshHealthId === srv.id}
                      <Loader2 class="w-3.5 h-3.5 animate-spin" />
                    {:else}
                      <HeartPulse class="w-3.5 h-3.5" />
                    {/if}
                  </button>
                  <button
                    type="button"
                    on:click={() => {
                      serverForm = { ...srv };
                      sshTestResult = null;
                      showServerModal = true;
                    }}
                    class="p-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer transition active:scale-95"
                  >
                    <Edit3 class="w-3 h-3" />
                  </button>
                  <button
                    type="button"
                    on:click={() => deleteSshServer(srv.id)}
                    class="p-1 rounded-lg bg-slate-800 hover:bg-rose-500/20 text-rose-400 cursor-pointer transition active:scale-95"
                  >
                    <Trash2 class="w-3 h-3" />
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- Right Terminal & Health Panel -->
      <div class="lg:col-span-8 flex flex-col gap-2.5 min-h-0 overflow-hidden">
        {#if sshHealth}
          <div class="bg-slate-900/90 border border-cyan-500/30 rounded-2xl p-3 flex flex-wrap items-center justify-between gap-3 text-xs shrink-0">
            <div class="flex items-center gap-3 flex-wrap">
              <span class="font-bold text-cyan-300 flex items-center gap-1">
                <Server class="w-3.5 h-3.5" /> {sshHealth.kernel}
              </span>
              <span class="text-slate-400">Uptime: {sshHealth.uptime}</span>
              <span class="px-2 py-0.5 rounded bg-slate-800 text-emerald-300 font-mono">
                CPU Load: {sshHealth.load_1m} ({sshHealth.cpu_cores} cores)
              </span>
              <span class="px-2 py-0.5 rounded bg-slate-800 text-amber-300 font-mono">
                RAM: {sshHealth.ram_used_mb}/{sshHealth.ram_total_mb} MB
              </span>
              <span class="px-2 py-0.5 rounded bg-slate-800 text-indigo-300 font-mono">
                Disk /: {sshHealth.disk_used_mb}/{sshHealth.disk_total_mb} MB
              </span>
            </div>
            <button type="button" on:click={() => (sshHealth = null)} class="text-slate-400 hover:text-white cursor-pointer">
              <X class="w-4 h-4" />
            </button>
          </div>
        {/if}

        <div class="flex-1 min-h-0 bg-[#050912] border border-slate-800 rounded-2xl flex flex-col overflow-hidden">
          <!-- Terminal Tabs & Copy/Paste Toolbar -->
          <div class="px-3 py-2 bg-slate-900/90 border-b border-slate-800 flex items-center justify-between gap-2 flex-wrap">
            <div class="flex items-center gap-1 overflow-x-auto">
              {#each termTabs as tab (tab.sessionId)}
                <div
                  class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs cursor-pointer {activeTermId === tab.sessionId
                    ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40'
                    : 'bg-slate-800 text-slate-400'}"
                >
                  <button type="button" on:click={() => selectTermTab(tab.sessionId)} class="flex items-center gap-1.5 cursor-pointer">
                    <span class="w-2 h-2 rounded-full {tab.status === 'connected' ? 'bg-emerald-400' : 'bg-rose-400'}"></span>
                    <span>{tab.server.name}</span>
                  </button>
                  <button type="button" on:click={() => closeTermTab(tab.sessionId)} class="hover:text-white cursor-pointer">
                    <X class="w-3 h-3" />
                  </button>
                </div>
              {/each}
              {#if termTabs.length === 0}
                <span class="text-xs text-slate-500">Pilih server di sebelah kiri lalu klik Connect</span>
              {/if}
            </div>

            {#if activeTermId}
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  on:click={copyTerminalSelection}
                  title="Copy selection (Ctrl+Shift+C)"
                  class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-[11px] flex items-center gap-1 cursor-pointer transition active:scale-95"
                >
                  <Copy class="w-3 h-3" /> Copy (Ctrl+Shift+C)
                </button>
                <button
                  type="button"
                  on:click={pasteToTerminal}
                  title="Paste to terminal (Ctrl+Shift+V)"
                  class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-[11px] flex items-center gap-1 cursor-pointer transition active:scale-95"
                >
                  <ClipboardPaste class="w-3 h-3" /> Paste (Ctrl+Shift+V)
                </button>
              </div>
            {/if}
          </div>

          <div class="flex-1 min-h-0 relative overflow-hidden">
            {#each termTabs as tab (tab.sessionId)}
              <div
                use:attachXterm={tab.sessionId}
                class="absolute inset-0 p-2 overflow-hidden {activeTermId === tab.sessionId ? 'block' : 'hidden'}"
              ></div>
            {/each}
          </div>
        </div>
      </div>
    </div>

  <!-- TAB 2: DATABASE MANAGER -->
  {:else if activeSubTab === "db"}
    <div class="flex-1 min-h-0 overflow-y-auto space-y-3 pr-1">
      <!-- On-Demand Clients & DB Admin Installer -->
      {#if dbTools}
        <div class="bg-slate-900/80 border border-slate-800 rounded-2xl p-3.5 space-y-2">
          <div class="flex items-center justify-between flex-wrap gap-2">
            <div>
              <h3 class="text-xs font-bold text-white">On-Demand DB Clients &amp; Lightweight DB Admin</h3>
              <p class="text-[11px] text-slate-400">
                Tidak di-bundle agar app ringan. Install sesuai kebutuhan — port DB Admin otomatis di 48910–48999 (bebas bentrok WAMP/XAMPP).
              </p>
            </div>
            <button type="button" on:click={loadDbOverview} class="p-1.5 rounded-lg bg-slate-800 text-slate-300 hover:text-white cursor-pointer">
              <RefreshCw class="w-3.5 h-3.5" />
            </button>
          </div>
          <div class="flex flex-wrap gap-2 text-[11px]">
            <span class="px-2.5 py-1 rounded-lg border {dbTools.mysqldump_client ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300' : 'bg-slate-800 border-slate-700 text-slate-400'}">
              MySQL Client: {dbTools.mysqldump_client ? "Ready" : "Not in PATH"}
            </span>
            <span class="px-2.5 py-1 rounded-lg border {dbTools.pg_dump_client ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300' : 'bg-slate-800 border-slate-700 text-slate-400'}">
              Postgres Client: {dbTools.pg_dump_client ? "Ready" : "Not in PATH"}
            </span>
            {#each [
              { id: "sqlite_client", label: "SQLite CLI", ok: !!dbTools.sqlite_client },
              { id: "php", label: "PHP Runtime", ok: !!dbTools.php_bin },
              { id: "adminer", label: "Adminer (Light)", ok: dbTools.adminer_installed },
              { id: "phpmyadmin", label: "phpMyAdmin", ok: dbTools.phpmyadmin_installed },
              { id: "pgweb", label: "pgweb (Postgres)", ok: !!dbTools.pgweb_bin },
            ] as tool}
              <button
                type="button"
                disabled={installingTool === tool.id}
                on:click={() => installDbTool(tool.id)}
                class="px-2.5 py-1 rounded-lg border flex items-center gap-1 cursor-pointer {tool.ok
                  ? 'bg-emerald-500/15 border-emerald-500/30 text-emerald-300'
                  : 'bg-cyan-500/15 border-cyan-500/30 text-cyan-300 hover:bg-cyan-500/25'}"
              >
                {#if installingTool === tool.id}
                  <Loader2 class="w-3 h-3 animate-spin" />
                {:else}
                  <Download class="w-3 h-3" />
                {/if}
                <span>{tool.label}: {tool.ok ? "Installed (Reinstall)" : "Install"}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Connections Header -->
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-bold text-white">Saved Database Configs ({dbConns.length})</h3>
        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={() => {
              schedForm = { connection_id: dbConns[0]?.id || "", interval_minutes: 1440, keep_last: 7, enabled: true };
              showSchedModal = true;
            }}
            disabled={dbConns.length === 0}
            class="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-amber-300 text-xs font-semibold flex items-center gap-1.5 cursor-pointer disabled:opacity-40"
          >
            <Clock class="w-3.5 h-3.5" /> + Auto-Backup Schedule
          </button>
          <button
            type="button"
            on:click={() => {
              dbForm = {
                id: "",
                name: "",
                db_type: "mysql",
                host: "127.0.0.1",
                port: 3306,
                username: "root",
                password: "",
                database_name: "",
                ssh_server_id: "",
                admin_tool: "adminer",
                backup_dir: "",
              };
              showDbModal = true;
            }}
            class="px-3 py-1.5 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 text-xs font-bold flex items-center gap-1.5 cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" /> Add Database
          </button>
        </div>
      </div>

      <!-- DB Cards -->
      <div class="grid grid-cols-1 xl:grid-cols-2 gap-3">
        {#each dbConns as conn (conn.id)}
          {@const running = dbTools?.running_admins.find((a) => a.connection_id === conn.id)}
          {@const health = dbHealthMap[conn.id]}
          {@const connBackups = dbHistory.filter((b) => b.connection_id === conn.id)}
          {@const connSched = dbSchedules.find((s) => s.connection_id === conn.id)}
          <div class="bg-slate-900/75 border border-slate-800 rounded-2xl p-4 space-y-3">
            <div class="flex items-start justify-between gap-2">
              <div>
                <div class="flex items-center gap-2">
                  <span class="text-sm font-bold text-white">{conn.name}</span>
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-bold uppercase bg-indigo-500/20 text-indigo-300 border border-indigo-500/30">
                    {conn.db_type}
                  </span>
                  <span class="px-2 py-0.5 rounded-full text-[10px] bg-slate-800 text-slate-300">
                    Admin: {conn.admin_tool}
                  </span>
                </div>
                <div class="text-xs font-mono text-slate-400 mt-0.5">
                  {conn.username}@{conn.host}:{conn.port}/{conn.database_name}
                  {conn.ssh_server_id ? " • via SSH Tunnel" : ""}
                </div>
              </div>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  on:click={() => {
                    dbForm = { ...conn };
                    showDbModal = true;
                  }}
                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 cursor-pointer"
                >
                  <Edit3 class="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  on:click={async () => {
                    await invoke("dbm_delete_connection", { id: conn.id });
                    await loadDbOverview();
                  }}
                  class="p-1.5 rounded-lg bg-slate-800 hover:bg-rose-500/20 text-rose-400 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            {#if health}
              <div class="p-2.5 rounded-xl text-[11px] font-mono border {health.ok ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-200' : 'bg-rose-500/10 border-rose-500/30 text-rose-200'}">
                {#if health.ok}
                  {health.version} • {health.latency_ms}ms • Conn: {health.active_connections} • Tables: {health.table_count} • Size: {health.size_mb} MB
                {:else}
                  {health.message}
                {/if}
              </div>
            {/if}

            <div class="flex flex-wrap items-center gap-1.5">
              {#if running}
                <button
                  type="button"
                  on:click={() => openExternal(running.url)}
                  class="px-2.5 py-1.5 rounded-lg bg-emerald-500 text-slate-950 font-bold text-xs flex items-center gap-1 cursor-pointer"
                >
                  <ExternalLink class="w-3.5 h-3.5" /> Open ({running.port})
                </button>
                <button
                  type="button"
                  on:click={() => stopDbAdmin(conn.id)}
                  class="px-2.5 py-1.5 rounded-lg bg-rose-500/20 text-rose-300 text-xs font-semibold flex items-center gap-1 cursor-pointer"
                >
                  <Square class="w-3 h-3" /> Stop Admin
                </button>
              {:else}
                <button
                  type="button"
                  disabled={launchingDbId === conn.id}
                  on:click={() => launchDbAdmin(conn)}
                  class="px-2.5 py-1.5 rounded-lg bg-indigo-500/20 hover:bg-indigo-500/30 text-indigo-300 border border-indigo-500/30 text-xs font-semibold flex items-center gap-1 cursor-pointer transition active:scale-95 disabled:opacity-60"
                >
                  {#if launchingDbId === conn.id}
                    <Loader2 class="w-3.5 h-3.5 animate-spin" />
                    <span>Opening...</span>
                  {:else}
                    <ExternalLink class="w-3.5 h-3.5" />
                    <span>Launch {conn.admin_tool}</span>
                  {/if}
                </button>
              {/if}

              <button
                type="button"
                disabled={checkingDbId === conn.id}
                on:click={() => checkDbHealth(conn.id)}
                class="px-2.5 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-cyan-300 text-xs font-semibold flex items-center gap-1 cursor-pointer transition active:scale-95 disabled:opacity-60"
              >
                {#if checkingDbId === conn.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin" />
                {:else}
                  <HeartPulse class="w-3.5 h-3.5" />
                {/if}
                <span>Health</span>
              </button>

              <button
                type="button"
                disabled={backingUpDbId === conn.id}
                on:click={() => backupDbNow(conn.id)}
                class="px-2.5 py-1.5 rounded-lg bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/30 text-xs font-semibold flex items-center gap-1 cursor-pointer transition active:scale-95 disabled:opacity-60"
              >
                {#if backingUpDbId === conn.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin" />
                {:else}
                  <Download class="w-3.5 h-3.5" />
                {/if}
                <span>Backup .sql.gz</span>
              </button>

              <button
                type="button"
                disabled={restoringDbId === conn.id}
                on:click={() => restoreDbFile(conn)}
                class="px-2.5 py-1.5 rounded-lg bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/30 text-xs font-semibold flex items-center gap-1 cursor-pointer transition active:scale-95 disabled:opacity-60"
              >
                {#if restoringDbId === conn.id}
                  <Loader2 class="w-3.5 h-3.5 animate-spin" />
                {:else}
                  <Upload class="w-3.5 h-3.5" />
                {/if}
                <span>Safe Restore</span>
              </button>
            </div>

            <!-- Per-Database Backup History (Manual + Scheduled) -->
            <div class="pt-2 border-t border-slate-800/80 space-y-1.5">
              <div class="flex items-center justify-between text-[11px]">
                <span class="font-semibold text-slate-300">
                  Backup History ({connBackups.length})
                </span>
                {#if connSched}
                  <span class="text-amber-300 font-mono">
                    Auto: every {connSched.interval_minutes}m (keep {connSched.keep_last})
                  </span>
                {/if}
              </div>
              {#if connBackups.length === 0}
                <div class="text-[11px] text-slate-500">Belum ada riwayat backup manual/otomatis.</div>
              {:else}
                <div class="max-h-28 overflow-y-auto space-y-1 pr-1">
                  {#each connBackups.slice(0, 6) as bk (bk.id)}
                    <div class="flex items-center justify-between gap-2 px-2 py-1 rounded-lg bg-slate-950/75 border border-slate-800/80 text-[10px]">
                      <div class="min-w-0">
                        <div class="font-mono text-slate-300 truncate">{bk.file_path.split(/[\\/]/).pop()}</div>
                        <div class="text-slate-500">
                          {new Date(bk.created_at * 1000).toLocaleString()} • {Math.round(bk.size_bytes / 1024)} KB
                        </div>
                      </div>
                      <button
                        type="button"
                        disabled={restoringDbId === conn.id}
                        on:click={() => restoreSpecificBackup(conn, bk.file_path)}
                        class="px-2 py-0.5 rounded bg-amber-500/15 hover:bg-amber-500/30 text-amber-300 font-semibold shrink-0 cursor-pointer transition active:scale-95"
                      >
                        Restore
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <!-- Schedules & Backup History -->
      {#if dbSchedules.length > 0 || dbHistory.length > 0}
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-3">
          <div class="bg-slate-900/75 border border-slate-800 rounded-2xl p-3.5 space-y-2">
            <h4 class="text-xs font-bold text-white">Auto-Backup Schedules</h4>
            {#each dbSchedules as sc (sc.id)}
              {@const cname = dbConns.find((c) => c.id === sc.connection_id)?.name || sc.connection_id}
              <div class="flex items-center justify-between text-xs p-2 rounded-lg bg-slate-950/70 border border-slate-800">
                <div>
                  <span class="font-semibold text-white">{cname}</span>
                  <span class="text-slate-400"> • every {sc.interval_minutes}m (keep {sc.keep_last})</span>
                </div>
                <button
                  type="button"
                  on:click={async () => {
                    await invoke("dbm_delete_schedule", { id: sc.id });
                    await loadDbOverview();
                  }}
                  class="text-rose-400 hover:text-rose-300 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          </div>

          <div class="bg-slate-900/75 border border-slate-800 rounded-2xl p-3.5 space-y-2">
            <h4 class="text-xs font-bold text-white">Recent Backups (.sql.gz)</h4>
            {#each dbHistory.slice(0, 8) as bk (bk.id)}
              <div class="flex items-center justify-between text-[11px] p-2 rounded-lg bg-slate-950/70 border border-slate-800 font-mono">
                <span class="truncate text-slate-300">{bk.file_path}</span>
                <span class="text-emerald-300 shrink-0 ml-2">{Math.round(bk.size_bytes / 1024)} KB</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>

  <!-- TAB 3: UPTIME MONITOR -->
  {:else if activeSubTab === "uptime"}
    <div class="flex-1 min-h-0 overflow-y-auto space-y-3 pr-1">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-wrap items-center gap-1.5">
          <button
            type="button"
            on:click={() => (selectedUptimeGroupId = "")}
            class="px-2.5 py-1 rounded-full text-xs font-semibold cursor-pointer {selectedUptimeGroupId === ''
              ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40'
              : 'bg-slate-800 text-slate-400'}"
          >
            All ({uptimeMonitors.length})
          </button>
          {#each uptimeGroups as g}
            <button
              type="button"
              on:click={() => (selectedUptimeGroupId = g.id)}
              class="px-2.5 py-1 rounded-full text-xs font-semibold cursor-pointer border {selectedUptimeGroupId === g.id
                ? 'bg-slate-800 text-white border-emerald-400'
                : 'bg-slate-900 text-slate-400 border-slate-700'}"
              style="border-left-color: {g.color}; border-left-width: 3px;"
            >
              {g.name}
            </button>
          {/each}
        </div>

        <div class="flex items-center gap-1.5">
          <button
            type="button"
            disabled={publishingStatus}
            on:click={shareGroupStatusPage}
            class="px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-cyan-300 text-xs font-semibold flex items-center gap-1 cursor-pointer transition active:scale-95 disabled:opacity-60"
          >
            {#if publishingStatus}
              <Loader2 class="w-3.5 h-3.5 animate-spin" />
            {:else}
              <Share2 class="w-3.5 h-3.5" />
            {/if}
            <span>Share Public Status</span>
          </button>
          <button
            type="button"
            on:click={() => {
              uptimeGroupForm = { id: "", name: "", color: "#10b981" };
              showUptimeGroupModal = true;
            }}
            class="px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-semibold flex items-center gap-1 cursor-pointer"
          >
            <FolderPlus class="w-3.5 h-3.5" /> + Group
          </button>
          <button
            type="button"
            on:click={() => {
              monitorForm = {
                id: "",
                group_id: selectedUptimeGroupId,
                name: "",
                monitor_type: "http",
                target: "https://",
                interval_minutes: 5,
                timeout_secs: 10,
                expected_status: 200,
                enabled: true,
              };
              showMonitorModal = true;
            }}
            class="px-3 py-1.5 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 text-xs font-bold flex items-center gap-1 cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" /> Add Monitor
          </button>
        </div>
      </div>

      {#if sharedStatusUrl}
        <div class="bg-cyan-500/10 border border-cyan-500/30 rounded-xl px-3 py-2 flex items-center justify-between gap-2 text-xs">
          <div class="min-w-0">
            <span class="text-cyan-300 font-semibold">Public Status URL: </span>
            <a href={sharedStatusUrl} target="_blank" rel="noreferrer" class="font-mono text-white underline truncate">{sharedStatusUrl}</a>
          </div>
          <button
            type="button"
            on:click={() => openExternal(sharedStatusUrl)}
            class="px-2.5 py-1 rounded-lg bg-cyan-500 text-slate-950 font-bold text-[11px] shrink-0 cursor-pointer active:scale-95"
          >
            Open
          </button>
        </div>
      {/if}

      <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
        {#each uptimeMonitors.filter((m) => !selectedUptimeGroupId || m.group_id === selectedUptimeGroupId) as mon (mon.id)}
          {@const dayCutoff = Math.floor(Date.now() / 1000) - 86400}
          {@const history = uptimeChecks.filter((c) => c.monitor_id === mon.id && c.checked_at >= dayCutoff).slice(0, 72).reverse()}
          {@const upCount = history.filter((c) => c.status === 'up').length}
          {@const uptimePct = history.length > 0 ? ((upCount / history.length) * 100).toFixed(1) : '—'}
          <div class="bg-slate-900/75 border border-slate-800 rounded-2xl p-4 space-y-2.5">
            <div class="flex items-start justify-between gap-2">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-bold uppercase {mon.current_status === 'up'
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40'
                    : mon.current_status === 'down'
                      ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
                      : 'bg-slate-800 text-slate-400'}">
                    {mon.current_status}
                  </span>
                  <span class="text-sm font-bold text-white truncate">{mon.name}</span>
                </div>
                <div class="text-xs font-mono text-slate-400 truncate mt-0.5">
                  {mon.target} • {mon.last_latency_ms}ms • every {mon.interval_minutes}m
                </div>
              </div>
              <div class="flex items-center gap-1 shrink-0">
                <button
                  type="button"
                  disabled={checkingMonitorId === mon.id}
                  on:click={() => checkMonitorNow(mon.id)}
                  class="px-2 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-cyan-300 text-[11px] font-semibold cursor-pointer"
                >
                  {checkingMonitorId === mon.id ? "..." : "Check"}
                </button>
                <button
                  type="button"
                  on:click={async () => {
                    await invoke("uptime_delete_monitor", { id: mon.id });
                    await loadUptimeOverview();
                  }}
                  class="p-1 rounded-lg bg-slate-800 hover:bg-rose-500/20 text-rose-400 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            {#if mon.last_error}
              <div class="text-[11px] text-rose-300 font-mono truncate">{mon.last_error}</div>
            {/if}

            <!-- 24h Per-Check Status Bars (Green = UP, Red = DOWN) -->
            <div class="space-y-1">
              <div class="flex items-center justify-between text-[10px] text-slate-400">
                <span>24h Checks ({history.length})</span>
                <span class="font-mono text-emerald-300">{uptimePct}% uptime</span>
              </div>
              <div class="flex items-center gap-[3px] h-6 overflow-hidden">
                {#if history.length === 0}
                  <div class="flex-1 h-full rounded bg-slate-800/70"></div>
                {:else}
                  {#each history as chk}
                    <span
                      class="flex-1 min-w-[3px] max-w-[10px] h-full rounded-[2px] transition-opacity hover:opacity-80 {chk.status === 'up' ? 'bg-emerald-400' : 'bg-rose-500'}"
                      title="{chk.status.toUpperCase()} • {chk.latency_ms}ms • {new Date(chk.checked_at * 1000).toLocaleTimeString()}"
                    ></span>
                  {/each}
                {/if}
              </div>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<!-- Modals -->
{#if showGroupModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-sm space-y-3">
      <h3 class="text-sm font-bold text-white">SSH Server Group</h3>
      <input type="text" bind:value={groupForm.name} placeholder="Group name (e.g. Production)" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white" />
      <input type="color" bind:value={groupForm.color} class="w-12 h-8 rounded bg-transparent cursor-pointer" />
      <div class="flex justify-end gap-2">
        <button type="button" on:click={() => (showGroupModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-xs text-slate-300">Cancel</button>
        <button type="button" on:click={saveSshGroup} class="px-3 py-1.5 rounded-xl bg-cyan-500 text-slate-950 font-bold text-xs">Save</button>
      </div>
    </div>
  </div>
{/if}

{#if showServerModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-md space-y-2.5 text-xs">
      <h3 class="text-sm font-bold text-white">SSH Server Config</h3>
      <input type="text" bind:value={serverForm.name} placeholder="Server Name (e.g. VPS Prod)" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      <select bind:value={serverForm.group_id} class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
        <option value="">No Group</option>
        {#each sshGroups as g}<option value={g.id}>{g.name}</option>{/each}
      </select>
      <div class="grid grid-cols-3 gap-2">
        <input type="text" bind:value={serverForm.host} placeholder="Host / IP" class="col-span-2 px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="number" bind:value={serverForm.port} placeholder="22" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      </div>
      <div class="grid grid-cols-2 gap-2">
        <input type="text" bind:value={serverForm.username} placeholder="Username" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <select bind:value={serverForm.auth_type} class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
          <option value="password">Password</option>
          <option value="key">Private Key</option>
        </select>
      </div>
      {#if serverForm.auth_type === "key"}
        <input type="text" bind:value={serverForm.private_key_path} placeholder="Path to private key (id_ed25519 / .pem)" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      {/if}
      <input type="password" bind:value={serverForm.password} placeholder={serverForm.auth_type === "key" ? "Key passphrase (optional)" : "SSH Password"} class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      {#if sshTestResult}
        <div class="px-2.5 py-1.5 rounded-xl text-[11px] font-mono border {sshTestResult.ok ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300' : 'bg-rose-500/10 border-rose-500/30 text-rose-300'}">
          {sshTestResult.msg}
        </div>
      {/if}
      <div class="flex items-center justify-between gap-2 pt-1">
        <button
          type="button"
          disabled={testingSsh}
          on:click={testSshConnection}
          class="px-3 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-cyan-300 font-semibold flex items-center gap-1.5 cursor-pointer transition active:scale-95 disabled:opacity-60"
        >
          {#if testingSsh}
            <Loader2 class="w-3.5 h-3.5 animate-spin" />
            <span>Testing...</span>
          {:else}
            <Play class="w-3.5 h-3.5" />
            <span>Test Connection</span>
          {/if}
        </button>
        <div class="flex items-center gap-2">
          <button type="button" on:click={() => (showServerModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-slate-300 cursor-pointer active:scale-95">Cancel</button>
          <button type="button" on:click={saveSshServer} class="px-3 py-1.5 rounded-xl bg-cyan-500 text-slate-950 font-bold cursor-pointer active:scale-95">Save</button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if showDbModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-md space-y-2.5 text-xs">
      <h3 class="text-sm font-bold text-white">Database Connection</h3>
      <input type="text" bind:value={dbForm.name} placeholder="Connection Name" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      <div class="grid grid-cols-2 gap-2">
        <select bind:value={dbForm.db_type} class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
          <option value="mysql">MySQL / MariaDB</option>
          <option value="postgres">PostgreSQL</option>
          <option value="sqlite">SQLite</option>
        </select>
        <select bind:value={dbForm.admin_tool} class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
          <option value="adminer">Adminer</option>
          <option value="phpmyadmin">phpMyAdmin</option>
          <option value="pgweb">pgweb</option>
        </select>
      </div>
      <div class="grid grid-cols-3 gap-2">
        <input type="text" bind:value={dbForm.host} placeholder="Host" class="col-span-2 px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="number" bind:value={dbForm.port} placeholder="3306" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      </div>
      <div class="grid grid-cols-2 gap-2">
        <input type="text" bind:value={dbForm.username} placeholder="Username" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="password" bind:value={dbForm.password} placeholder="Password" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      </div>
      <input type="text" bind:value={dbForm.database_name} placeholder="Database name (or SQLite file path)" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      <select bind:value={dbForm.ssh_server_id} class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
        <option value="">Direct Connection (No SSH Tunnel)</option>
        {#each sshServers as s}<option value={s.id}>SSH Tunnel: {s.name} ({s.host})</option>{/each}
      </select>
      <div class="flex justify-end gap-2 pt-1">
        <button type="button" on:click={() => (showDbModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-slate-300">Cancel</button>
        <button type="button" on:click={saveDbConn} class="px-3 py-1.5 rounded-xl bg-cyan-500 text-slate-950 font-bold">Save</button>
      </div>
    </div>
  </div>
{/if}

{#if showSchedModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-sm space-y-2.5 text-xs">
      <h3 class="text-sm font-bold text-white">Auto-Backup Schedule (.sql.gz)</h3>
      <select bind:value={schedForm.connection_id} class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
        {#each dbConns as c}<option value={c.id}>{c.name}</option>{/each}
      </select>
      <div class="grid grid-cols-2 gap-2">
        <input type="number" bind:value={schedForm.interval_minutes} placeholder="Interval (mins)" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="number" bind:value={schedForm.keep_last} placeholder="Keep last N" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      </div>
      <div class="flex justify-end gap-2 pt-1">
        <button type="button" on:click={() => (showSchedModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-slate-300">Cancel</button>
        <button type="button" on:click={saveDbSchedule} class="px-3 py-1.5 rounded-xl bg-cyan-500 text-slate-950 font-bold">Save</button>
      </div>
    </div>
  </div>
{/if}

{#if showUptimeGroupModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-sm space-y-3">
      <h3 class="text-sm font-bold text-white">Uptime Group</h3>
      <input type="text" bind:value={uptimeGroupForm.name} placeholder="Group name" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-xs text-white" />
      <input type="color" bind:value={uptimeGroupForm.color} class="w-12 h-8 rounded bg-transparent cursor-pointer" />
      <div class="flex justify-end gap-2">
        <button type="button" on:click={() => (showUptimeGroupModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-xs text-slate-300">Cancel</button>
        <button type="button" on:click={saveUptimeGroup} class="px-3 py-1.5 rounded-xl bg-emerald-500 text-slate-950 font-bold text-xs">Save</button>
      </div>
    </div>
  </div>
{/if}

{#if showMonitorModal}
  <div class="fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4">
    <div class="bg-slate-900 border border-slate-700 rounded-2xl p-4 w-full max-w-md space-y-2.5 text-xs">
      <h3 class="text-sm font-bold text-white">Add Uptime Monitor</h3>
      <input type="text" bind:value={monitorForm.name} placeholder="Service Name" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      <div class="grid grid-cols-2 gap-2">
        <select bind:value={monitorForm.group_id} class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
          <option value="">No Group</option>
          {#each uptimeGroups as g}<option value={g.id}>{g.name}</option>{/each}
        </select>
        <select bind:value={monitorForm.monitor_type} class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white">
          <option value="http">HTTP / HTTPS</option>
          <option value="tcp">TCP Port</option>
        </select>
      </div>
      <input type="text" bind:value={monitorForm.target} placeholder="https://example.com or 1.2.3.4:5432" class="w-full px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      <div class="grid grid-cols-3 gap-2">
        <input type="number" bind:value={monitorForm.interval_minutes} placeholder="Interval (min)" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="number" bind:value={monitorForm.timeout_secs} placeholder="Timeout (s)" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
        <input type="number" bind:value={monitorForm.expected_status} placeholder="200" class="px-3 py-2 rounded-xl bg-slate-950 border border-slate-700 text-white" />
      </div>
      <div class="flex justify-end gap-2 pt-1">
        <button type="button" on:click={() => (showMonitorModal = false)} class="px-3 py-1.5 rounded-xl bg-slate-800 text-slate-300">Cancel</button>
        <button type="button" on:click={saveMonitor} class="px-3 py-1.5 rounded-xl bg-emerald-500 text-slate-950 font-bold">Save</button>
      </div>
    </div>
  </div>
{/if}
