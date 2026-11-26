import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface ClipboardHistoryItem {
  id: string;
  content_type: 'text' | 'image';
  content: string;
  preview: string;
  pinned: boolean;
  created_at: number;
}

export interface NoteChecklistEntry {
  id: string;
  text: string;
  checked: boolean;
}

export interface NoteItem {
  id: string;
  title: string;
  content: string;
  note_type: 'note' | 'checklist';
  checklist: NoteChecklistEntry[];
  images: string[];
  color: string;
  labels: string[];
  pinned: boolean;
  archived: boolean;
  created_at: number;
  updated_at: number;
}

export interface DiskTelemetry {
  name: string;
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  usage_percent: number;
}

export interface SystemTelemetry {
  cpu_usage_percent: number;
  cpu_brand: string;
  cpu_cores_physical: number;
  cpu_cores_logical: number;
  cpu_freq_mhz: number;
  ram_used_bytes: number;
  ram_total_bytes: number;
  ram_usage_percent: number;
  swap_used_bytes: number;
  swap_total_bytes: number;
  gpu_name: string;
  gpu_usage_percent: number;
  net_rx_bytes_per_sec: number;
  net_tx_bytes_per_sec: number;
  hostname: string;
  os_name: string;
  os_version: string;
  kernel_version: string;
  arch: string;
  uptime_secs: number;
  cpu_temp_celsius?: number | null;
  disks: DiskTelemetry[];
}

export interface DailyTransferStat {
  day_epoch_secs: number;
  sent_bytes: number;
  received_bytes: number;
}

export interface TransferAnalytics {
  total_sent_bytes: number;
  total_received_bytes: number;
  total_sent_files: number;
  total_received_files: number;
  sent_sessions: number;
  received_sessions: number;
  failed_sessions: number;
  lan_bytes: number;
  wan_bytes: number;
  http_bytes: number;
  daily: DailyTransferStat[];
}

export interface TelegramAllowedUser {
  user_id: number;
  label: string;
}

export interface AiMcpConfig {
  mcp_enabled: boolean;
  mcp_port: number;
  mcp_auth_token: string;
  llm_provider: string;
  llm_base_url: string;
  llm_api_key: string;
  llm_model: string;
  llm_temperature: number;
  llm_max_tokens: number;
  persona_name: string;
  persona_preset: string;
  persona_language: string;
  persona_system_prompt: string;
  telegram_enabled: boolean;
  telegram_bot_token: string;
  telegram_allowed_users: TelegramAllowedUser[];
}

export interface AiMcpRuntimeStatus {
  mcp: {
    running: boolean;
    port: number;
    url: string;
    last_error?: string | null;
  };
  telegram: {
    running: boolean;
    bot_username?: string | null;
    last_error?: string | null;
  };
}

export interface MediaTrack {
  id: string;
  source_type: 'local' | 'ytmusic';
  title: string;
  artist: string;
  duration_secs: number;
  uri_or_id: string;
  thumbnail_url?: string | null;
  is_video: boolean;
  stream_url?: string | null;
}

export interface MediaPlayerState {
  current_track: MediaTrack | null;
  queue: MediaTrack[];
  current_index: number | null;
  is_playing: boolean;
  position_secs: number;
  duration_secs: number;
  volume: number;
  muted: boolean;
  shuffle: boolean;
  repeat_mode: 'off' | 'all' | 'one';
  video_mode: boolean;
  active_playlist_id: string | null;
}

export interface YtMusicSearchItem {
  video_id: string;
  title: string;
  artist: string;
  album?: string | null;
  duration_text: string;
  duration_secs: number;
  thumbnail_url?: string | null;
  is_video: boolean;
}

export interface ResolvedMediaStream {
  video_id: string;
  stream_url: string;
  is_video: boolean;
}

export interface MediaPlaylist {
  id: string;
  name: string;
  created_at: number;
  updated_at: number;
  item_count: number;
}

export interface MediaPlaylistItem {
  id: string;
  playlist_id: string;
  source_type: 'local' | 'ytmusic';
  title: string;
  artist: string;
  duration_secs: number;
  uri_or_id: string;
  thumbnail_url?: string | null;
  is_video: boolean;
  position: number;
  created_at: number;
}

export interface AppSettings {
  device_id: string;
  alias_name: string;
  user_id: string | null;
  user_email: string | null;
  user_name: string | null;
  auth_token: string | null;
  discoverable: boolean;
  autostart: boolean;
  speed_limit_mbps: number;
  download_dir: string;
  signaling_url: string;
  stun_addr: string;
  auto_categorize?: boolean;
  max_concurrent_downloads?: number;
  theme?: string;
}

export interface AppStateSnapshot {
  settings: AppSettings;
  ed25519_pubkey: string;
  os_type: string;
  local_ip: string;
  mac_address?: string | null;
  listen_port: number;
  wan_reflexive_addr: string | null;
  signaling_connected: boolean;
  queued_cli_files: string[];
  queued_convert_files?: string[];
  my_rank_exp?: number;
}

export interface ImageMeta {
  path: string;
  filename: string;
  width: number;
  height: number;
  size_bytes: number;
  format: string;
}

export interface ConvertResult {
  input_path: string;
  output_path: string;
  original_bytes: number;
  output_bytes: number;
  width: number;
  height: number;
}

export interface MobileIconZipResult {
  zip_path: string;
  total_files: number;
  zip_bytes: number;
}

export interface DocumentEnginesInfo {
  has_word: boolean;
  has_libreoffice: boolean;
}

export interface DocumentConvertResult {
  input_path: string;
  output_path: string;
  original_bytes: number;
  output_bytes: number;
  engine_used: string;
}

export interface LanStatusInfo {
  active: boolean;
  kind: 'local' | 'tailscale' | 'raydrop_mesh' | 'none';
  label: string;
  detail: string;
  ip: string | null;
  local_active: boolean;
  tailscale_active: boolean;
  mesh_active: boolean;
}

export interface Contact {
  device_id: string;
  alias_name: string;
  custom_name?: string | null;
  user_email: string | null;
  user_name: string | null;
  os_type: string | null;
  is_own_device: boolean;
  ed25519_pubkey: string | null;
  network_type: 'lan' | 'wan';
  last_known_ip: string | null;
  last_known_port: number | null;
  trust_level: 'ask' | 'trusted_auto_accept' | 'blocked';
  is_online: boolean;
  last_seen_at: number;
  created_at: number;
  avatar_hash?: string | null;
  rank_exp?: number;
  mac_address?: string | null;
  mesh_status?: 'none' | 'pending_out' | 'pending_in' | 'approved';
}

export interface CloudDeviceNode {
  device_id: string;
  user_id: string;
  user_email: string;
  user_name: string;
  device_name: string;
  os_type: string;
  ed25519_pubkey: string;
  last_ip: string | null;
  is_online: boolean;
  last_seen_at: number;
  created_at: number;
}

export interface CloudContactGroup {
  user_id: string;
  email: string;
  name: string;
  trust_level: 'ask' | 'trusted_auto_accept' | 'blocked';
  is_online: boolean;
  devices: CloudDeviceNode[];
}

export interface CloudStateResponse {
  token?: string;
  user: {
    id: string;
    email: string;
    name: string;
  };
  my_devices: CloudDeviceNode[];
  contacts: CloudContactGroup[];
}

export interface ManifestFileEntry {
  file_id: number;
  relative_path: string;
  size_bytes: number;
  modified_unix_ms: number;
}

export interface IncomingTransferEvent {
  session_id: string;
  sender_device_id: string;
  sender_alias: string;
  network_route: 'lan_direct' | 'wan_p2p';
  total_files: number;
  total_size_bytes: number;
  files: ManifestFileEntry[];
}

export interface TransferRecord {
  id: string;
  peer_device_id: string;
  peer_alias: string;
  direction: 'incoming' | 'outgoing';
  network_route: 'lan_direct' | 'wan_p2p';
  total_files: number;
  total_bytes: number;
  transferred_bytes: number;
  status: 'in_progress' | 'paused' | 'completed' | 'cancelled' | 'failed' | 'rejected';
  target_path: string | null;
  source_paths?: string | null;
  error_reason?: string | null;
  created_at: number;
  speed_mbps?: number;
  eta_seconds?: number;
}

export interface ChatFileItem {
  name: string;
  size: number;
  path?: string;
  thumbnail?: string;
}

export interface ChatMessage {
  id: string;
  peer_device_id: string;
  direction: 'incoming' | 'outgoing';
  content: string;
  msg_type: 'text' | 'file_transfer' | 'clipboard' | string;
  transfer_session_id?: string | null;
  files_json?: string | null;
  is_read: boolean;
  created_at: number;
  sender_device_id?: string | null;
  sender_alias?: string | null;
}

export interface GroupMember {
  device_id: string;
  alias: string;
  avatar?: string | null;
  role: 'owner' | 'admin' | 'member';
  joined_at: number;
}

export interface ChatGroup {
  id: string;
  name: string;
  description?: string | null;
  avatar?: string | null;
  owner_id: string;
  send_permission: 'all' | 'admins_only' | 'custom';
  allowed_senders: string[];
  members: GroupMember[];
  created_at: number;
  updated_at: number;
}

export interface ClipboardSyncedPayload {
  from_device_id: string;
  sender_alias: string;
  content: string;
  auto_copied: boolean;
}

export interface WebShareItem {
  id: number;
  name: string;
  size_bytes: number;
  size_human: string;
}

export interface WebShareInfo {
  url: string;
  local_url?: string;
  share_id?: string;
  local_ip: string;
  port: number;
  total_files: number;
  total_bytes: number;
  total_bytes_human: string;
  files: WebShareItem[];
}

export interface FolderSyncTask {
  id: string;
  local_path: string;
  target_device_id: string;
  target_alias: string;
  is_auto_sync: boolean;
  last_sync_at: number | null;
  created_at: number;
}

export interface SharedFolder {
  id: string;
  name: string;
  local_path: string;
  is_active: boolean;
  created_at: number;
}

export interface RemoteShareEntry {
  id: string;
  name: string;
  is_default: boolean;
}

export interface RemoteFileItem {
  name: string;
  relative_path: string;
  is_dir: boolean;
  size_bytes: number;
  modified_unix_ms: number;
}

export interface TorrentTaskInfo {
  id: string;
  name: string;
  magnet_url: string;
  total_bytes: number;
  downloaded_bytes: number;
  uploaded_bytes: number;
  download_speed_bps: number;
  upload_speed_bps: number;
  progress: number;
  status: 'fetching_metadata' | 'downloading' | 'paused' | 'completed' | 'error' | string;
  peers_connected: number;
  save_path: string;
  error?: string | null;
  created_at: number;
}

export interface HttpDownloadTaskInfo {
  id: string;
  url: string;
  filename: string;
  total_bytes: number;
  downloaded_bytes: number;
  download_speed_bps: number;
  progress: number;
  status: 'starting' | 'downloading' | 'paused' | 'completed' | 'error' | string;
  segments_count: number;
  save_path: string;
  error?: string | null;
  created_at: number;
}

export interface TransferProgressEvent {
  session_id: string;
  peer_device_id: string;
  peer_alias: string;
  direction: 'incoming' | 'outgoing';
  network_route: 'lan_direct' | 'wan_p2p';
  transferred_bytes: number;
  total_bytes: number;
  speed_mbps: number;
  eta_seconds: number;
  status: string;
}

export function getApiBaseUrl(signalingUrl: string): string {
  const trimmed = (signalingUrl || 'wss://stun.raydrop.web.id/ws').trim();
  return trimmed
    .replace(/^wss:\/\//i, 'https://')
    .replace(/^ws:\/\//i, 'http://')
    .replace(/\/ws\/?$/i, '');
}

export interface UpdateCheckResult {
  has_update: boolean;
  current_version: string;
  latest_version: string;
  release_title: string;
  release_notes: string;
  release_date: string;
  html_url: string;
  download_url: string | null;
  asset_name: string | null;
  asset_size: number | null;
  is_building?: boolean;
}

export interface UpdateProgressPayload {
  downloaded: number;
  total: number;
  percent: number;
  speed_bytes_sec: number;
}

export interface RemoteDesktopSettings {
  enabled: boolean;
  pin: string;
  unattended: boolean;
}

export const raydropApi = {
  getAppState: () => invoke<AppStateSnapshot>('get_app_state'),
  saveCloudSession: (params: {
    userId: string;
    userEmail: string;
    userName: string;
    authToken: string;
    deviceName?: string;
  }) => invoke<AppSettings>('save_cloud_session', params),
  logoutCloudSession: () => invoke<AppSettings>('logout_cloud_session'),
  syncCloudDataToLocal: (
    myDevices: CloudDeviceNode[],
    contacts: CloudContactGroup[]
  ) => invoke<Contact[]>('sync_cloud_data_to_local', { myDevices, contacts }),
  deleteLocalContact: (params: { deviceId?: string; email?: string }) =>
    invoke<void>('delete_local_contact', params),
  toggleDiscoverability: (enabled: boolean) =>
    invoke<boolean>('toggle_discoverability', { enabled }),
  toggleAutostart: (enabled: boolean) =>
    invoke<boolean>('toggle_autostart', { enabled }),
  updateSettings: (params: {
    aliasName: string;
    downloadDir: string;
    signalingUrl: string;
    stunAddr: string;
    autostart?: boolean;
    speedLimitMbps?: number;
    autoCategorize?: boolean;
    maxConcurrentDownloads?: number;
  }) => invoke<AppSettings>('update_settings', params),
  getContacts: () => invoke<Contact[]>('get_contacts'),
  addWanContact: (params: {
    deviceId: string;
    aliasName: string;
    directIpPort?: string | null;
  }) => invoke<Contact>('add_wan_contact', params),
  updateContactTrust: (
    deviceId: string,
    trustLevel: Contact['trust_level'],
    userEmail?: string | null
  ) => invoke<void>('update_contact_trust', { deviceId, trustLevel, userEmail }),
  initiateTransfer: (targetDeviceId: string, filePaths: string[]) =>
    invoke<string>('initiate_transfer', { targetDeviceId, filePaths }),
  resumeTransfer: (sessionId: string) =>
    invoke<string>('resume_transfer', { sessionId }),
  pauseTransfer: (sessionId: string) =>
    invoke<void>('pause_transfer', { sessionId }),
  respondTransferRequest: (
    sessionId: string,
    accepted: boolean,
    targetDirectory?: string | null
  ) =>
    invoke<void>('respond_transfer_request', {
      sessionId,
      accepted,
      targetDirectory,
    }),
  cancelTransfer: (sessionId: string) =>
    invoke<void>('cancel_transfer', { sessionId }),
  getTransferHistory: () => invoke<TransferRecord[]>('get_transfer_history'),
  deleteTransferRecord: (id: string) => invoke<void>('delete_transfer_record', { id }),
  clearTransferHistory: () => invoke<void>('clear_transfer_history'),
  registerOsContextMenu: () => invoke<string>('register_os_context_menu'),
  getWin11DirectMenu: () => invoke<boolean>('get_win11_direct_menu'),
  toggleWin11DirectMenu: (enabled: boolean) =>
    invoke<boolean>('toggle_win11_direct_menu', { enabled }),
  openFilePath: (path: string) => invoke<void>('open_file_path', { path }),
  openFolderPath: (path: string) => invoke<void>('open_folder_path', { path }),
  checkAppUpdate: () => invoke<UpdateCheckResult>('check_app_update'),
  downloadAndInstallUpdate: (downloadUrl: string, assetName: string) =>
    invoke<string>('download_and_install_update', { downloadUrl, assetName }),
  sendChatMessage: (
    peerDeviceId: string,
    content: string,
    msgType?: string,
    transferSessionId?: string | null,
    filesJson?: string | null
  ) =>
    invoke<ChatMessage>('send_chat_message', {
      peerDeviceId,
      content,
      msgType,
      transferSessionId,
      filesJson,
    }),
  getChatMessages: (peerDeviceId: string, limit?: number, offset?: number) =>
    invoke<ChatMessage[]>('get_chat_messages', { peerDeviceId, limit, offset }),
  markChatMessagesRead: (peerDeviceId: string) =>
    invoke<void>('mark_chat_messages_read', { peerDeviceId }),
  clearChatMessages: (peerDeviceId: string) =>
    invoke<void>('clear_chat_messages', { peerDeviceId }),
  getChatThreadPeers: () =>
    invoke<[string, string | null, number][]>('get_chat_thread_peers'),
  getUnreadChatCounts: () =>
    invoke<Record<string, number>>('get_unread_chat_counts'),
  createChatGroup: (
    name: string,
    description?: string | null,
    avatar?: string | null,
    memberDeviceIds: string[] = []
  ) =>
    invoke<ChatGroup>('create_chat_group', {
      name,
      description,
      avatar,
      memberDeviceIds,
    }),
  getChatGroups: () => invoke<ChatGroup[]>('get_chat_groups'),
  getChatGroup: (groupId: string) =>
    invoke<ChatGroup | null>('get_chat_group', { groupId }),
  updateChatGroupDetails: (
    groupId: string,
    name: string,
    description?: string | null,
    avatar?: string | null
  ) =>
    invoke<ChatGroup>('update_chat_group_details', {
      groupId,
      name,
      description,
      avatar,
    }),
  inviteGroupMembers: (groupId: string, newDeviceIds: string[]) =>
    invoke<ChatGroup>('invite_group_members', { groupId, newDeviceIds }),
  removeGroupMember: (groupId: string, targetDeviceId: string) =>
    invoke<ChatGroup>('remove_group_member', { groupId, targetDeviceId }),
  setMemberRole: (groupId: string, targetDeviceId: string, newRole: string) =>
    invoke<ChatGroup>('set_member_role', { groupId, targetDeviceId, newRole }),
  setGroupSendPermission: (
    groupId: string,
    sendPermission: string,
    allowedSenders?: string[]
  ) =>
    invoke<ChatGroup>('set_group_send_permission', {
      groupId,
      sendPermission,
      allowedSenders,
    }),
  leaveChatGroup: (groupId: string) =>
    invoke<void>('leave_chat_group', { groupId }),
  sendGroupChatMessage: (
    groupId: string,
    content: string,
    msgType?: string,
    filesJson?: string | null
  ) =>
    invoke<ChatMessage>('send_group_chat_message', {
      groupId,
      content,
      msgType,
      filesJson,
    }),
  updateTrayBadge: (count: number) =>
    invoke<void>('update_tray_badge', { count }),
  openExternalUrl: (url: string) =>
    invoke<void>('open_external_url', { url }),
  getFileThumbnail: (path: string) =>
    invoke<string>('get_file_thumbnail', { path }),
  startWebShare: (paths: string[]) =>
    invoke<WebShareInfo>('start_web_share', { paths }),
  stopWebShare: () => invoke<void>('stop_web_share'),
  readSystemClipboard: () => invoke<string>('read_system_clipboard'),
  readSystemClipboardIfChanged: (lastSeq: number) =>
    invoke<[number, string] | null>('read_system_clipboard_if_changed', { lastSeq }),
  getClipboardSequenceNumber: () => invoke<number>('get_clipboard_sequence_number'),
  listClipboardHistory: () => invoke<ClipboardHistoryItem[]>('list_clipboard_history'),
  copyClipboardHistoryItem: (id: string) => invoke<void>('copy_clipboard_history_item', { id }),
  togglePinClipboardItem: (id: string) => invoke<boolean>('toggle_pin_clipboard_item', { id }),
  deleteClipboardHistoryItem: (id: string) => invoke<void>('delete_clipboard_history_item', { id }),
  clearClipboardHistory: () => invoke<void>('clear_clipboard_history'),
  listNotes: (includeArchived: boolean) => invoke<NoteItem[]>('list_notes', { includeArchived }),
  saveNote: (note: NoteItem) => invoke<NoteItem>('save_note', { note }),
  togglePinNote: (id: string) => invoke<boolean>('toggle_pin_note', { id }),
  toggleArchiveNote: (id: string) => invoke<boolean>('toggle_archive_note', { id }),
  deleteNote: (id: string) => invoke<void>('delete_note', { id }),
  writeSystemClipboard: (content: string) =>
    invoke<void>('write_system_clipboard', { content, text: content }),
  saveBase64Image: (dataUrl: string, customFilename?: string) =>
    invoke<string>('save_base64_image', {
      dataUrl,
      customFilename: customFilename ?? null,
    }),
  broadcastClipboard: (content: string, targetDeviceIds?: string[]) =>
    invoke<number>('broadcast_clipboard', {
      content,
      targetDeviceIds: targetDeviceIds ?? null,
    }),
  setSpeedLimit: (speedLimitMbps: number) =>
    invoke<number>('set_speed_limit', { speedLimitMbps }),
  listFolderSyncTasks: () =>
    invoke<FolderSyncTask[]>('list_folder_sync_tasks'),
  addFolderSyncTask: (params: {
    localPath: string;
    targetDeviceId: string;
    isAutoSync: boolean;
  }) => invoke<FolderSyncTask>('add_folder_sync_task', params),
  toggleFolderSyncAuto: (taskId: string, isAuto: boolean) =>
    invoke<void>('toggle_folder_sync_auto', { taskId, isAuto }),
  deleteFolderSyncTask: (taskId: string) =>
    invoke<void>('delete_folder_sync_task', { taskId }),
  triggerFolderSync: (taskId: string) =>
    invoke<string>('trigger_folder_sync', { taskId }),
  addTorrentMagnet: (magnetUrl: string, customPath?: string) =>
    invoke<TorrentTaskInfo>('add_torrent_magnet', {
      magnetUrl,
      customPath: customPath ?? null,
    }),
  listTorrentTasks: () =>
    invoke<TorrentTaskInfo[]>('list_torrent_tasks'),
  pauseTorrentTask: (id: string) =>
    invoke<void>('pause_torrent_task', { id }),
  resumeTorrentTask: (id: string) =>
    invoke<void>('resume_torrent_task', { id }),
  deleteTorrentTask: (id: string, deleteFiles: boolean) =>
    invoke<void>('delete_torrent_task', { id, deleteFiles }),
  getPendingMagnetLink: () =>
    invoke<string | null>('get_pending_magnet_link'),
  addHttpDownload: (url: string, customSaveDir?: string, customFilename?: string) =>
    invoke<HttpDownloadTaskInfo>('add_http_download', {
      url,
      customSaveDir: customSaveDir ?? null,
      customFilename: customFilename ?? null,
    }),
  listHttpDownloads: () =>
    invoke<HttpDownloadTaskInfo[]>('list_http_downloads'),
  pauseHttpDownload: (id: string) =>
    invoke<void>('pause_http_download', { id }),
  resumeHttpDownload: (id: string) =>
    invoke<void>('resume_http_download', { id }),
  updateHttpDownloadUrl: (id: string, newUrl: string) =>
    invoke<HttpDownloadTaskInfo>('update_http_download_url', { id, newUrl }),
  deleteHttpDownload: (id: string, deleteFile: boolean) =>
    invoke<void>('delete_http_download', { id, deleteFile }),
  startHttpDownloadNow: (id: string) =>
    invoke<void>('start_http_download_now', { id }),
  executeSystemPowerAction: (action: 'shutdown' | 'sleep' | 'hibernate') =>
    invoke<void>('execute_system_power_action', { action }),
  calculateFileHash: (path: string) =>
    invoke<string>('calculate_file_hash', { path }),
  getDirectListenAddress: () =>
    invoke<{ ip: string; port: number }>('get_direct_listen_address'),
  connectPeerByIp: (ip: string, port?: number | null) =>
    invoke<Contact>('connect_peer_by_ip', { ip, port: port ?? null }),
  openBrowserExtensionDir: () =>
    invoke<string>('open_browser_extension_dir'),
  openBrowserExtensionUrl: (browser: string) =>
    invoke<void>('open_browser_extension_url', { browser }),
  setMyAvatar: (imageData: string) =>
    invoke<string>('set_my_avatar', { imageData }),
  getMyAvatar: () =>
    invoke<string | null>('get_my_avatar'),
  getPeerAvatar: (peerDeviceId: string) =>
    invoke<string | null>('get_peer_avatar', { peerDeviceId }),
  requestPeerAvatar: (peerDeviceId: string) =>
    invoke<void>('request_peer_avatar', { peerDeviceId }),
  setContactCustomName: (deviceId: string, customName: string) =>
    invoke<Contact>('set_contact_custom_name', { deviceId, customName }),
  listSharedFolders: () =>
    invoke<SharedFolder[]>('list_shared_folders'),
  addSharedFolder: (name: string, localPath: string) =>
    invoke<SharedFolder>('add_shared_folder', { name, localPath }),
  deleteSharedFolder: (folderId: string) =>
    invoke<void>('delete_shared_folder', { folderId }),
  browseRemoteShares: (peerDeviceId: string) =>
    invoke<RemoteShareEntry[]>('browse_remote_shares', { peerDeviceId }),
  browseRemoteDir: (peerDeviceId: string, shareId: string, subpath: string) =>
    invoke<RemoteFileItem[]>('browse_remote_dir', { peerDeviceId, shareId, subpath }),
  pullRemoteFile: (peerDeviceId: string, shareId: string, relativePath: string) =>
    invoke<string>('pull_remote_file', { peerDeviceId, shareId, relativePath }),
  sendMeetingSignal: (targetDeviceId: string, sessionId: string, payload: any) =>
    invoke<void>('send_meeting_signal', { targetDeviceId, sessionId, payload }),
  getRemoteDesktopSettings: () =>
    invoke<RemoteDesktopSettings>('get_remote_desktop_settings'),
  saveRemoteDesktopSettings: (enabled: boolean, pin: string, unattended: boolean) =>
    invoke<void>('save_remote_desktop_settings', { enabled, pin, unattended }),
  verifyRemoteDesktopPin: (pin: string) =>
    invoke<boolean>('verify_remote_desktop_pin', { pin }),
  simulateRemoteMouseMove: (normX: number, normY: number) =>
    invoke<void>('simulate_remote_mouse_move', { normX, normY }),
  simulateRemoteMouseMoveRelative: (dx: number, dy: number) =>
    invoke<void>('simulate_remote_mouse_move_relative', { dx, dy }),
  simulateRemoteGamepad: (
    buttons: number,
    lx: number,
    ly: number,
    rx: number,
    ry: number,
    lt: number,
    rt: number
  ) =>
    invoke<void>('simulate_remote_gamepad', { buttons, lx, ly, rx, ry, lt, rt }),
  requestDirectMesh: (deviceId: string) =>
    invoke<void>('request_direct_mesh', { deviceId }),
  respondDirectMesh: (deviceId: string, accept: boolean) =>
    invoke<void>('respond_direct_mesh', { deviceId, accept }),
  revokeDirectMesh: (deviceId: string) =>
    invoke<void>('revoke_direct_mesh', { deviceId }),
  getLanConnectionStatuses: () =>
    invoke<Record<string, LanStatusInfo>>('get_lan_connection_statuses'),
  getVirtualDisplayStatus: () =>
    invoke<{
      supported: boolean;
      installed: boolean;
      enabled: boolean;
      status_text: string;
    }>('get_virtual_display_status'),
  setVirtualDisplayEnabled: (enabled: boolean) =>
    invoke<{
      supported: boolean;
      installed: boolean;
      enabled: boolean;
      status_text: string;
    }>('set_virtual_display_enabled', { enabled }),
  setClientKeyboardLock: (enabled: boolean) =>
    invoke<void>('set_client_keyboard_lock', { enabled }),
  simulateRemoteMouseClick: (
    button: string,
    action: string,
    alt?: boolean,
    ctrl?: boolean,
    shift?: boolean,
    meta?: boolean
  ) =>
    invoke<void>('simulate_remote_mouse_click', { button, action, alt, ctrl, shift, meta }),
  simulateRemoteMouseWheel: (deltaY: number) =>
    invoke<void>('simulate_remote_mouse_wheel', { deltaY }),
  simulateRemoteKeyboardKey: (key: string, code: string, action: string) =>
    invoke<void>('simulate_remote_keyboard_key', { key, code, action }),
  simulateRemoteShortcut: (shortcut: string) =>
    invoke<void>('simulate_remote_shortcut', { shortcut }),
  releaseRemoteModifiers: () =>
    invoke<void>('release_remote_modifiers'),
  setHostCursorHidden: (hidden: boolean) =>
    invoke<void>('set_host_cursor_hidden', { hidden }),
  getHostScreenSize: () =>
    invoke<[number, number]>('get_host_screen_size'),
  getHostMonitors: () =>
    invoke<{ index: number; name: string; x: number; y: number; width: number; height: number; is_primary: boolean }[]>('get_host_monitors'),
  setHostTargetMonitor: (monitorIndex: number | null) =>
    invoke<void>('set_host_target_monitor', { monitorIndex }),
  getMonitorResolutions: (monitorIndex: number) =>
    invoke<[number, number][]>('get_monitor_resolutions', { monitorIndex }),
  setMonitorResolution: (monitorIndex: number, width: number, height: number) =>
    invoke<void>('set_monitor_resolution', { monitorIndex, width, height }),
  getExtendPlatformInfo: () =>
    invoke<{
      platform: string;
      session_type: string;
      has_xrandr: boolean;
      has_nvidia: boolean;
      virtual_active: boolean;
      virtual_output: string | null;
    }>('get_extend_platform_info'),
  enableLinuxVirtualDisplay: (width = 1280, height = 720) =>
    invoke<{
      index: number;
      name: string;
      device_name: string;
      x: number;
      y: number;
      width: number;
      height: number;
      is_primary: boolean;
    }>('enable_linux_virtual_display', { width, height }),
  disableLinuxVirtualDisplay: () =>
    invoke<void>('disable_linux_virtual_display'),
  executeRemotePowerAction: (action: string) =>
    invoke<void>('execute_remote_power_action', { action }),
  sendWakeOnLan: (macAddress: string, targetHost?: string, port?: number) =>
    invoke<void>('send_wake_on_lan', {
      macAddress,
      targetHost: targetHost?.trim() ? targetHost.trim() : null,
      port: port && port > 0 ? port : null,
    }),
  startHostAudioLoopback: () =>
    invoke<void>('start_host_audio_loopback'),
  stopHostAudioLoopback: () =>
    invoke<void>('stop_host_audio_loopback'),
  startSecondScreenServer: (nativeCapture = false, monitorIndex = 0) =>
    invoke<{ url: string; token: string; port: number }>('start_second_screen_server', {
      nativeCapture,
      monitorIndex,
    }),
  setSecondScreenSource: (nativeCapture: boolean, monitorIndex?: number) =>
    invoke<void>('set_second_screen_source', { nativeCapture, monitorIndex }),
  stopSecondScreenServer: () =>
    invoke<void>('stop_second_screen_server'),
  pushSecondScreenFrame: (frameBase64: string) =>
    invoke<void>('push_second_screen_frame', { frameBase64 }),
  recordActivityTick: (activeFeatures: string[], seconds: number) =>
    invoke<number>('record_activity_tick', { activeFeatures, seconds }),
  openSystemDisplaySettings: () =>
    invoke<void>('open_system_display_settings'),
  startNativeScreenCapture: () =>
    invoke<void>('start_native_screen_capture'),
  stopNativeScreenCapture: () =>
    invoke<void>('stop_native_screen_capture'),
  ackNativeScreenFrame: () =>
    invoke<void>('ack_native_screen_frame'),
  isSecureDesktopActive: () =>
    invoke<boolean>('is_secure_desktop_active'),
  setUiTheme: (theme: string) =>
    invoke<string>('set_ui_theme', { theme }),
  getSystemTelemetry: () =>
    invoke<SystemTelemetry>('get_system_telemetry'),
  getTransferAnalytics: () =>
    invoke<TransferAnalytics>('get_transfer_analytics'),
  getImageMetadata: (path: string) =>
    invoke<ImageMeta>('get_image_metadata', { path }),
  convertAndProcessImages: (params: {
    inputPaths: string[];
    targetFormat: string;
    resizeWidth?: number | null;
    resizeHeight?: number | null;
    keepAspect: boolean;
    quality?: number | null;
    outputDir?: string | null;
  }) =>
    invoke<ConvertResult[]>('convert_and_process_images', {
      inputPaths: params.inputPaths,
      targetFormat: params.targetFormat,
      resizeWidth: params.resizeWidth || null,
      resizeHeight: params.resizeHeight || null,
      keepAspect: params.keepAspect,
      quality: params.quality || null,
      outputDir: params.outputDir || null,
    }),
  generateMobileIconsZip: (params: {
    inputPath: string;
    platform: 'both' | 'android' | 'ios';
    outputZipPath?: string | null;
  }) =>
    invoke<MobileIconZipResult>('generate_mobile_icons_zip', {
      inputPath: params.inputPath,
      platform: params.platform,
      outputZipPath: params.outputZipPath || null,
    }),
  detectDocumentEngines: () =>
    invoke<DocumentEnginesInfo>('detect_document_engines'),
  convertDocuments: (params: {
    inputPaths: string[];
    targetFormat: 'pdf' | 'docx';
    outputDir?: string | null;
  }) =>
    invoke<DocumentConvertResult[]>('convert_documents', {
      inputPaths: params.inputPaths,
      targetFormat: params.targetFormat,
      outputDir: params.outputDir || null,
    }),
  checkUnattendedBootService: () =>
    invoke<boolean>('check_unattended_boot_service'),
  installUnattendedBootService: () =>
    invoke<string>('install_unattended_boot_service'),
  checkWindowsServiceStatus: () =>
    invoke<string>('check_windows_service_status'),
  installWindowsService: () =>
    invoke<string>('install_windows_service'),
  uninstallWindowsService: () =>
    invoke<string>('uninstall_windows_service'),
  mediaSearchYtmusic: (query: string, filter: 'all' | 'songs' | 'videos' = 'all') =>
    invoke<YtMusicSearchItem[]>('media_search_ytmusic', { query, filter }),
  mediaResolveStream: (videoId: string, preferVideo = false) =>
    invoke<ResolvedMediaStream>('media_resolve_stream', { videoId, preferVideo }),
  mediaGetState: () => invoke<MediaPlayerState>('media_get_state'),
  mediaSyncFrontendState: (playerState: MediaPlayerState) =>
    invoke<void>('media_sync_frontend_state', { playerState }),
  mediaDispatchCommand: (action: string, payload?: unknown) =>
    invoke<void>('media_dispatch_command', { action, payload: payload ?? null }),
  mediaListPlaylists: () => invoke<MediaPlaylist[]>('media_list_playlists'),
  mediaCreatePlaylist: (name: string) =>
    invoke<MediaPlaylist>('media_create_playlist', { name }),
  mediaRenamePlaylist: (id: string, name: string) =>
    invoke<void>('media_rename_playlist', { id, name }),
  mediaDeletePlaylist: (id: string) =>
    invoke<void>('media_delete_playlist', { id }),
  mediaListPlaylistItems: (playlistId: string) =>
    invoke<MediaPlaylistItem[]>('media_list_playlist_items', { playlistId }),
  mediaAddPlaylistItem: (params: {
    playlistId: string;
    sourceType: 'local' | 'ytmusic';
    title: string;
    artist: string;
    durationSecs: number;
    uriOrId: string;
    thumbnailUrl?: string | null;
    isVideo: boolean;
  }) => invoke<MediaPlaylistItem>('media_add_playlist_item', params),
  mediaRemovePlaylistItem: (itemId: string) =>
    invoke<void>('media_remove_playlist_item', { itemId }),
  getAiMcpConfig: () => invoke<AiMcpConfig>('get_ai_mcp_config'),
  saveAiMcpConfig: (config: AiMcpConfig) =>
    invoke<AiMcpConfig>('save_ai_mcp_config', { config }),
  getAiMcpRuntimeStatus: () => invoke<AiMcpRuntimeStatus>('get_ai_mcp_runtime_status'),
  testLlmConnection: (config: AiMcpConfig) =>
    invoke<string>('test_llm_connection_cmd', { config }),
  verifyTelegramToken: (token: string) =>
    invoke<string>('verify_telegram_token_cmd', { token }),
  listenEvent: <T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> =>
    listen<T>(event, (e) => handler(e.payload)),
};

export const cloudApi = {
  async register(
    baseUrl: string,
    payload: {
      email: string;
      name: string;
      password: string;
      device_id: string;
      device_name: string;
      os_type: string;
      ed25519_pubkey: string;
    }
  ): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/auth/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Registration failed');
    return data;
  },

  async login(
    baseUrl: string,
    payload: {
      email: string;
      password: string;
      device_id: string;
      device_name: string;
      os_type: string;
      ed25519_pubkey: string;
    }
  ): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Login failed');
    return data;
  },

  async getState(baseUrl: string, token: string): Promise<CloudStateResponse> {
    const res = await fetch(`${baseUrl}/api/state`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to fetch cloud state');
    return data;
  },

  async renameDevice(
    baseUrl: string,
    token: string,
    deviceId: string,
    deviceName: string
  ): Promise<{ my_devices: CloudDeviceNode[] }> {
    const res = await fetch(`${baseUrl}/api/devices/${encodeURIComponent(deviceId)}`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ device_name: deviceName }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to rename device');
    return data;
  },

  async deleteDevice(
    baseUrl: string,
    token: string,
    deviceId: string
  ): Promise<{ my_devices: CloudDeviceNode[] }> {
    const res = await fetch(`${baseUrl}/api/devices/${encodeURIComponent(deviceId)}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to delete device');
    return data;
  },

  async addContactByEmail(
    baseUrl: string,
    token: string,
    email: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ email }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to add contact');
    return data;
  },

  async updateContactTrust(
    baseUrl: string,
    token: string,
    email: string,
    trustLevel: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts`, {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({ email, trust_level: trustLevel }),
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to update contact trust');
    return data;
  },

  async deleteContactByEmail(
    baseUrl: string,
    token: string,
    email: string
  ): Promise<{ contacts: CloudContactGroup[] }> {
    const res = await fetch(`${baseUrl}/api/contacts/${encodeURIComponent(email)}`, {
      method: 'DELETE',
      headers: { Authorization: `Bearer ${token}` },
    });
    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Failed to remove contact');
    return data;
  },
};

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(i === 0 ? 0 : 2)} ${sizes[i]}`;
}

export function formatEta(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !isFinite(seconds) || seconds <= 0) {
    return '--';
  }
  const s = Math.round(seconds);
  if (s < 60) {
    return `${s}s`;
  }
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const secs = s % 60;

  if (days > 0) {
    return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  }
  if (hours > 0) {
    return minutes > 0 ? `${hours}h ${minutes}m` : `${hours}h`;
  }
  if (minutes > 0) {
    return secs > 0 ? `${minutes}m ${secs}s` : `${minutes}m`;
  }
  return `${secs}s`;
}
