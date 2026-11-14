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

def update_release(tag="v1.2.8", title=None, body=None):
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
    tag = sys.argv[1] if len(sys.argv) > 1 else "v1.2.8"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: Interactive Clipboard Image Actions, Native File Save & Feedback Animation"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Interactive Clipboard Image Actions & Native File Save</h2>

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

#### 🖼️ Interactive Clipboard Image Actions & Visual Feedback
- **Animated Confirmation for Copy:** Clicking "Copy Image" on chat bubbles or within the full-screen lightbox modal now triggers an instant green checkmark confirmation (`Copied!`) and writes the true bitmap image into the OS clipboard.
- **Loader Spinner & Checkmark for Downloads:** The "Download" button now features an interactive real-time loading spinner (`Loader2`) while writing the image, followed by an animated success checkmark.
- **Direct OS Downloads Directory Persistence:** Image files are decoded natively in Rust via the `save_base64_image` Tauri IPC command and saved directly into the user's Downloads directory, bypassing WebView2 data URL limitations.
- **Resilient Multi-Layer Clipboard Fallback:** Built-in retries handle temporary OS clipboard locks on Windows, with automatic fallbacks to HTML5 Clipboard items and text representations to guarantee responsive UI feedback.

#### 🌐 Universal Web Share Over Internet (Public Relay Tunnel)
- **Zero-App Browser Downloads Anywhere in the World:** Anyone can download your shared files directly in Safari, Chrome, or Firefox without installing Raydrop or joining the same Wi-Fi. Simply share the QR code or link (`https://stun.raydrop.web.id/share/...`), and files stream on-demand directly from your device over an encrypted relay tunnel.
- **1-Click Copy QR Code Image:** Copy the dynamic QR code PNG bitmap directly to your operating system clipboard with a single button, ready for instant paste (`Ctrl+V`) into WhatsApp, Discord, Telegram, or email.
- **Instant Social Media Share Buttons:** Share links directly to **WhatsApp**, **Telegram**, **Twitter / X**, and **Email** with one click.
- **Dual-Mode Toggle:** Effortlessly switch between **Over Internet (Public Relay)** and **Local Wi-Fi Only** modes.

#### ⚡ Responsive UI Layout & Overflow Fix
- **Streamlined Top Transfer Hub:** Eliminated the redundant Quick Send bar from the main dashboard, removing horizontal layout overflow and guaranteeing a clean, responsive interface on laptops, tablets, and phones.
- **Elastic Drop Zone:** File drag-and-drop zone dynamically adapts to window resizing without clipping text or border boundaries.
"""
    update_release(tag=tag, title=title, body=body)
