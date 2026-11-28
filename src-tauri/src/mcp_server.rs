use crate::db::storage::NoteItem;
use crate::state::AppState;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerRuntime {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub last_error: Option<String>,
}

impl Default for McpServerRuntime {
    fn default() -> Self {
        Self {
            running: false,
            port: 48765,
            url: "http://127.0.0.1:48765/mcp".to_string(),
            last_error: None,
        }
    }
}

pub struct McpServerManager {
    stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub runtime: Mutex<McpServerRuntime>,
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self {
            stop_flag: Mutex::new(None),
            runtime: Mutex::new(McpServerRuntime::default()),
        }
    }
}

pub fn get_mcp_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "media_search_ytmusic",
            "description": "Search songs or music videos on YouTube Music.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keywords (song title, artist, or video)" },
                    "filter": { "type": "string", "enum": ["all", "songs", "videos"], "description": "Filter results by songs or videos" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "media_play_ytmusic",
            "description": "Search YouTube Music and immediately play the top matching song or music video in RayAmp.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Song or video query to search and play" },
                    "video_id": { "type": "string", "description": "Optional exact YouTube videoId if already known" },
                    "prefer_video": { "type": "boolean", "description": "True to open video mode and play music video" }
                }
            }
        }),
        json!({
            "name": "media_download_ytmusic",
            "description": "Search YouTube Music and download the top matching song or video directly as MP3 (audio) or MP4 (video) to RayDrop's downloads folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Song or video search query" },
                    "video_id": { "type": "string", "description": "Optional exact YouTube videoId if already known" },
                    "format": { "type": "string", "enum": ["mp3", "mp4"], "description": "Download format: mp3 (audio) or mp4 (video)" }
                },
                "required": ["format"]
            }
        }),
        json!({
            "name": "media_control",
            "description": "Control RayAmp playback: toggle, pause, resume, stop, next, prev, set_volume, or set_video_mode.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["toggle", "pause", "resume", "stop", "next", "prev", "set_volume", "set_video_mode"]
                    },
                    "volume": { "type": "number", "description": "Volume from 0.0 to 1.0 (used with set_volume)" },
                    "video_mode": { "type": "boolean", "description": "Enable/disable video view (used with set_video_mode)" }
                },
                "required": ["action"]
            }
        }),
        json!({
            "name": "media_get_status",
            "description": "Get current RayAmp playback status, active track, queue length, volume, and video mode.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "playlist_list",
            "description": "List all saved RayAmp media playlists.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "playlist_create",
            "description": "Create a new RayAmp media playlist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Playlist name" }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "playlist_add_ytmusic",
            "description": "Search YouTube Music and add the top matching track/video to a saved playlist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "playlist_id": { "type": "string", "description": "Target playlist ID" },
                    "query": { "type": "string", "description": "Song or video search query" },
                    "prefer_video": { "type": "boolean" }
                },
                "required": ["playlist_id", "query"]
            }
        }),
        json!({
            "name": "playlist_play",
            "description": "Play a saved playlist in RayAmp by playlist_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "playlist_id": { "type": "string", "description": "Playlist ID to play" }
                },
                "required": ["playlist_id"]
            }
        }),
        json!({
            "name": "playlist_get_items",
            "description": "List all tracks inside a saved RayAmp playlist by playlist_id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "playlist_id": { "type": "string", "description": "Playlist ID to inspect" }
                },
                "required": ["playlist_id"]
            }
        }),
        json!({
            "name": "media_queue_list",
            "description": "Get all tracks currently in the RayAmp playback queue along with current_index and playback status.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "media_queue_add",
            "description": "Search YouTube Music and add one or more songs/videos to the RayAmp playback queue without interrupting playback (or start if idle).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Single song or video query to search and enqueue" },
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional list of song queries to search and enqueue in order"
                    },
                    "video_id": { "type": "string", "description": "Optional exact YouTube videoId" },
                    "prefer_video": { "type": "boolean", "description": "True to prefer music videos" },
                    "play_if_idle": { "type": "boolean", "description": "Start playing immediately if RayAmp is currently idle (default true)" }
                }
            }
        }),
        json!({
            "name": "media_queue_play_index",
            "description": "Immediately play a specific song in the RayAmp queue by its 0-based index.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "index": { "type": "integer", "description": "0-based queue index to play" }
                },
                "required": ["index"]
            }
        }),
        json!({
            "name": "media_queue_clear",
            "description": "Clear all tracks from the RayAmp playback queue.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "devices_list",
            "description": "List known RayDrop devices/contacts with online status, OS, device_id, and MAC address.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "generate_image",
            "description": "Generate an image from a text prompt using local ComfyUI (if running) or RayDrop AGY CLI Bridge, and return the saved image file path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Detailed image generation prompt" }
                },
                "required": ["prompt"]
            }
        }),
        json!({
            "name": "skill_read",
            "description": "Read the full SKILL.md instructions for an enabled RayBot skill by its id or slug.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill_id_or_slug": { "type": "string", "description": "Skill id or slug from the Enabled RayBot Skills list" }
                },
                "required": ["skill_id_or_slug"]
            }
        }),
        json!({
            "name": "language_tutor_list_languages",
            "description": "List commonly learned languages supported by the Language Learning Buddy (English, Japanese, Korean, Russian, German, Indonesian, Mandarin, Spanish, French, Arabic) along with the user's saved progress summary.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "language_tutor_get_progress",
            "description": "Get persistent learning progress and the current dynamic syllabus lesson (story hook, grammar focus, target vocab, goal) for a specific language.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "Language key or name, e.g. english, japanese, korean, russian, german, indonesian, mandarin, spanish, french, arabic" }
                },
                "required": ["language"]
            }
        }),
        json!({
            "name": "language_tutor_update_progress",
            "description": "Save persistent language learning progress (XP, learned vocabulary, notes, CEFR level, or advance to the next dynamic lesson) for a specific language.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "language": { "type": "string", "description": "Target language key, e.g. english, japanese, korean, russian, german, indonesian" },
                    "advance_lesson": { "type": "boolean", "description": "Set true when the user completes the current lesson goal to unlock the next lesson" },
                    "add_vocab": { "type": "array", "items": { "type": "string" }, "description": "Newly practiced words/phrases to save" },
                    "xp_earned": { "type": "integer", "description": "XP points earned in this exchange (e.g. 10-30)" },
                    "cefr_level": { "type": "string", "description": "Optional CEFR level update (A1, A2, B1, B2, C1)" },
                    "notes": { "type": "string", "description": "Optional tutor notes on strengths or areas to reinforce" }
                },
                "required": ["language"]
            }
        }),
        json!({
            "name": "project_get_root",
            "description": "Get the configured projects root directory (defaults to user home), active project, and registered projects count.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "project_set_root",
            "description": "Set the root workspace folder where coding projects live (e.g. 'C:\\Users\\user\\projects' or '~'), and scan it for projects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "root_dir": { "type": "string", "description": "Absolute path or '~' path to the projects root directory" }
                },
                "required": ["root_dir"]
            }
        }),
        json!({
            "name": "project_scan",
            "description": "Scan the projects root directory (or a specified root_dir) to discover and register projects and their tech stacks persistently.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "root_dir": { "type": "string", "description": "Optional new root directory to set before scanning" }
                }
            }
        }),
        json!({
            "name": "project_list",
            "description": "List all persistently registered projects, the currently active project, and all cross-project relations.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "project_select",
            "description": "Select the active coding project by registered name or directory path.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project name (e.g. 'raydrop') or directory path" }
                },
                "required": ["project"]
            }
        }),
        json!({
            "name": "project_add_relation",
            "description": "Save a persistent relationship between two projects (e.g. 'raydrop-web' consumes_api 'raydrop-server', or 'waifu-on-telegram' inspired 'raydrop').",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source_project": { "type": "string", "description": "Source project name" },
                    "target_project": { "type": "string", "description": "Target project name" },
                    "relation_type": { "type": "string", "description": "Relationship type (e.g. depends_on, consumes_api, shares_schema, forks, deploys_with)" },
                    "description": { "type": "string", "description": "Short explanation of how the two projects relate" }
                },
                "required": ["source_project", "target_project", "relation_type"]
            }
        }),
        json!({
            "name": "project_save_memory",
            "description": "Persistently save an architecture summary and/or pending tasks for a project so RayBot remembers it across conversations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Project name (defaults to active project)" },
                    "summary": { "type": "string", "description": "Concise architecture/purpose summary" },
                    "pending_tasks": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of pending tasks or TODOs for this project"
                    }
                }
            }
        }),
        json!({
            "name": "code_list_files",
            "description": "List files inside a project directory (skipping .git, node_modules, target, dist, __pycache__).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" },
                    "subdir": { "type": "string", "description": "Optional subdirectory relative to project root" },
                    "max_entries": { "type": "integer", "description": "Maximum files to return (default 200, max 500)" }
                }
            }
        }),
        json!({
            "name": "code_search",
            "description": "Search for a text pattern across source files in a project.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Substring to search for" },
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" },
                    "subdir": { "type": "string", "description": "Optional subdirectory relative to project root" },
                    "case_sensitive": { "type": "boolean", "description": "Case-sensitive search (default false)" },
                    "max_matches": { "type": "integer", "description": "Maximum matches to return (default 50, max 150)" }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "code_read_file",
            "description": "Read a text file inside a project with line numbers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root (or absolute inside project)" },
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" },
                    "start_line": { "type": "integer", "description": "1-indexed start line (default 1)" },
                    "end_line": { "type": "integer", "description": "1-indexed end line (default start_line + 249)" }
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "code_write_file",
            "description": "Create or overwrite a file inside a project directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root" },
                    "content": { "type": "string", "description": "Full file content to write" },
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" }
                },
                "required": ["path", "content"]
            }
        }),
        json!({
            "name": "code_edit_file",
            "description": "Edit an existing file in a project by replacing an exact unique old_text block with new_text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path relative to project root" },
                    "old_text": { "type": "string", "description": "Exact existing text to replace (must match uniquely)" },
                    "new_text": { "type": "string", "description": "Replacement text" },
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" }
                },
                "required": ["path", "old_text", "new_text"]
            }
        }),
        json!({
            "name": "code_run_command",
            "description": "Run a shell command inside the active (or specified) project directory (e.g., tests, build, git status).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Command to execute in the project root" },
                    "project": { "type": "string", "description": "Optional project name (defaults to active project)" },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 60, max 180)" }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "device_wake_on_lan",
            "description": "Send a Wake-on-LAN Magic Packet to wake up a PC on LAN or WAN.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mac_address": { "type": "string", "description": "Target MAC address (AA:BB:CC:DD:EE:FF)" },
                    "wan_host": { "type": "string", "description": "Optional DDNS hostname or Public IP for WAN WoL" },
                    "port": { "type": "integer", "description": "Optional UDP port (default 9)" }
                },
                "required": ["mac_address"]
            }
        }),
        json!({
            "name": "system_telemetry",
            "description": "Get live system telemetry of this host PC (CPU, RAM, GPU, Disks, Network, Uptime).",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "clipboard_get_history",
            "description": "Read recent clipboard history entries stored by RayDrop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max items to return (default 10)" }
                }
            }
        }),
        json!({
            "name": "notes_list",
            "description": "List saved RayDrop notes and checklists.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "include_archived": { "type": "boolean" }
                }
            }
        }),
        json!({
            "name": "notes_create",
            "description": "Create a new note in RayDrop.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["title", "content"]
            }
        }),
        json!({
            "name": "shared_folders_list",
            "description": "List local folders shared on this device for remote browsing.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

pub fn to_openai_tools() -> Vec<Value> {
    get_mcp_tool_definitions()
        .into_iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t["name"],
                    "description": t["description"],
                    "parameters": t["inputSchema"]
                }
            })
        })
        .collect()
}

pub async fn execute_mcp_tool(
    app: &AppHandle,
    state: &Arc<AppState>,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    match name {
        "media_search_ytmusic" => {
            let query = args["query"].as_str().unwrap_or("").trim();
            let filter = args["filter"].as_str().unwrap_or("all");
            let items = crate::media_player::search_ytmusic(query, filter).await?;
            serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
        }
        "media_play_ytmusic" => {
            let prefer_video = args["prefer_video"].as_bool().unwrap_or(false);
            let mut video_id = args["video_id"].as_str().unwrap_or("").trim().to_string();
            let mut title = "YouTube Music Track".to_string();
            let mut artist = "YouTube Music".to_string();
            let mut duration_secs = 0u64;
            let mut thumbnail_url = None;
            let mut is_video = prefer_video;

            if video_id.is_empty() {
                let query = args["query"].as_str().unwrap_or("").trim();
                if query.is_empty() {
                    return Err("Provide either `query` or `video_id`".to_string());
                }
                let filter = if prefer_video { "videos" } else { "all" };
                let results = crate::media_player::search_ytmusic(query, filter).await?;
                let first = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| format!("No YouTube Music results found for '{}'", query))?;
                video_id = first.video_id;
                title = first.title;
                artist = first.artist;
                duration_secs = first.duration_secs;
                thumbnail_url = first.thumbnail_url;
                is_video = prefer_video || first.is_video;
            }

            let track = crate::media_player::MediaTrack {
                id: format!("yt-{}", video_id),
                source_type: "ytmusic".to_string(),
                title: title.clone(),
                artist: artist.clone(),
                duration_secs,
                uri_or_id: video_id.clone(),
                thumbnail_url,
                is_video,
                stream_url: None,
            };

            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "play_track",
                    "payload": { "track": track }
                }),
            );

            Ok(json!({
                "status": "playing_requested",
                "title": title,
                "artist": artist,
                "video_id": video_id,
                "is_video": is_video
            })
            .to_string())
        }
        "media_download_ytmusic" => {
            let fmt = args["format"].as_str().unwrap_or("mp3");
            let mut video_id = args["video_id"].as_str().unwrap_or("").trim().to_string();
            let mut title_hint = None;
            if video_id.is_empty() {
                let query = args["query"].as_str().unwrap_or("").trim();
                if query.is_empty() {
                    return Err("Provide either `query` or `video_id`".to_string());
                }
                let filter = if fmt == "mp4" { "videos" } else { "all" };
                let results = crate::media_player::search_ytmusic(query, filter).await?;
                let first = results
                    .into_iter()
                    .next()
                    .ok_or_else(|| format!("No YouTube Music results found for '{}'", query))?;
                video_id = first.video_id;
                title_hint = Some(first.title);
            }
            let dir = state
                .settings
                .lock()
                .map(|s| s.download_dir.clone())
                .map_err(|e| e.to_string())?;
            let res = crate::media_player::download_ytmusic_media(
                Some(app),
                &video_id,
                title_hint.as_deref(),
                fmt,
                &dir,
            )
            .await?;
            serde_json::to_string_pretty(&res).map_err(|e| e.to_string())
        }
        "media_control" => {
            let action = args["action"].as_str().unwrap_or("toggle");
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": action,
                    "payload": args
                }),
            );
            Ok(json!({ "status": "ok", "action": action }).to_string())
        }
        "media_get_status" => {
            let st = state
                .media_player
                .lock()
                .map(|g| g.clone())
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&st).map_err(|e| e.to_string())
        }
        "playlist_list" => {
            let list = state.db.list_media_playlists()?;
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
        }
        "playlist_create" => {
            let pname = args["name"].as_str().unwrap_or("New Playlist");
            let pl = state.db.create_media_playlist(pname)?;
            serde_json::to_string_pretty(&pl).map_err(|e| e.to_string())
        }
        "playlist_add_ytmusic" => {
            let pid = args["playlist_id"].as_str().unwrap_or("").trim();
            let query = args["query"].as_str().unwrap_or("").trim();
            let prefer_video = args["prefer_video"].as_bool().unwrap_or(false);
            if pid.is_empty() || query.is_empty() {
                return Err("`playlist_id` and `query` are required".to_string());
            }
            let filter = if prefer_video { "videos" } else { "all" };
            let results = crate::media_player::search_ytmusic(query, filter).await?;
            let first = results
                .into_iter()
                .next()
                .ok_or_else(|| format!("No results for '{}'", query))?;
            let item = state.db.add_media_playlist_item(
                pid,
                "ytmusic",
                &first.title,
                &first.artist,
                first.duration_secs,
                &first.video_id,
                first.thumbnail_url.as_deref(),
                prefer_video || first.is_video,
            )?;
            serde_json::to_string_pretty(&item).map_err(|e| e.to_string())
        }
        "playlist_play" => {
            let pid = args["playlist_id"].as_str().unwrap_or("").trim();
            if pid.is_empty() {
                return Err("`playlist_id` is required".to_string());
            }
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "play_playlist",
                    "payload": { "playlist_id": pid }
                }),
            );
            Ok(json!({ "status": "ok", "playlist_id": pid }).to_string())
        }
        "playlist_get_items" => {
            let pid = args["playlist_id"].as_str().unwrap_or("").trim();
            if pid.is_empty() {
                return Err("`playlist_id` is required".to_string());
            }
            let items = state.db.list_media_playlist_items(pid)?;
            serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
        }
        "generate_image" => {
            let prompt = args["prompt"].as_str().unwrap_or("").trim();
            if prompt.is_empty() {
                return Err("`prompt` is required".to_string());
            }
            let b64 = crate::llm_bridge::generate_image_via_bridge(state, prompt).await?;
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("Failed decoding generated image: {e}"))?;
            let out_dir = std::env::temp_dir().join("raydrop_generated");
            std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
            let out_path = out_dir.join(format!("gen_{}.png", uuid::Uuid::new_v4().simple()));
            std::fs::write(&out_path, &bytes).map_err(|e| e.to_string())?;
            let path_str = out_path.to_string_lossy().to_string();
            Ok(json!({
                "status": "generated",
                "image_path": path_str,
                "prompt": prompt
            })
            .to_string())
        }
        "skill_read" => {
            let key = args["skill_id_or_slug"].as_str().unwrap_or("").trim();
            if key.is_empty() {
                return Err("`skill_id_or_slug` is required".to_string());
            }
            let rec = match crate::skills::storage::get_skill_by_id(&state.db, key)? {
                Some(s) => s,
                None => {
                    let all = crate::skills::storage::search_skills(
                        &state.db,
                        &crate::skills::models::SkillsSearchQuery::default(),
                    )?;
                    all.into_iter()
                        .find(|s| s.slug.eq_ignore_ascii_case(key) || s.name.eq_ignore_ascii_case(key))
                        .ok_or_else(|| format!("Skill '{}' not found", key))?
                }
            };
            if let Some(ref pkg_dir) = rec.local_package_path {
                let p = std::path::Path::new(pkg_dir).join("SKILL.md");
                if let Ok(md) = std::fs::read_to_string(&p) {
                    return Ok(md);
                }
            }
            let preview =
                crate::skills::storage::read_skill_file_preview(&state.db, &rec.id, "SKILL.md")?;
            Ok(preview.content.unwrap_or_else(|| rec.description.clone()))
        }
        "language_tutor_list_languages" => {
            let langs = supported_languages();
            let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
            let mut progress_map: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
            if let Ok(mut stmt) = conn.prepare(
                "SELECT language, cefr_level, current_unit, current_lesson, xp FROM language_learning_progress",
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        json!({
                            "cefr_level": r.get::<_, String>(1)?,
                            "current_unit": r.get::<_, i64>(2)?,
                            "current_lesson": r.get::<_, i64>(3)?,
                            "xp": r.get::<_, i64>(4)?
                        }),
                    ))
                }) {
                    for row in rows.flatten() {
                        progress_map.insert(row.0, row.1);
                    }
                }
            }
            let list: Vec<Value> = langs
                .into_iter()
                .map(|(key, name, native)| {
                    json!({
                        "key": key,
                        "name": name,
                        "native_name": native,
                        "progress": progress_map.get(key).cloned().unwrap_or(Value::Null)
                    })
                })
                .collect();
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
        }
        "language_tutor_get_progress" => {
            let raw_lang = args["language"].as_str().unwrap_or("english");
            let (lang_key, display_name) = normalize_language_key(raw_lang);
            let now = crate::db::storage::now_unix_secs() as i64;
            let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
            let _ = conn.execute(
                "INSERT OR IGNORE INTO language_learning_progress (language, display_name, cefr_level, current_unit, current_lesson, xp, completed_lessons_json, learned_vocab_json, notes, updated_at)
                 VALUES (?1, ?2, 'A1', 1, 1, 0, '[]', '[]', '', ?3)",
                rusqlite::params![lang_key, display_name, now],
            );
            let row = conn
                .query_row(
                    "SELECT cefr_level, current_unit, current_lesson, xp, completed_lessons_json, learned_vocab_json, notes, updated_at
                     FROM language_learning_progress WHERE language = ?1",
                    rusqlite::params![lang_key],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)? as u32,
                            r.get::<_, i64>(2)? as u32,
                            r.get::<_, i64>(3)? as u32,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, String>(6)?,
                            r.get::<_, i64>(7)? as u64,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;
            let lesson = get_syllabus_lesson(row.1, row.2);
            serde_json::to_string_pretty(&json!({
                "language": lang_key,
                "display_name": display_name,
                "cefr_level": row.0,
                "current_unit": row.1,
                "current_lesson": row.2,
                "xp": row.3,
                "completed_lessons": serde_json::from_str::<Vec<String>>(&row.4).unwrap_or_default(),
                "learned_vocab": serde_json::from_str::<Vec<String>>(&row.5).unwrap_or_default(),
                "notes": row.6,
                "updated_at": row.7,
                "current_module": lesson
            }))
            .map_err(|e| e.to_string())
        }
        "language_tutor_update_progress" => {
            let raw_lang = args["language"].as_str().unwrap_or("english");
            let (lang_key, display_name) = normalize_language_key(raw_lang);
            let advance = args["advance_lesson"].as_bool().unwrap_or(false);
            let xp_add = args["xp_earned"].as_u64().unwrap_or(15) as i64;
            let new_cefr = args["cefr_level"].as_str().map(|s| s.trim().to_string());
            let new_notes = args["notes"].as_str().map(|s| s.trim().to_string());
            let add_vocab: Vec<String> = args["add_vocab"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            let now = crate::db::storage::now_unix_secs() as i64;
            let conn = state.db.conn.lock().map_err(|e| e.to_string())?;
            let _ = conn.execute(
                "INSERT OR IGNORE INTO language_learning_progress (language, display_name, cefr_level, current_unit, current_lesson, xp, completed_lessons_json, learned_vocab_json, notes, updated_at)
                 VALUES (?1, ?2, 'A1', 1, 1, 0, '[]', '[]', '', ?3)",
                rusqlite::params![lang_key, display_name, now],
            );

            let (mut cefr, mut unit, mut lesson, mut xp, comp_raw, vocab_raw, mut notes) = conn
                .query_row(
                    "SELECT cefr_level, current_unit, current_lesson, xp, completed_lessons_json, learned_vocab_json, notes
                     FROM language_learning_progress WHERE language = ?1",
                    rusqlite::params![lang_key],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)? as u32,
                            r.get::<_, i64>(2)? as u32,
                            r.get::<_, i64>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, String>(6)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;

            let mut completed: Vec<String> = serde_json::from_str(&comp_raw).unwrap_or_default();
            let mut vocab: Vec<String> = serde_json::from_str(&vocab_raw).unwrap_or_default();

            for w in add_vocab {
                if !vocab.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
                    vocab.push(w);
                }
            }
            if vocab.len() > 200 {
                let drain = vocab.len() - 200;
                vocab.drain(0..drain);
            }
            xp += xp_add.max(0);
            if let Some(c) = new_cefr.filter(|s| !s.is_empty()) {
                cefr = c;
            }
            if let Some(n) = new_notes.filter(|s| !s.is_empty()) {
                notes = n;
            }
            if advance {
                let code = format!("{}.{}", unit, lesson);
                if !completed.contains(&code) {
                    completed.push(code);
                }
                if lesson < 3 {
                    lesson += 1;
                } else if unit < 4 {
                    unit += 1;
                    lesson = 1;
                    cefr = match unit {
                        2 => "A2".to_string(),
                        3 => "B1".to_string(),
                        _ => "B2".to_string(),
                    };
                }
            }

            let comp_json = serde_json::to_string(&completed).unwrap_or_else(|_| "[]".to_string());
            let vocab_json = serde_json::to_string(&vocab).unwrap_or_else(|_| "[]".to_string());
            conn.execute(
                "UPDATE language_learning_progress
                 SET cefr_level = ?1, current_unit = ?2, current_lesson = ?3, xp = ?4,
                     completed_lessons_json = ?5, learned_vocab_json = ?6, notes = ?7, updated_at = ?8
                 WHERE language = ?9",
                rusqlite::params![cefr, unit as i64, lesson as i64, xp, comp_json, vocab_json, notes, now, lang_key],
            )
            .map_err(|e| e.to_string())?;

            let next_module = get_syllabus_lesson(unit, lesson);
            serde_json::to_string_pretty(&json!({
                "status": "updated",
                "language": lang_key,
                "display_name": display_name,
                "cefr_level": cefr,
                "current_unit": unit,
                "current_lesson": lesson,
                "xp": xp,
                "completed_lessons": completed,
                "learned_vocab_count": vocab.len(),
                "current_module": next_module
            }))
            .map_err(|e| e.to_string())
        }
        "project_get_root" => {
            let overview = crate::project_workspace::get_projects_overview(&state.db)?;
            serde_json::to_string_pretty(&json!({
                "root_dir": overview.root_dir,
                "active_project": overview.active_project,
                "projects_count": overview.projects.len(),
                "relations_count": overview.relations.len()
            }))
            .map_err(|e| e.to_string())
        }
        "project_set_root" => {
            let root_dir = args
                .get("root_dir")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'root_dir'")?;
            let overview = crate::project_workspace::set_projects_root(&state.db, root_dir)?;
            serde_json::to_string_pretty(&overview).map_err(|e| e.to_string())
        }
        "project_scan" => {
            let overview = if let Some(root_dir) = args
                .get("root_dir")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                crate::project_workspace::set_projects_root(&state.db, root_dir)?
            } else {
                crate::project_workspace::scan_projects_in_root(&state.db)?
            };
            serde_json::to_string_pretty(&overview).map_err(|e| e.to_string())
        }
        "project_list" => {
            let overview = crate::project_workspace::get_projects_overview(&state.db)?;
            serde_json::to_string_pretty(&overview).map_err(|e| e.to_string())
        }
        "project_select" => {
            let project = args
                .get("project")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'project'")?;
            let entry = crate::project_workspace::select_active_project(&state.db, project)?;
            serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())
        }
        "project_add_relation" => {
            let src = args
                .get("source_project")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'source_project'")?;
            let dst = args
                .get("target_project")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'target_project'")?;
            let rel = args
                .get("relation_type")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'relation_type'")?;
            let desc = args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let saved =
                crate::project_workspace::add_project_relation(&state.db, src, dst, rel, desc)?;
            serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())
        }
        "project_save_memory" => {
            let project = args.get("project").and_then(|v| v.as_str());
            let summary = args.get("summary").and_then(|v| v.as_str());
            let pending_tasks = args.get("pending_tasks").and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<String>>()
                })
            });
            let updated = crate::project_workspace::save_project_memory(
                &state.db,
                project,
                summary,
                pending_tasks,
            )?;
            serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())
        }
        "code_list_files" => {
            let project = args.get("project").and_then(|v| v.as_str());
            let subdir = args.get("subdir").and_then(|v| v.as_str());
            let max_entries = args
                .get("max_entries")
                .and_then(|v| v.as_u64())
                .unwrap_or(200)
                .clamp(1, 500) as usize;
            let (proj, target_dir) =
                crate::project_workspace::resolve_project_path(&state.db, project, subdir)?;
            if !target_dir.is_dir() {
                return Err(format!("'{}' is not a directory", target_dir.display()));
            }
            let proj_root = dunce::canonicalize(&proj.path)
                .unwrap_or_else(|_| std::path::PathBuf::from(&proj.path));
            let mut files = Vec::new();
            let walker = walkdir::WalkDir::new(&target_dir)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !matches!(
                        name.as_ref(),
                        ".git" | "node_modules" | "target" | "__pycache__" | ".venv" | "dist" | "build"
                    )
                });
            for entry in walker.flatten() {
                if entry.file_type().is_file() {
                    let rel = entry
                        .path()
                        .strip_prefix(&proj_root)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push(rel);
                    if files.len() >= max_entries {
                        break;
                    }
                }
            }
            serde_json::to_string_pretty(&json!({
                "project": proj.name,
                "root": proj.path,
                "count": files.len(),
                "truncated": files.len() >= max_entries,
                "files": files
            }))
            .map_err(|e| e.to_string())
        }
        "code_search" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .ok_or("Missing non-empty 'query'")?;
            let project = args.get("project").and_then(|v| v.as_str());
            let subdir = args.get("subdir").and_then(|v| v.as_str());
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let max_matches = args
                .get("max_matches")
                .and_then(|v| v.as_u64())
                .unwrap_or(50)
                .clamp(1, 150) as usize;
            let (proj, target_dir) =
                crate::project_workspace::resolve_project_path(&state.db, project, subdir)?;
            let proj_root = dunce::canonicalize(&proj.path)
                .unwrap_or_else(|_| std::path::PathBuf::from(&proj.path));
            let needle = if case_sensitive {
                query.to_string()
            } else {
                query.to_lowercase()
            };

            let mut matches = Vec::new();
            let walker = walkdir::WalkDir::new(&target_dir)
                .into_iter()
                .filter_entry(|e| {
                    let name = e.file_name().to_string_lossy();
                    !matches!(
                        name.as_ref(),
                        ".git" | "node_modules" | "target" | "__pycache__" | ".venv" | "dist" | "build"
                    )
                });
            for entry in walker.flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.metadata().map(|m| m.len() > 1_048_576).unwrap_or(true) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(entry.path()) else {
                    continue;
                };
                if bytes.iter().take(512).any(|&b| b == 0) {
                    continue;
                }
                let text = String::from_utf8_lossy(&bytes);
                let rel = entry
                    .path()
                    .strip_prefix(&proj_root)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .replace('\\', "/");
                for (idx, line) in text.lines().enumerate() {
                    let hay = if case_sensitive {
                        line.to_string()
                    } else {
                        line.to_lowercase()
                    };
                    if hay.contains(&needle) {
                        let snippet: String = line.chars().take(200).collect();
                        matches.push(json!({
                            "file": rel,
                            "line": idx + 1,
                            "text": snippet
                        }));
                        if matches.len() >= max_matches {
                            break;
                        }
                    }
                }
                if matches.len() >= max_matches {
                    break;
                }
            }
            serde_json::to_string_pretty(&json!({
                "project": proj.name,
                "query": query,
                "match_count": matches.len(),
                "matches": matches
            }))
            .map_err(|e| e.to_string())
        }
        "code_read_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?;
            let project = args.get("project").and_then(|v| v.as_str());
            let start_line = args
                .get("start_line")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .max(1) as usize;
            let end_line = args
                .get("end_line")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(start_line + 249)
                .min(start_line + 399);

            let (proj, file_path) =
                crate::project_workspace::resolve_project_path(&state.db, project, Some(path))?;
            if !file_path.is_file() {
                return Err(format!("File not found: '{}'", file_path.display()));
            }
            let content = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read '{}': {}", file_path.display(), e))?;
            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len();
            let s_idx = (start_line - 1).min(total_lines);
            let e_idx = end_line.min(total_lines);
            let numbered = lines[s_idx..e_idx]
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}: {}", s_idx + i + 1, l))
                .collect::<Vec<_>>()
                .join("\n");

            serde_json::to_string_pretty(&json!({
                "project": proj.name,
                "path": path,
                "start_line": start_line,
                "end_line": e_idx,
                "total_lines": total_lines,
                "content": numbered
            }))
            .map_err(|e| e.to_string())
        }
        "code_write_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?;
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'content'")?;
            let project = args.get("project").and_then(|v| v.as_str());
            let (proj, file_path) =
                crate::project_workspace::resolve_project_path(&state.db, project, Some(path))?;
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent dir: {}", e))?;
            }
            std::fs::write(&file_path, content.as_bytes())
                .map_err(|e| format!("Failed to write '{}': {}", file_path.display(), e))?;
            serde_json::to_string_pretty(&json!({
                "status": "written",
                "project": proj.name,
                "path": file_path.to_string_lossy(),
                "bytes": content.len()
            }))
            .map_err(|e| e.to_string())
        }
        "code_edit_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'path'")?;
            let old_text = args
                .get("old_text")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'old_text'")?;
            let new_text = args
                .get("new_text")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'new_text'")?;
            let project = args.get("project").and_then(|v| v.as_str());
            let (proj, file_path) =
                crate::project_workspace::resolve_project_path(&state.db, project, Some(path))?;
            let current = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read '{}': {}", file_path.display(), e))?;
            let count = current.matches(old_text).count();
            if count == 0 {
                return Err(format!("'old_text' not found in '{}'", path));
            }
            if count > 1 {
                return Err(format!(
                    "'old_text' matched {} times in '{}'; include more surrounding lines to make it unique.",
                    count, path
                ));
            }
            let updated = current.replacen(old_text, new_text, 1);
            std::fs::write(&file_path, updated.as_bytes())
                .map_err(|e| format!("Failed to write '{}': {}", file_path.display(), e))?;
            serde_json::to_string_pretty(&json!({
                "status": "edited",
                "project": proj.name,
                "path": file_path.to_string_lossy()
            }))
            .map_err(|e| e.to_string())
        }
        "code_run_command" => {
            let command = args
                .get("command")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .ok_or("Missing 'command'")?;
            let project = args.get("project").and_then(|v| v.as_str());
            let timeout_secs = args
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(60)
                .clamp(1, 180);
            let proj = crate::project_workspace::resolve_project_dir(&state.db, project)?;

            let mut cmd = if cfg!(target_os = "windows") {
                let mut c = tokio::process::Command::new("cmd");
                c.args(["/C", command]);
                c
            } else {
                let mut c = tokio::process::Command::new("sh");
                c.args(["-c", command]);
                c
            };
            cmd.current_dir(&proj.path);
            #[cfg(target_os = "windows")]
            {
                cmd.creation_flags(0x08000000);
            }

            let output = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                cmd.output(),
            )
            .await
            .map_err(|_| format!("Command timed out after {}s", timeout_secs))?
            .map_err(|e| format!("Failed to execute command: {}", e))?;

            let stdout_full = String::from_utf8_lossy(&output.stdout);
            let stderr_full = String::from_utf8_lossy(&output.stderr);
            let stdout: String = stdout_full.chars().take(12_000).collect();
            let stderr: String = stderr_full.chars().take(12_000).collect();

            serde_json::to_string_pretty(&json!({
                "project": proj.name,
                "cwd": proj.path,
                "exit_code": output.status.code(),
                "success": output.status.success(),
                "stdout": stdout,
                "stderr": stderr
            }))
            .map_err(|e| e.to_string())
        }
        "media_queue_list" => {
            let st = state
                .media_player
                .lock()
                .map(|g| g.clone())
                .map_err(|e| e.to_string())?;
            let queue_summary: Vec<Value> = st
                .queue
                .iter()
                .enumerate()
                .map(|(idx, t)| {
                    json!({
                        "index": idx,
                        "id": t.id,
                        "title": t.title,
                        "artist": t.artist,
                        "duration_secs": t.duration_secs,
                        "is_video": t.is_video,
                        "uri_or_id": t.uri_or_id
                    })
                })
                .collect();
            serde_json::to_string_pretty(&json!({
                "is_playing": st.is_playing,
                "current_index": st.current_index,
                "current_track": st.current_track,
                "queue_length": st.queue.len(),
                "queue": queue_summary
            }))
            .map_err(|e| e.to_string())
        }
        "media_queue_add" => {
            let prefer_video = args["prefer_video"].as_bool().unwrap_or(false);
            let play_if_idle = args["play_if_idle"].as_bool().unwrap_or(true);
            let mut tracks = Vec::new();

            if let Some(vid) = args["video_id"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                tracks.push(crate::media_player::MediaTrack {
                    id: format!("yt-{}", vid),
                    source_type: "ytmusic".to_string(),
                    title: "YouTube Music Track".to_string(),
                    artist: "YouTube Music".to_string(),
                    duration_secs: 0,
                    uri_or_id: vid.to_string(),
                    thumbnail_url: None,
                    is_video: prefer_video,
                    stream_url: None,
                });
            } else {
                let mut queries: Vec<String> = Vec::new();
                if let Some(arr) = args["queries"].as_array() {
                    for q in arr {
                        if let Some(s) = q.as_str().map(|x| x.trim()).filter(|x| !x.is_empty()) {
                            queries.push(s.to_string());
                        }
                    }
                }
                if let Some(q) = args["query"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    if queries.is_empty() {
                        queries.push(q.to_string());
                    }
                }
                if queries.is_empty() {
                    return Err("Provide `query`, `queries`, or `video_id`".to_string());
                }
                let filter = if prefer_video { "videos" } else { "all" };
                for q in queries {
                    let results = crate::media_player::search_ytmusic(&q, filter).await?;
                    if let Some(first) = results.into_iter().next() {
                        tracks.push(crate::media_player::MediaTrack {
                            id: format!("yt-{}", first.video_id),
                            source_type: "ytmusic".to_string(),
                            title: first.title,
                            artist: first.artist,
                            duration_secs: first.duration_secs,
                            uri_or_id: first.video_id,
                            thumbnail_url: first.thumbnail_url,
                            is_video: prefer_video || first.is_video,
                            stream_url: None,
                        });
                    }
                }
                if tracks.is_empty() {
                    return Err("No YouTube Music tracks matched your query".to_string());
                }
            }

            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "enqueue_tracks",
                    "payload": {
                        "tracks": tracks,
                        "play_if_idle": play_if_idle
                    }
                }),
            );

            Ok(json!({
                "status": "queued",
                "added_count": tracks.len(),
                "tracks": tracks
            })
            .to_string())
        }
        "media_queue_play_index" => {
            let idx = args["index"]
                .as_u64()
                .ok_or_else(|| "`index` (integer) is required".to_string())? as usize;
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "play_queue_index",
                    "payload": { "index": idx }
                }),
            );
            Ok(json!({ "status": "ok", "index": idx }).to_string())
        }
        "media_queue_clear" => {
            let _ = app.emit(
                "media_player_remote_action",
                json!({
                    "action": "clear_queue",
                    "payload": {}
                }),
            );
            Ok(json!({ "status": "ok", "cleared": true }).to_string())
        }
        "devices_list" => {
            let contacts = state.db.list_contacts()?;
            let summary: Vec<Value> = contacts
                .into_iter()
                .map(|c| {
                    json!({
                        "device_id": c.device_id,
                        "alias_name": c.alias_name,
                        "custom_name": c.custom_name,
                        "os_type": c.os_type,
                        "is_own_device": c.is_own_device,
                        "is_online": c.is_online,
                        "mac_address": c.mac_address,
                        "network_type": c.network_type
                    })
                })
                .collect();
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
        }
        "device_wake_on_lan" => {
            let mac = args["mac_address"].as_str().unwrap_or("").to_string();
            let wan = args["wan_host"].as_str().map(|s| s.to_string());
            let port = args["port"].as_u64().map(|p| p as u16);
            let target_id = args["target_device_id"].as_str().map(|s| s.to_string());
            let msg = crate::commands::send_wake_on_lan_auto_inner(
                state,
                target_id,
                mac.clone(),
                wan.clone(),
                port,
            )
            .await?;
            Ok(json!({
                "status": "magic_packet_sent",
                "message": msg,
                "mac_address": mac,
                "wan_host": wan,
                "port": port.unwrap_or(9)
            })
            .to_string())
        }
        "system_telemetry" => {
            let t = tokio::task::spawn_blocking(crate::system_monitor::collect_system_telemetry)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string_pretty(&t).map_err(|e| e.to_string())
        }
        "clipboard_get_history" => {
            let limit = args["limit"].as_u64().unwrap_or(10) as usize;
            let items = state.db.list_clipboard_history(limit.min(30))?;
            serde_json::to_string_pretty(&items).map_err(|e| e.to_string())
        }
        "notes_list" => {
            let inc = args["include_archived"].as_bool().unwrap_or(false);
            let notes = state.db.list_notes(inc)?;
            serde_json::to_string_pretty(&notes).map_err(|e| e.to_string())
        }
        "notes_create" => {
            let title = args["title"].as_str().unwrap_or("AI Note").to_string();
            let content = args["content"].as_str().unwrap_or("").to_string();
            let note = NoteItem {
                id: String::new(),
                title,
                content,
                note_type: "note".to_string(),
                checklist: Vec::new(),
                images: Vec::new(),
                color: "default".to_string(),
                labels: vec!["ai".to_string()],
                pinned: false,
                archived: false,
                created_at: 0,
                updated_at: 0,
            };
            let saved = state.db.upsert_note(note)?;
            serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())
        }
        "shared_folders_list" => {
            let folders = state.db.list_shared_folders()?;
            serde_json::to_string_pretty(&folders).map_err(|e| e.to_string())
        }
        _ => Err(format!("Unknown MCP tool: {}", name)),
    }
}

pub async fn handle_mcp_jsonrpc(
    app: &AppHandle,
    state: &Arc<AppState>,
    req: Value,
) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    // Notifications (no id) do not require a response
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": "raydrop-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": get_mcp_tool_definitions()
        })),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(json!({}));
            let tool_name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match execute_mcp_tool(app, state, tool_name, &args).await {
                Ok(text) => Ok(json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                })),
                Err(err) => Ok(json!({
                    "content": [{ "type": "text", "text": err }],
                    "isError": true
                })),
            }
        }
        _ => Err((-32601, format!("Method not found: {}", method))),
    };

    match result {
        Ok(res) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": res
        })),
        Err((code, msg)) => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": msg }
        })),
    }
}

pub fn restart_mcp_server(
    app: AppHandle,
    state: Arc<AppState>,
    enabled: bool,
    port: u16,
    auth_token: String,
) {
    if let Ok(mut guard) = state.mcp_server.stop_flag.lock() {
        if let Some(old_flag) = guard.take() {
            old_flag.store(true, Ordering::Relaxed);
        }
        if !enabled {
            if let Ok(mut rt) = state.mcp_server.runtime.lock() {
                rt.running = false;
                rt.port = port;
                rt.url = format!("http://127.0.0.1:{}/mcp", port);
                rt.last_error = None;
            }
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        *guard = Some(stop.clone());

        let state_bg = state.clone();
        tauri::async_runtime::spawn(async move {
            run_mcp_http_loop(app, state_bg, port, auth_token, stop).await;
        });
    }
}

async fn run_mcp_http_loop(
    app: AppHandle,
    state: Arc<AppState>,
    port: u16,
    auth_token: String,
    stop: Arc<AtomicBool>,
) {
    // Try configured port first, then automatically fall back to nearby free ports if occupied
    let mut listener_opt: Option<(TcpListener, u16)> = None;
    let mut last_bind_err = String::new();
    for offset in 0..=20u16 {
        let candidate_port = port.saturating_add(offset);
        if candidate_port == 0 {
            continue;
        }
        let addr = format!("127.0.0.1:{}", candidate_port);
        match TcpListener::bind(&addr).await {
            Ok(l) => {
                listener_opt = Some((l, candidate_port));
                break;
            }
            Err(e) => {
                last_bind_err = e.to_string();
            }
        }
    }

    if listener_opt.is_none() {
        if let Ok(l) = TcpListener::bind("127.0.0.1:0").await {
            if let Ok(local) = l.local_addr() {
                listener_opt = Some((l, local.port()));
            }
        }
    }

    let (listener, actual_port) = match listener_opt {
        Some(pair) => pair,
        None => {
            warn!("Failed to bind MCP HTTP server near port {}: {}", port, last_bind_err);
            if let Ok(mut rt) = state.mcp_server.runtime.lock() {
                rt.running = false;
                rt.port = port;
                rt.last_error = Some(format!("Port {} bind error: {}", port, last_bind_err));
            }
            return;
        }
    };

    let bind_addr = format!("127.0.0.1:{}", actual_port);
    info!("RayDrop MCP HTTP server listening on http://{}/mcp", bind_addr);
    if let Ok(mut rt) = state.mcp_server.runtime.lock() {
        rt.running = true;
        rt.port = actual_port;
        rt.url = format!("http://{}/mcp", bind_addr);
        rt.last_error = None;
    }

    while !stop.load(Ordering::Relaxed) {
        let accept_fut = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            listener.accept(),
        )
        .await;

        let Ok(Ok((mut socket, _))) = accept_fut else {
            continue;
        };

        let app_c = app.clone();
        let state_c = state.clone();
        let token_c = auth_token.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let Ok(n) = socket.read(&mut buf).await else {
                return;
            };
            if n == 0 {
                return;
            }
            let raw = String::from_utf8_lossy(&buf[..n]);
            let Some((head, body)) = raw.split_once("\r\n\r\n") else {
                return;
            };

            let first_line = head.lines().next().unwrap_or("");
            if first_line.starts_with("OPTIONS ") {
                let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }
            if first_line.starts_with("GET /health") {
                let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"ok\":true,\"server\":\"raydrop-mcp\"}";
                let _ = socket.write_all(resp.as_bytes()).await;
                return;
            }

            if !token_c.trim().is_empty() {
                let expected = format!("authorization: bearer {}", token_c.trim().to_lowercase());
                let authorized = head
                    .lines()
                    .any(|l| l.trim().to_lowercase() == expected);
                if !authorized {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"Unauthorized\"}";
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }
            }

            let req_json: Value = match serde_json::from_str(body.trim()) {
                Ok(v) => v,
                Err(e) => {
                    let err_body = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        err_body.len(),
                        err_body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    return;
                }
            };

            match handle_mcp_jsonrpc(&app_c, &state_c, req_json).await {
                Some(reply) => {
                    let reply_str = reply.to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply_str.len(),
                        reply_str
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                }
                None => {
                    let resp = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = socket.write_all(resp.as_bytes()).await;
                }
            }
        });
    }
}

fn supported_languages() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("english", "English", "English"),
        ("japanese", "Japanese", "日本語"),
        ("korean", "Korean", "한국어"),
        ("russian", "Russian", "Русский"),
        ("german", "German", "Deutsch"),
        ("indonesian", "Indonesian", "Bahasa Indonesia"),
        ("mandarin", "Mandarin Chinese", "中文"),
        ("spanish", "Spanish", "Español"),
        ("french", "French", "Français"),
        ("arabic", "Arabic", "العربية"),
    ]
}

fn normalize_language_key(raw: &str) -> (String, String) {
    let low = raw.trim().to_lowercase();
    match low.as_str() {
        "en" | "eng" | "english" | "inggris" => ("english".into(), "English".into()),
        "ja" | "jp" | "japanese" | "jepang" => ("japanese".into(), "Japanese (日本語)".into()),
        "ko" | "kr" | "korean" | "korea" => ("korean".into(), "Korean (한국어)".into()),
        "ru" | "rus" | "russian" | "rusia" => ("russian".into(), "Russian (Русский)".into()),
        "de" | "ger" | "german" | "germany" | "deutsch" | "jerman" => ("german".into(), "German (Deutsch)".into()),
        "id" | "ind" | "indonesian" | "indonesia" | "bahasa indonesia" => ("indonesian".into(), "Indonesian".into()),
        "zh" | "cn" | "mandarin" | "chinese" | "china" => ("mandarin".into(), "Mandarin (中文)".into()),
        "es" | "spanish" | "spanyol" => ("spanish".into(), "Spanish (Español)".into()),
        "fr" | "french" | "prancis" => ("french".into(), "French (Français)".into()),
        "ar" | "arabic" | "arab" => ("arabic".into(), "Arabic (العربية)".into()),
        _ => {
            let clean: String = low
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            let key = if clean.is_empty() { "english".to_string() } else { clean };
            let title = raw.trim().to_string();
            (key, if title.is_empty() { "English".to_string() } else { title })
        }
    }
}

fn get_syllabus_lesson(unit: u32, lesson: u32) -> Value {
    match (unit, lesson) {
        (1, 1) => json!({
            "unit": "Unit 1: Everyday & Campus Life (A1-A2)",
            "id": "1.1",
            "title": "Warm Greetings, Small Talk & Moods",
            "cefr": "A1",
            "story_hook": "We just bumped into each other in the morning breeze and are catching up before the day starts.",
            "grammar_focus": "Present tense for current feelings & moods",
            "goal": "Practice natural greetings, describing how you feel, and asking back smoothly."
        }),
        (1, 2) => json!({
            "unit": "Unit 1: Everyday & Campus Life (A1-A2)",
            "id": "1.2",
            "title": "Ordering Coffee & Street Food Together",
            "cefr": "A1-A2",
            "story_hook": "It started drizzling outside, so we ducked into a cozy cafe and are looking at the menu together.",
            "grammar_focus": "Polite requests & food/drink quantities",
            "goal": "Order food/drinks politely and customize an order naturally."
        }),
        (1, 3) => json!({
            "unit": "Unit 1: Everyday & Campus Life (A1-A2)",
            "id": "1.3",
            "title": "Daily Routines, Hobbies & Late Nights",
            "cefr": "A2",
            "story_hook": "Relaxing after a long day, talking about daily habits, sleep schedules, and favorite hobbies.",
            "grammar_focus": "Frequency adverbs + habitual tense",
            "goal": "Describe daily routines and hobbies smoothly without word-for-word translation."
        }),
        (2, 1) => json!({
            "unit": "Unit 2: Feelings, Plans & Stories (A2-B1)",
            "id": "2.1",
            "title": "Giving Compliments & Reacting Naturally",
            "cefr": "A2",
            "story_hook": "Getting ready to head out together and trading playful compliments and banter.",
            "grammar_focus": "Adjectives, adverbs & natural compliment responses",
            "goal": "Give warm, natural compliments and respond without sounding stiff."
        }),
        (2, 2) => json!({
            "unit": "Unit 2: Feelings, Plans & Stories (A2-B1)",
            "id": "2.2",
            "title": "Making Plans, Invitations & Weekend Hangouts",
            "cefr": "B1",
            "story_hook": "Planning a weekend hangout—deciding between a movie, night market, or quiet picnic.",
            "grammar_focus": "Future forms & polite invitations",
            "goal": "Propose ideas, accept/decline politely, and negotiate plans."
        }),
        (2, 3) => json!({
            "unit": "Unit 2: Feelings, Plans & Stories (A2-B1)",
            "id": "2.3",
            "title": "Sharing Stories & Past Experiences",
            "cefr": "B1",
            "story_hook": "Sitting over warm drinks at night, sharing funny or memorable moments from the past.",
            "grammar_focus": "Past tense storytelling & sequence connectors",
            "goal": "Tell a short story in the past tense clearly."
        }),
        (3, 1) => json!({
            "unit": "Unit 3: Real-World Situations & Travel (B1)",
            "id": "3.1",
            "title": "At the Airport, Hotel & Asking Directions",
            "cefr": "B1",
            "story_hook": "We just landed in a new city, checking into our hotel and finding a hidden food spot nearby.",
            "grammar_focus": "Indirect polite questions & directions",
            "goal": "Navigate travel situations and ask polite questions."
        }),
        (3, 2) => json!({
            "unit": "Unit 3: Real-World Situations & Travel (B1)",
            "id": "3.2",
            "title": "Shopping, Preferences & Polite Returns",
            "cefr": "B1",
            "story_hook": "Strolling through a shopping district trying on clothes and comparing options.",
            "grammar_focus": "Comparatives, preferences & polite requests",
            "goal": "Compare items and handle shopping/returns naturally."
        }),
        (3, 3) => json!({
            "unit": "Unit 3: Real-World Situations & Travel (B1)",
            "id": "3.3",
            "title": "Handling Problems & Asking for Help",
            "cefr": "B1",
            "story_hook": "A small mishap happened on our trip (dead battery or delayed train) and we figure it out together.",
            "grammar_focus": "Modals of request & necessity",
            "goal": "Stay calm in unexpected situations and ask for help."
        }),
        (4, 1) => json!({
            "unit": "Unit 4: Study, Career & Persuasion (B1-B2)",
            "id": "4.1",
            "title": "Explaining Ideas, Projects & Passions",
            "cefr": "B1-B2",
            "story_hook": "Chatting at a cafe about a project, tech idea, or dream you are excited to build.",
            "grammar_focus": "Cause/effect connectors & structured explanations",
            "goal": "Explain complex ideas clearly in the target language."
        }),
        (4, 2) => json!({
            "unit": "Unit 4: Study, Career & Persuasion (B1-B2)",
            "id": "4.2",
            "title": "Debating Opinions & Nuanced Agreement",
            "cefr": "B2",
            "story_hook": "Having a friendly late-night debate on work-life balance, AI, or travel styles.",
            "grammar_focus": "Expressing nuance, concessions & hypotheticals",
            "goal": "Agree/disagree diplomatically with supporting reasons."
        }),
        _ => json!({
            "unit": "Unit 4: Study, Career & Persuasion (B1-B2)",
            "id": "4.3",
            "title": "Free-Flow Fluency & Storytelling",
            "cefr": "B2+",
            "story_hook": "Open-ended, natural friend-to-friend conversation on any topic you choose.",
            "grammar_focus": "Natural idioms, tone & fluency",
            "goal": "Hold an effortless, natural conversation like a native friend."
        }),
    }
}
