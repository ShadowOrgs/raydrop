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

import time

def update_release(tag="v1.2.15", title=None, body=None, wait_for_assets=False):
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

    max_attempts = 40 if wait_for_assets else 1
    for attempt in range(max_attempts):
        req = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(req) as resp:
                release_data = json.loads(resp.read().decode("utf-8"))
            
            # If wait_for_assets is requested, check if binaries have arrived
            assets = release_data.get("assets", [])
            if wait_for_assets and len(assets) == 0 and attempt < max_attempts - 1:
                print(f"Release {tag} found, but assets are still compiling by GitHub Actions. Waiting 15s ({attempt+1}/{max_attempts})...")
                time.sleep(15)
                continue

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

            with urllib.request.urlopen(patch_req) as patch_resp:
                result = json.loads(patch_resp.read().decode("utf-8"))
                print(f"Successfully updated release {tag} with {len(result.get('assets', []))} assets!")
                print(f"URL: {result.get('html_url')}")
            return
        except urllib.error.HTTPError as e:
            if e.code == 404:
                if wait_for_assets and attempt < max_attempts - 1:
                    print(f"Release {tag} not created yet (GitHub Action in progress). Waiting 15s ({attempt+1}/{max_attempts})...")
                    time.sleep(15)
                    continue
                # Fallback: create release
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
                with urllib.request.urlopen(post_req) as post_resp:
                    result = json.loads(post_resp.read().decode("utf-8"))
                    print(f"Successfully created release {tag}!")
                    print(f"URL: {result.get('html_url')}")
                return
            else:
                raise

if __name__ == "__main__":
    wait_flag = "--wait" in sys.argv
    clean_args = [a for a in sys.argv[1:] if not a.startswith("--")]
    tag = clean_args[0] if clean_args else "v1.2.15"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: Profile Avatars, Contact Initials, Device Inspector, Transfer Image Zoom & Chat OS Toasts"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Profile Avatars, Contact Initials, Device Inspector, Transfer Image Zoom & Chat OS Toasts</h2>

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

#### 🔔 Windows Native Chat Notification Toasts
- **Non-Intrusive Bottom-Right Action Center Notifications:** Incoming P2P and cloud chat messages now trigger native Windows notifications without forcing the main Raydrop window to popup or steal keyboard focus.

#### 🏷️ Device Custom Initials & Nicknames
- **Personalized Device Identification:** Easily set local initials and custom nicknames for peers and devices (saved persistently in local SQLite contacts) to instantly tell apart devices named `Raydrop-Device`.
- **Integrated Display:** Custom initials and nicknames are displayed throughout Chat headers, Chat sidebar, Transfer file pickers, incoming request prompts, and Contact list tabs.

#### 🛡️ Chrome Extension Cache-Clear Protection
- **Zero Replay of Historical Downloads:** Hardened `chrome.downloads.onCreated` listener to strictly verify that download items are active and initiated within the last 15 seconds, preventing Chrome from re-queuing old download history when browser cache is cleared.

#### 👥 Automatic Two-Way Chat Contact Creation
- **Auto-Add Contacts on Replied Chat:** Once two users exchange messages (send and reply), both peers are automatically added to each other's contacts with real-time UI synchronization.

#### 🖼️ User Profile Avatars & Peer Syncing
- **Upload & Cache Avatars:** Users can upload custom avatars in Header or Tray Settings. Avatars are automatically synchronized over local mDNS discovery and P2P chat messages, and saved persistently to local cache.

#### 🔍 Transfer History Image Thumbnails & Zoom Lightbox Modal
- **Direct Image Thumbnails:** Completed transfers containing images now display thumbnail previews instead of generic extension badges.
- **Interactive Lightbox:** Clicking any image opens a full lightbox modal with Zoom In (+), Zoom Out (-), 100% Reset, mouse wheel zooming, drag-and-pan navigation, and Open File / Open Folder shortcuts.

#### 💻 Contact List Device Inspector
- **Comprehensive Device Breakdown:** Contact cards now feature a dedicated "Linked Devices" section detailing every device registered to the user, complete with OS type, IP address, online status, custom initials, and direct Chat & File transfer actions.
"""
    update_release(tag=tag, title=title, body=body, wait_for_assets=wait_flag)
