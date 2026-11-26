import { writable, get } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';
import { appSettings, activeMainTab } from './state';
import { myAvatar } from './avatars';
import { contacts } from './contacts';
import { playAppSound } from './presence';

export interface MeetingParticipant {
  deviceId: string;
  alias: string;
  avatar: string | null;
  isMuted: boolean;
  isCameraOn: boolean;
  isScreenSharing: boolean;
  isSpeaking: boolean;
  isHandRaised: boolean;
  stream: MediaStream | null;
}

export interface MeetingChatMsg {
  id: string;
  senderId: string;
  senderName: string;
  text: string;
  timestamp: number;
}

export interface MeetingInvite {
  roomId: string;
  roomTitle: string;
  fromDeviceId: string;
  fromAlias: string;
  fromAvatar: string | null;
}

export const isInMeeting = writable<boolean>(false);
export const currentRoomId = writable<string>('');
export const currentRoomTitle = writable<string>('');
export const isMicMuted = writable<boolean>(false);
export const isCameraOn = writable<boolean>(false);
export const isScreenSharing = writable<boolean>(false);
export const isHandRaised = writable<boolean>(false);
export const isPushToTalk = writable<boolean>(false);
export const isHoldingPtt = writable<boolean>(false);
export const isRecording = writable<boolean>(false);
export const recordingDuration = writable<number>(0);
export const participants = writable<Record<string, MeetingParticipant>>({});
export const meetingChatMessages = writable<MeetingChatMsg[]>([]);
export const floatingReactions = writable<Array<{ id: string; emoji: string; from: string }>>([]);
export const incomingInvite = writable<MeetingInvite | null>(null);
export const localAudioStream = writable<MediaStream | null>(null);
export const localVideoStream = writable<MediaStream | null>(null);

// Audio analyser for local speaking detection
let audioCtx: AudioContext | null = null;
let analyser: AnalyserNode | null = null;
let audioCheckInterval: any = null;
let recordingTimer: any = null;
let recordingDrawTimer: any = null;
let recordingAudioCtx: AudioContext | null = null;
let recordingCanvasStream: MediaStream | null = null;
let mediaRecorder: MediaRecorder | null = null;
let recordedChunks: Blob[] = [];

// Known peers in room or invited to room
const knownRoomPeers: Set<string> = new Set();

// WebRTC peer connections keyed by remote deviceId
const peerConnections: Map<string, RTCPeerConnection> = new Map();
const remoteStreams: Map<string, MediaStream> = new Map();
// Queue for ICE candidates that arrive before setRemoteDescription
const pendingCandidates: Map<string, RTCIceCandidateInit[]> = new Map();

const ICE_SERVERS: RTCConfiguration = {
  iceServers: [
    { urls: 'stun:stun.raydrop.web.id:3478' },
    { urls: 'stun:stun.l.google.com:19302' },
    { urls: 'stun:stun1.l.google.com:19302' },
    { urls: 'stun:stun2.l.google.com:19302' },
  ],
  iceCandidatePoolSize: 10,
};

function getMyInfo() {
  const settings = get(appSettings);
  const avatar = get(myAvatar);
  return {
    deviceId: settings.device_id,
    alias: settings.alias_name || 'Raydrop User',
    avatar: avatar || null,
  };
}

export async function initLocalAudio() {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
      video: false,
    });
    localAudioStream.set(stream);

    // Setup speaking level analyser
    try {
      audioCtx = new (window.AudioContext || (window as any).webkitAudioContext)();
      const source = audioCtx.createMediaStreamSource(stream);
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = 512;
      source.connect(analyser);

      const dataArray = new Uint8Array(analyser.frequencyBinCount);
      let wasSpeaking = false;

      audioCheckInterval = setInterval(() => {
        if (!analyser || get(isMicMuted)) {
          if (wasSpeaking) {
            wasSpeaking = false;
            broadcastState({ isSpeaking: false });
          }
          return;
        }

        analyser.getByteFrequencyData(dataArray);
        let sum = 0;
        for (let i = 0; i < dataArray.length; i++) {
          sum += dataArray[i];
        }
        const avg = sum / dataArray.length;
        const isSpeaking = avg > 20;

        if (isSpeaking !== wasSpeaking) {
          wasSpeaking = isSpeaking;
          broadcastState({ isSpeaking });
        }
      }, 200);
    } catch (e) {
      console.warn('AudioContext speaking detection skipped:', e);
    }

    return stream;
  } catch (err) {
    console.warn('Failed to get user microphone:', err);
    return null;
  }
}

export async function createMeetingRoom(title = 'Online Meeting'): Promise<string> {
  const roomId = 'room_' + Math.random().toString(36).substring(2, 9);
  await joinMeetingRoom(roomId, title);
  return roomId;
}

export async function joinMeetingRoom(roomId: string, title = 'Online Meeting', hostDeviceId?: string) {
  leaveMeetingRoom();

  currentRoomId.set(roomId);
  currentRoomTitle.set(title);
  isInMeeting.set(true);
  participants.set({});
  meetingChatMessages.set([]);
  pendingCandidates.clear();

  await initLocalAudio();

  const joinPayload = {
    type: 'room_join',
    roomId,
    user: getMyInfo(),
    isMuted: get(isMicMuted),
    isCameraOn: get(isCameraOn),
    isScreenSharing: get(isScreenSharing),
  };

  if (hostDeviceId) {
    knownRoomPeers.add(hostDeviceId);
    sendDirectSignal(hostDeviceId, joinPayload);
  } else {
    // Announce to all contacts and known peers
    const allContacts = get(contacts);
    const targetIds = new Set<string>(knownRoomPeers);
    allContacts.forEach((c) => targetIds.add(c.device_id));
    targetIds.forEach((tid) => {
      if (tid && tid !== getMyInfo().deviceId) {
        sendDirectSignal(tid, joinPayload);
      }
    });
  }
}

export function leaveMeetingRoom() {
  const roomId = get(currentRoomId);
  if (roomId) {
    broadcastRoomSignal({
      type: 'room_leave',
      deviceId: get(appSettings).device_id,
    });
  }

  // Close all peer connections
  peerConnections.forEach((pc) => {
    try {
      pc.close();
    } catch (_) { }
  });
  peerConnections.clear();
  remoteStreams.clear();
  pendingCandidates.clear();
  knownRoomPeers.clear();

  // Stop local tracks
  const aStream = get(localAudioStream);
  if (aStream) {
    aStream.getTracks().forEach((t) => t.stop());
    localAudioStream.set(null);
  }

  const vStream = get(localVideoStream);
  if (vStream) {
    vStream.getTracks().forEach((t) => t.stop());
    localVideoStream.set(null);
  }

  if (audioCheckInterval) clearInterval(audioCheckInterval);
  if (audioCtx) {
    audioCtx.close().catch(() => { });
    audioCtx = null;
  }

  if (get(isRecording)) {
    stopRecording();
  }

  isInMeeting.set(false);
  currentRoomId.set('');
  currentRoomTitle.set('');
  participants.set({});
  isScreenSharing.set(false);
  isCameraOn.set(false);
  isMicMuted.set(false);
  isHandRaised.set(false);
}

export async function inviteContactToMeeting(target: string | string[]) {
  const ids = Array.isArray(target) ? target : [target];
  const roomId = get(currentRoomId);
  const title = get(currentRoomTitle);
  const me = getMyInfo();

  for (const targetDeviceId of ids) {
    if (!targetDeviceId) continue;
    knownRoomPeers.add(targetDeviceId);

    await raydropApi.sendMeetingSignal(
      targetDeviceId,
      `meeting:${roomId}`,
      {
        type: 'meeting_invite',
        roomId,
        roomTitle: title,
        fromDeviceId: me.deviceId,
        fromAlias: me.alias,
        fromAvatar: me.avatar,
      }
    ).catch(() => { });
  }
}

export function toggleMicMute() {
  const current = get(isMicMuted);
  const next = !current;
  isMicMuted.set(next);

  const stream = get(localAudioStream);
  if (stream) {
    stream.getAudioTracks().forEach((t) => (t.enabled = !next));
  }

  broadcastState({ isMuted: next });
}

export async function toggleCamera() {
  const current = get(isCameraOn);
  if (current) {
    // Turn off camera
    const vStream = get(localVideoStream);
    if (vStream) {
      vStream.getVideoTracks().forEach((t) => t.stop());
      localVideoStream.set(null);
    }
    isCameraOn.set(false);
    removeTrackFromPeers('video');
    broadcastState({ isCameraOn: false });
  } else {
    // Turn on camera
    try {
      const vStream = await navigator.mediaDevices.getUserMedia({
        video: { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
        audio: false,
      });
      localVideoStream.set(vStream);
      isCameraOn.set(true);
      isScreenSharing.set(false);
      addTrackToPeers(vStream.getVideoTracks()[0]);
      broadcastState({ isCameraOn: true, isScreenSharing: false });
    } catch (err) {
      console.warn('Failed to access camera:', err);
    }
  }
}

export async function toggleScreenShare() {
  const current = get(isScreenSharing);
  if (current) {
    // Turn off screen share
    const vStream = get(localVideoStream);
    if (vStream) {
      vStream.getVideoTracks().forEach((t) => t.stop());
      localVideoStream.set(null);
    }
    isScreenSharing.set(false);
    removeTrackFromPeers('video');
    broadcastState({ isScreenSharing: false });
  } else {
    // Start screen share (allow choosing any monitor, app window, or browser surface)
    try {
      const displayStream = await (navigator.mediaDevices as any).getDisplayMedia({
        video: {
          width: { ideal: 2560, max: 3840 },
          height: { ideal: 1440, max: 2160 },
          frameRate: { ideal: 30, max: 30 },
        },
        audio: false,
        surfaceSwitching: 'include',
        selfBrowserSurface: 'include',
        monitorTypeSurfaces: 'include',
      });

      const shareTrack = displayStream.getVideoTracks()[0];
      if (shareTrack && 'contentHint' in shareTrack) {
        (shareTrack as any).contentHint = 'text';
      }

      // Handle user clicking native browser "Stop sharing" bar
      shareTrack.onended = () => {
        isScreenSharing.set(false);
        removeTrackFromPeers('video');
        broadcastState({ isScreenSharing: false });
      };

      localVideoStream.set(displayStream);
      isScreenSharing.set(true);
      isCameraOn.set(false);
      addTrackToPeers(displayStream.getVideoTracks()[0]);
      broadcastState({ isScreenSharing: true, isCameraOn: false });
    } catch (err) {
      console.warn('Failed to share screen:', err);
    }
  }
}

export async function switchMeetingScreenShare() {
  try {
    const displayStream = await (navigator.mediaDevices as any).getDisplayMedia({
      video: {
        width: { ideal: 2560, max: 3840 },
        height: { ideal: 1440, max: 2160 },
        frameRate: { ideal: 30, max: 30 },
      },
      audio: false,
      surfaceSwitching: 'include',
      selfBrowserSurface: 'include',
      monitorTypeSurfaces: 'include',
    });

    const shareTrack = displayStream.getVideoTracks()[0];
    if (shareTrack && 'contentHint' in shareTrack) {
      (shareTrack as any).contentHint = 'text';
    }

    const oldStream = get(localVideoStream);
    if (oldStream) {
      oldStream.getVideoTracks().forEach((t) => t.stop());
    }

    shareTrack.onended = () => {
      isScreenSharing.set(false);
      removeTrackFromPeers('video');
      broadcastState({ isScreenSharing: false });
    };

    localVideoStream.set(displayStream);
    isScreenSharing.set(true);
    isCameraOn.set(false);
    addTrackToPeers(displayStream.getVideoTracks()[0]);
    broadcastState({ isScreenSharing: true, isCameraOn: false });
  } catch (err) {
    console.warn('Failed to switch shared screen:', err);
  }
}

export function toggleHandRaise() {
  const current = get(isHandRaised);
  const next = !current;
  isHandRaised.set(next);
  broadcastState({ isHandRaised: next });
}

export function sendMeetingReaction(emoji: string) {
  const me = getMyInfo();
  const id = Math.random().toString(36).substring(2, 9);
  floatingReactions.update((list) => [...list, { id, emoji, from: me.alias }]);
  setTimeout(() => {
    floatingReactions.update((list) => list.filter((r) => r.id !== id));
  }, 3500);

  broadcastRoomSignal({
    type: 'reaction',
    id,
    emoji,
    from: me.alias,
  });
}

export function sendMeetingChat(text: string) {
  if (!text.trim()) return;
  const me = getMyInfo();
  const msg: MeetingChatMsg = {
    id: Math.random().toString(36).substring(2, 9),
    senderId: me.deviceId,
    senderName: me.alias,
    text: text.trim(),
    timestamp: Date.now(),
  };

  meetingChatMessages.update((msgs) => [...msgs, msg]);
  broadcastRoomSignal({
    type: 'meeting_chat',
    msg,
  });
}

function cleanupRecordingResources(videoCache: Map<string, HTMLVideoElement>) {
  if (recordingDrawTimer) {
    clearInterval(recordingDrawTimer);
    recordingDrawTimer = null;
  }
  if (recordingCanvasStream) {
    recordingCanvasStream.getTracks().forEach((t) => t.stop());
    recordingCanvasStream = null;
  }
  if (recordingAudioCtx) {
    recordingAudioCtx.close().catch(() => {});
    recordingAudioCtx = null;
  }
  for (const v of videoCache.values()) {
    v.srcObject = null;
  }
  videoCache.clear();
}

export function startRecording() {
  recordedChunks = [];
  const videoCache = new Map<string, HTMLVideoElement>();
  try {
    const aStream = get(localAudioStream);
    const vStream = get(localVideoStream);
    const peersMap = get(participants);

    const hasAnyAudio =
      (aStream && aStream.getAudioTracks().length > 0) ||
      Object.values(peersMap).some((p) => p.stream && p.stream.getAudioTracks().length > 0);
    const hasAnyVideo =
      (vStream && vStream.getVideoTracks().length > 0) ||
      Object.values(peersMap).some((p) => p.stream && p.stream.getVideoTracks().length > 0);

    if (!hasAnyAudio && !hasAnyVideo) {
      alert('No audio or video stream available to record.');
      return;
    }

    // 1. Canvas video compositor (captures local camera/screen + remote participants)
    const canvas = document.createElement('canvas');
    canvas.width = 1280;
    canvas.height = 720;
    const ctx = canvas.getContext('2d');

    const getOrCreateVideo = (stream: MediaStream): HTMLVideoElement => {
      const existing = videoCache.get(stream.id);
      if (existing) return existing;
      const vid = document.createElement('video');
      vid.muted = true;
      vid.playsInline = true;
      vid.autoplay = true;
      vid.srcObject = stream;
      vid.play().catch(() => {});
      videoCache.set(stream.id, vid);
      return vid;
    };

    const drawFrame = () => {
      if (!ctx) return;
      ctx.fillStyle = '#020617';
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      const curLocalVid = get(localVideoStream);
      const curPeers = Object.values(get(participants));
      const sources: Array<{ label: string; stream: MediaStream; isScreen: boolean }> = [];

      if (curLocalVid && curLocalVid.getVideoTracks().some((t) => t.readyState === 'live')) {
        sources.push({
          label: get(appSettings).alias_name || 'You',
          stream: curLocalVid,
          isScreen: get(isScreenSharing),
        });
      }
      for (const p of curPeers) {
        if (p.stream && p.stream.getVideoTracks().some((t) => t.readyState === 'live')) {
          sources.push({
            label: p.alias,
            stream: p.stream,
            isScreen: p.isScreenSharing,
          });
        }
      }

      if (sources.length === 0) {
        ctx.fillStyle = '#0f172a';
        ctx.fillRect(40, 40, canvas.width - 80, canvas.height - 80);
        ctx.fillStyle = '#38bdf8';
        ctx.font = 'bold 34px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(get(currentRoomTitle) || 'Raydrop Meeting', canvas.width / 2, canvas.height / 2 - 12);
        ctx.fillStyle = '#94a3b8';
        ctx.font = '20px sans-serif';
        ctx.fillText('Recording Audio (No Active Camera / Screen)', canvas.width / 2, canvas.height / 2 + 28);
        return;
      }

      const drawContained = (
        vid: HTMLVideoElement,
        x: number,
        y: number,
        w: number,
        h: number,
        label: string
      ) => {
        ctx.fillStyle = '#0f172a';
        ctx.fillRect(x, y, w, h);
        if (vid.videoWidth > 0 && vid.videoHeight > 0) {
          const scale = Math.min(w / vid.videoWidth, h / vid.videoHeight);
          const dw = vid.videoWidth * scale;
          const dh = vid.videoHeight * scale;
          const dx = x + (w - dw) / 2;
          const dy = y + (h - dh) / 2;
          ctx.drawImage(vid, dx, dy, dw, dh);
        }
        ctx.fillStyle = 'rgba(2, 6, 23, 0.72)';
        ctx.fillRect(x + 10, y + h - 34, Math.min(220, w - 20), 24);
        ctx.fillStyle = '#f8fafc';
        ctx.font = 'bold 13px sans-serif';
        ctx.textAlign = 'left';
        ctx.fillText(label, x + 18, y + h - 18);
      };

      const screenSource = sources.find((s) => s.isScreen);
      if (screenSource) {
        drawContained(getOrCreateVideo(screenSource.stream), 0, 0, canvas.width, canvas.height, `${screenSource.label} (Screen)`);
        const cams = sources.filter((s) => s !== screenSource).slice(0, 3);
        cams.forEach((cam, idx) => {
          const pipW = 240;
          const pipH = 135;
          const pipX = canvas.width - pipW - 16;
          const pipY = canvas.height - (pipH + 12) * (idx + 1);
          drawContained(getOrCreateVideo(cam.stream), pipX, pipY, pipW, pipH, cam.label);
        });
      } else {
        const cols = sources.length === 1 ? 1 : sources.length <= 4 ? 2 : 3;
        const rows = Math.ceil(sources.length / cols);
        const cellW = canvas.width / cols;
        const cellH = canvas.height / rows;
        sources.forEach((src, i) => {
          const col = i % cols;
          const row = Math.floor(i / cols);
          drawContained(getOrCreateVideo(src.stream), col * cellW, row * cellH, cellW, cellH, src.label);
        });
      }
    };

    drawFrame();
    recordingDrawTimer = setInterval(drawFrame, 42); // ~24 fps
    recordingCanvasStream = canvas.captureStream(24);

    // 2. Mix local + remote audio tracks via AudioContext
    const combinedTracks: MediaStreamTrack[] = [...recordingCanvasStream.getVideoTracks()];
    try {
      const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
      recordingAudioCtx = new AudioCtx();
      const dest = recordingAudioCtx.createMediaStreamDestination();
      let connectedAudioCount = 0;

      if (aStream && aStream.getAudioTracks().length > 0) {
        recordingAudioCtx.createMediaStreamSource(aStream).connect(dest);
        connectedAudioCount++;
      }
      for (const p of Object.values(peersMap)) {
        if (p.stream && p.stream.getAudioTracks().length > 0) {
          recordingAudioCtx.createMediaStreamSource(p.stream).connect(dest);
          connectedAudioCount++;
        }
      }
      if (connectedAudioCount > 0) {
        combinedTracks.push(...dest.stream.getAudioTracks());
      }
    } catch (_) {
      if (aStream) combinedTracks.push(...aStream.getAudioTracks());
    }

    const combinedStream = new MediaStream(combinedTracks);
    const mimeCandidates = [
      'video/webm; codecs=vp9,opus',
      'video/webm; codecs=vp8,opus',
      'video/webm',
    ];
    const mime = mimeCandidates.find((m) => MediaRecorder.isTypeSupported(m)) || '';

    mediaRecorder = mime
      ? new MediaRecorder(combinedStream, { mimeType: mime, videoBitsPerSecond: 2_500_000 })
      : new MediaRecorder(combinedStream);
    mediaRecorder.ondataavailable = (e) => {
      if (e.data.size > 0) recordedChunks.push(e.data);
    };

    mediaRecorder.onstop = () => {
      cleanupRecordingResources(videoCache);
      const blob = new Blob(recordedChunks, { type: 'video/webm' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `Raydrop_Meeting_${new Date().toISOString().slice(0, 10)}.webm`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10000);
    };

    mediaRecorder.start(1000);
    isRecording.set(true);
    recordingDuration.set(0);

    recordingTimer = setInterval(() => {
      recordingDuration.update((d) => d + 1);
    }, 1000);
  } catch (err) {
    cleanupRecordingResources(videoCache);
    console.warn('Failed to start recording:', err);
  }
}

export function stopRecording() {
  if (mediaRecorder && mediaRecorder.state !== 'inactive') {
    mediaRecorder.stop();
  }
  isRecording.set(false);
  if (recordingTimer) clearInterval(recordingTimer);
}

// ----------------- WebRTC Peer Handshake & Signaling -----------------

async function addTrackToPeers(track: MediaStreamTrack) {
  for (const [peerId, pc] of peerConnections.entries()) {
    try {
      const transceivers = pc.getTransceivers();
      let transceiver = transceivers.find(
        (t) =>
          (t.sender.track && t.sender.track.kind === track.kind) ||
          (t.receiver.track && t.receiver.track.kind === track.kind)
      );

      if (transceiver) {
        transceiver.direction = 'sendrecv';
        await transceiver.sender.replaceTrack(track);
      } else {
        const senders = pc.getSenders();
        const existingSender = senders.find((s) => s.track && s.track.kind === track.kind);
        if (existingSender) {
          await existingSender.replaceTrack(track);
        } else {
          const stream = get(localVideoStream) || new MediaStream([track]);
          const addedSender = pc.addTrack(track, stream);
          transceiver = pc.getTransceivers().find((t) => t.sender === addedSender);
        }
      }
      if (track.kind === 'video') {
        const activeSender =
          transceiver?.sender || pc.getSenders().find((s) => s.track === track);
        if (activeSender) {
          try {
            const caps = RTCRtpSender.getCapabilities?.('video');
            if (caps?.codecs && transceiver?.setCodecPreferences) {
              const preferredOrder = ['video/VP9', 'video/H264', 'video/AV1', 'video/VP8'];
              const sorted = [...caps.codecs].sort((a, b) => {
                const ia = preferredOrder.indexOf(a.mimeType);
                const ib = preferredOrder.indexOf(b.mimeType);
                return (ia === -1 ? 99 : ia) - (ib === -1 ? 99 : ib);
              });
              transceiver.setCodecPreferences(sorted);
            }
          } catch (_) { }
          try {
            const p = activeSender.getParameters();
            (p as any).degradationPreference = 'maintain-resolution';
            if (p.encodings && p.encodings.length > 0) {
              p.encodings[0].maxBitrate = 15_000_000;
              p.encodings[0].minBitrate = 2_500_000;
            }
            await activeSender.setParameters(p);
          } catch (_) { }
        }
      }
      await createAndSendOffer(peerId);
    } catch (err) {
      console.warn(`Failed to add track to peer ${peerId}:`, err);
    }
  }
}

async function removeTrackFromPeers(kind: 'audio' | 'video') {
  for (const [peerId, pc] of peerConnections.entries()) {
    try {
      const transceivers = pc.getTransceivers();
      let transceiver = transceivers.find(
        (t) =>
          (t.sender.track && t.sender.track.kind === kind) ||
          (t.receiver.track && t.receiver.track.kind === kind)
      );
      if (transceiver) {
        await transceiver.sender.replaceTrack(null);
        transceiver.direction = 'recvonly';
      } else {
        const senders = pc.getSenders();
        const sender = senders.find((s) => s.track && s.track.kind === kind);
        if (sender) {
          await sender.replaceTrack(null);
        }
      }
      await createAndSendOffer(peerId);
    } catch (err) {
      console.warn(`Failed to remove track from peer ${peerId}:`, err);
    }
  }
}

function getOrCreatePeerConnection(peerDeviceId: string): RTCPeerConnection {
  let pc = peerConnections.get(peerDeviceId);
  if (pc) return pc;

  pc = new RTCPeerConnection(ICE_SERVERS);
  peerConnections.set(peerDeviceId, pc);

  // Pre-negotiate audio and video transceivers so track replacement (screen share / camera) works seamlessly without renegotiation
  try {
    pc.addTransceiver('audio', { direction: 'sendrecv' });
    pc.addTransceiver('video', { direction: 'sendrecv' });
  } catch (err) {
    console.warn('Transceiver add warning:', err);
  }

  // Attach local audio track if ready
  const aStream = get(localAudioStream);
  if (aStream && aStream.getAudioTracks().length > 0) {
    const aTransceiver = pc.getTransceivers().find(
      (t) =>
        (t.sender.track && t.sender.track.kind === 'audio') ||
        (t.receiver.track && t.receiver.track.kind === 'audio')
    );
    if (aTransceiver) {
      aTransceiver.sender.replaceTrack(aStream.getAudioTracks()[0]).catch(() => { });
    } else {
      pc.addTrack(aStream.getAudioTracks()[0], aStream);
    }
  }

  // Attach local video track if ready
  const vStream = get(localVideoStream);
  if (vStream && vStream.getVideoTracks().length > 0) {
    const vTransceiver = pc.getTransceivers().find(
      (t) =>
        (t.sender.track && t.sender.track.kind === 'video') ||
        (t.receiver.track && t.receiver.track.kind === 'video')
    );
    if (vTransceiver) {
      vTransceiver.sender.replaceTrack(vStream.getVideoTracks()[0]).catch(() => { });
    } else {
      pc.addTrack(vStream.getVideoTracks()[0], vStream);
    }
  }

  // ICE candidates
  pc.onicecandidate = (event) => {
    if (event.candidate) {
      sendDirectSignal(peerDeviceId, {
        type: 'webrtc_candidate',
        candidate: event.candidate,
      });
    }
  };

  // Remote tracks
  pc.ontrack = (event) => {
    let stream = remoteStreams.get(peerDeviceId);
    if (!stream) {
      stream = new MediaStream();
      remoteStreams.set(peerDeviceId, stream);
    }

    // Replace old track of same kind (e.g. old camera track replaced by active screen track)
    const oldTracks = stream.getTracks().filter((t) => t.kind === event.track.kind);
    for (const old of oldTracks) {
      if (old.id !== event.track.id) {
        stream.removeTrack(old);
      }
    }

    if (!stream.getTracks().some((t) => t.id === event.track.id)) {
      stream.addTrack(event.track);
    }

    const pushFreshStream = () => {
      const freshStream = new MediaStream(stream!.getTracks());
      participants.update((map) => {
        if (map[peerDeviceId]) {
          map[peerDeviceId].stream = freshStream;
        }
        return { ...map };
      });
    };

    event.track.onunmute = pushFreshStream;
    event.track.onended = pushFreshStream;
    event.track.onmute = pushFreshStream;
    pushFreshStream();
  };

  // Only handle terminal failure (never delete participant on transient 'disconnected')
  pc.onconnectionstatechange = () => {
    if (pc?.connectionState === 'failed') {
      setTimeout(() => {
        if (pc?.connectionState === 'failed') {
          participants.update((map) => {
            delete map[peerDeviceId];
            return { ...map };
          });
          peerConnections.delete(peerDeviceId);
        }
      }, 5000);
    }
  };

  return pc;
}

function boostMeetingSdpBitrate(sdp: string, kbps: number = 15000): string {
  if (!sdp) return sdp;
  const bitrateParams = `x-google-min-bitrate=2500;x-google-max-bitrate=${kbps};x-google-start-bitrate=8000`;
  const lines = sdp.split(/\r\n|\n/);
  const videoPts = new Set<string>();
  const existingFmtpPts = new Set<string>();
  let scanning = false;
  for (const l of lines) {
    if (l.startsWith('m=video')) { scanning = true; continue; }
    if (l.startsWith('m=')) { scanning = false; continue; }
    if (!scanning) continue;
    const rm = /^a=rtpmap:(\d+)\s+([A-Za-z0-9_-]+)/.exec(l);
    if (rm) {
      const codec = rm[2].toUpperCase();
      if (codec !== 'RTX' && codec !== 'RED' && codec !== 'ULPFEC' && codec !== 'FLEXFEC') {
        videoPts.add(rm[1]);
      }
    }
    const fm = /^a=fmtp:(\d+)\s+/.exec(l);
    if (fm) existingFmtpPts.add(fm[1]);
  }
  const out: string[] = [];
  let inVideo = false;
  let bwAdded = false;
  for (const line of lines) {
    if (line.startsWith('m=video')) { inVideo = true; bwAdded = false; }
    else if (line.startsWith('m=')) { inVideo = false; }
    if (inVideo && (line.startsWith('b=AS:') || line.startsWith('b=TIAS:'))) continue;
    if (inVideo && !bwAdded && line.startsWith('c=IN ')) {
      out.push(line);
      out.push(`b=AS:${kbps}`);
      out.push(`b=TIAS:${kbps * 1000}`);
      bwAdded = true;
      continue;
    }
    if (inVideo && line.startsWith('a=fmtp:')) {
      const m = /^a=fmtp:(\d+)\s+/.exec(line);
      if (m && videoPts.has(m[1]) && !line.includes('x-google-min-bitrate')) {
        out.push(`${line};${bitrateParams}`);
        continue;
      }
    }
    out.push(line);
    if (inVideo && line.startsWith('a=rtpmap:')) {
      const m = /^a=rtpmap:(\d+)\s+/.exec(line);
      if (m && videoPts.has(m[1]) && !existingFmtpPts.has(m[1])) {
        out.push(`a=fmtp:${m[1]} ${bitrateParams}`);
      }
    }
  }
  return out.join('\r\n');
}

async function createAndSendOffer(peerDeviceId: string) {
  const pc = getOrCreatePeerConnection(peerDeviceId);
  if (pc.signalingState !== 'stable') {
    await new Promise((r) => setTimeout(r, 150));
    if (pc.signalingState !== 'stable') return;
  }
  try {
    const rawOffer = await pc.createOffer();
    const offer = { type: rawOffer.type, sdp: boostMeetingSdpBitrate(rawOffer.sdp || '', 15000) };
    await pc.setLocalDescription(offer);
    sendDirectSignal(peerDeviceId, {
      type: 'webrtc_offer',
      sdp: offer,
    });
  } catch (err) {
    console.warn(`Failed to create offer for ${peerDeviceId}:`, err);
  }
}

async function flushPendingCandidates(peerDeviceId: string, pc: RTCPeerConnection) {
  const queue = pendingCandidates.get(peerDeviceId);
  if (!queue || queue.length === 0) return;
  for (const cand of queue) {
    try {
      await pc.addIceCandidate(new RTCIceCandidate(cand));
    } catch (err) {
      console.warn(`Failed to add queued candidate for ${peerDeviceId}:`, err);
    }
  }
  pendingCandidates.delete(peerDeviceId);
}

async function handleReceivedOffer(peerDeviceId: string, sdp: RTCSessionDescriptionInit) {
  const pc = getOrCreatePeerConnection(peerDeviceId);
  try {
    if (pc.signalingState !== 'stable') {
      await Promise.all([
        pc.setLocalDescription({ type: 'rollback' } as any),
        pc.setRemoteDescription(new RTCSessionDescription(sdp)),
      ]).catch(async () => {
        await pc.setRemoteDescription(new RTCSessionDescription(sdp));
      });
    } else {
      await pc.setRemoteDescription(new RTCSessionDescription(sdp));
    }
    await flushPendingCandidates(peerDeviceId, pc);
    const rawAnswer = await pc.createAnswer();
    const answer = { type: rawAnswer.type, sdp: boostMeetingSdpBitrate(rawAnswer.sdp || '', 15000) };
    await pc.setLocalDescription(answer);
    sendDirectSignal(peerDeviceId, {
      type: 'webrtc_answer',
      sdp: answer,
    });
  } catch (err) {
    console.warn(`Failed to answer offer from ${peerDeviceId}:`, err);
  }
}

async function handleReceivedAnswer(peerDeviceId: string, sdp: RTCSessionDescriptionInit) {
  const pc = peerConnections.get(peerDeviceId);
  if (!pc) return;
  try {
    await pc.setRemoteDescription(new RTCSessionDescription(sdp));
    await flushPendingCandidates(peerDeviceId, pc);
  } catch (err) {
    console.warn(`Failed to set remote answer for ${peerDeviceId}:`, err);
  }
}

async function handleReceivedCandidate(peerDeviceId: string, candidate: RTCIceCandidateInit) {
  const pc = peerConnections.get(peerDeviceId);
  if (!pc || !pc.remoteDescription) {
    const queue = pendingCandidates.get(peerDeviceId) || [];
    queue.push(candidate);
    pendingCandidates.set(peerDeviceId, queue);
    return;
  }
  try {
    await pc.addIceCandidate(new RTCIceCandidate(candidate));
  } catch (err) {
    console.warn(`Failed to add ICE candidate for ${peerDeviceId}:`, err);
  }
}

function broadcastState(state: Partial<MeetingParticipant>) {
  const myDevId = get(appSettings).device_id;
  broadcastRoomSignal({
    type: 'state_update',
    deviceId: myDevId,
    ...state,
  });
}

function broadcastRoomSignal(payload: any) {
  const parts = get(participants);
  const roomId = get(currentRoomId);
  if (!roomId) return;

  const targetIds = new Set<string>([
    ...Object.keys(parts),
    ...Array.from(knownRoomPeers),
  ]);

  targetIds.forEach((targetId) => {
    if (targetId && targetId !== getMyInfo().deviceId) {
      sendDirectSignal(targetId, payload);
    }
  });
}

function sendDirectSignal(targetDeviceId: string, payload: any) {
  const roomId = get(currentRoomId);
  if (!roomId || !targetDeviceId) return;

  raydropApi.sendMeetingSignal(
    targetDeviceId,
    `meeting:${roomId}`,
    payload
  ).catch((err) => {
    console.warn(`Failed to send meeting signal to ${targetDeviceId}:`, err);
  });
}

// ----------------- Global Meeting Signal Listener -----------------

export function setupMeetingSignalListener() {
  raydropApi.listenEvent<{
    from_device_id: string;
    session_id: string;
    payload: any;
  }>('meeting_signal_received', async (event) => {
    const fromId = event.from_device_id;
    const data = event.payload;
    if (!data || !fromId) return;

    // 1. Meeting Invite
    if (data.type === 'meeting_invite') {
      playAppSound('meeting_invite');
      incomingInvite.set({
        roomId: data.roomId,
        roomTitle: data.roomTitle || 'Online Meeting',
        fromDeviceId: data.fromDeviceId || fromId,
        fromAlias: data.fromAlias || 'Contact',
        fromAvatar: data.fromAvatar || null,
      });
      return;
    }

    const myRoomId = get(currentRoomId);
    if (!myRoomId || !get(isInMeeting)) return;

    // 2. Peer Joining Room
    if (data.type === 'room_join') {
      if (data.roomId && data.roomId !== myRoomId) return;

      const u = data.user;
      const isNewJoiner = !knownRoomPeers.has(fromId);
      knownRoomPeers.add(fromId);
      if (isNewJoiner) {
        playAppSound('meeting_join');
      }

      participants.update((map) => {
        map[fromId] = {
          deviceId: fromId,
          alias: u?.alias || 'Peer',
          avatar: u?.avatar || null,
          isMuted: data.isMuted ?? false,
          isCameraOn: data.isCameraOn ?? false,
          isScreenSharing: data.isScreenSharing ?? false,
          isSpeaking: false,
          isHandRaised: false,
          stream: remoteStreams.get(fromId) || null,
        };
        return { ...map };
      });

      // Existing participants excluding new joiner
      const currentParts = Object.values(get(participants))
        .filter((p) => p.deviceId !== fromId)
        .map((p) => ({
          deviceId: p.deviceId,
          alias: p.alias,
          avatar: p.avatar,
          isMuted: p.isMuted,
          isCameraOn: p.isCameraOn,
          isScreenSharing: p.isScreenSharing,
        }));

      // Reply with our info and existing room participants
      sendDirectSignal(fromId, {
        type: 'room_welcome',
        roomId: myRoomId,
        user: getMyInfo(),
        isMuted: get(isMicMuted),
        isCameraOn: get(isCameraOn),
        isScreenSharing: get(isScreenSharing),
        existingParticipants: currentParts,
      });

      // Initiate WebRTC offer to new joiner
      await createAndSendOffer(fromId);
      return;
    }

    // 3. Room Welcome (response to join)
    if (data.type === 'room_welcome') {
      if (data.roomId && data.roomId !== myRoomId) return;

      const u = data.user;
      const isNewJoiner = !knownRoomPeers.has(fromId);
      knownRoomPeers.add(fromId);
      if (isNewJoiner) {
        playAppSound('meeting_join');
      }

      participants.update((map) => {
        map[fromId] = {
          deviceId: fromId,
          alias: u?.alias || 'Host',
          avatar: u?.avatar || null,
          isMuted: data.isMuted ?? false,
          isCameraOn: data.isCameraOn ?? false,
          isScreenSharing: data.isScreenSharing ?? false,
          isSpeaking: false,
          isHandRaised: false,
          stream: remoteStreams.get(fromId) || null,
        };
        return { ...map };
      });

      // Also mesh with other participants in the room
      if (Array.isArray(data.existingParticipants)) {
        for (const peer of data.existingParticipants) {
          if (peer.deviceId && peer.deviceId !== getMyInfo().deviceId && !get(participants)[peer.deviceId]) {
            knownRoomPeers.add(peer.deviceId);
            participants.update((map) => {
              map[peer.deviceId] = {
                deviceId: peer.deviceId,
                alias: peer.alias || 'Peer',
                avatar: peer.avatar || null,
                isMuted: peer.isMuted ?? false,
                isCameraOn: peer.isCameraOn ?? false,
                isScreenSharing: peer.isScreenSharing ?? false,
                isSpeaking: false,
                isHandRaised: false,
                stream: remoteStreams.get(peer.deviceId) || null,
              };
              return { ...map };
            });
            sendDirectSignal(peer.deviceId, {
              type: 'room_join',
              roomId: myRoomId,
              user: getMyInfo(),
              isMuted: get(isMicMuted),
              isCameraOn: get(isCameraOn),
              isScreenSharing: get(isScreenSharing),
            });
          }
        }
      }
      return;
    }

    // 4. WebRTC Signaling
    if (data.type === 'webrtc_offer') {
      await handleReceivedOffer(fromId, data.sdp);
      return;
    }
    if (data.type === 'webrtc_answer') {
      await handleReceivedAnswer(fromId, data.sdp);
      return;
    }
    if (data.type === 'webrtc_candidate') {
      await handleReceivedCandidate(fromId, data.candidate);
      return;
    }

    // 5. In-room State Update
    if (data.type === 'state_update') {
      participants.update((map) => {
        if (map[fromId]) {
          if (typeof data.isMuted === 'boolean') map[fromId].isMuted = data.isMuted;
          if (typeof data.isCameraOn === 'boolean') map[fromId].isCameraOn = data.isCameraOn;
          if (typeof data.isScreenSharing === 'boolean') map[fromId].isScreenSharing = data.isScreenSharing;
          if (typeof data.isSpeaking === 'boolean') map[fromId].isSpeaking = data.isSpeaking;
          if (typeof data.isHandRaised === 'boolean') {
            if (data.isHandRaised && !map[fromId].isHandRaised) {
              playAppSound('hand_raise');
            }
            map[fromId].isHandRaised = data.isHandRaised;
          }
          if (!map[fromId].stream && remoteStreams.has(fromId)) {
            map[fromId].stream = remoteStreams.get(fromId)!;
          }
        }
        return { ...map };
      });
      return;
    }

    // 6. Floating Reaction
    if (data.type === 'reaction') {
      floatingReactions.update((list) => [
        ...list,
        { id: data.id, emoji: data.emoji, from: data.from },
      ]);
      setTimeout(() => {
        floatingReactions.update((list) => list.filter((r) => r.id !== data.id));
      }, 3500);
      return;
    }

    // 7. Meeting Chat
    if (data.type === 'meeting_chat' && data.msg) {
      meetingChatMessages.update((msgs) => [...msgs, data.msg]);
      return;
    }

    // 8. Peer Left Room
    if (data.type === 'room_leave') {
      const pc = peerConnections.get(fromId);
      if (pc) {
        pc.close();
        peerConnections.delete(fromId);
      }
      remoteStreams.delete(fromId);
      knownRoomPeers.delete(fromId);
      participants.update((map) => {
        delete map[fromId];
        return { ...map };
      });
      return;
    }
  });
}
