const RAYDROP_PORT = 45321;
const PING_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/ping`;
const DOWNLOAD_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/download`;

const statusBadge = document.getElementById("statusBadge");
const statusText = document.getElementById("statusText");
const urlInput = document.getElementById("urlInput");
const startBtn = document.getElementById("startBtn");
const msgFeedback = document.getElementById("msgFeedback");
const autoCaptureToggle = document.getElementById("autoCaptureToggle");
const masterToggle = document.getElementById("masterToggle");

// Debugger UI elements
const tabButtons = document.querySelectorAll(".tab-btn");
const tabPanels = document.querySelectorAll(".tab-panel");
const activeTabUrlEl = document.getElementById("activeTabUrl");
const responsiveScanChk = document.getElementById("responsiveScanChk");
const runAuditBtn = document.getElementById("runAuditBtn");
const toggleLiveBtn = document.getElementById("toggleLiveBtn");
const debugFeedback = document.getElementById("debugFeedback");
const auditSummaryCard = document.getElementById("auditSummaryCard");
const overallScoreRing = document.getElementById("overallScoreRing");
const scoreNetwork = document.getElementById("scoreNetwork");
const scoreSpeed = document.getElementById("scoreSpeed");
const scoreUi = document.getElementById("scoreUi");
const scoreSecurity = document.getElementById("scoreSecurity");
const fixPreview = document.getElementById("fixPreview");

let currentTabId = null;
let isLiveActive = false;

tabButtons.forEach((btn) => {
  btn.addEventListener("click", () => {
    const target = btn.dataset.tab;
    tabButtons.forEach((b) => b.classList.toggle("active", b === btn));
    tabPanels.forEach((p) => p.classList.toggle("active", p.id === `tab-${target}`));
  });
});

function isExtVersionOlder(installed, latest) {
  const parse = (v) => String(v || "0").replace(/^v/, "").split(".").map((n) => parseInt(n, 10) || 0);
  const a = parse(installed);
  const b = parse(latest);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) {
    if ((a[i] || 0) < (b[i] || 0)) return true;
    if ((a[i] || 0) > (b[i] || 0)) return false;
  }
  return false;
}

async function checkStatus() {
  const localVer = chrome.runtime.getManifest().version;
  const footerEl = document.getElementById("extVersionFooter");
  if (footerEl) footerEl.textContent = `Raydrop Extension v${localVer}`;

  try {
    const res = await fetch(`${PING_URL}?extVersion=${encodeURIComponent(localVer)}`, { method: "GET" });
    if (res.ok) {
      const data = await res.json().catch(() => ({}));
      const latestVer = data.extensionVersion;
      const banner = document.getElementById("extUpdateBanner");
      const titleEl = document.getElementById("extUpdateTitle");
      if (banner && latestVer && isExtVersionOlder(localVer, latestVer)) {
        banner.style.display = "block";
        if (titleEl) titleEl.textContent = `Update Tersedia: v${localVer} → v${latestVer}`;
      } else if (banner) {
        banner.style.display = "none";
      }

      statusBadge.className = "status-badge";
      statusText.textContent = "Connected";
      return true;
    }
  } catch (_) {}

  statusBadge.className = "status-badge disconnected";
  statusText.textContent = "Offline (Open App)";
  return false;
}

chrome.storage.local.get(["enabled", "autoCapture"], (res) => {
  if (res.enabled !== undefined) masterToggle.checked = res.enabled;
  if (res.autoCapture !== undefined) autoCaptureToggle.checked = res.autoCapture;
});

autoCaptureToggle.addEventListener("change", (e) => {
  chrome.storage.local.set({ autoCapture: e.target.checked });
});

masterToggle.addEventListener("change", (e) => {
  chrome.storage.local.set({ enabled: e.target.checked });
});

startBtn.addEventListener("click", async () => {
  const url = urlInput.value.trim();
  if (!url) {
    showFeedback(msgFeedback, "Please enter a valid URL or magnet link", "#fb7185");
    return;
  }

  startBtn.textContent = "...";
  try {
    const res = await fetch(DOWNLOAD_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url, connections: 8 })
    });

    if (res.ok) {
      showFeedback(msgFeedback, "Download sent to Raydrop Accelerator!", "#34d399");
      urlInput.value = "";
    } else {
      showFeedback(msgFeedback, "Failed to add download. Is Raydrop active?", "#fb7185");
    }
  } catch (_) {
    showFeedback(msgFeedback, "Cannot connect to Raydrop app. Make sure it's running.", "#fb7185");
  } finally {
    startBtn.textContent = "Catch";
  }
});

function showFeedback(el, text, color) {
  el.style.display = "block";
  el.style.color = color;
  el.textContent = text;
  setTimeout(() => {
    el.style.display = "none";
  }, 4000);
}

function getScoreColor(score) {
  if (score >= 90) return "#10b981";
  if (score >= 70) return "#06b6d4";
  return "#f43f5e";
}

function renderAuditSummary(report) {
  if (!report || !report.scores) return;
  auditSummaryCard.style.display = "block";
  const { overall, network, speed, ui, security } = report.scores;
  overallScoreRing.textContent = overall;
  overallScoreRing.style.borderColor = getScoreColor(overall);
  overallScoreRing.style.color = getScoreColor(overall);
  scoreNetwork.textContent = network;
  scoreSpeed.textContent = speed;
  scoreUi.textContent = ui;
  scoreSecurity.textContent = security;

  fixPreview.innerHTML = "";
  const fixes = report.fixes || [];
  if (fixes.length === 0) {
    fixPreview.innerHTML = `<div style="color:#34d399;">No critical issues found!</div>`;
    return;
  }
  fixes.slice(0, 3).forEach((f) => {
    const color = f.severity === "critical" ? "#f43f5e" : f.severity === "warning" ? "#f59e0b" : "#38bdf8";
    const row = document.createElement("div");
    row.className = "fix-item";
    row.innerHTML = `<span class="sev-dot" style="background:${color}"></span><span>${escapeHtml(f.title)}</span>`;
    fixPreview.appendChild(row);
  });
  if (fixes.length > 3) {
    const more = document.createElement("div");
    more.style.color = "#94a3b8";
    more.textContent = `+${fixes.length - 3} more items in Raydrop Desktop`;
    fixPreview.appendChild(more);
  }
}

function escapeHtml(str) {
  return String(str).replace(/[&<>"']/g, (m) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
  }[m]));
}

function updateLiveButton() {
  if (isLiveActive) {
    toggleLiveBtn.className = "btn btn-full btn-stop";
    toggleLiveBtn.textContent = "Stop Live Debug";
  } else {
    toggleLiveBtn.className = "btn btn-full btn-live";
    toggleLiveBtn.textContent = "Start Live Debug (Network & Errors)";
  }
}

async function initDebuggerTab() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (tab) {
      currentTabId = tab.id;
      activeTabUrlEl.textContent = tab.url || "Unknown URL";
    }
  } catch (_) {
    activeTabUrlEl.textContent = "No active tab";
  }

  chrome.runtime.sendMessage({ type: "GET_DEBUG_STATE" }, (res) => {
    if (res) {
      isLiveActive = Boolean(res.active);
      updateLiveButton();
      if (res.lastAudit) renderAuditSummary(res.lastAudit);
    }
  });
}

runAuditBtn.addEventListener("click", () => {
  if (!currentTabId) return;
  runAuditBtn.disabled = true;
  runAuditBtn.textContent = "Analyzing Page...";
  chrome.runtime.sendMessage(
    {
      type: "RUN_FULL_AUDIT",
      tabId: currentTabId,
      includeResponsiveScan: responsiveScanChk.checked
    },
    (res) => {
      runAuditBtn.disabled = false;
      runAuditBtn.textContent = "Run Full Audit (UI, Speed, Latency)";
      if (!res || res.error) {
        showFeedback(debugFeedback, (res && res.error) || "Audit failed", "#fb7185");
        return;
      }
      renderAuditSummary(res.report);
      if (res.sentToDesktop) {
        showFeedback(debugFeedback, "Audit complete! Opened in Raydrop Desktop.", "#34d399");
      } else {
        showFeedback(debugFeedback, "Audit complete (Raydrop Desktop offline).", "#fbbf24");
      }
    }
  );
});

toggleLiveBtn.addEventListener("click", () => {
  if (!currentTabId) return;
  const msgType = isLiveActive ? "STOP_LIVE_DEBUG" : "START_LIVE_DEBUG";
  if (isLiveActive) {
    toggleLiveBtn.disabled = true;
    toggleLiveBtn.textContent = "Stopping & Analyzing Flow...";
  }
  chrome.runtime.sendMessage({ type: msgType, tabId: currentTabId }, (res) => {
    toggleLiveBtn.disabled = false;
    if (res && !res.error) {
      isLiveActive = Boolean(res.active);
      updateLiveButton();
      if (res.report) {
        renderAuditSummary(res.report);
      }
      showFeedback(
        debugFeedback,
        isLiveActive
          ? "Live Debug started! Interact with the page, then click Stop."
          : res.report
            ? res.sentToDesktop
              ? "Flow analyzed & opened in Raydrop Desktop!"
              : "Flow analyzed! (Raydrop Desktop offline)"
            : "Live Debug stopped.",
        isLiveActive ? "#fbbf24" : "#34d399"
      );
    } else {
      updateLiveButton();
      if (res && res.error) {
        showFeedback(debugFeedback, res.error, "#fb7185");
      }
    }
  });
});

checkStatus();
initDebuggerTab();
