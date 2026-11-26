# Raydrop MCP Server

Model Context Protocol (MCP) server that lets AI assistants (Antigravity, Claude Desktop, Cursor, Windsurf, etc.) control all features of a running **Raydrop** desktop instance.

## Prerequisites
1. **Raydrop Desktop** must be running (listens on `http://127.0.0.1:45321`).
2. **Node.js 18+** (only required for `stdio` transport; HTTP MCP clients can connect directly to `http://127.0.0.1:45321/mcp`).

## Setup (Stdio — Antigravity / Claude Desktop / Cursor)
Add this to your MCP configuration (`mcp_config.json`, `claude_desktop_config.json`, or `.cursor/mcp.json`):

```json
{
  "mcpServers": {
    "raydrop": {
      "command": "node",
      "args": ["C:/Users/adens/raydrop/mcp-server/index.mjs"]
    }
  }
}
```

## Setup (HTTP JSON-RPC)
If your MCP client supports HTTP JSON-RPC directly:
- URL: `http://127.0.0.1:45321/mcp`

## Available Tools
- `raydrop_get_status` — Device ID, alias, IP/port, signaling status, and system telemetry (CPU/RAM/disk/OS)
- `raydrop_list_contacts` — List own devices, LAN peers, and WAN contacts
- `raydrop_get_lan_statuses` — Live LAN status (`Local LAN`, `Tailscale`, `Raydrop Mesh`) for every device
- `raydrop_send_files` — Send local files or folders to a peer device
- `raydrop_list_transfers` — List active and recent file transfers
- `raydrop_control_transfer` — Pause, resume, or cancel a transfer
- `raydrop_send_chat` — Send an encrypted P2P chat message
- `raydrop_get_chat_history` — Read chat history with a peer
- `raydrop_manage_mesh` — Request, approve, decline, or revoke Raydrop Direct Mesh (Virtual LAN)
- `raydrop_wake_on_lan` — Wake an offline device via Wake-on-LAN
- `raydrop_start_download` — Start an accelerated HTTP download or Magnet/Torrent download
- `raydrop_list_downloads` — List active/completed HTTP & Torrent downloads
- `raydrop_clipboard` — Read/write system clipboard or inspect clipboard history
- `raydrop_notes` — List or create/update notes in Raydrop Notes
- `raydrop_list_monitors` — List connected monitors and Virtual Display Driver status
- `raydrop_remote_input` — Simulate mouse/keyboard/text input on this host
- `raydrop_convert_images` — Convert, resize, or compress images (PNG/JPG/WebP/ICO/PDF)
