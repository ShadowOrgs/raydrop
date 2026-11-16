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

async function checkStatus() {
  try {
    const res = await fetch(PING_URL, { method: "GET" });
    if (res.ok) {
      statusBadge.className = "status-badge";
      statusText.textContent = "Connected";
      return true;
    }
  } catch (_) {}

  statusBadge.className = "status-badge disconnected";
  statusText.textContent = "Offline (Open App)";
  return false;
}

// Load persisted configuration
chrome.storage.local.get(["enabled", "autoCapture"], (res) => {
  if (res.enabled !== undefined) masterToggle.checked = res.enabled;
  if (res.autoCapture !== undefined) autoCaptureToggle.checked = res.autoCapture;
});

// Auto-Capture Toggle
autoCaptureToggle.addEventListener("change", (e) => {
  chrome.storage.local.set({ autoCapture: e.target.checked });
});

// Master Toggle
masterToggle.addEventListener("change", (e) => {
  chrome.storage.local.set({ enabled: e.target.checked });
});

// Download button action
startBtn.addEventListener("click", async () => {
  const url = urlInput.value.trim();
  if (!url) {
    showFeedback("Please enter a valid URL or magnet link", "#fb7185");
    return;
  }

  startBtn.textContent = "...";
  try {
    const res = await fetch(DOWNLOAD_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: url,
        connections: 8
      })
    });

    if (res.ok) {
      showFeedback("Download sent to Raydrop Accelerator!", "#34d399");
      urlInput.value = "";
    } else {
      showFeedback("Failed to add download. Is Raydrop active?", "#fb7185");
    }
  } catch (err) {
    showFeedback("Cannot connect to Raydrop app. Make sure it's running.", "#fb7185");
  } finally {
    startBtn.textContent = "Catch";
  }
});

function showFeedback(text, color) {
  msgFeedback.style.display = "block";
  msgFeedback.style.color = color;
  msgFeedback.textContent = text;
  setTimeout(() => {
    msgFeedback.style.display = "none";
  }, 3500);
}

// Check initial status
checkStatus();
