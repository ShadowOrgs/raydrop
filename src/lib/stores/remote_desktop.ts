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

let activePeerConnection: RTCPeerConnection | null = null;
let activeDataChannel: RTCDataChannel | null = null;
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
}

// ----------------- Host Handling Logic -----------------

// Helper to acquire monitor screen stream safely
export async function acquireHostScreenStream(): Promise<MediaStream | null> {
  let stream = get(hostScreenStream);
  if (stream && stream.getVideoTracks().length > 0 && stream.getVideoTracks()[0].readyState === 'live') {
    return stream;
  }

  try {
    stream = await (navigator.mediaDevices as any).getDisplayMedia({
      video: {
        displaySurface: 'monitor',
        frameRate: { ideal: 60, max: 60 },
        width: { ideal: 1920, max: 2560 },
        height: { ideal: 1080, max: 1440 },
      },
      audio: false,
    });
  } catch (err) {
    console.warn('Standard monitor displayMedia failed, trying fallback:', err);
    try {
      stream = await (navigator.mediaDevices as any).getDisplayMedia({
        video: true,
        audio: false,
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
  try {
    const caps: any = track.getCapabilities ? track.getCapabilities() : {};
    if ('cursor' in caps) {
      await track.applyConstraints({ cursor: 'never' as any });
    }
  } catch (_) {}

  track.onended = () => {
    stopHostScreenCapture();
    disconnectRemoteSession();
  };

  hostScreenStream.set(stream);
  isHostSharingScreen.set(true);
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

  // 1. PIN matches -> Auto connect directly! No approval needed on host.
  if (enteredPin && enteredPin === hostPin) {
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
  const myAlias = get(appSettings).alias_name || 'Raydrop Host';

  // Ensure host screen stream is ready (auto-selects screen with cursor: 'never')
  const stream = await acquireHostScreenStream();
  if (!stream) {
    sendRemoteSignal(clientDeviceId, sessionId, {
      type: 'remote_auth_rejected',
      reason: 'Host screen capture could not be started or permission was cancelled.',
    });
    return;
  }

  // Accept auth and set active session
  activeRemoteSession.set({
    isHost: true,
    isClient: false,
    peerDeviceId: clientDeviceId,
    peerAlias: clientAlias,
    sessionId,
  });
  remoteConnectionStatus.set('connected');

  sendRemoteSignal(clientDeviceId, sessionId, {
    type: 'remote_auth_accepted',
    hostAlias: myAlias,
  });

  // Setup WebRTC Host Peer Connection
  await setupHostPeerConnection(clientDeviceId, sessionId, stream);
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

  // Add Screen Share Video Track
  const videoTrack = stream.getVideoTracks()[0];
  const sender = pc.addTrack(videoTrack, stream);

  // Set degradationPreference to maintain resolution for crisp text!
  try {
    const params = sender.getParameters();
    (params as any).degradationPreference = 'maintain-resolution';
    if (params.encodings && params.encodings.length > 0) {
      params.encodings[0].maxBitrate = 8_000_000;
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

  // Create and send offer to client
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);

  sendRemoteSignal(clientDeviceId, sessionId, {
    type: 'remote_webrtc_offer',
    sdp: offer,
  });
}

function setupHostDataChannel(dc: RTCDataChannel) {
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
        raydropApi.simulateRemoteMouseClick(data.b, data.a).catch(() => {});
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

  // Timeout guard (30 seconds for manual approval, 15 seconds for PIN): If host is unreachable or not responding, abort cleanly
  const timeoutMs = pin.trim() ? 15000 : 30000;
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
  await pc.setLocalDescription(answer);

  sendRemoteSignal(hostDeviceId, sessionId, {
    type: 'remote_webrtc_answer',
    sdp: answer,
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

// ----------------- Cleanup & Teardown -----------------

export function stopHostScreenCapture() {
  const stream = get(hostScreenStream);
  if (stream) {
    stream.getTracks().forEach((t) => t.stop());
  }
  hostScreenStream.set(null);
  isHostSharingScreen.set(false);
}

function cleanupPeerConnection() {
  clearAuthTimer();
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
  clientRemoteStream.set(null);
  pendingIceCandidates = [];
}

export function disconnectRemoteSession() {
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
