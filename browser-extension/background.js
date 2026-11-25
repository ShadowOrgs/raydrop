const RAYDROP_PORT = 45321;
const RAYDROP_API_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/download`;
const RAYDROP_PING_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/ping`;

const DEFAULT_EXTENSIONS = [
  "zip", "rar", "7z", "tar", "gz", "bz2", "xz",
  "iso", "img", "bin", "exe", "msi", "dmg", "pkg", "deb", "apk",
  "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv",
  "mp3", "flac", "wav", "m4a", "aac",
  "pdf", "docx", "xlsx", "pptx"
];

// In-memory set to prevent duplicate download interception loops
const handledDownloadIds = new Set();

// Initialize context menu and default settings on installation
chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.get(["enabled", "extensions", "autoCapture"], (res) => {
    if (res.enabled === undefined) chrome.storage.local.set({ enabled: true });
    if (res.autoCapture === undefined) chrome.storage.local.set({ autoCapture: true });
    if (!res.extensions) chrome.storage.local.set({ extensions: DEFAULT_EXTENSIONS });
  });

  chrome.contextMenus.create({
    id: "raydrop-download-link",
    title: "Download with Raydrop Accelerator",
    contexts: ["link"]
  });

  chrome.contextMenus.create({
    id: "raydrop-download-media",
    title: "Download Media with Raydrop",
    contexts: ["image", "video", "audio"]
  });

  updateConnectionStatusBadge();
});

// Context Menu Action Handler
chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let targetUrl = "";
  if (info.menuItemId === "raydrop-download-link" && info.linkUrl) {
    targetUrl = info.linkUrl;
  } else if (info.menuItemId === "raydrop-download-media" && info.srcUrl) {
    targetUrl = info.srcUrl;
  }

  if (targetUrl) {
    sendToRaydrop(targetUrl, "", tab?.url || "");
  }
});

// Automatic Browser Download Interceptor
chrome.downloads.onCreated.addListener(async (downloadItem) => {
  if (handledDownloadIds.has(downloadItem.id)) return;

  // 1. Only intercept active, in-progress downloads (not historical or completed items restored by browser)
  if (downloadItem.state && downloadItem.state !== "in_progress") {
    return;
  }

  // 2. Ignore completed items or items with endTime
  if (downloadItem.endTime) {
    return;
  }

  // 3. Ignore items started in the past (> 15 seconds ago, e.g. cache clear / history rebuild / browser session restore)
  if (downloadItem.startTime) {
    const startMs = new Date(downloadItem.startTime).getTime();
    if (!isNaN(startMs) && Date.now() - startMs > 15000) {
      return;
    }
  }

  // 4. Ignore downloads that have already received bytes from a previous session
  if (downloadItem.bytesReceived && downloadItem.bytesReceived > 0) {
    return;
  }

  const config = await chrome.storage.local.get(["enabled", "autoCapture", "extensions"]);
  if (config.enabled === false || config.autoCapture === false) return;

  const url = downloadItem.finalUrl || downloadItem.url;
  if (!url || url.startsWith("blob:") || url.startsWith("data:") || url.startsWith("chrome://")) {
    return;
  }

  // Check file extension
  const filename = downloadItem.filename || "";
  const ext = getUrlOrFileExtension(url, filename);
  const extensionsList = config.extensions || DEFAULT_EXTENSIONS;

  const isMatchedExt = ext && extensionsList.includes(ext.toLowerCase());
  const isMagnet = url.startsWith("magnet:?");

  if (isMatchedExt || isMagnet) {
    handledDownloadIds.add(downloadItem.id);
    // Auto-expire from handled set after 5 seconds so retrying/redownloading is not blocked
    setTimeout(() => {
      handledDownloadIds.delete(downloadItem.id);
    }, 5000);

    try {
      // Cancel Chrome's native single-threaded download
      await chrome.downloads.cancel(downloadItem.id);
      chrome.downloads.erase({ id: downloadItem.id });
    } catch (e) {
      console.warn("Could not cancel native download:", e);
    }

    // Forward to Raydrop Multi-Segment Accelerator
    const success = await sendToRaydrop(url, filename, downloadItem.referrer || "");
    if (!success) {
      // If Raydrop desktop is closed, show notification
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon128.png",
        title: "Raydrop Downloader",
        message: "Raydrop desktop app is not running. Please open Raydrop to start accelerated download."
      });
    }
  }
});

// Clear handled download IDs when state transitions to interrupted, cancelled, or complete
chrome.downloads.onChanged.addListener((delta) => {
  if (delta.state && (delta.state.current === "interrupted" || delta.state.current === "complete")) {
    handledDownloadIds.delete(delta.id);
  }
});

function getUrlOrFileExtension(url, filename) {
  if (filename) {
    const parts = filename.split(".");
    if (parts.length > 1) return parts.pop().toLowerCase();
  }
  try {
    const parsed = new URL(url);
    const pathname = parsed.pathname;
    const parts = pathname.split(".");
    if (parts.length > 1) return parts.pop().split("/")[0].toLowerCase();
  } catch (_) {}
  return "";
}

// Send download intent to Raydrop local loopback API
async function sendToRaydrop(url, filename, referrer) {
  try {
    const res = await fetch(RAYDROP_API_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: url.trim(),
        filename: filename || null,
        referrer: referrer || null,
        connections: 8
      })
    });

    if (res.ok) {
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon128.png",
        title: "Raydrop Accelerator Active",
        message: `Download started in Raydrop: ${filename || url.slice(0, 40)}...`
      });
      return true;
    }
  } catch (err) {
    console.warn("Failed to dispatch to Raydrop desktop loopback:", err);
  }
  return false;
}

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

// Check Raydrop App Connection & Update Badge
async function updateConnectionStatusBadge() {
  if (debugSession.active) {
    chrome.action.setBadgeText({ text: "DBG" });
    chrome.action.setBadgeBackgroundColor({ color: "#f59e0b" });
    return;
  }
  try {
    const localVer = chrome.runtime.getManifest().version;
    const res = await fetch(`${RAYDROP_PING_URL}?extVersion=${encodeURIComponent(localVer)}`, { method: "GET" });
    if (res.ok) {
      const data = await res.json().catch(() => ({}));
      const latestVer = data.extensionVersion;
      if (latestVer && isExtVersionOlder(localVer, latestVer)) {
        chrome.action.setBadgeText({ text: "UPD" });
        chrome.action.setBadgeBackgroundColor({ color: "#f59e0b" });
        chrome.storage.local.get(["notifiedExtVersion"], (st) => {
          if (st.notifiedExtVersion !== latestVer) {
            chrome.storage.local.set({ notifiedExtVersion: latestVer });
            chrome.notifications.create({
              type: "basic",
              iconUrl: "icons/icon128.png",
              title: `Raydrop Extension Update (v${latestVer})`,
              message: `New version v${latestVer} is ready! Open Raydrop → Browser Extension → Open Folder, then click Reload in chrome://extensions.`
            });
          }
        });
        return;
      }
      chrome.action.setBadgeText({ text: "ON" });
      chrome.action.setBadgeBackgroundColor({ color: "#06b6d4" }); // Cyan
      return;
    }
  } catch (_) {}
  chrome.action.setBadgeText({ text: "" });
}

// ============================================================================
// ON-DEMAND WEB DEBUGGER & ANALYZER (Zero-overhead when idle)
// ============================================================================
const RAYDROP_DEBUG_SESSION_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/debug/session`;
const RAYDROP_DEBUG_AUDIT_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/debug/audit`;
const RAYDROP_DEBUG_EVENTS_URL = `http://127.0.0.1:${RAYDROP_PORT}/api/debug/events`;

const debugSession = {
  active: false,
  tabId: null,
  url: "",
  title: "",
  startedAt: null,
  requestTimings: new Map(),
  batchQueue: [],
  recordedRequests: [],
  recordedErrors: [],
  liveCls: 0,
  flushTimer: null
};

function queueDebugEvent(event) {
  if (!debugSession.active) return;
  debugSession.batchQueue.push(event);
  if (event.kind === "network_request" && event.data && debugSession.recordedRequests.length < 500) {
    debugSession.recordedRequests.push(event.data);
  } else if (event.kind === "console_error" && event.data && debugSession.recordedErrors.length < 100) {
    debugSession.recordedErrors.push(event.data);
  } else if (event.kind === "vital_update" && event.data && typeof event.data.cls === "number") {
    debugSession.liveCls = Math.max(debugSession.liveCls, event.data.cls);
  }
  if (debugSession.batchQueue.length >= 25) {
    flushDebugBatch();
  }
}

async function flushDebugBatch() {
  if (debugSession.batchQueue.length === 0) return;
  const events = debugSession.batchQueue.splice(0, debugSession.batchQueue.length);
  try {
    await fetch(RAYDROP_DEBUG_EVENTS_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        tabId: debugSession.tabId,
        url: debugSession.url,
        events
      })
    });
  } catch (_) {
    // Raydrop desktop may be closed; drop batch silently to avoid memory growth
  }
}

function onWebReqBefore(details) {
  if (!debugSession.active || details.tabId !== debugSession.tabId) return;
  debugSession.requestTimings.set(details.requestId, {
    url: details.url,
    method: details.method,
    type: details.type,
    startTime: details.timeStamp
  });
}

function onWebReqStarted(details) {
  if (!debugSession.active || details.tabId !== debugSession.tabId) return;
  const rec = debugSession.requestTimings.get(details.requestId);
  if (rec) {
    rec.ttfb = Math.max(0, Math.round(details.timeStamp - rec.startTime));
    rec.ip = details.ip || null;
    rec.fromCache = Boolean(details.fromCache);
    rec.statusCode = details.statusCode;
  }
}

function onWebReqCompleted(details) {
  if (!debugSession.active || details.tabId !== debugSession.tabId) return;
  const rec = debugSession.requestTimings.get(details.requestId);
  debugSession.requestTimings.delete(details.requestId);
  const start = rec ? rec.startTime : details.timeStamp;
  const duration = Math.max(0, Math.round(details.timeStamp - start));
  queueDebugEvent({
    kind: "network_request",
    timestamp: Date.now(),
    data: {
      url: details.url,
      method: details.method,
      type: details.type,
      statusCode: details.statusCode,
      duration,
      ttfb: rec ? rec.ttfb : duration,
      ip: details.ip || (rec && rec.ip) || null,
      fromCache: Boolean(details.fromCache || (rec && rec.fromCache)),
      error: details.statusCode >= 400 ? `HTTP ${details.statusCode}` : null
    }
  });
}

function onWebReqError(details) {
  if (!debugSession.active || details.tabId !== debugSession.tabId) return;
  const rec = debugSession.requestTimings.get(details.requestId);
  debugSession.requestTimings.delete(details.requestId);
  const start = rec ? rec.startTime : details.timeStamp;
  queueDebugEvent({
    kind: "network_request",
    timestamp: Date.now(),
    data: {
      url: details.url,
      method: details.method,
      type: details.type,
      statusCode: 0,
      duration: Math.max(0, Math.round(details.timeStamp - start)),
      ttfb: 0,
      ip: null,
      fromCache: false,
      error: details.error || "Network Error"
    }
  });
}

async function startLiveDebug(tabId) {
  if (debugSession.active) {
    await stopLiveDebug();
  }
  const tab = await chrome.tabs.get(tabId);
  debugSession.active = true;
  debugSession.tabId = tabId;
  debugSession.url = tab.url || "";
  debugSession.title = tab.title || "";
  debugSession.startedAt = Date.now();
  debugSession.requestTimings.clear();
  debugSession.batchQueue = [];
  debugSession.recordedRequests = [];
  debugSession.recordedErrors = [];
  debugSession.liveCls = 0;
  debugSession.recordedRequests = [];
  debugSession.recordedErrors = [];
  debugSession.liveCls = 0;

  const filter = { urls: ["<all_urls>"], tabId };
  chrome.webRequest.onBeforeRequest.addListener(onWebReqBefore, filter);
  chrome.webRequest.onResponseStarted.addListener(onWebReqStarted, filter);
  chrome.webRequest.onCompleted.addListener(onWebReqCompleted, filter);
  chrome.webRequest.onErrorOccurred.addListener(onWebReqError, filter);

  try {
    await chrome.scripting.executeScript({ target: { tabId }, files: ["debug_collector.js"] });
    await chrome.scripting.executeScript({
      target: { tabId },
      func: () => window.__raydropDebug && window.__raydropDebug.startLive()
    });
  } catch (e) {
    console.warn("Could not inject live debug collector:", e);
  }

  debugSession.flushTimer = setInterval(flushDebugBatch, 800);
  updateConnectionStatusBadge();

  try {
    await fetch(RAYDROP_DEBUG_SESSION_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        action: "start",
        tabId,
        url: debugSession.url,
        title: debugSession.title,
        timestamp: debugSession.startedAt
      })
    });
  } catch (_) {}

  return { active: true, tabId, url: debugSession.url, title: debugSession.title };
}

async function stopLiveDebug() {
  if (!debugSession.active) return { active: false };
  const prevTabId = debugSession.tabId;
  const prevUrl = debugSession.url;
  const recordedRequests = debugSession.recordedRequests.slice();
  const recordedErrors = debugSession.recordedErrors.slice();
  const recordedCls = debugSession.liveCls;

  await flushDebugBatch();
  if (debugSession.flushTimer) {
    clearInterval(debugSession.flushTimer);
    debugSession.flushTimer = null;
  }

  chrome.webRequest.onBeforeRequest.removeListener(onWebReqBefore);
  chrome.webRequest.onResponseStarted.removeListener(onWebReqStarted);
  chrome.webRequest.onCompleted.removeListener(onWebReqCompleted);
  chrome.webRequest.onErrorOccurred.removeListener(onWebReqError);

  let finalAudit = null;
  if (prevTabId !== null) {
    try {
      await chrome.scripting.executeScript({ target: { tabId: prevTabId }, files: ["debug_collector.js"] });
      const [{ result }] = await chrome.scripting.executeScript({
        target: { tabId: prevTabId },
        func: () => {
          if (!window.__raydropDebug) return null;
          window.__raydropDebug.stopLive();
          return window.__raydropDebug.runAudit();
        }
      });
      finalAudit = result;
    } catch (_) {}
  }

  debugSession.active = false;
  debugSession.tabId = null;
  debugSession.requestTimings.clear();
  debugSession.batchQueue = [];
  debugSession.recordedRequests = [];
  debugSession.recordedErrors = [];
  debugSession.liveCls = 0;
  updateConnectionStatusBadge();

  try {
    await fetch(RAYDROP_DEBUG_SESSION_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        action: "stop",
        tabId: prevTabId,
        url: prevUrl,
        timestamp: Date.now()
      })
    });
  } catch (_) {}

  let report = null;
  let sentToDesktop = false;
  if (finalAudit) {
    if (recordedCls > (finalAudit.vitals?.cls || 0)) {
      finalAudit.vitals = { ...(finalAudit.vitals || {}), cls: recordedCls };
    }
    const durations = recordedRequests.map((r) => r.duration).filter((d) => typeof d === "number");
    const min = durations.length ? Math.min(...durations) : null;
    const max = durations.length ? Math.max(...durations) : null;
    const avg = durations.length ? Math.round(durations.reduce((a, b) => a + b, 0) / durations.length) : null;
    const ping = {
      statusCode: 200,
      headers: {},
      min,
      avg,
      max,
      jitter: min !== null && max !== null ? max - min : null,
      samples: durations.slice(-10)
    };
    const responsiveMatrix = [
      {
        label: "Current",
        width: finalAudit.ui.viewportWidth,
        height: finalAudit.ui.viewportHeight,
        scrollWidth: finalAudit.ui.scrollWidth,
        hasOverflow: finalAudit.ui.hasOverflow,
        overflowPx: finalAudit.ui.overflowPx,
        offenderCount: finalAudit.ui.overflowElements.length
      }
    ];
    const { scores, fixes } = computeScoresAndFixes(
      finalAudit,
      ping,
      responsiveMatrix,
      { requests: recordedRequests, errors: recordedErrors }
    );
    report = {
      id: `flow_${Date.now()}`,
      ...finalAudit,
      ping,
      responsiveMatrix,
      scores,
      fixes
    };
    chrome.storage.local.set({ lastAudit: report });
    try {
      const res = await fetch(RAYDROP_DEBUG_AUDIT_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(report)
      });
      sentToDesktop = res.ok;
    } catch (_) {}
  }

  return { active: false, report, sentToDesktop };
}

chrome.tabs.onUpdated.addListener((tabId, info) => {
  if (debugSession.active && tabId === debugSession.tabId && info.status === "complete") {
    chrome.scripting.executeScript({ target: { tabId }, files: ["debug_collector.js"] })
      .then(() =>
        chrome.scripting.executeScript({
          target: { tabId },
          func: () => window.__raydropDebug && window.__raydropDebug.startLive()
        })
      )
      .catch(() => {});
  }
});

chrome.tabs.onRemoved.addListener((tabId) => {
  if (debugSession.active && debugSession.tabId === tabId) {
    stopLiveDebug();
  }
});

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.type === "RAYDROP_DEBUG_PAGE_EVENT") {
    if (debugSession.active && sender.tab && sender.tab.id === debugSession.tabId) {
      queueDebugEvent({
        kind: msg.kind,
        timestamp: msg.timestamp || Date.now(),
        data: msg.data
      });
    }
    return false;
  }

  if (msg.type === "GET_DEBUG_STATE") {
    chrome.storage.local.get(["lastAudit"], (res) => {
      sendResponse({
        active: debugSession.active,
        tabId: debugSession.tabId,
        url: debugSession.url,
        lastAudit: res.lastAudit || null
      });
    });
    return true;
  }

  if (msg.type === "START_LIVE_DEBUG") {
    startLiveDebug(msg.tabId).then(sendResponse).catch((e) => sendResponse({ error: e.message }));
    return true;
  }

  if (msg.type === "STOP_LIVE_DEBUG") {
    stopLiveDebug().then(sendResponse).catch((e) => sendResponse({ error: e.message }));
    return true;
  }

  if (msg.type === "RUN_FULL_AUDIT") {
    runFullAudit(msg.tabId, Boolean(msg.includeResponsiveScan))
      .then(sendResponse)
      .catch((e) => sendResponse({ error: e.message }));
    return true;
  }
});

async function measureEndpointPing(targetUrl) {
  const samples = [];
  let statusCode = 0;
  let headersObj = {};

  for (let i = 0; i < 5; i++) {
    const t0 = performance.now();
    try {
      const res = await fetch(targetUrl, { method: i === 0 ? "GET" : "HEAD", cache: "no-store" });
      const elapsed = Math.round(performance.now() - t0);
      samples.push(elapsed);
      if (i === 0) {
        statusCode = res.status;
        const pick = [
          "cache-control",
          "content-encoding",
          "content-type",
          "server",
          "strict-transport-security",
          "content-security-policy",
          "x-frame-options",
          "access-control-allow-origin"
        ];
        for (const h of pick) {
          const val = res.headers.get(h);
          if (val) headersObj[h] = val;
        }
      }
    } catch (_) {
      samples.push(null);
    }
  }

  const valid = samples.filter((v) => typeof v === "number");
  if (valid.length === 0) {
    return { statusCode, headers: headersObj, min: null, avg: null, max: null, jitter: null, samples };
  }
  const min = Math.min(...valid);
  const max = Math.max(...valid);
  const avg = Math.round(valid.reduce((a, b) => a + b, 0) / valid.length);
  const jitter = max - min;
  return { statusCode, headers: headersObj, min, avg, max, jitter, samples };
}

async function scanResponsiveViewport(url, width, height, label) {
  let win = null;
  try {
    win = await chrome.windows.create({
      url,
      type: "popup",
      width: width + 16,
      height: height + 38,
      focused: false
    });
    const tabId = win.tabs && win.tabs[0] ? win.tabs[0].id : null;
    if (!tabId) return { label, width, height, error: "No tab created" };

    await new Promise((resolve) => {
      const timeout = setTimeout(resolve, 6000);
      function listener(updatedId, info) {
        if (updatedId === tabId && info.status === "complete") {
          chrome.tabs.onUpdated.removeListener(listener);
          clearTimeout(timeout);
          setTimeout(resolve, 350);
        }
      }
      chrome.tabs.onUpdated.addListener(listener);
    });

    await chrome.scripting.executeScript({ target: { tabId }, files: ["debug_collector.js"] });
    const [{ result }] = await chrome.scripting.executeScript({
      target: { tabId },
      func: () => window.__raydropDebug ? window.__raydropDebug.checkOverflowOnly() : null
    });
    return { label, width, height, ...(result || { error: "Audit failed" }) };
  } catch (e) {
    return { label, width, height, error: e.message };
  } finally {
    if (win && win.id) {
      try { await chrome.windows.remove(win.id); } catch (_) {}
    }
  }
}

function computeScoresAndFixes(audit, ping, responsiveMatrix, flowSummary = null) {
  const fixes = [];
  let networkScore = 100;
  let speedScore = 100;
  let uiScore = 100;
  let securityScore = 100;

  const nav = audit.navigation || {};
  const vitals = audit.vitals || {};
  const ui = audit.ui || {};
  const resources = audit.resources || {};
  const headers = (ping && ping.headers) || {};

  // 1. Network & Latency
  if (nav.ttfb > 800) {
    networkScore -= 20;
    fixes.push({
      category: "Network",
      severity: "critical",
      title: `TTFB lambat (${nav.ttfb} ms)`,
      target: audit.url,
      suggestion: "Optimalkan response server/database, gunakan caching (CDN/Redis), atau kurangi blocking middleware."
    });
  } else if (nav.ttfb > 400) {
    networkScore -= 10;
    fixes.push({
      category: "Network",
      severity: "warning",
      title: `TTFB perlu dioptimalkan (${nav.ttfb} ms)`,
      target: audit.url,
      suggestion: "Targetkan TTFB < 300ms dengan edge caching atau keep-alive connection."
    });
  }

  if (ping && ping.jitter > 150) {
    networkScore -= 10;
    fixes.push({
      category: "Network",
      severity: "warning",
      title: `Jitter koneksi tinggi (${ping.jitter} ms)`,
      target: audit.url,
      suggestion: "Latency tidak stabil antar request; cek kestabilan origin server atau routing CDN."
    });
  }

  const slowResources = (resources.slowest || []).filter((r) => r.duration > 1000);
  if (slowResources.length > 0) {
    networkScore -= Math.min(25, slowResources.length * 6);
    fixes.push({
      category: "Network",
      severity: "warning",
      title: `${slowResources.length} resource/API lambat (> 1 detik)`,
      target: slowResources[0].url,
      suggestion: "Periksa endpoint/aset paling lambat di tab Network Waterfall, pertimbangkan lazy-loading atau pagination."
    });
  }

  // 2. Performance & Speed
  if (vitals.lcp && vitals.lcp > 4000) {
    speedScore -= 25;
    fixes.push({
      category: "Speed",
      severity: "critical",
      title: `LCP sangat lambat (${vitals.lcp} ms)`,
      target: "Largest Contentful Paint",
      suggestion: "Preload hero image/font utama dan kurangi render-blocking JS/CSS."
    });
  } else if (vitals.lcp && vitals.lcp > 2500) {
    speedScore -= 12;
    fixes.push({
      category: "Speed",
      severity: "warning",
      title: `LCP di atas batas ideal (${vitals.lcp} ms)`,
      target: "Largest Contentful Paint",
      suggestion: "Targetkan LCP <= 2500ms dengan optimasi ukuran gambar hero & critical CSS."
    });
  }

  if (vitals.cls > 0.25) {
    speedScore -= 20;
    fixes.push({
      category: "Speed",
      severity: "critical",
      title: `Layout shift tinggi (CLS ${vitals.cls})`,
      target: "Cumulative Layout Shift",
      suggestion: "Tetapkan width/height eksplisit pada gambar, video, dan container iklan/skeleton."
    });
  } else if (vitals.cls > 0.1) {
    speedScore -= 10;
    fixes.push({
      category: "Speed",
      severity: "warning",
      title: `CLS perlu diperbaiki (${vitals.cls})`,
      target: "Cumulative Layout Shift",
      suggestion: "Hindari menyisipkan elemen dinamis di atas konten yang sudah ter-render."
    });
  }

  const hasHeaders = Object.keys(headers).length > 0;
  const encoding = headers["content-encoding"] || "";
  if (hasHeaders && !encoding.includes("br") && !encoding.includes("gzip")) {
    speedScore -= 10;
    fixes.push({
      category: "Speed",
      severity: "warning",
      title: "Kompresi Brotli/Gzip tidak terdeteksi",
      target: "Content-Encoding header",
      suggestion: "Aktifkan Brotli (br) atau Gzip di web server / CDN untuk memperkecil payload HTML/JS/CSS."
    });
  }

  if ((resources.totalTransfer || 0) > 3 * 1024 * 1024) {
    speedScore -= 15;
    fixes.push({
      category: "Speed",
      severity: "warning",
      title: `Total transfer halaman besar (${(resources.totalTransfer / (1024 * 1024)).toFixed(2)} MB)`,
      target: `${resources.totalRequests} requests`,
      suggestion: "Kompres gambar ke WebP/AVIF dan lakukan code-splitting pada bundle JS."
    });
  }

  // 3. UI & Responsive
  if (!ui.viewportMeta) {
    uiScore -= 20;
    fixes.push({
      category: "UI",
      severity: "critical",
      title: "Meta viewport tidak ditemukan",
      target: "<head>",
      suggestion: 'Tambahkan <meta name="viewport" content="width=device-width, initial-scale=1.0"> agar layout responsif di mobile.'
    });
  }

  if (ui.hasOverflow) {
    uiScore -= 20;
    const firstEl = ui.overflowElements && ui.overflowElements[0] ? ui.overflowElements[0].selector : "document";
    fixes.push({
      category: "UI",
      severity: "critical",
      title: `Horizontal overflow (+${ui.overflowPx}px) di viewport ${ui.viewportWidth}px`,
      target: firstEl,
      suggestion: "Gunakan max-width: 100%, flex-wrap, atau periksa elemen dengan lebar fixed yang melebihi layar."
    });
  }

  if (responsiveMatrix && responsiveMatrix.some((r) => r.hasOverflow)) {
    const failedVp = responsiveMatrix.filter((r) => r.hasOverflow).map((r) => `${r.label} (${r.width}px)`).join(", ");
    uiScore -= 15;
    fixes.push({
      category: "UI",
      severity: "critical",
      title: `Layout pecah/overflow pada viewport: ${failedVp}`,
      target: "Responsive Breakpoints",
      suggestion: "Perbaiki media query / grid/flex container pada breakpoint tersebut agar tidak muncul horizontal scroll."
    });
  }

  if (ui.brokenImages && ui.brokenImages.length > 0) {
    uiScore -= Math.min(20, ui.brokenImages.length * 10);
    fixes.push({
      category: "UI",
      severity: "critical",
      title: `${ui.brokenImages.length} gambar gagal dimuat (broken image)`,
      target: ui.brokenImages[0].selector,
      suggestion: "Perbaiki URL src gambar atau pastikan aset tersedia di server/CDN."
    });
  }

  if (ui.oversizedImages && ui.oversizedImages.length > 0) {
    uiScore -= Math.min(15, ui.oversizedImages.length * 5);
    fixes.push({
      category: "UI",
      severity: "warning",
      title: `${ui.oversizedImages.length} gambar resolusi berlebih (oversized)`,
      target: `${ui.oversizedImages[0].selector} (${ui.oversizedImages[0].natural} → ${ui.oversizedImages[0].rendered})`,
      suggestion: "Gunakan atribut srcset atau resize gambar sesuai ukuran render agar hemat bandwidth."
    });
  }

  if (ui.smallTapTargets && ui.smallTapTargets.length > 3) {
    uiScore -= 10;
    fixes.push({
      category: "UI",
      severity: "info",
      title: `${ui.smallTapTargets.length} tombol/link terlalu kecil (< 44px)`,
      target: ui.smallTapTargets[0].selector,
      suggestion: "Perbesar padding/min-height tombol minimal 44x44px agar nyaman disentuh di layar mobile."
    });
  }

  // 4. Console & Security
  if (!audit.isHttps) {
    securityScore -= 25;
    fixes.push({
      category: "Security",
      severity: "critical",
      title: "Halaman belum menggunakan HTTPS",
      target: audit.url,
      suggestion: "Aktifkan TLS/SSL (HTTPS) untuk melindungi data dan mengaktifkan fitur web modern."
    });
  }

  if (ui.mixedContent && ui.mixedContent.length > 0) {
    securityScore -= 20;
    fixes.push({
      category: "Security",
      severity: "critical",
      title: `${ui.mixedContent.length} mixed-content HTTP di halaman HTTPS`,
      target: ui.mixedContent[0],
      suggestion: "Ubah seluruh URL resource eksternal menjadi https://."
    });
  }

  if (hasHeaders && audit.isHttps && !headers["strict-transport-security"]) {
    securityScore -= 10;
    fixes.push({
      category: "Security",
      severity: "info",
      title: "Header HSTS (Strict-Transport-Security) belum aktif",
      target: "Response Headers",
      suggestion: "Tambahkan header Strict-Transport-Security untuk memaksa koneksi HTTPS."
    });
  }

  if (hasHeaders && !headers["content-security-policy"]) {
    securityScore -= 10;
    fixes.push({
      category: "Security",
      severity: "info",
      title: "Header Content-Security-Policy (CSP) belum diset",
      target: "Response Headers",
      suggestion: "Tambahkan CSP untuk mencegah serangan XSS dan injeksi script."
    });
  }

  if (flowSummary) {
    const failedReqs = (flowSummary.requests || []).filter((r) => r.statusCode === 0 || r.statusCode >= 400);
    if (failedReqs.length > 0) {
      networkScore -= Math.min(35, failedReqs.length * 10);
      fixes.push({
        category: "Network",
        severity: "critical",
        title: `${failedReqs.length} request gagal/error (${failedReqs[0].statusCode || "NET ERR"}) selama alur user`,
        target: `${failedReqs[0].method} ${failedReqs[0].url}`,
        suggestion: "Periksa payload request, auth token, atau handler API di server untuk endpoint yang gagal saat alur berlangsung."
      });
    }

    const slowFlowReqs = (flowSummary.requests || []).filter((r) => r.duration > 800);
    if (slowFlowReqs.length > 0) {
      networkScore -= Math.min(20, slowFlowReqs.length * 5);
      fixes.push({
        category: "Network",
        severity: "warning",
        title: `${slowFlowReqs.length} API/request lambat (> 800ms) selama alur user`,
        target: `${slowFlowReqs[0].method} ${slowFlowReqs[0].url} (${slowFlowReqs[0].duration}ms)`,
        suggestion: "Optimalkan query atau proses backend pada endpoint tersebut agar alur interaksi terasa responsif."
      });
    }

    const jsErrs = flowSummary.errors || [];
    if (jsErrs.length > 0) {
      securityScore -= Math.min(35, jsErrs.length * 12);
      fixes.push({
        category: "Console",
        severity: "critical",
        title: `${jsErrs.length} JS runtime error terjadi selama alur user`,
        target: jsErrs[0].source || "console.error",
        suggestion: `Error: "${jsErrs[0].message}". Tangani exception atau validasi input pada komponen terkait.`
      });
    }
  }

  networkScore = Math.max(0, Math.min(100, networkScore));
  speedScore = Math.max(0, Math.min(100, speedScore));
  uiScore = Math.max(0, Math.min(100, uiScore));
  securityScore = Math.max(0, Math.min(100, securityScore));

  const overall = Math.round(
    networkScore * 0.3 +
    speedScore * 0.25 +
    uiScore * 0.25 +
    securityScore * 0.2
  );

  return {
    scores: {
      overall,
      network: networkScore,
      speed: speedScore,
      ui: uiScore,
      security: securityScore
    },
    fixes
  };
}

async function runFullAudit(tabId, includeResponsiveScan) {
  const tab = await chrome.tabs.get(tabId);
  if (!tab.url || tab.url.startsWith("chrome://") || tab.url.startsWith("edge://")) {
    throw new Error("Cannot audit internal browser pages.");
  }

  await chrome.scripting.executeScript({ target: { tabId }, files: ["debug_collector.js"] });
  const [{ result: audit }] = await chrome.scripting.executeScript({
    target: { tabId },
    func: () => window.__raydropDebug.runAudit()
  });

  const ping = await measureEndpointPing(tab.url);

  let responsiveMatrix = [
    {
      label: "Current",
      width: audit.ui.viewportWidth,
      height: audit.ui.viewportHeight,
      scrollWidth: audit.ui.scrollWidth,
      hasOverflow: audit.ui.hasOverflow,
      overflowPx: audit.ui.overflowPx,
      offenderCount: audit.ui.overflowElements.length
    }
  ];

  if (includeResponsiveScan) {
    const mobile = await scanResponsiveViewport(tab.url, 375, 812, "Mobile");
    const tablet = await scanResponsiveViewport(tab.url, 768, 1024, "Tablet");
    responsiveMatrix = [mobile, tablet, ...responsiveMatrix];
  }

  const { scores, fixes } = computeScoresAndFixes(audit, ping, responsiveMatrix);

  const report = {
    id: `audit_${Date.now()}`,
    ...audit,
    ping,
    responsiveMatrix,
    scores,
    fixes
  };

  chrome.storage.local.set({ lastAudit: report });

  let sentToDesktop = false;
  try {
    const res = await fetch(RAYDROP_DEBUG_AUDIT_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(report)
    });
    sentToDesktop = res.ok;
  } catch (_) {}

  return { report, sentToDesktop };
}

// Heartbeat ping every 15 seconds
setInterval(updateConnectionStatusBadge, 15000);
updateConnectionStatusBadge();
