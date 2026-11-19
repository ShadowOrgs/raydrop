import { writable, get } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';
import { appSettings, activeMainTab } from './state';
import { myAvatar } from './avatars';

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
let mediaRecorder: MediaRecorder | null = null;
let recordedChunks: Blob[] = [];

// WebRTC peer connections keyed by remote deviceId
const peerConnections: Map<string, RTCPeerConnection> = new Map();
const remoteStreams: Map<string, MediaStream> = new Map();

const ICE_SERVERS = {
  iceServers: [
    { urls: 'stun:stun.raydrop.web.id:3478' },
    { urls: 'stun:stun.l.google.com:19302' },
  ],
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

export async function joinMeetingRoom(roomId: string, title = 'Online Meeting') {
  leaveMeetingRoom();

  currentRoomId.set(roomId);
  currentRoomTitle.set(title);
  isInMeeting.set(true);
  participants.set({});
  meetingChatMessages.set([]);

  await initLocalAudio();

  // Announce presence to any peers that might be in the room
  // (Or wait for peers to be invited)
  broadcastRoomSignal({
    type: 'room_join',
    user: getMyInfo(),
    isMuted: get(isMicMuted),
    isCameraOn: get(isCameraOn),
    isScreenSharing: get(isScreenSharing),
  });
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
    } catch (_) {}
  });
  peerConnections.clear();
  remoteStreams.clear();

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
    audioCtx.close().catch(() => {});
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
}

export async function inviteContactToMeeting(targetDeviceId: string) {
  const roomId = get(currentRoomId);
  const title = get(currentRoomTitle);
  const me = getMyInfo();

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
  );
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
    // Start screen share
    try {
      const displayStream = await (navigator.mediaDevices as any).getDisplayMedia({
        video: { cursor: 'always' },
        audio: true,
      });

      // Handle user clicking native browser "Stop sharing" bar
      displayStream.getVideoTracks()[0].onended = () => {
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

export function startRecording() {
  recordedChunks = [];
  try {
    const aStream = get(localAudioStream);
    const vStream = get(localVideoStream);

    const combinedTracks: MediaStreamTrack[] = [];
    if (aStream) combinedTracks.push(...aStream.getAudioTracks());
    if (vStream) combinedTracks.push(...vStream.getVideoTracks());

    if (combinedTracks.length === 0) {
      alert('No audio or video stream available to record.');
      return;
    }

    const combinedStream = new MediaStream(combinedTracks);
    const mime = MediaRecorder.isTypeSupported('video/webm; codecs=vp9,opus')
      ? 'video/webm; codecs=vp9,opus'
      : 'video/webm';

    mediaRecorder = new MediaRecorder(combinedStream, { mimeType: mime });
    mediaRecorder.ondataavailable = (e) => {
      if (e.data.size > 0) recordedChunks.push(e.data);
    };

    mediaRecorder.onstop = () => {
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

function addTrackToPeers(track: MediaStreamTrack) {
  peerConnections.forEach((pc) => {
    try {
      pc.addTrack(track);
    } catch (_) {}
  });
}

function removeTrackFromPeers(kind: 'audio' | 'video') {
  peerConnections.forEach((pc) => {
    try {
      const senders = pc.getSenders();
      const sender = senders.find((s) => s.track && s.track.kind === kind);
      if (sender) pc.removeTrack(sender);
    } catch (_) {}
  });
}

function getOrCreatePeerConnection(peerDeviceId: string): RTCPeerConnection {
  let pc = peerConnections.get(peerDeviceId);
  if (pc) return pc;

  pc = new RTCPeerConnection(ICE_SERVERS);
  peerConnections.set(peerDeviceId, pc);

  // Add local tracks
  const aStream = get(localAudioStream);
  if (aStream) {
    aStream.getAudioTracks().forEach((track) => pc!.addTrack(track, aStream));
  }
  const vStream = get(localVideoStream);
  if (vStream) {
    vStream.getVideoTracks().forEach((track) => pc!.addTrack(track, vStream));
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
    stream.addTrack(event.track);

    participants.update((map) => {
      if (map[peerDeviceId]) {
        map[peerDeviceId].stream = stream!;
      }
      return { ...map };
    });
  };

  pc.onconnectionstatechange = () => {
    if (pc?.connectionState === 'disconnected' || pc?.connectionState === 'failed') {
      participants.update((map) => {
        delete map[peerDeviceId];
        return { ...map };
      });
      peerConnections.delete(peerDeviceId);
    }
  };

  return pc;
}

async function createAndSendOffer(peerDeviceId: string) {
  const pc = getOrCreatePeerConnection(peerDeviceId);
  try {
    const offer = await pc.createOffer();
    await pc.setLocalDescription(offer);
    sendDirectSignal(peerDeviceId, {
      type: 'webrtc_offer',
      sdp: offer,
    });
  } catch (err) {
    console.warn(`Failed to create offer for ${peerDeviceId}:`, err);
  }
}

async function handleReceivedOffer(peerDeviceId: string, sdp: RTCSessionDescriptionInit) {
  const pc = getOrCreatePeerConnection(peerDeviceId);
  try {
    await pc.setRemoteDescription(new RTCSessionDescription(sdp));
    const answer = await pc.createAnswer();
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
  } catch (err) {
    console.warn(`Failed to set remote answer for ${peerDeviceId}:`, err);
  }
}

async function handleReceivedCandidate(peerDeviceId: string, candidate: RTCIceCandidateInit) {
  const pc = peerConnections.get(peerDeviceId);
  if (!pc) return;
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

  Object.keys(parts).forEach((targetId) => {
    sendDirectSignal(targetId, payload);
  });
}

function sendDirectSignal(targetDeviceId: string, payload: any) {
  const roomId = get(currentRoomId);
  if (!roomId || !targetDeviceId) return;

  raydropApi.sendMeetingSignal(
    targetDeviceId,
    `meeting:${roomId}`,
    payload
  ).catch(() => {});
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
      incomingInvite.set({
        roomId: data.roomId,
        roomTitle: data.roomTitle || 'Online Meeting',
        fromDeviceId: data.fromDeviceId,
        fromAlias: data.fromAlias || 'Contact',
        fromAvatar: data.fromAvatar || null,
      });
      return;
    }

    const myRoomId = get(currentRoomId);
    if (!myRoomId || !get(isInMeeting)) return;

    // 2. Peer Joining Room
    if (data.type === 'room_join') {
      const u = data.user;
      participants.update((map) => {
        map[fromId] = {
          deviceId: fromId,
          alias: u.alias || 'Peer',
          avatar: u.avatar || null,
          isMuted: data.isMuted ?? false,
          isCameraOn: data.isCameraOn ?? false,
          isScreenSharing: data.isScreenSharing ?? false,
          isSpeaking: false,
          isHandRaised: false,
          stream: remoteStreams.get(fromId) || null,
        };
        return { ...map };
      });

      // Reply with our info and initiate WebRTC offer
      sendDirectSignal(fromId, {
        type: 'room_welcome',
        user: getMyInfo(),
        isMuted: get(isMicMuted),
        isCameraOn: get(isCameraOn),
        isScreenSharing: get(isScreenSharing),
      });

      await createAndSendOffer(fromId);
      return;
    }

    // 3. Room Welcome (response to join)
    if (data.type === 'room_welcome') {
      const u = data.user;
      participants.update((map) => {
        map[fromId] = {
          deviceId: fromId,
          alias: u.alias || 'Peer',
          avatar: u.avatar || null,
          isMuted: data.isMuted ?? false,
          isCameraOn: data.isCameraOn ?? false,
          isScreenSharing: data.isScreenSharing ?? false,
          isSpeaking: false,
          isHandRaised: false,
          stream: remoteStreams.get(fromId) || null,
        };
        return { ...map };
      });
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
          if (typeof data.isHandRaised === 'boolean') map[fromId].isHandRaised = data.isHandRaised;
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
      participants.update((map) => {
        delete map[fromId];
        return { ...map };
      });
      return;
    }
  });
}
