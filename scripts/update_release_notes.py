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

def update_release(tag="v1.2.11", title=None, body=None):
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
    tag = sys.argv[1] if len(sys.argv) > 1 else "v1.2.11"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: Android Build Fix, Transfer UI Overhaul & Browser Extension Integration"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Android Build Fix, Transfer UI Overhaul & Browser Extension Integration</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage, BitTorrent Swarm Engine & High-Speed Multi-Threaded Download Accelerator.</strong><br/>
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

#### 🤖 Android CI Cross-Compilation Fix (`openssl-sys` Isolation)
- **Resolved Android NDK Compilation Failure:** Isolated `librqbit` BitTorrent engine into desktop-only platform configurations (`cfg(not(any(target_os = "android", target_os = "ios")))`).
- **Eliminated OpenSSL Native C Requirement:** Replaced native C OpenSSL dependency chain with safe pure Rust TLS and mobile stubs on Android/iOS, ensuring 100% clean CI/CD APK generation across all Android targets (`aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`).

#### 🎨 Transfer Dashboard UI Overhaul & Full-Width Drop Zone
- **Dedicated Full-Width Drop Zone:** Relocated the file and folder drag-and-drop zone to a dedicated 100% width horizontal strip beneath the action bar. Solved container horizontal overflow and eliminated button cramping.
- **Compact & Organized Action Toolbar:** Downscaled and streamlined action buttons into sleek, logically grouped pill controls (`Send Files`, `Folder`, `Web Share`, `Clipboard`, `Folder Sync` + `+ Add URL / Torrent`, `Extension`).

#### 🌐 100% English Language Standardisation (i18n)
- **Universal English Localization:** Fully translated all remaining in-app tutorial guides, the 30-second browser extension installation wizard, contact blocking banners, chat timestamps, and file storage notifications to English.

#### 🧩 Official Chrome & Edge Browser Extension (Manifest V3)
- **Automatic Download Interception (IDM Mode):** Browser extension detects and captures downloads from Chrome, Edge, Brave, and Opera into Raydrop's 8-connection parallel accelerator.
- **In-App 1-Click Installer:** Built-in modal with 1-click extension folder reveal and local desktop loopback bridge health monitor (`http://127.0.0.1:45321`).
- **Browser Context Menus:** Right-click any link or media file to download directly via Raydrop Accelerator.

#### ⚡ IDM-Style HTTP Multi-Segment Accelerator & BitTorrent Magnet Engine
- **Multi-Connection Segmented Acceleration:** Splitting downloads into 4–8 concurrent chunks using HTTP `Range: bytes=X-Y` requests with native OS zero-stitching file writes.
- **Embedded BitTorrent Swarm Engine:** Full BitTorrent peer-to-peer downloading powered by `librqbit v9.1.0` with OS `magnet:` protocol association and live swarm metrics.
"""
    update_release(tag=tag, title=title, body=body)
