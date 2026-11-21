import { writable, get } from 'svelte/store';
import { raydropApi, type RemoteDesktopSettings } from '../utils/tauri-ipc';
import { appSettings } from './state';
import { contacts } from './contacts';

// ICE Servers for WebRTC peer connection
const ICE_SERVERS: RTCConfiguration = {
  iceServers: [
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
    { urls: 'stun:stun2.l.google.com:19302' },
  ],
  iceCandidatePoolSize: 10,
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
export const remoteQualityMode = writable<'60fps' | 'balanced' | 'low_bandwidth'>('60fps');
export const remoteHostMonitors = writable<
  { index: number; name: string; x: number; y: number; width: number; height: number; is_primary: boolean }[]
>([]);
export const activeMonitorIndex = writable<number>(0);
export const remoteAudioEnabled = writable<boolean>(true);

let activePeerConnection: RTCPeerConnection | null = null;
let activeDataChannel: RTCDataChannel | null = null;
let activeVideoSender: RTCRtpSender | null = null;
let pingInterval: any = null;
let pendingIceCandidates: RTCIceCandidateInit[] = [];
let isListenerInitialized = false;
let authTimeoutTimer: any = null;

function clearAuthTimer() {
  if (authTimeoutTimer) {
    clearTimeout(authTimeoutTimer);
    authTimeoutTimer = null;
  }
}

// Load persisted settings from SQLite backend
export async function loadRemoteDesktopSettings() {
  try {
    const settings = await raydropApi.getRemoteDesktopSettings();
    if (settings) {
      remoteHostSettings.set(settings);
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

    // 1. Host receives authentication request from a client
    if (payload.type === 'remote_request_auth') {
      await handleIncomingAuthRequest(from_device_id, rawSessionId, payload);
      return;
    }

    // 2. Client receives authentication rejection
    if (payload.type === 'remote_auth_rejected') {
      clearAuthTimer();
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(payload.reason || 'Remote connection rejected by host.');
      disconnectRemoteSession();
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

    // 7. Session terminated by peer
    if (payload.type === 'remote_session_ended') {
      disconnectRemoteSession();
      return;
    }

    // 8. Fallback signaling channel for clipboard read reply
    if (payload.type === 'remote_clip_read_res') {
      if (payload.content) {
        await raydropApi.writeSystemClipboard(payload.content).catch(() => {});
      }
    }
  });

  // Forward native WASAPI host audio loopback chunks over WebRTC DataChannel
  raydropApi.listenEvent<string>('host_audio_chunk', (b64) => {
    const sess = get(activeRemoteSession);
    if (sess?.isHost && activeDataChannel && activeDataChannel.readyState === 'open' && b64) {
      try {
        activeDataChannel.send(JSON.stringify({ t: 'aud', d: b64 }));
      } catch (_) {}
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
    } catch (_) {}
    silentAudioKeepAlive = null;
  }
}

// Helper to acquire monitor screen stream safely without host cursor
export async function acquireHostScreenStream(): Promise<MediaStream | null> {
  let stream = get(hostScreenStream);
  if (stream && stream.getVideoTracks().length > 0 && stream.getVideoTracks()[0].readyState === 'live') {
    startBackgroundKeepAlive();
    return stream;
  }

  try {
    stream = await (navigator.mediaDevices as any).getDisplayMedia({
      video: {
        cursor: 'never', // Completely removes host OS cursor from video capture frames
        displaySurface: 'monitor',
        frameRate: { ideal: 60, max: 60 },
        width: { ideal: 1920, max: 2560 },
        height: { ideal: 1080, max: 1440 },
      },
      audio: true,
      selfBrowserSurface: 'exclude',
      surfaceSwitching: 'include',
      systemAudio: 'include',
      monitorTypeSurfaces: 'include',
    });
  } catch (err) {
    console.warn('Standard monitor displayMedia with audio failed, trying fallback:', err);
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
      console.warn('Fallback displayMedia failed:', e2);
      return null;
    }
  }

  if (!stream || stream.getVideoTracks().length === 0) {
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
    // Treat stream as detail/text to avoid video macroblocking & compression blur on app switches
    (track as any).contentHint = 'detail';
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

  // 1. PIN matches -> Send immediate acknowledgment so client clears timeout, then start session
  if (enteredPin && enteredPin === hostPin) {
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_accepted',
      hostAlias: myAlias,
    });
    await proceedHostAcceptRemote(clientDeviceId, sessionId, payload.fromAlias || 'Remote Client');
    return;
  }

  // 2. No PIN provided -> Requires manual approval by the host user
  if (!enteredPin) {
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

  // Hide host OS cursor so screen capture frames have zero baked cursor (no double cursor on client)
  raydropApi.setHostCursorHidden(true).catch(() => {});

  // Reset any stuck modifier keys from previous session
  raydropApi.releaseRemoteModifiers().catch(() => {});

  // Setup WebRTC Host Peer Connection
  await setupHostPeerConnection(clientDeviceId, sessionId, stream);
}

// Utility to tune SDP bitrate: smooth 500kbps-8Mbps adaptive bandwidth for WAN & LAN without bufferbloat
function boostSdpBitrate(sdp: string, kbps: number = 8000): string {
  if (!sdp) return sdp;
  const bps = kbps * 1000;
  const lines = sdp.split(/\r\n|\n/);
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

    if (inVideo && !bandwidthAdded && (line.startsWith('c=') || line.startsWith('a='))) {
      result.push(`b=AS:${kbps}`);
      result.push(`b=TIAS:${bps}`);
      bandwidthAdded = true;
    }

    if (inVideo && line.startsWith('a=fmtp:')) {
      if (!line.includes('x-google-min-bitrate')) {
        result.push(`${line};x-google-min-bitrate=500;x-google-max-bitrate=${kbps};x-google-start-bitrate=2500`);
        continue;
      }
    }

    result.push(line);
  }

  return result.join('\r\n');
}

async function setupHostPeerConnection(
  clientDeviceId: string,
  sessionId: string,
  stream: MediaStream
) {
  cleanupPeerConnection();

  const pc = new RTCPeerConnection(ICE_SERVERS);
  activePeerConnection = pc;

  // Create DataChannel for low latency mouse/key/clipboard input
  const dc = pc.createDataChannel('raydrop_remote_input', { ordered: true });
  activeDataChannel = dc;
  setupHostDataChannel(dc);

  // Add Screen Share Video & System Audio Tracks
  const videoTrack = stream.getVideoTracks()[0];
  if (videoTrack && 'contentHint' in videoTrack) {
    (videoTrack as any).contentHint = 'detail';
  }
  const sender = pc.addTrack(videoTrack, stream);
  activeVideoSender = sender;

  for (const audioTrack of stream.getAudioTracks()) {
    pc.addTrack(audioTrack, stream);
  }

  // Set degradationPreference to maintain resolution with adaptive 500k-8M bitrate
  try {
    const params = sender.getParameters();
    (params as any).degradationPreference = 'maintain-resolution';
    if (params.encodings && params.encodings.length > 0) {
      params.encodings[0].maxBitrate = 8_000_000;
      params.encodings[0].minBitrate = 500_000;
      params.encodings[0].maxFramerate = 60;
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

  pc.onconnectionstatechange = () => {
    if (pc.connectionState === 'failed') {
      disconnectRemoteSession();
    }
  };

  // Create and send offer to client with boosted bitrate
  const offer = await pc.createOffer();
  const boostedOffer = {
    type: offer.type,
    sdp: boostSdpBitrate(offer.sdp || '', 8000),
  };
  await pc.setLocalDescription(boostedOffer);

  sendRemoteSignal(clientDeviceId, sessionId, {
    type: 'remote_webrtc_offer',
    sdp: boostedOffer,
  });
}

function setupHostDataChannel(dc: RTCDataChannel) {
  dc.onopen = async () => {
    try {
      const monitors = await raydropApi.getHostMonitors();
      if (dc.readyState === 'open') {
        dc.send(JSON.stringify({ t: 'monitors_list', monitors }));
      }
    } catch (_) {}
    // Start native WASAPI loopback audio streaming from host PC
    raydropApi.startHostAudioLoopback().catch((err) => {
      console.warn('Native host audio loopback start warning:', err);
    });
  };

  dc.onmessage = async (event) => {
    try {
      const data = JSON.parse(event.data);
      if (!data || !data.t) return;

      // Mouse Move
      if (data.t === 'mm') {
        raydropApi.simulateRemoteMouseMove(data.x, data.y).catch(() => {});
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
          .catch(() => {});
        return;
      }

      // Explicit release modifiers
      if (data.t === 'release_modifiers') {
        raydropApi.releaseRemoteModifiers().catch(() => {});
        return;
      }

      // Mouse Wheel
      if (data.t === 'mw') {
        raydropApi.simulateRemoteMouseWheel(data.dy).catch(() => {});
        return;
      }

      // Keyboard Key
      if (data.t === 'kk') {
        raydropApi.simulateRemoteKeyboardKey(data.k, data.c, data.a).catch(() => {});
        return;
      }

      // Shortcut
      if (data.t === 'sc') {
        raydropApi.simulateRemoteShortcut(data.s).catch(() => {});
        return;
      }

      // Clipboard Write from Remote Client (Text or Data URL Image)
      if (data.t === 'clip_write') {
        if (data.content) {
          await raydropApi.writeSystemClipboard(data.content).catch(() => {});
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
        } catch (_) {}
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
        raydropApi.setHostTargetMonitor(data.monitorIndex ?? 0).catch(() => {});
        return;
      }

      // Remote Power Management Action (lock, sleep, restart, shutdown)
      if (data.t === 'power_action' && data.action) {
        raydropApi.executeRemotePowerAction(data.action).catch(() => {});
        return;
      }
    } catch (_) {}
  };
}

// ----------------- Client Handling Logic -----------------

export async function connectToRemoteHost(targetDeviceId: string, pin: string) {
  clearAuthTimer();
  cleanupPeerConnection();

  const settings = get(appSettings);
  const myDeviceId = settings.device_id;
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

  // Timeout guard (45 seconds for manual approval, 35 seconds for PIN): If host is unreachable or not responding, abort cleanly
  const timeoutMs = pin.trim() ? 35000 : 45000;
  authTimeoutTimer = setTimeout(() => {
    const status = get(remoteConnectionStatus);
    if (status === 'verifying_pin' || status === 'connecting') {
      remoteConnectionStatus.set('error');
      remoteErrorMessage.set(
        pin.trim()
          ? 'Connection timed out. Host did not respond. Please ensure Raydrop is running on the host device and Allow Remote Access is enabled.'
          : 'Request timed out. Host did not accept the remote control request.'
      );
      cleanupPeerConnection();
    }
  }, timeoutMs);

  // Send authentication request with PIN
  sendRemoteSignal(targetDeviceId, sessionId, {
    type: 'remote_request_auth',
    sessionId,
    fromDeviceId: myDeviceId,
    fromAlias: myAlias,
    pin: pin.trim(),
  });
}

async function handleIncomingOffer(
  hostDeviceId: string,
  sessionId: string,
  sdp: RTCSessionDescriptionInit
) {
  cleanupPeerConnection();

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

  pc.onconnectionstatechange = () => {
    if (pc.connectionState === 'failed') {
      remoteConnectionStatus.set('disconnected');
    }
  };

  await pc.setRemoteDescription(new RTCSessionDescription(sdp));

  // Flush pending ICE candidates if any arrived before offer
  for (const c of pendingIceCandidates) {
    try {
      await pc.addIceCandidate(new RTCIceCandidate(c));
    } catch (_) {}
  }
  pendingIceCandidates = [];

  const answer = await pc.createAnswer();
  const boostedAnswer = {
    type: answer.type,
    sdp: boostSdpBitrate(answer.sdp || '', 15000),
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
    } catch (_) {}
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
      clientAudioCtx.resume().catch(() => {});
    }
  } catch (_) {}
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
      clientAudioCtx.resume().catch(() => {});
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

    const src = clientAudioCtx.createBufferSource();
    src.buffer = audioBuf;
    src.connect(clientAudioCtx.destination);

    const now = clientAudioCtx.currentTime;
    if (nextAudioPlayTime < now || nextAudioPlayTime - now > 0.25) {
      nextAudioPlayTime = now + 0.04;
    }
    src.start(nextAudioPlayTime);
    nextAudioPlayTime += audioBuf.duration;
  } catch (_) {}
}

function setupClientDataChannel(dc: RTCDataChannel) {
  dc.onopen = () => {
    remoteConnectionStatus.set('connected');
    // Start periodic ping to measure latency
    if (pingInterval) clearInterval(pingInterval);
    pingInterval = setInterval(() => {
      if (dc.readyState === 'open') {
        dc.send(JSON.stringify({ t: 'ping', time: Date.now() }));
      }
    }, 2000);
  };

  dc.onmessage = async (event) => {
    try {
      const data = JSON.parse(event.data);
      if (!data) return;

      // Pong latency measurement
      if (data.t === 'pong' && data.time) {
        const rtt = Math.max(1, Date.now() - data.time);
        remoteLatencyMs.set(rtt);
        return;
      }

      // Received clipboard from Host
      if (data.t === 'clip_read_res' && data.content) {
        await raydropApi.writeSystemClipboard(data.content).catch(() => {});
        alert('📋 Host clipboard contents copied to your local clipboard!');
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
    } catch (_) {}
  };

  dc.onclose = () => {
    if (pingInterval) clearInterval(pingInterval);
  };
}

// ----------------- Client Input Dispatchers -----------------

export function sendInputToHost(eventData: any) {
  if (activeDataChannel && activeDataChannel.readyState === 'open') {
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
  raydropApi.setHostCursorHidden(false).catch(() => {});
  stopBackgroundKeepAlive();
  const stream = get(hostScreenStream);
  if (stream) {
    stream.getTracks().forEach((t) => t.stop());
  }
  hostScreenStream.set(null);
  isHostSharingScreen.set(false);
}

function cleanupPeerConnection() {
  clearAuthTimer();
  raydropApi.stopHostAudioLoopback().catch(() => {});
  if (clientAudioCtx) {
    try {
      clientAudioCtx.close();
    } catch (_) {}
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
    } catch (_) {}
    activeDataChannel = null;
  }
  if (activePeerConnection) {
    try {
      activePeerConnection.close();
    } catch (_) {}
    activePeerConnection = null;
  }
  activeVideoSender = null;
  clientRemoteStream.set(null);
  pendingIceCandidates = [];
}

export function disconnectRemoteSession() {
  raydropApi.setHostCursorHidden(false).catch(() => {});
  stopBackgroundKeepAlive();
  clearAuthTimer();
  const session = get(activeRemoteSession);
  if (session) {
    sendRemoteSignal(session.peerDeviceId, session.sessionId, {
      type: 'remote_session_ended',
    });
  }

  cleanupPeerConnection();
  activeRemoteSession.set(null);
  remoteConnectionStatus.set('disconnected');
  remoteLatencyMs.set(0);
}
