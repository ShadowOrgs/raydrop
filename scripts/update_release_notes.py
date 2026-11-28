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

    repo = "ShadowOrgs/raydrop"
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
    tag = clean_args[0] if clean_args else "v1.2.99"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: DevOps Suite (SSH, DB & Uptime), Discord-Style Chat Calls & Remote Fixes"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/ShadowOrgs/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: DevOps Suite (SSH, DB & Uptime), Discord-Style Chat Calls & Remote Fixes</h2>

<p align="center">
  <strong>Fast, Resumable, End-to-End Encrypted File Sharing Across Devices with Zero Server Storage, BitTorrent Swarm Engine & High-Speed Multi-Threaded Download Accelerator.</strong><br/>
  Available natively on <b>Windows</b>, <b>macOS</b>, <b>Linux</b>, and <b>Android</b>.
</p>

---

### 📦 Available Downloads

| Operating System | Package | Architecture | Download Link |
| :--- | :---: | :---: | :--- |
| **Windows** | `.msi` | x64 (64-bit) | [**Raydrop_{ver}_x64_en-US.msi**](https://github.com/ShadowOrgs/raydrop/releases/download/{tag}/Raydrop_{ver}_x64_en-US.msi) |
| **macOS** | `.dmg` | Universal (Apple Silicon & Intel) | [**Raydrop_{ver}_universal.dmg**](https://github.com/ShadowOrgs/raydrop/releases/download/{tag}/Raydrop_{ver}_universal.dmg) |
| **Linux (Debian / Ubuntu)** | `.deb` | x64 (amd64) | [**raydrop_{ver}_amd64.deb**](https://github.com/ShadowOrgs/raydrop/releases/download/{tag}/raydrop_{ver}_amd64.deb) |
| **Linux (Universal)** | `.AppImage` | x64 (amd64) | [**Raydrop_{ver}_amd64.AppImage**](https://github.com/ShadowOrgs/raydrop/releases/download/{tag}/Raydrop_{ver}_amd64.AppImage) |
| **Android** | `.apk` | ARM64 (`arm64-v8a`) | [**Raydrop-arm64-v8a.apk**](https://github.com/ShadowOrgs/raydrop/releases/download/{tag}/Raydrop-arm64-v8a.apk) |

---

### 🚀 What's New & Highlights in {tag}

#### 🛠️ DevOps Suite (Remote SSH • DB Manager • Uptime Monitor)
- **Persistent Remote SSH & Server Health:** Multi-group SSH terminal (Termius-style xterm.js with copy/paste & keepalive) plus one-click remote server CPU, RAM, Disk, and Load monitoring.
- **Database Manager, Scheduled Backups & Safe Restore:** Multi-config MySQL/PostgreSQL manager with on-demand seamless client & DB Admin (Adminer / phpMyAdmin on conflict-free ports) download/install, DB health checks, scheduled cron `.sql.gz` backups, and safe restore (ensuring target DB is clean before restoring).
- **Uptime Monitor & Alerts:** Grouped HTTP/endpoint uptime checks with configurable intervals, outage/recovery history, shareable status URL, and real-time notifications to Raydrop & Telegram Bot.

#### 📞 Discord-Style Voice & Video Calls in Chat & Group Chat
- **1-on-1 & Group Calls:** Separate **Voice Call** and **Video Call** buttons in both DM and Group Chat headers, ringing all member devices in the group via the built-in Meeting engine.
- **Looping Ringtone & Auto-Focus Popup:** Discord-style incoming call popup (`z-[90]`) with looping Web Audio ringtone, outgoing ringback tone, and automatic window focus/attention.

#### 🖥️ UI & Remote Desktop Fixes
- **FloatingRail "More" Popup:** Kept primary navigation items in the left rail and grouped secondary tools (Copy History, Notes, Converter, Web Debugger, AI Skills, MCP Library, LLM Bridge, DevOps) into a sleek **Menu Lainnya** popover.
- **Game Mode Cursor & Desktop Switch Fixes:** Clamped Chromium pointer-lock delta spikes and discarded initial warp outliers in Game Mode; cleanly exited pointer lock when switching back to Desktop Mode; added a manual Close (`X`) button to the `Ctrl+Alt+Shift+Q` banner.
- **Global Host Remote Approval Popup:** Incoming PIN-less Remote Desktop requests now automatically bring Raydrop to the foreground and show the Allow/Decline modal on any active tab.

#### ✂️ Previous Highlights (Snipping Image Editor, RayAmp, RayBot & Mobile)
- **Snipping-Tool Image Editor (Copy History), RayAmp Now-Playing Broadcast, Remote RayBot Device Switcher, RayBot Multi-Project Coding Workspace, Direct Skills/MCP, and LLM Bridge CLI Execution Modes.**
"""
    update_release(tag=tag, title=title, body=body, wait_for_assets=wait_flag)
