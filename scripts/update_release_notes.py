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

def update_release(tag="v1.2.12", title=None, body=None, wait_for_assets=False):
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
    tag = clean_args[0] if clean_args else "v1.2.12"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: IDM-Style Refresh Download Address, Instant Resume & Large File Engine"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/asepindrak/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: IDM-Style Refresh Download Address, Instant Resume & Large File Engine</h2>

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

#### 🔄 IDM-Style Refresh Download Address (Expired Session Resume)
- **Resume Expired Session Links:** Solved temporary token and expiring link issues (403 Forbidden / 410 Gone). When a download session expires or errors, paste the new download URL to seamlessly resume from the exact byte offset without re-downloading or truncating existing files.
- **Auto-Refresh via Browser Extension:** Re-triggering a download for the same file in Google Chrome or Microsoft Edge automatically updates the active/paused task in Raydrop with the new session URL and resumes downloading instantly.
- **Dedicated UI Refresh Address Modal:** Easily update links with a single click from the download card or error banner.

#### ⚡ Instant NTFS Sparse File Preallocation (0ms Delay on Multi-Gigabyte Files)
- **Eliminated Synchronous Disk Freezes:** Added native Windows `FSCTL_SET_SPARSE` support, eliminating the 10+ second zero-filling disk allocation lag when starting 6GB+ files (such as Ubuntu ISOs).
- **IDM-Standard Range Probe (`bytes=0-0`):** Converted server capability probe to standard Range request, accurately detecting 206 Partial Content support and allocating up to 8 parallel streams.
- **Streaming Timeout Elimination:** Removed reqwest 30-second client timeout on large file streams, allowing uninterrupted multi-gigabyte downloads.

#### 🧩 Chrome & Edge Browser Extension (Manifest V3)
- **Automatic Download Interception:** Captures web downloads (ISO, EXE, ZIP, APK, MP4, etc.) directly into Raydrop's multi-segment engine.
- **In-App 1-Click Installer:** Built-in modal with automatic folder reveal and loopback bridge monitor (`http://127.0.0.1:45321`).
"""
    update_release(tag=tag, title=title, body=body, wait_for_assets=wait_flag)
