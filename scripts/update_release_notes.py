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
    tag = clean_args[0] if clean_args else "v1.2.98"
    ver = tag.lstrip("v")
    title = f"Raydrop {tag}: Snipping Image Editor, RayAmp Now-Playing, Remote RayBot & Mobile Fixes"
    
    body = f"""<p align="center">
  <img src="https://raw.githubusercontent.com/ShadowOrgs/raydrop/main/assets/logo.png" alt="Raydrop Logo" width="220" />
</p>

<h2 align="center">Raydrop {tag}: Snipping Image Editor, RayAmp Now-Playing, Remote RayBot & Mobile Fixes</h2>

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

#### ✂️ Snipping-Tool Image Editor, RayAmp Now-Playing & Remote RayBot
- **Snipping-Tool Image Editor (Copy History):** Edit captured images with Pen, Highlighter, Eraser, Arrow, Rectangle, Circle, Blur/Redact, Text, Crop, Undo/Redo, Save PNG, and Save & Copy.
- **RayAmp Now-Playing Broadcast & Mobile Media:** Discord-style `Listening to ...` activity in Contacts & Chat, responsive full-screen mobile RayAmp, and Android local audio/video file picker support.
- **Remote RayBot Device Switcher & Mobile/WebRTC Fixes:** Switch target device in RayBot Chat UI across your own online devices, fixed `createOffer ... signalingState is 'closed'` on Remote Desktop reconnect, fixed mobile Profile button & APK v1/v2/v3 signing, added Share Contact, global toast notifications, complete bottom menu, and fixed LLM Bridge button overflow.

#### ⚡ Startup Freeze & WebView2 CPU Hotfix
- **Zero-Idle-CPU RayAmp Visualizer:** Stopped the 60fps `requestAnimationFrame` canvas gradient loop when RayAmp is paused/idle or hidden, eliminating background WebView2 GPU/Renderer CPU spikes on Windows.
- **Non-Blocking Startup & Throttled Polling:** Moved `llm_bridge::init_from_db` off the Tokio async worker into `spawn_blocking`, disabled UI torrent/HTTP broadcast loops inside `--system-agent`, and relaxed frontend status/LAN polling to `10s`.

#### 💻 RayBot Multi-Project Coding Workspace & Cross-Project Relations
- **Persistent Project Registry & Relations:** Configurable projects root folder (default `~`), auto-scan tech stacks, persistent SQLite memory & cross-project relations (`raybot_projects`, `raybot_project_relations`), and a quick Project Workspace bar in RayBot Chat UI.
- **Scoped Coding & Workspace MCP Tools:** 13 new `project_*` and `code_*` tools (`project_set_root`, `project_scan`, `project_select`, `project_add_relation`, `project_save_memory`, `code_list_files`, `code_search`, `code_read_file`, `code_write_file`, `code_edit_file`, `code_run_command`) usable from both RayBot Chat UI and Telegram Bot.

#### 🧠 Direct RayBot Skills, MCP & Persistent Language Tutor
- **Provider-Agnostic Skills & MCP:** Enable Skills & MCP servers directly on RayBot regardless of LLM provider (Ollama, LM Studio, KoboldCPP, OpenAI, Gemini, Claude, LLM Bridge).
- **Language Tutor Skill (`language-tutor`):** Pre-existing installable skill supporting English, Japanese, Korean, Russian, German, Indonesian, and more with dynamic CEFR syllabus & persistent per-language SQLite progress.

#### ⚙️ LLM Bridge CLI Execution Modes, Image Bridge, Local Vision & ComfyUI
- **CLI Execution Modes & Extra Flags:** Per-provider `YOLO`, `Auto-Edit`, and `Safe` (read-only prompt guard) modes across `agy`, `claude`, `codex`, `gemini`, and `opencode`/custom CLIs, plus custom extra flags.
- **Image Bridge, Local Vision & ComfyUI + Waifu Persona:** Host-side image storage & local path forwarding to CLIs, automatic Local LLM vision routing, ComfyUI image generation in RayBot Chat UI & Telegram Bot, and a new **Waifu Companion** persona style.
"""
    update_release(tag=tag, title=title, body=body, wait_for_assets=wait_flag)
