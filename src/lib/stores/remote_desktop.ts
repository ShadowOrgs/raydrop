import { writable, get } from 'svelte/store';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { raydropApi, type RemoteDesktopSettings } from '../utils/tauri-ipc';
import { appSettings } from './state';
import { contacts } from './contacts';
import { playAppSound } from './presence';

// ICE Servers for WebRTC peer connection (STUN + OpenRelay TURN/TURNS fallback for WAN/CGNAT/Tailscale)
const ICE_SERVERS: RTCConfiguration = {
  iceServers: [
    { urls: 'stun:stun.raydrop.web.id:3478' },
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
    {
      urls: 'turn:openrelay.metered.ca:80',
      username: 'openrelayproject',
      credential: 'openrelayproject',
    },
    {
      urls: 'turn:openrelay.metered.ca:443',
      username: 'openrelayproject',
      credential: 'openrelayproject',
    },
    {
      urls: 'turns:openrelay.metered.ca:443?transport=tcp',
      username: 'openrelayproject',
      credential: 'openrelayproject',
    },
  ],
  iceCandidatePoolSize: 6,
};

export interface RemoteSessionInfo {
  isHost: boolean;
  isClient: boolean;
  peerDeviceId: string;
  peerAlias: string;
  sessionId: string;
}

export const remoteHostSettings = writable<RemoteDesktopSettings>({
  enabled: true,
  pin: '123456',
  unattended: true,
});

export interface RemoteAuthRequest {
  clientDeviceId: string;
  sessionId: string;
  fromAlias: string;
}

export const pendingRemoteAuthRequest = writable<RemoteAuthRequest | null>(null);

export const isHostSharingScreen = writable(false);
export const hostScreenStream = writable<MediaStream | null>(null);
export const clientRemoteStream = writable<MediaStream | null>(null);
export const activeRemoteSession = writable<RemoteSessionInfo | null>(null);
export const remoteConnectionStatus = writable<
  'idle' | 'connecting' | 'verifying_pin' | 'connected' | 'disconnected' | 'error'
>('idle');
export const remoteErrorMessage = writable<string | null>(null);
export const remoteLatencyMs = writable<number>(0);
export const remoteStreamFps = writable<number>(0);
export const remoteStreamMbps = writable<number>(0);
export const remoteQualityMode = writable<'60fps' | 'balanced' | 'low_bandwidth'>('60fps');
export const remoteHostMonitors = writable<
  { index: number; name: string; x: number; y: number; width: number; height: number; is_primary: boolean }[]
>([]);
export const activeMonitorIndex = writable<number>(0);
export const remoteAudioEnabled = writable<boolean>(true);
export const remoteCursorShape = writable<string>('default');
export const remoteSessionMode = writable<'desktop' | 'game'>('desktop');
export const activeGamepadCount = writable<number>(0);
export const remoteDebugInfo = writable<string>('');

let activePeerConnection: RTCPeerConnection | null = null;
let activeDataChannel: RTCDataChannel | null = null;
let activeVideoSender: RTCRtpSender | null = null;
let pingInterval: any = null;
let authRetryTimer: any = null;
let pendingIceCandidates: RTCIceCandidateInit[] = [];
let isListenerInitialized = false;
let authTimeoutTimer: any = null;
let authRetryTimer2: any = null;
let iceWatchdogTimer: any = null;
const recentSignalKeys = new Set<string>();

function isDuplicateSignal(sessionId: string, type: string): boolean {
  if (type === 'remote_webrtc_candidate') return false;
  const key = `${sessionId}:${type}`;
  if (recentSignalKeys.has(key)) return true;
  recentSignalKeys.add(key);
  setTimeout(() => recentSignalKeys.delete(key), 30_000);
  return false;
}

function clearAuthTimer() {
  if (authTimeoutTimer) {
    clearTimeout(authTimeoutTimer);
    authTimeoutTimer = null;
  }
  if (authRetryTimer) {
    clearTimeout(authRetryTimer);
    authRetryTimer = null;
  }
  if (authRetryTimer2) {
    clearTimeout(authRetryTimer2);
    authRetryTimer2 = null;
  }
}

let isSettingsLoaded = false;
// Load persisted settings from SQLite backend
export async function loadRemoteDesktopSettings() {
  try {
    const settings = await raydropApi.getRemoteDesktopSettings();
    if (settings) {
      remoteHostSettings.set(settings);
      isSettingsLoaded = true;
    }
  } catch (err) {
    console.warn('Failed to load remote desktop settings:', err);
  }
}

// Save settings to SQLite backend
export async function saveRemoteDesktopSettings(enabled: boolean, pin: string, unattended: boolean) {
  try {
    await raydropApi.saveRemoteDesktopSettings(enabled, pin, unattended);
    remoteHostSettings.set({ enabled, pin, unattended });
  } catch (err: any) {
    console.error('Failed to save remote desktop settings:', err);
    throw err;
  }
}

// Send remote signal via LAN TCP or WAN relay
function sendRemoteSignal(targetDeviceId: string, sessionId: string, payload: any) {
  raydropApi
    .sendMeetingSignal(targetDeviceId, `remote:${sessionId}`, payload)
    .catch((err) => {
      console.warn(`Failed to send remote signal to ${targetDeviceId}:`, err);
    });
}

// ----------------- Global Remote Signal Listener -----------------
export function initRemoteDesktopListener() {
  if (isListenerInitialized) return;
  isListenerInitialized = true;

  raydropApi.listenEvent<{
    from_device_id: string;
    session_id: string;
    payload: any;
  }>('meeting_signal_received', async (event) => {
    const { from_device_id, session_id, payload } = event;
    if (!session_id || !session_id.startsWith('remote:') || !payload) return;

    const rawSessionId = session_id.replace(/^remote:/, '');
    const currentSession = get(activeRemoteSession);

    // Deduplicate dual-path (WAN Relay + LAN TCP) signaling packets
    if (isDuplicateSignal(rawSessionId, payload.type)) {
      return;
    }

    // 1. Host receives authentication request from a client
    if (payload.type === 'remote_request_auth') {
      await handleIncomingAuthRequest(from_device_id, rawSessionId, payload);
      return;
    }

    // 2. Client receives authentication rejection
    if (payload.type === 'remote_auth_rejected') {
      clearAuthTimer();
      cleanupPeerConnection();
      activeRemoteSession.set(null);
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(payload.reason || 'Remote connection rejected by host.');
      return;
    }

    // 3. Client receives authentication acceptance -> awaiting WebRTC offer
    if (payload.type === 'remote_auth_accepted') {
      clearAuthTimer();
      remoteConnectionStatus.set('connecting');
      return;
    }

    // 4. Client receives WebRTC offer from host
    if (payload.type === 'remote_webrtc_offer') {
      await handleIncomingOffer(from_device_id, rawSessionId, payload.sdp);
      return;
    }

    // 5. Host receives WebRTC answer from client
    if (payload.type === 'remote_webrtc_answer') {
      await handleIncomingAnswer(payload.sdp);
      return;
    }

    // 6. ICE candidate exchange
    if (payload.type === 'remote_webrtc_candidate') {
      await handleIncomingIceCandidate(payload.candidate);
      return;
    }

    // 7. Session terminated by peer (only if it matches the currently active session)
    if (payload.type === 'remote_session_ended') {
      if (currentSession && currentSession.sessionId === rawSessionId) {
        disconnectRemoteSession();
      }
      return;
    }

    // 8. Fallback signaling channel for clipboard read reply
    if (payload.type === 'remote_clip_read_res') {
      if (payload.content) {
        await raydropApi.writeSystemClipboard(payload.content).catch(() => { });
      }
    }
  });

  // Forward native WASAPI host audio loopback chunks over WebRTC DataChannel
  raydropApi.listenEvent<string>('host_audio_chunk', (b64) => {
    const sess = get(activeRemoteSession);
    if (sess?.isHost && activeDataChannel && activeDataChannel.readyState === 'open' && b64) {
      // Drop stale audio frames if DataChannel is congested so input & audio don't build multi-second lag
      if (activeDataChannel.bufferedAmount > 256 * 1024) return;
      try {
        activeDataChannel.send(JSON.stringify({ t: 'aud', d: b64 }));
      } catch (_) { }
    }
  });

  // Forward host OS cursor shape changes (text I-beam, hand pointer, resize arrows, move, etc.)
  raydropApi.listenEvent<string>('host_cursor_shape', (shape) => {
    const sess = get(activeRemoteSession);
    if (sess?.isHost && activeDataChannel && activeDataChannel.readyState === 'open' && shape) {
      try {
        activeDataChannel.send(JSON.stringify({ t: 'cur', s: shape }));
      } catch (_) { }
    }
  });
}

// ----------------- Host Handling Logic -----------------

let silentAudioKeepAlive: AudioContext | null = null;

export function startBackgroundKeepAlive() {
  if (silentAudioKeepAlive) return;
  try {
    const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioContextClass) return;
    const ctx = new AudioContextClass();
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    gain.gain.value = 0.00001; // Inaudible, prevents Windows & Chromium background throttling/freeze on minimize
    osc.connect(gain);
    gain.connect(ctx.destination);
    osc.start();
    silentAudioKeepAlive = ctx;
  } catch (err) {
    console.warn('Could not initialize silent audio keepalive:', err);
  }
}

export function stopBackgroundKeepAlive() {
  if (silentAudioKeepAlive) {
    try {
      silentAudioKeepAlive.close();
    } catch (_) { }
    silentAudioKeepAlive = null;
  }
}

let nativeCanvasEl: HTMLCanvasElement | null = null;
let nativeCanvasCtx: CanvasRenderingContext2D | null = null;
let isNativeFrameListenerSet = false;
let pendingFrameDraw = false;

async function startNativeCanvasStream(): Promise<MediaStream | null> {
  try {
    if (!nativeCanvasEl) {
      let initW = 1920;
      let initH = 1080;
      try {
        const [sw, sh] = await raydropApi.getHostScreenSize();
        if (sw > 0 && sh > 0) {
          const scale = sw > 2560 ? 2560 / sw : 1;
          initW = Math.max(2, Math.round(sw * scale) & ~1);
          initH = Math.max(2, Math.round(sh * scale) & ~1);
        }
      } catch (_) {}
      nativeCanvasEl = document.createElement('canvas');
      nativeCanvasEl.width = initW;
      nativeCanvasEl.height = initH;
      nativeCanvasCtx = nativeCanvasEl.getContext('2d', { alpha: false });
      if (nativeCanvasCtx) {
        nativeCanvasCtx.imageSmoothingEnabled = false;
        nativeCanvasCtx.fillStyle = '#000000';
        nativeCanvasCtx.fillRect(0, 0, initW, initH);
      }
    } else if (!nativeCanvasCtx) {
      nativeCanvasCtx = nativeCanvasEl.getContext('2d', { alpha: false });
    }

    if (!isNativeFrameListenerSet) {
      isNativeFrameListenerSet = true;
      await raydropApi.listenEvent<string>('host_screen_frame', (b64) => {
        if (!nativeCanvasEl || pendingFrameDraw || !b64) {
          raydropApi.ackNativeScreenFrame().catch(() => { });
          return;
        }
        pendingFrameDraw = true;
        const img = new Image();
        img.onload = () => {
          if (nativeCanvasEl) {
            if (nativeCanvasEl.width !== img.width || nativeCanvasEl.height !== img.height) {
              nativeCanvasEl.width = img.width;
              nativeCanvasEl.height = img.height;
              nativeCanvasCtx = nativeCanvasEl.getContext('2d', { alpha: false });
              if (nativeCanvasCtx) {
                nativeCanvasCtx.imageSmoothingEnabled = false;
              }
            }
            if (nativeCanvasCtx) {
              nativeCanvasCtx.drawImage(img, 0, 0);
            }
          }
          img.onload = null;
          img.onerror = null;
          img.src = '';
          pendingFrameDraw = false;
          raydropApi.ackNativeScreenFrame().catch(() => { });
        };
        img.onerror = () => {
          img.onload = null;
          img.onerror = null;
          img.src = '';
          pendingFrameDraw = false;
          raydropApi.ackNativeScreenFrame().catch(() => { });
        };
        img.src = (b64.startsWith('iVBOR') ? 'data:image/png;base64,' : 'data:image/jpeg;base64,') + b64;
      });
    }

    pendingFrameDraw = false;
    await raydropApi.stopNativeScreenCapture().catch(() => { });
    await raydropApi.startNativeScreenCapture();
    const stream = nativeCanvasEl.captureStream(30);
    return stream;
  } catch (err) {
    console.warn('Native zero-prompt screen capture unavailable, falling back to browser picker:', err);
    return null;
  }
}

// Helper to acquire monitor screen stream safely without host cursor:
// 1. Pre-login / Winlogon Lock Screen (--system-agent): Native Windows GDI Capture (switches thread to Winlogon desktop)
// 2. Normal Logged-in Desktop: GPU-accelerated zero-prompt getDisplayMedia (DXGI/WGC, ~2% CPU, 60fps, no cursor blink)
export async function acquireHostScreenStream(): Promise<MediaStream | null> {
  let stream = get(hostScreenStream);
  if (stream && stream.getVideoTracks().length > 0 && stream.getVideoTracks()[0].readyState === 'live') {
    startBackgroundKeepAlive();
    return stream;
  }

  const mode = get(remoteSessionMode);
  const requireNativeWinlogonCapture = await raydropApi.isSecureDesktopActive().catch(() => false);
  const isWindowsHost =
    typeof navigator !== 'undefined' && /Win/i.test(navigator.platform || navigator.userAgent);
  // Use Zero-Prompt Native GDI BitBlt capture on Windows so unattended Remote Desktop
  // NEVER hangs on a browser "Choose what to share" screen-picker modal (including Indonesian Windows / multi-monitor).
  if (requireNativeWinlogonCapture || isWindowsHost) {
    const nativeStream = await startNativeCanvasStream();
    if (nativeStream && nativeStream.getVideoTracks().length > 0) {
      const track = nativeStream.getVideoTracks()[0];
      if ('contentHint' in track) {
        (track as any).contentHint = 'text';
      }
      hostScreenStream.set(nativeStream);
      isHostSharingScreen.set(true);
      startBackgroundKeepAlive();
      return nativeStream;
    }
  }

  // Try GPU-accelerated Browser displayMedia (60fps DXGI/WGC for Game Mode, or fallback)
  try {
    stream = await (navigator.mediaDevices as any).getDisplayMedia({
      video: {
        cursor: 'never', // Completely removes host OS cursor from video capture frames
        displaySurface: 'monitor',
        frameRate: mode === 'game' ? { ideal: 60, max: 60 } : { ideal: 30, max: 30 },
        width: { ideal: 2560, max: 3840 },
        height: { ideal: 1440, max: 2160 },
      },
      audio: false, // Audio is streamed cleanly via native WASAPI loopback (prevents duplicate/clicking WebRTC audio)
      selfBrowserSurface: 'exclude',
      surfaceSwitching: 'include',
      monitorTypeSurfaces: 'include',
    });
  } catch (err) {
    console.warn('Standard monitor displayMedia with audio failed, trying video-only fallback:', err);
    try {
      stream = await (navigator.mediaDevices as any).getDisplayMedia({
        video: {
          cursor: 'never',
          displaySurface: 'monitor',
        },
        audio: false,
        monitorTypeSurfaces: 'include',
      });
    } catch (e2) {
      console.warn('displayMedia unavailable (e.g. Winlogon/pre-login desktop), switching to Native GDI fallback:', e2);
      stream = null;
    }
  }

  // 2. Fallback to Native Windows GDI Capture (Pre-login Winlogon / Lock screen)
  if (!stream || stream.getVideoTracks().length === 0) {
    const nativeStream = await startNativeCanvasStream();
    if (nativeStream && nativeStream.getVideoTracks().length > 0) {
      const track = nativeStream.getVideoTracks()[0];
      if ('contentHint' in track) {
        (track as any).contentHint = 'text';
      }
      hostScreenStream.set(nativeStream);
      isHostSharingScreen.set(true);
      startBackgroundKeepAlive();
      return nativeStream;
    }
    return null;
  }

  const track = stream.getVideoTracks()[0];

  // Prevent users from accidentally sharing only the Raydrop window (which freezes on minimize/maximize)
  const trackSettings = track.getSettings ? track.getSettings() : ({} as any);
  if (trackSettings.displaySurface && trackSettings.displaySurface !== 'monitor') {
    stream.getTracks().forEach((t) => t.stop());
    alert(
      "⚠️ Please choose 'Entire Screen' when sharing your desktop.\n\nSharing only a single window (like Raydrop) causes Windows to stop rendering and freeze frames whenever that window is minimized or maximized, and prevents controlling other apps."
    );
    return null;
  }

  track.onmute = () => {
    console.warn('Screen capture track temporarily muted by OS');
  };
  track.onunmute = () => {
    console.log('Screen capture track unmuted by OS');
  };

  if ('contentHint' in track) {
    // 'detail' keeps HUD/text sharp at 60fps in Game Mode; 'text' keeps crisp subpixel text in Desktop Mode
    (track as any).contentHint = mode === 'game' ? 'detail' : 'text';
  }

  track.onended = () => {
    stopHostScreenCapture();
    disconnectRemoteSession();
  };

  hostScreenStream.set(stream);
  isHostSharingScreen.set(true);
  startBackgroundKeepAlive();
  return stream;
}

export async function startHostScreenCapture(): Promise<boolean> {
  const stream = await acquireHostScreenStream();
  return !!stream;
}

async function handleIncomingAuthRequest(
  clientDeviceId: string,
  sessionId: string,
  payload: any
) {
  if (!isSettingsLoaded) {
    await loadRemoteDesktopSettings();
  }
  if (payload.mode === 'game' || payload.mode === 'desktop') {
    remoteSessionMode.set(payload.mode);
  }
  const settings = get(remoteHostSettings);
  const myAlias = get(appSettings).alias_name || 'Raydrop Host';

  // Check if remote control is enabled
  if (!settings.enabled) {
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_rejected',
      reason: 'Remote desktop access is disabled on this device.',
    });
    return;
  }

  const enteredPin = (payload.pin || '').trim();
  const hostPin = (settings.pin || '').trim();

  // 1. PIN matches -> Clean up any stale session, send immediate ack, then start new session
  if (enteredPin && enteredPin === hostPin) {
    if (activePeerConnection || get(activeRemoteSession)) {
      cleanupPeerConnection();
      stopHostScreenCapture();
      activeRemoteSession.set(null);
    }
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_accepted',
      hostAlias: myAlias,
    });
    await proceedHostAcceptRemote(clientDeviceId, sessionId, payload.fromAlias || 'Remote Client');
    return;
  }

  // 2. No PIN provided -> Requires manual approval by the host user
  if (!enteredPin) {
    playAppSound('remote_request');
    pendingRemoteAuthRequest.set({
      clientDeviceId,
      sessionId,
      fromAlias: payload.fromAlias || 'Remote Client',
    });
    return;
  }

  // 3. Incorrect PIN provided
  sendRemoteSignal(clientDeviceId, sessionId, {
    type: 'remote_auth_rejected',
    reason: 'Incorrect Security PIN! Access denied.',
  });
}

// User manually approves an incoming remote request (when no PIN was used)
export async function acceptPendingRemoteRequest() {
  const req = get(pendingRemoteAuthRequest);
  if (!req) return;
  pendingRemoteAuthRequest.set(null);
  const myAlias = get(appSettings).alias_name || 'Raydrop Host';
  sendRemoteSignal(req.clientDeviceId, req.sessionId, {
    type: 'remote_auth_accepted',
    hostAlias: myAlias,
  });
  await proceedHostAcceptRemote(req.clientDeviceId, req.sessionId, req.fromAlias);
}

// User manually rejects an incoming remote request
export function rejectPendingRemoteRequest() {
  const req = get(pendingRemoteAuthRequest);
  if (!req) return;
  pendingRemoteAuthRequest.set(null);
  sendRemoteSignal(req.clientDeviceId, req.sessionId, {
    type: 'remote_auth_rejected',
    reason: 'Remote request was declined by the host user.',
  });
}

async function proceedHostAcceptRemote(
  clientDeviceId: string,
  sessionId: string,
  clientAlias: string
) {
  // Clean up any stale/lingering previous session before starting a new one
  if (activePeerConnection || get(activeRemoteSession)) {
    cleanupPeerConnection();
    stopHostScreenCapture();
  }

  // Ensure host screen stream is ready (auto-selects screen with cursor: 'never')
  const stream = await acquireHostScreenStream();
  if (!stream) {
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_rejected',
      reason: 'Host screen capture could not be started or permission was cancelled.',
    });
    return;
  }

  // Set active session
  activeRemoteSession.set({
    isHost: true,
    isClient: false,
    peerDeviceId: clientDeviceId,
    peerAlias: clientAlias,
    sessionId,
  });
  remoteConnectionStatus.set('connected');

  // In Desktop Mode, hide host OS cursor so screen capture frames have zero baked cursor (no double cursor).
  // In Game Mode, client uses pointer lock (hides local cursor), so keep Host OS cursor visible!
  raydropApi.setHostCursorHidden(get(remoteSessionMode) !== 'game').catch(() => { });

  // Reset any stuck modifier keys from previous session
  raydropApi.releaseRemoteModifiers().catch(() => { });

  // Setup WebRTC Host Peer Connection (minimize only AFTER DataChannel opens so WebView2 doesn't throttle handshake)
  try {
    await setupHostPeerConnection(clientDeviceId, sessionId, stream);
  } catch (err: any) {
    console.error('Failed to setup host peer connection:', err);
    stopHostScreenCapture();
    cleanupPeerConnection();
    activeRemoteSession.set(null);
    remoteConnectionStatus.set('idle');
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_rejected',
      reason: `Host WebRTC setup failed: ${err?.message || err}`,
    });
  }
}

// Utility to tune SDP bitrate across video codecs without forcing bufferbloat on slower WAN links
function boostSdpBitrate(sdp: string, kbps: number = 20000): string {
  if (!sdp) return sdp;
  const bps = kbps * 1000;
  const minKbps = kbps >= 25000 ? 2000 : 1000;
  const startKbps = kbps >= 25000 ? 6000 : 3500;
  const bitrateParams = `x-google-min-bitrate=${minKbps};x-google-max-bitrate=${kbps};x-google-start-bitrate=${startKbps}`;

  const lines = sdp.split(/\r\n|\n/);
  // Collect video codec payload types and which ones already have a=fmtp:
  const videoCodecPts = new Set<string>();
  const existingFmtpPts = new Set<string>();
  let scanningVideo = false;
  for (const l of lines) {
    if (l.startsWith('m=video')) {
      scanningVideo = true;
      continue;
    }
    if (l.startsWith('m=')) {
      scanningVideo = false;
      continue;
    }
    if (!scanningVideo) continue;
    const rtpmapMatch = /^a=rtpmap:(\d+)\s+([A-Za-z0-9_-]+)/.exec(l);
    if (rtpmapMatch) {
      const codec = rtpmapMatch[2].toUpperCase();
      if (codec !== 'RTX' && codec !== 'RED' && codec !== 'ULPFEC' && codec !== 'FLEXFEC') {
        videoCodecPts.add(rtpmapMatch[1]);
      }
    }
    const fmtpMatch = /^a=fmtp:(\d+)\s+/.exec(l);
    if (fmtpMatch) {
      existingFmtpPts.add(fmtpMatch[1]);
    }
  }

  const result: string[] = [];
  let inVideo = false;
  let bandwidthAdded = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.startsWith('m=video')) {
      inVideo = true;
      bandwidthAdded = false;
      result.push(line);
      continue;
    }
    if (line.startsWith('m=audio') || line.startsWith('m=application')) {
      inVideo = false;
    }

    if (inVideo && (line.startsWith('b=AS:') || line.startsWith('b=TIAS:'))) {
      continue;
    }

    if (inVideo && !bandwidthAdded && line.startsWith('a=')) {
      result.push(`b=AS:${kbps}`);
      result.push(`b=TIAS:${bps}`);
      bandwidthAdded = true;
    }

    if (inVideo && line.startsWith('a=fmtp:')) {
      const m = /^a=fmtp:(\d+)\s+/.exec(line);
      if (m && videoCodecPts.has(m[1]) && !line.includes('x-google-min-bitrate')) {
        result.push(`${line};${bitrateParams}`);
        continue;
      }
    }

    result.push(line);

    if (inVideo && !bandwidthAdded && line.startsWith('c=')) {
      result.push(`b=AS:${kbps}`);
      result.push(`b=TIAS:${bps}`);
      bandwidthAdded = true;
    }

    // Inject a=fmtp:<pt> right after a=rtpmap:<pt> for codecs like VP8/VP9 that lack fmtp
    if (inVideo && line.startsWith('a=rtpmap:')) {
      const m = /^a=rtpmap:(\d+)\s+/.exec(line);
      if (m && videoCodecPts.has(m[1]) && !existingFmtpPts.has(m[1])) {
        result.push(`a=fmtp:${m[1]} ${bitrateParams}`);
      }
    }
  }

  return result.join('\r\n');
}

async function setupHostPeerConnection(
  clientDeviceId: string,
  sessionId: string,
  stream: MediaStream
) {
  cleanupPeerConnection();
  raydropApi.setHostCursorHidden(get(remoteSessionMode) !== 'game').catch(() => { });

  pendingIceCandidates = [];
  const pc = new RTCPeerConnection(ICE_SERVERS);
  activePeerConnection = pc;

  // Create DataChannel for low latency mouse/key/clipboard input
  const dc = pc.createDataChannel('raydrop_remote_input', { ordered: true });
  activeDataChannel = dc;
  setupHostDataChannel(dc);

  // Add Screen Share Video & System Audio Tracks
  const isGame = get(remoteSessionMode) === 'game';
  const videoTrack = stream.getVideoTracks()[0];
  if (videoTrack && 'contentHint' in videoTrack) {
    (videoTrack as any).contentHint = isGame ? 'detail' : 'text';
  }
  const sender = pc.addTrack(videoTrack, stream);
  activeVideoSender = sender;

  for (const audioTrack of stream.getAudioTracks()) {
    pc.addTrack(audioTrack, stream);
  }

  // Prefer H264 -> VP9 -> VP8 -> AV1 for broadest cross-device WebRTC compatibility
  try {
    const caps = RTCRtpSender.getCapabilities?.('video');
    if (caps?.codecs) {
      const preferredOrder = ['video/H264', 'video/VP9', 'video/VP8', 'video/AV1'];
      const sortedCodecs = [...caps.codecs].sort((a, b) => {
        const ia = preferredOrder.indexOf(a.mimeType);
        const ib = preferredOrder.indexOf(b.mimeType);
        return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
      });
      const videoTransceiver = pc.getTransceivers().find((t) => t.sender === sender);
      if (sortedCodecs.length > 0) {
        videoTransceiver?.setCodecPreferences?.(sortedCodecs);
      }
    }
  } catch (_) {}

  // Maintain native resolution with smooth adaptive bitrate (no bufferbloat lag on WAN)
  const targetMaxKbps = isGame ? 30_000 : 20_000;
  try {
    const params = sender.getParameters();
    (params as any).degradationPreference = 'maintain-resolution';
    if (params.encodings && params.encodings.length > 0) {
      params.encodings[0].maxBitrate = targetMaxKbps * 1000;
      params.encodings[0].minBitrate = isGame ? 2_000_000 : 1_000_000;
      params.encodings[0].maxFramerate = isGame ? 60 : 30;
    }
    await sender.setParameters(params);
  } catch (err) {
    console.warn('Encoding tuning warning:', err);
  }

  pc.onicecandidate = (event) => {
    if (event.candidate) {
      sendRemoteSignal(clientDeviceId, sessionId, {
        type: 'remote_webrtc_candidate',
        candidate: event.candidate,
      });
    }
  };

  pc.oniceconnectionstatechange = () => {
    if (activePeerConnection !== pc) return;
    if (pc.iceConnectionState === 'connected' || pc.iceConnectionState === 'completed') {
      if (iceWatchdogTimer) {
        clearTimeout(iceWatchdogTimer);
        iceWatchdogTimer = null;
      }
    } else if (pc.iceConnectionState === 'failed') {
      disconnectRemoteSession();
    }
  };

  if (iceWatchdogTimer) clearTimeout(iceWatchdogTimer);
  iceWatchdogTimer = setTimeout(() => {
    if (activePeerConnection === pc && pc.iceConnectionState !== 'connected' && pc.iceConnectionState !== 'completed' && pc.connectionState !== 'connected') {
      disconnectRemoteSession();
    }
  }, 35000);

  pc.onconnectionstatechange = () => {
    if (activePeerConnection !== pc) return;
    if (pc.connectionState === 'connected') {
      if (iceWatchdogTimer) {
        clearTimeout(iceWatchdogTimer);
        iceWatchdogTimer = null;
      }
    } else if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
      disconnectRemoteSession();
    } else if (pc.connectionState === 'disconnected') {
      // Restore host cursor immediately on connection drop so host isn't stuck with hidden cursor
      raydropApi.setHostCursorHidden(false).catch(() => { });
      raydropApi.releaseRemoteModifiers().catch(() => { });
      setTimeout(() => {
        if (activePeerConnection === pc && pc.connectionState === 'disconnected') {
          disconnectRemoteSession();
        }
      }, 5000);
    }
  };

  // Create and send offer to client with boosted bitrate
  const offer = await pc.createOffer();
  const boostedOffer = {
    type: offer.type,
    sdp: boostSdpBitrate(offer.sdp || '', targetMaxKbps),
  };
  await pc.setLocalDescription(boostedOffer);

  sendRemoteSignal(clientDeviceId, sessionId, {
    type: 'remote_webrtc_offer',
    sdp: boostedOffer,
  });
}

let hostWatchdogInterval: any = null;
let lastClientPingAt = 0;
let stopHostClipboardSync: (() => void) | null = null;
let stopClientClipboardSync: (() => void) | null = null;
let triggerClientClipboardCheck: (() => void) | null = null;

interface ClipSyncController {
  onRemoteReceived: (content: string) => Promise<void>;
  handleChunk: (msg: any) => Promise<void>;
  checkNow: () => Promise<void>;
  stop: () => void;
}

function createAutoClipboardSync(dc: RTCDataChannel): ClipSyncController {
  let lastSeq = 0;
  let lastContent = '';
  let suppressUntil = 0;
  let stopped = false;
  let polling = false;
  const incomingChunks = new Map<string, { total: number; parts: string[]; received: number }>();

  const sendContent = (content: string) => {
    if (dc.readyState !== 'open' || !content) return;
    // Cap at ~6MB base64 to keep DataChannel responsive
    if (content.length > 6_000_000) return;
    try {
      if (content.length <= 48_000) {
        dc.send(JSON.stringify({ t: 'clip_sync', content }));
      } else {
        const chunkSize = 48_000;
        const total = Math.ceil(content.length / chunkSize);
        const id = `${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
        for (let i = 0; i < total; i++) {
          dc.send(
            JSON.stringify({
              t: 'clip_chunk',
              id,
              idx: i,
              total,
              data: content.slice(i * chunkSize, (i + 1) * chunkSize),
            })
          );
        }
      }
    } catch (err) {
      console.warn('Auto clipboard send error:', err);
    }
  };

  const checkNow = async () => {
    if (stopped || polling || dc.readyState !== 'open') return;
    polling = true;
    try {
      const res = await raydropApi.readSystemClipboardIfChanged(lastSeq).catch(() => null);
      if (res && Array.isArray(res) && res.length === 2) {
        const [seq, content] = res;
        lastSeq = seq;
        if (content && content !== lastContent && Date.now() > suppressUntil) {
          lastContent = content;
          sendContent(content);
        }
      }
    } finally {
      polling = false;
    }
  };

  // Seed initial clipboard sequence/content without pushing stale pre-session clipboard
  (async () => {
    try {
      const res = await raydropApi.readSystemClipboardIfChanged(0).catch(() => null);
      if (res && Array.isArray(res) && res.length === 2) {
        lastSeq = res[0];
        lastContent = res[1];
      } else {
        lastSeq = await raydropApi.getClipboardSequenceNumber().catch(() => 0);
      }
    } catch (_) {}
  })();

  const timer = setInterval(checkNow, 450);

  const onRemoteReceived = async (content: string) => {
    if (!content || content === lastContent) return;
    suppressUntil = Date.now() + 1000;
    lastContent = content;
    await raydropApi.writeSystemClipboard(content).catch(() => {});
    lastSeq = await raydropApi.getClipboardSequenceNumber().catch(() => lastSeq);
  };

  const handleChunk = async (msg: any) => {
    if (!msg || !msg.id || typeof msg.idx !== 'number' || typeof msg.total !== 'number' || typeof msg.data !== 'string') {
      return;
    }
    let entry = incomingChunks.get(msg.id);
    if (!entry) {
      entry = { total: msg.total, parts: new Array(msg.total), received: 0 };
      incomingChunks.set(msg.id, entry);
      // Cleanup stale incomplete chunks
      if (incomingChunks.size > 8) {
        const firstKey = incomingChunks.keys().next().value;
        if (firstKey) incomingChunks.delete(firstKey);
      }
    }
    if (msg.idx >= 0 && msg.idx < entry.total && entry.parts[msg.idx] === undefined) {
      entry.parts[msg.idx] = msg.data;
      entry.received++;
    }
    if (entry.received >= entry.total) {
      incomingChunks.delete(msg.id);
      await onRemoteReceived(entry.parts.join(''));
    }
  };

  return {
    onRemoteReceived,
    handleChunk,
    checkNow,
    stop: () => {
      stopped = true;
      clearInterval(timer);
      incomingChunks.clear();
    },
  };
}

export function triggerLocalClipboardSyncNow() {
  if (triggerClientClipboardCheck) {
    setTimeout(() => {
      triggerClientClipboardCheck?.();
    }, 80);
  }
}

function setupHostDataChannel(dc: RTCDataChannel) {
  dc.onopen = async () => {
    lastClientPingAt = Date.now();
    // Auto-minimize Host Raydrop window only after WebRTC DataChannel is open
    try {
      getCurrentWindow().minimize().catch(() => { });
    } catch (_) { }
    if (hostWatchdogInterval) clearInterval(hostWatchdogInterval);
    hostWatchdogInterval = setInterval(() => {
      const sess = get(activeRemoteSession);
      if (sess?.isHost && Date.now() - lastClientPingAt > 8000) {
        // Client stopped pinging (frozen/disconnected) -> restore cursor immediately
        raydropApi.setHostCursorHidden(false).catch(() => { });
        raydropApi.releaseRemoteModifiers().catch(() => { });
      }
    }, 3000);

    try {
      const monitors = await raydropApi.getHostMonitors();
      if (dc.readyState === 'open') {
        dc.send(JSON.stringify({ t: 'monitors_list', monitors }));
      }
    } catch (_) { }
    // Start native WASAPI loopback audio streaming from host PC
    raydropApi.startHostAudioLoopback().catch((err) => {
      console.warn('Native host audio loopback start warning:', err);
    });

    // Start automatic bidirectional clipboard sync (Host -> Client)
    if (stopHostClipboardSync) stopHostClipboardSync();
    const hostClipSync = createAutoClipboardSync(dc);
    stopHostClipboardSync = hostClipSync.stop;
    (dc as any)._hostClipSync = hostClipSync;
  };

  dc.onclose = () => {
    if (hostWatchdogInterval) {
      clearInterval(hostWatchdogInterval);
      hostWatchdogInterval = null;
    }
    if (stopHostClipboardSync) {
      stopHostClipboardSync();
      stopHostClipboardSync = null;
    }
    raydropApi.setHostCursorHidden(false).catch(() => { });
    raydropApi.releaseRemoteModifiers().catch(() => { });
  };

  dc.onmessage = async (event) => {
    lastClientPingAt = Date.now();
    try {
      const data = JSON.parse(event.data);
      if (!data || !data.t) return;

      // Mouse Move (Absolute)
      if (data.t === 'mm') {
        raydropApi.simulateRemoteMouseMove(data.x, data.y).catch(() => { });
        return;
      }

      // Mouse Move (Relative delta for Game Mode FPS/3D Pointer Lock)
      if (data.t === 'mm_rel') {
        raydropApi.simulateRemoteMouseMoveRelative(Math.round(data.dx || 0), Math.round(data.dy || 0)).catch(() => { });
        return;
      }

      // Gamepad / Joystick input (Game Mode)
      if (data.t === 'gp') {
        raydropApi
          .simulateRemoteGamepad(
            data.b || 0,
            data.lx || 0,
            data.ly || 0,
            data.rx || 0,
            data.ry || 0,
            data.lt || 0,
            data.rt || 0
          )
          .catch(() => { });
        return;
      }

      // Dynamic Mode Switch (Desktop <-> Game)
      if (data.t === 'set_mode' && (data.mode === 'desktop' || data.mode === 'game')) {
        remoteSessionMode.set(data.mode);
        const isGameNow = data.mode === 'game';
        // In Game Mode, client uses pointer lock (hides local cursor), so restore Host OS cursor
        // so desktop/menu/2D game cursors remain visible; in Desktop Mode hide Host cursor to avoid double cursor.
        raydropApi.setHostCursorHidden(!isGameNow).catch(() => { });
        if (activeVideoSender) {
          try {
            const p = activeVideoSender.getParameters();
            (p as any).degradationPreference = 'maintain-resolution';
            if (p.encodings && p.encodings.length > 0) {
              p.encodings[0].maxBitrate = isGameNow ? 30_000_000 : 20_000_000;
              p.encodings[0].minBitrate = isGameNow ? 2_000_000 : 1_000_000;
              p.encodings[0].maxFramerate = isGameNow ? 60 : 30;
            }
            await activeVideoSender.setParameters(p);
          } catch (_) { }
          // Re-acquire stream for the new mode (e.g. DXGI 60fps for Game Mode, Native GDI for Desktop Mode)
          stopHostScreenCapture();
          const newStream = await acquireHostScreenStream();
          const newTrack = newStream?.getVideoTracks()[0];
          if (newTrack && activeVideoSender) {
            await activeVideoSender.replaceTrack(newTrack).catch(() => { });
          }
        }
        return;
      }

      // Mouse Click
      if (data.t === 'mc') {
        raydropApi
          .simulateRemoteMouseClick(
            data.b,
            data.a,
            data.alt,
            data.ctrl,
            data.shift,
            data.meta
          )
          .catch(() => { });
        return;
      }

      // Explicit release modifiers
      if (data.t === 'release_modifiers') {
        raydropApi.releaseRemoteModifiers().catch(() => { });
        return;
      }

      // Mouse Wheel
      if (data.t === 'mw') {
        raydropApi.simulateRemoteMouseWheel(data.dy).catch(() => { });
        return;
      }

      // Keyboard Key
      if (data.t === 'kk') {
        raydropApi.simulateRemoteKeyboardKey(data.k, data.c, data.a).catch(() => { });
        return;
      }

      // Shortcut
      if (data.t === 'sc') {
        raydropApi.simulateRemoteShortcut(data.s).catch(() => { });
        return;
      }

      // Auto Clipboard Sync or Write from Remote Client (Text or Data URL Image)
      if ((data.t === 'clip_sync' || data.t === 'clip_write') && data.content) {
        const sync = (dc as any)._hostClipSync as ClipSyncController | undefined;
        if (sync) {
          await sync.onRemoteReceived(data.content);
        } else {
          await raydropApi.writeSystemClipboard(data.content).catch(() => { });
        }
        return;
      }

      if (data.t === 'clip_chunk') {
        const sync = (dc as any)._hostClipSync as ClipSyncController | undefined;
        if (sync) {
          await sync.handleChunk(data);
        }
        return;
      }

      // Clipboard Read Request from Remote Client
      if (data.t === 'clip_read_req') {
        try {
          const content = await raydropApi.readSystemClipboard();
          if (content && dc.readyState === 'open') {
            dc.send(JSON.stringify({ t: 'clip_read_res', content }));
          }
        } catch (_) { }
        return;
      }

      // Latency Ping from Client
      if (data.t === 'ping') {
        if (dc.readyState === 'open') {
          dc.send(JSON.stringify({ t: 'pong', time: data.time }));
        }
        return;
      }

      // Switch Target Monitor Bounds
      if (data.t === 'switch_monitor') {
        raydropApi.setHostTargetMonitor(data.monitorIndex ?? 0).catch(() => { });
        return;
      }

      // Remote Power Management Action (lock, sleep, restart, shutdown)
      if (data.t === 'power_action' && data.action) {
        raydropApi.executeRemotePowerAction(data.action).catch(() => { });
        return;
      }
    } catch (_) { }
  };
}

// ----------------- Client Handling Logic -----------------

export async function connectToRemoteHost(targetDeviceId: string, pin: string) {
  clearAuthTimer();
  cleanupPeerConnection();

  const settings = get(appSettings);
  const myDeviceId = settings.device_id;
  cleanupPeerConnection();
  const myAlias = settings.alias_name || 'Raydrop Client';

  remoteErrorMessage.set(null);
  remoteConnectionStatus.set('verifying_pin');

  const sessionId = `rem_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;

  // Find contact alias
  const allContacts = get(contacts);
  const peerContact = allContacts.find((c) => c.device_id === targetDeviceId);
  const peerAlias = peerContact ? peerContact.custom_name || peerContact.alias_name : 'Host PC';

  activeRemoteSession.set({
    isHost: false,
    isClient: true,
    peerDeviceId: targetDeviceId,
    peerAlias,
    sessionId,
  });

  // Timeout guard (45 seconds): Covers worst-case 25s signaling reconnect + retry + WebRTC handshake
  const timeoutMs = 45000;
  authTimeoutTimer = setTimeout(() => {
    const status = get(remoteConnectionStatus);
    if (status === 'verifying_pin' || status === 'connecting') {
      cleanupPeerConnection();
      activeRemoteSession.set(null);
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(
        pin.trim()
          ? 'Connection timed out. Host did not respond. Please ensure Raydrop is running on the host device and Allow Remote Access is enabled.'
          : 'Request timed out. Host did not accept the remote control request.'
      );
    }
  }, timeoutMs);

  // Send authentication request with PIN & preferred session mode
  const authPayload = {
    type: 'remote_request_auth',
    sessionId,
    fromDeviceId: myDeviceId,
    fromAlias: myAlias,
    pin: pin.trim(),
    mode: get(remoteSessionMode),
  };
  sendRemoteSignal(targetDeviceId, sessionId, authPayload);

  // Retry at 3s and 8s if still waiting on verifying_pin (guards against transient signaling reconnect)
  authRetryTimer = setTimeout(() => {
    const sess = get(activeRemoteSession);
    if (sess?.sessionId === sessionId && get(remoteConnectionStatus) === 'verifying_pin') {
      sendRemoteSignal(targetDeviceId, sessionId, authPayload);
      authRetryTimer2 = setTimeout(() => {
        const sess2 = get(activeRemoteSession);
        if (sess2?.sessionId === sessionId && get(remoteConnectionStatus) === 'verifying_pin') {
          sendRemoteSignal(targetDeviceId, sessionId, authPayload);
        }
      }, 5000);
    }
  }, 3000);
}

async function handleIncomingOffer(
  hostDeviceId: string,
  sessionId: string,
  sdp: RTCSessionDescriptionInit
) {
  const earlyCandidates =
    get(activeRemoteSession)?.sessionId === sessionId ? [...pendingIceCandidates] : [];
  cleanupPeerConnection();
  pendingIceCandidates = earlyCandidates;

  const pc = new RTCPeerConnection(ICE_SERVERS);
  activePeerConnection = pc;

  pc.ondatachannel = (event) => {
    const dc = event.channel;
    activeDataChannel = dc;
    setupClientDataChannel(dc);
  };

  pc.ontrack = (event) => {
    let stream = event.streams && event.streams[0] ? event.streams[0] : null;
    if (!stream && event.track) {
      stream = new MediaStream([event.track]);
    }
    if (stream) {
      remoteErrorMessage.set(null);
      clientRemoteStream.set(stream);
      remoteConnectionStatus.set('connected');
    }
  };

  pc.onicecandidate = (event) => {
    if (event.candidate) {
      sendRemoteSignal(hostDeviceId, sessionId, {
        type: 'remote_webrtc_candidate',
        candidate: event.candidate,
      });
    }
  };

  function updateDebug() {
    if (activePeerConnection !== pc) return;
    remoteDebugInfo.set(
      `ICE: ${pc.iceConnectionState} • PC: ${pc.connectionState} • DC: ${activeDataChannel?.readyState || 'none'} • Sig: ${pc.signalingState}`
    );
  }
  updateDebug();

  pc.oniceconnectionstatechange = () => {
    if (activePeerConnection !== pc) return;
    updateDebug();
    if (pc.iceConnectionState === 'failed') {
      if (iceWatchdogTimer) {
        clearTimeout(iceWatchdogTimer);
        iceWatchdogTimer = null;
      }
      cleanupPeerConnection();
      activeRemoteSession.set(null);
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(`P2P WebRTC ICE connection failed (${pc.iceConnectionState}). Check firewall or Tailscale/NAT connectivity between host and client.`);
    }
  };

  if (iceWatchdogTimer) clearTimeout(iceWatchdogTimer);
  iceWatchdogTimer = setTimeout(() => {
    if (
      activePeerConnection === pc &&
      pc.connectionState !== 'connected' &&
      activeDataChannel?.readyState !== 'open'
    ) {
      const state = `ICE=${pc.iceConnectionState}, PC=${pc.connectionState}, DC=${activeDataChannel?.readyState || 'none'}`;
      cleanupPeerConnection();
      activeRemoteSession.set(null);
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(`P2P video handshake timed out (${state}). Ensure both devices are updated and UDP/Tailscale is reachable.`);
    }
  }, 35000);

  pc.onconnectionstatechange = () => {
    if (activePeerConnection !== pc) return;
    updateDebug();
    if (pc.connectionState === 'connected' && activeDataChannel?.readyState === 'open') {
      if (iceWatchdogTimer) {
        clearTimeout(iceWatchdogTimer);
        iceWatchdogTimer = null;
      }
    } else if (pc.connectionState === 'failed' || pc.connectionState === 'closed') {
      const wasConnected = get(remoteConnectionStatus) === 'connected';
      cleanupPeerConnection();
      activeRemoteSession.set(null);
      if (wasConnected) {
        remoteConnectionStatus.set('disconnected');
      } else {
        remoteConnectionStatus.set('error');
        remoteErrorMessage.set('WebRTC peer connection failed. Please try reconnecting.');
      }
    } else if (pc.connectionState === 'disconnected') {
      setTimeout(() => {
        if (activePeerConnection === pc && pc.connectionState === 'disconnected') {
          cleanupPeerConnection();
          activeRemoteSession.set(null);
          remoteConnectionStatus.set('disconnected');
        }
      }, 4000);
    }
  };

  await pc.setRemoteDescription(new RTCSessionDescription(sdp));

  // Flush pending ICE candidates if any arrived before offer
  for (const c of pendingIceCandidates) {
    try {
      await pc.addIceCandidate(new RTCIceCandidate(c));
    } catch (_) { }
  }
  pendingIceCandidates = [];

  const answer = await pc.createAnswer();
  const boostedAnswer = {
    type: answer.type,
    sdp: boostSdpBitrate(answer.sdp || '', 30000),
  };
  await pc.setLocalDescription(boostedAnswer);

  sendRemoteSignal(hostDeviceId, sessionId, {
    type: 'remote_webrtc_answer',
    sdp: boostedAnswer,
  });
}

async function handleIncomingAnswer(sdp: RTCSessionDescriptionInit) {
  if (!activePeerConnection) return;
  await activePeerConnection.setRemoteDescription(new RTCSessionDescription(sdp));

  for (const c of pendingIceCandidates) {
    try {
      await activePeerConnection.addIceCandidate(new RTCIceCandidate(c));
    } catch (_) { }
  }
  pendingIceCandidates = [];
}

async function handleIncomingIceCandidate(candidate: RTCIceCandidateInit) {
  if (!activePeerConnection || !activePeerConnection.remoteDescription) {
    pendingIceCandidates.push(candidate);
    return;
  }
  try {
    await activePeerConnection.addIceCandidate(new RTCIceCandidate(candidate));
  } catch (err) {
    console.warn('Failed to add remote candidate:', err);
  }
}

let clientAudioCtx: AudioContext | null = null;
let nextAudioPlayTime = 0;
let clientAudioMuted = false;

export function setClientAudioMuted(muted: boolean) {
  clientAudioMuted = muted;
  if (!muted) {
    resumeClientAudio();
  }
}

export function resumeClientAudio() {
  if (clientAudioMuted) return;
  try {
    if (!clientAudioCtx) {
      const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
      if (AudioContextClass) {
        clientAudioCtx = new AudioContextClass({ sampleRate: 24000 });
        nextAudioPlayTime = 0;
      }
    }
    if (clientAudioCtx && clientAudioCtx.state === 'suspended') {
      clientAudioCtx.resume().catch(() => { });
    }
  } catch (_) { }
}

function playIncomingPcmChunk(b64: string) {
  if (clientAudioMuted) return;
  try {
    if (!clientAudioCtx) {
      const AudioContextClass = window.AudioContext || (window as any).webkitAudioContext;
      if (!AudioContextClass) return;
      clientAudioCtx = new AudioContextClass({ sampleRate: 24000 });
      nextAudioPlayTime = 0;
    }
    if (clientAudioCtx.state === 'suspended') {
      // Auto-resume if user has interacted
      clientAudioCtx.resume().catch(() => { });
      if (clientAudioCtx.state === 'suspended') return;
    }

    const bin = atob(b64);
    const len = bin.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
      bytes[i] = bin.charCodeAt(i);
    }
    const int16 = new Int16Array(bytes.buffer);
    if (int16.length === 0) return;

    const audioBuf = clientAudioCtx.createBuffer(1, int16.length, 24000);
    const channelData = audioBuf.getChannelData(0);
    for (let i = 0; i < int16.length; i++) {
      channelData[i] = int16[i] / 32768.0;
    }

    const now = clientAudioCtx.currentTime;
    const isResync = nextAudioPlayTime < now || nextAudioPlayTime - now > 0.30;
    if (isResync) {
      // Tiny 2ms fade-in on resync so chunk boundaries never produce an audible "bip/click"
      const fadeLen = Math.min(48, channelData.length);
      for (let i = 0; i < fadeLen; i++) {
        channelData[i] *= i / fadeLen;
      }
      nextAudioPlayTime = now + 0.08;
    }

    const src = clientAudioCtx.createBufferSource();
    src.buffer = audioBuf;
    src.connect(clientAudioCtx.destination);
    src.onended = () => {
      try {
        src.disconnect();
      } catch (_) { }
    };

    src.start(nextAudioPlayTime);
    nextAudioPlayTime += audioBuf.duration;
  } catch (_) { }
}

function setupClientDataChannel(dc: RTCDataChannel) {
  dc.onopen = () => {
    if (iceWatchdogTimer) {
      clearTimeout(iceWatchdogTimer);
      iceWatchdogTimer = null;
    }
    remoteDebugInfo.set('Connected');
    remoteConnectionStatus.set('connected');
    // Start periodic ping to measure latency
    if (pingInterval) clearInterval(pingInterval);
    let prevStatsBytes = 0;
    let prevStatsTime = 0;
    pingInterval = setInterval(async () => {
      if (dc.readyState === 'open') {
        dc.send(JSON.stringify({ t: 'ping', time: Date.now() }));
      }
      if (activePeerConnection) {
        try {
          const stats = await activePeerConnection.getStats();
          stats.forEach((report) => {
            if (report.type === 'inbound-rtp' && report.kind === 'video') {
              if (typeof report.framesPerSecond === 'number') {
                remoteStreamFps.set(Math.round(report.framesPerSecond));
              }
              const bytes = Number(report.bytesReceived || 0);
              const ts = Number(report.timestamp || Date.now());
              if (prevStatsTime > 0 && ts > prevStatsTime && bytes >= prevStatsBytes) {
                const mbps = ((bytes - prevStatsBytes) * 8) / ((ts - prevStatsTime) / 1000) / 1_000_000;
                remoteStreamMbps.set(Math.round(mbps * 10) / 10);
              }
              prevStatsBytes = bytes;
              prevStatsTime = ts;
            }
          });
        } catch (_) {}
      }
    }, 2000);

    // Start automatic bidirectional clipboard sync (Client -> Host)
    if (stopClientClipboardSync) stopClientClipboardSync();
    const clientClipSync = createAutoClipboardSync(dc);
    stopClientClipboardSync = clientClipSync.stop;
    triggerClientClipboardCheck = clientClipSync.checkNow;
    (dc as any)._clientClipSync = clientClipSync;

    if (get(remoteSessionMode) === 'game') {
      startClientGamepadPolling();
    }
  };

  dc.onmessage = async (event) => {
    try {
      const data = JSON.parse(event.data);
      if (!data) return;

      // Dynamic host cursor shape sync
      if (data.t === 'cur' && data.s) {
        remoteCursorShape.set(data.s);
        return;
      }

      // Pong latency measurement
      if (data.t === 'pong' && data.time) {
        const rtt = Math.max(1, Date.now() - data.time);
        remoteLatencyMs.set(rtt);
        return;
      }

      // Auto-received clipboard from Host (Text or Image)
      if ((data.t === 'clip_sync' || data.t === 'clip_read_res') && data.content) {
        const sync = (dc as any)._clientClipSync as ClipSyncController | undefined;
        if (sync) {
          await sync.onRemoteReceived(data.content);
        } else {
          await raydropApi.writeSystemClipboard(data.content).catch(() => { });
        }
        return;
      }

      if (data.t === 'clip_chunk') {
        const sync = (dc as any)._clientClipSync as ClipSyncController | undefined;
        if (sync) {
          await sync.handleChunk(data);
        }
        return;
      }

      // Received host monitors list
      if (data.t === 'monitors_list' && Array.isArray(data.monitors)) {
        remoteHostMonitors.set(data.monitors);
        const primary = data.monitors.find((m: any) => m.is_primary);
        if (primary) activeMonitorIndex.set(primary.index);
        return;
      }

      // Received native PCM audio chunk from Host
      if (data.t === 'aud' && data.d) {
        playIncomingPcmChunk(data.d);
        return;
      }
    } catch (_) { }
  };

  dc.onclose = () => {
    if (pingInterval) clearInterval(pingInterval);
    if (stopClientClipboardSync) {
      stopClientClipboardSync();
      stopClientClipboardSync = null;
    }
    triggerClientClipboardCheck = null;
  };
}

// ----------------- Client Input Dispatchers -----------------

export function sendInputToHost(eventData: any) {
  if (activeDataChannel && activeDataChannel.readyState === 'open') {
    // Drop high-frequency mouse moves when DataChannel buffer is backed up; never drop clicks/keys
    if (eventData?.t === 'mm' && activeDataChannel.bufferedAmount > 128 * 1024) {
      return;
    }
    activeDataChannel.send(JSON.stringify(eventData));
  }
}

// Copy local clipboard and send directly to remote Host (Text & Image)
export async function sendLocalClipboardToHost() {
  try {
    const content = await raydropApi.readSystemClipboard();
    if (!content || !content.trim()) {
      alert('Your local clipboard is currently empty.');
      return;
    }
    sendInputToHost({
      t: 'clip_write',
      content,
    });
    const isImg = content.startsWith('data:image/');
    alert(isImg ? '🖼️ Clipboard image sent to host!' : '📝 Clipboard text sent to host!');
  } catch (err) {
    console.warn('Failed to send clipboard to host:', err);
  }
}

// Request Host to send its clipboard back to Client
export function requestClipboardFromHost() {
  sendInputToHost({
    t: 'clip_read_req',
  });
}

// Trigger shortcut on Host
export function sendShortcutToHost(shortcut: string) {
  sendInputToHost({
    t: 'sc',
    s: shortcut,
  });
}

let gamepadRafId: number | null = null;
let lastGamepadSig = '';

function stopClientGamepadPolling() {
  if (gamepadRafId !== null) {
    cancelAnimationFrame(gamepadRafId);
    gamepadRafId = null;
  }
  lastGamepadSig = '';
  activeGamepadCount.set(0);
}

function startClientGamepadPolling() {
  stopClientGamepadPolling();
  const poll = () => {
    const sess = get(activeRemoteSession);
    if (!sess?.isClient || get(remoteSessionMode) !== 'game') {
      stopClientGamepadPolling();
      return;
    }
    const pads = navigator.getGamepads ? Array.from(navigator.getGamepads()).filter(Boolean) as Gamepad[] : [];
    activeGamepadCount.set(pads.length);
    if (pads.length > 0 && activeDataChannel && activeDataChannel.readyState === 'open') {
      const pad = pads[0];
      let b = 0;
      for (let i = 0; i < Math.min(pad.buttons.length, 18); i++) {
        if (pad.buttons[i]?.pressed) b |= 1 << i;
      }
      const lx = Number((pad.axes[0] || 0).toFixed(2));
      const ly = Number((pad.axes[1] || 0).toFixed(2));
      const rx = Number((pad.axes[2] || 0).toFixed(2));
      const ry = Number((pad.axes[3] || 0).toFixed(2));
      const lt = Number((pad.buttons[6]?.value || 0).toFixed(2));
      const rt = Number((pad.buttons[7]?.value || 0).toFixed(2));
      const sig = `${b}:${lx}:${ly}:${rx}:${ry}:${lt}:${rt}`;
      const stickMoving = Math.abs(rx) > 0.16 || Math.abs(ry) > 0.16;
      if (sig !== lastGamepadSig || stickMoving) {
        lastGamepadSig = sig;
        try {
          activeDataChannel.send(JSON.stringify({ t: 'gp', b, lx, ly, rx, ry, lt, rt }));
        } catch (_) { }
      }
    }
    gamepadRafId = requestAnimationFrame(poll);
  };
  gamepadRafId = requestAnimationFrame(poll);
}

export function setRemoteSessionMode(mode: 'desktop' | 'game') {
  remoteSessionMode.set(mode);
  const sess = get(activeRemoteSession);
  if (sess?.isClient) {
    sendInputToHost({ t: 'set_mode', mode });
    if (mode === 'game') {
      startClientGamepadPolling();
    } else {
      stopClientGamepadPolling();
      if (document.pointerLockElement) {
        document.exitPointerLock();
      }
    }
  }
}

// Switch target monitor bounds on Host
export function switchRemoteMonitor(monitorIndex: number) {
  activeMonitorIndex.set(monitorIndex);
  sendInputToHost({
    t: 'switch_monitor',
    monitorIndex,
  });
}

// Execute Power Action on Host (lock, sleep, restart, shutdown)
export function sendPowerActionToHost(action: 'lock' | 'sleep' | 'restart' | 'shutdown') {
  sendInputToHost({
    t: 'power_action',
    action,
  });
}

// ----------------- Cleanup & Teardown -----------------

export function stopHostScreenCapture() {
  raydropApi.setHostCursorHidden(false).catch(() => { });
  raydropApi.stopNativeScreenCapture().catch(() => { });
  stopBackgroundKeepAlive();
  const stream = get(hostScreenStream);
  if (stream) {
    stream.getTracks().forEach((t) => {
      t.onended = null;
      t.stop();
    });
  }
  hostScreenStream.set(null);
  isHostSharingScreen.set(false);
  nativeCanvasEl = null;
  nativeCanvasCtx = null;
  pendingFrameDraw = false;
}

function cleanupPeerConnection() {
  clearAuthTimer();
  if (iceWatchdogTimer) {
    clearTimeout(iceWatchdogTimer);
    iceWatchdogTimer = null;
  }
  stopClientGamepadPolling();
  raydropApi.setHostCursorHidden(false).catch(() => { });
  raydropApi.releaseRemoteModifiers().catch(() => { });
  raydropApi.stopHostAudioLoopback().catch(() => { });
  if (hostWatchdogInterval) {
    clearInterval(hostWatchdogInterval);
    hostWatchdogInterval = null;
  }
  if (clientAudioCtx) {
    try {
      clientAudioCtx.close();
    } catch (_) { }
    clientAudioCtx = null;
    nextAudioPlayTime = 0;
  }
  if (pingInterval) {
    clearInterval(pingInterval);
    pingInterval = null;
  }
  if (activeDataChannel) {
    try {
      activeDataChannel.close();
    } catch (_) { }
    activeDataChannel = null;
  }
  if (activePeerConnection) {
    try {
      activePeerConnection.close();
    } catch (_) { }
    activePeerConnection = null;
  }
  activeVideoSender = null;
  clientRemoteStream.set(null);
  remoteCursorShape.set('default');
  remoteDebugInfo.set('');
  pendingIceCandidates = [];
}

export function disconnectRemoteSession() {
  raydropApi.setHostCursorHidden(false).catch(() => { });
  stopBackgroundKeepAlive();
  clearAuthTimer();
  const session = get(activeRemoteSession);
  if (session) {
    if (get(remoteConnectionStatus) === 'connected') {
      playAppSound('remote_disconnect');
    }
    if (session.isHost) {
      stopHostScreenCapture();
    }
    sendRemoteSignal(session.peerDeviceId, session.sessionId, {
      type: 'remote_session_ended',
    });
  }

  const prevStatus = get(remoteConnectionStatus);
  cleanupPeerConnection();
  activeRemoteSession.set(null);
  remoteConnectionStatus.set(
    prevStatus === 'verifying_pin' || prevStatus === 'connecting' ? 'idle' : 'disconnected'
  );
  remoteLatencyMs.set(0);
  remoteStreamFps.set(0);
  remoteStreamMbps.set(0);
}
