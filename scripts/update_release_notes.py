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

def update_release(tag="v1.2.7", title=None, body=None):
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
    req = urllib.request.Request(patch_url, data=data, headers=headers, method="PATCH")
    req.add_header("Content-Type", "application/json; charset=utf-8")

    with urllib.request.urlopen(req) as resp:
        result = json.loads(resp.read().decode("utf-8"))
        print(f"Successfully updated release {tag}!")
        print(f"URL: {result.get('html_url')}")

if __name__ == "__main__":
    tag = sys.argv[1] if len(sys.argv) > 1 else "v1.2.7"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: Universal Web Share Over Internet, QR Copy & In-Chat Media"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Over-Internet Web Share, QR Copy & In-Chat Media</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage.</strong><br/>
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

#### 🌐 Universal Web Share Over Internet (Public Relay Tunnel)
- **Zero-App Browser Downloads Anywhere in the World:** Anyone can download your shared files directly in Safari, Chrome, or Firefox without installing Raydrop or joining the same Wi-Fi. Simply share the QR code or link (`https://stun.raydrop.web.id/share/...`), and files stream on-demand directly from your device over an encrypted relay tunnel.
- **1-Click Copy QR Code Image:** Copy the dynamic QR code PNG bitmap directly to your operating system clipboard with a single button, ready for instant paste (`Ctrl+V`) into WhatsApp, Discord, Telegram, or email.
- **Instant Social Media Share Buttons:** Share links directly to **WhatsApp**, **Telegram**, **Twitter / X**, and **Email** with one click.
- **Dual-Mode Toggle:** Effortlessly switch between **Over Internet (Public Relay)** and **Local Wi-Fi Only** modes.

#### 🖼️ In-Chat Clipboard Image Formatting & Lightbox Viewer
- **Native Image Card Rendering:** Received clipboard screenshots and images now automatically render as clean, interactive visual cards instead of massive walls of raw base64 text.
- **Instant Bitmap Copy to OS Clipboard:** A dedicated "Copy Image" button decodes the image on-the-fly and writes the physical image bitmap directly to your system clipboard—ready for `Ctrl+V` into Photoshop, Word, or messaging apps.
- **Full-Resolution Lightbox Modal:** Click any image in chat to open a high-res lightbox modal with zoom, quick copy, and direct file download options.
- **Clean In-App Toasts:** Notification banners cleanly show `🖼️ [Gambar Clipboard]` instead of raw base64 text.

#### ⚡ Responsive UI Layout & Overflow Fix
- **Streamlined Top Transfer Hub:** Eliminated the redundant Quick Send bar from the main dashboard, removing horizontal layout overflow and guaranteeing a clean, responsive interface on laptops, tablets, and phones.
- **Elastic Drop Zone:** File drag-and-drop zone dynamically adapts to window resizing without clipping text or border boundaries.

#### 📋 Universal Clipboard Sync & Mobile Enhancements (Inherited from v1.2.6)
- **Instant Cross-Device Auto-Paste:** When sending clipboard text to trusted peers, content is written straight into the recipient's system clipboard without manual copying.
- **Smart Magic-Bytes Header Inspection:** Automatic binary inspection (JPEG, PNG, GIF, WebP, PDF, MP4, MP3, WAV, ZIP/DOCX/XLSX, MKV) ensures files chosen from Android media pickers receive accurate extensions and clean filenames (`Image_...`, `Video_...`, etc.).
- **Multi-File Queueing on Mobile:** Interactive "+ Add More" file queue picker in the transfer modal.
- **Android Storage & Folder Handling:** Integrated handlers for "Open File" and "Open Folder" directing users to their native Download storage.
"""
    update_release(tag=tag, title=title, body=body)
