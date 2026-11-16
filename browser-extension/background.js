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

// Check Raydrop App Connection & Update Badge
async function updateConnectionStatusBadge() {
  try {
    const res = await fetch(RAYDROP_PING_URL, { method: "GET" });
    if (res.ok) {
      chrome.action.setBadgeText({ text: "ON" });
      chrome.action.setBadgeBackgroundColor({ color: "#06b6d4" }); // Cyan
      return;
    }
  } catch (_) {}
  chrome.action.setBadgeText({ text: "" });
}

// Heartbeat ping every 15 seconds
setInterval(updateConnectionStatusBadge, 15000);
updateConnectionStatusBadge();
