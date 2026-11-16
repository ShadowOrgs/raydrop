#!/usr/bin/env python3
import json
import subprocess
import urllib.request
import sys

def get_git_token():
    cred_proc = subprocess.Popen(
        ["git", "credential", "fill"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    stdout, _ = cred_proc.communicate("protocol=https\nhost=github.com\n\n")
    for line in stdout.splitlines():
        if line.startswith("password="):
            return line[len("password="):].strip()
    return None

def update_release(tag="v1.2.9", title=None, body=None):
    token = get_git_token()
    if not token:
        print("ERROR: GitHub token not found in git credentials.", file=sys.stderr)
        sys.exit(1)

    repo = "asepindrak/raydrop"
    url = f"https://api.github.com/repos/{repo}/releases/tags/{tag}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "User-Agent": "Raydrop-Release-Updater",
    }

    req = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(req) as resp:
            release_data = json.loads(resp.read().decode("utf-8"))
        release_id = release_data["id"]
        patch_url = f"https://api.github.com/repos/{repo}/releases/{release_id}"

        payload = {}
        if title:
            payload["name"] = title
        if body:
            payload["body"] = body

        data = json.dumps(payload).encode("utf-8")
        patch_req = urllib.request.Request(patch_url, data=data, headers=headers, method="PATCH")
        patch_req.add_header("Content-Type", "application/json; charset=utf-8")

        with urllib.request.urlopen(patch_req) as resp:
            result = json.loads(resp.read().decode("utf-8"))
            print(f"Successfully updated release {tag}!")
            print(f"URL: {result.get('html_url')}")
    except urllib.error.HTTPError as e:
        if e.code == 404:
            # Create new release
            create_url = f"https://api.github.com/repos/{repo}/releases"
            payload = {
                "tag_name": tag,
                "name": title or f"Raydrop {tag}",
                "body": body or "",
                "draft": False,
                "prerelease": False,
            }
            data = json.dumps(payload).encode("utf-8")
            post_req = urllib.request.Request(create_url, data=data, headers=headers, method="POST")
            post_req.add_header("Content-Type", "application/json; charset=utf-8")
            with urllib.request.urlopen(post_req) as resp:
                result = json.loads(resp.read().decode("utf-8"))
                print(f"Successfully created release {tag}!")
                print(f"URL: {result.get('html_url')}")
        else:
            raise

if __name__ == "__main__":
    tag = sys.argv[1] if len(sys.argv) > 1 else "v1.2.9"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: IDM-Style HTTP Multi-Segment Accelerator & BitTorrent Magnet Engine"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: IDM-Style HTTP Multi-Segment Accelerator & BitTorrent Magnet Engine</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage & High-Speed Multi-Threaded Download Manager.</strong><br/>
  Available natively on <b>Windows</b>, <b>macOS</b>, <b>Linux</b>, and <b>Android</b>.
</p>

---

### 📦 Available Downloads

| Operating System | Package | Architecture | Download Link |
| :--- | :---: | :---: | :--- |
| **Windows** | `.msi` | x64 (64-bit) | [**Raydrop_{ver}_x64_en-US.msi**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_{ver}_x64_en-US.msi) |
| **macOS** | `.dmg` | Universal (Apple Silicon & Intel) | [**Raydrop_{ver}_universal.dmg**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_{ver}_universal.dmg) |
| **Linux (Debian / Ubuntu)** | `.deb` | x64 (amd64) | [**Raydrop_{ver}_amd64.deb**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_{ver}_amd64.deb) |
| **Linux (Universal)** | `.AppImage` | x64 (amd64) | [**Raydrop_{ver}_amd64.AppImage**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop_{ver}_amd64.AppImage) |
| **Android** | `.apk` | ARM64 (`arm64-v8a`) | [**Raydrop-arm64-v8a.apk**](https://github.com/asepindrak/raydrop/releases/download/{tag}/Raydrop-arm64-v8a.apk) |

---

### 🚀 What's New & Highlights in {tag}

#### ⚡ IDM-Style HTTP Multi-Segment Parallel Download Accelerator
- **Multi-Connection Segmented Acceleration:** Accelerate direct web downloads by splitting files into 4–8 concurrent chunks using HTTP `Range: bytes=X-Y` requests, maximizing throughput on high-bandwidth fiber connections.
- **Zero-Stitch Direct File Write:** Chunks are written directly into target file byte offsets via native OS file handles (`seek_write` on Windows, `write_all_at` on Unix) without needing slow temporary file joins or disk re-copying.
- **Smart Fallback Single-Stream:** Automatically detects whether servers support partial content (`Accept-Ranges: bytes`) and falls back to robust single-stream streaming without download interruption.
- **Full Lifecycle Controls:** Pause, resume, cancel, and open downloaded folders with live real-time speed monitoring and progress bars.

#### 🧲 BitTorrent Swarm Engine & Native Magnet Link Deep Linking
- **Native librqbit Integration:** Full BitTorrent peer-to-peer swarm downloading embedded directly in Raydrop with low memory footprint and high I/O throughput.
- **OS Protocol Association (`magnet:`):** Windows Registry protocol handler registers Raydrop as the default magnet link handler, automatically opening links clicked in Chrome, Firefox, Edge, or Brave.
- **Single-Instance Deep Link Relay:** Clicking magnet links while Raydrop is already running seamlessly opens the download prompt without opening duplicate application windows.
- **Live Swarm Metrics:** Live peer counter, download speed indicator, seed status, and pause/resume capabilities.

#### 🎯 Unified URL / Magnet Downloader Modal
- **Smart Input Detection:** Simply paste any link into the `+ Add URL / Magnet` modal—Raydrop automatically detects whether it's an HTTP/HTTPS URL or a `magnet:?xt=` link, configures connections, and initiates the optimal download pipeline.

#### 🖼️ Universal Clipboard Image Preview & Fix
- **Robust Base64 Image Detection:** Resolved issue where raw base64 data strings were shown in Universal Clipboard bubbles. Images and screenshots are now consistently detected and rendered as interactive image cards.
- **Instant Copy & Download:** One-click bitmap copying into system clipboard (`Ctrl+V` ready) and native file download to Downloads folder with visual animated checkmarks.
"""
    update_release(tag=tag, title=title, body=body)
