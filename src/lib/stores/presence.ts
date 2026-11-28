import { writable, get } from 'svelte/store';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { contacts } from './contacts';
import { appSettings } from './state';
import type { Contact, IncomingTransferEvent, TransferRecord } from '../utils/tauri-ipc';

export interface PresenceToast {
  id: string;
  deviceId: string;
  displayName: string;
  isOnline: boolean;
  subtitle?: string;
  tone?: 'emerald' | 'cyan' | 'amber' | 'rose' | 'slate';
  timestamp: number;
}

export const presenceToasts = writable<PresenceToast[]>([]);

export interface PeerRayampActivity {
  title: string;
  artist: string;
  is_playing: boolean;
  source_type: string;
  updated_at: number;
}

export const peerActivities = writable<Record<string, PeerRayampActivity>>({});

export function notifyToast(
  displayName: string,
  subtitle: string,
  tone: 'emerald' | 'cyan' | 'amber' | 'rose' | 'slate' = 'emerald',
  durationMs = 3500
) {
  const id = `toast-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
  presenceToasts.update((list) => [
    ...list.slice(-3),
    {
      id,
      deviceId: 'local',
      displayName,
      isOnline: tone !== 'rose' && tone !== 'slate',
      subtitle,
      tone,
      timestamp: Date.now(),
    },
  ]);
  setTimeout(() => {
    presenceToasts.update((list) => list.filter((t) => t.id !== id));
  }, durationMs);
}
export const presenceSoundEnabled = writable<boolean>(
  typeof localStorage !== 'undefined'
    ? localStorage.getItem('raydrop_presence_sound_v1') !== 'false'
    : true
);

export function togglePresenceSound(): boolean {
  const next = !get(presenceSoundEnabled);
  presenceSoundEnabled.set(next);
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('raydrop_presence_sound_v1', next ? 'true' : 'false');
    }
  } catch (_) {}
  return next;
}

let sharedAudioCtx: AudioContext | null = null;

export function playPresenceSound(isOnline: boolean) {
  if (!get(presenceSoundEnabled)) return;
  try {
    const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioCtx) return;
    if (!sharedAudioCtx) {
      sharedAudioCtx = new AudioCtx();
    }
    if (sharedAudioCtx.state === 'suspended') {
      sharedAudioCtx.resume().catch(() => {});
    }
    const ctx = sharedAudioCtx;
    const now = ctx.currentTime;

    const playTone = (freq: number, startOffset: number, duration: number, peakGain: number) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.setValueAtTime(freq, now + startOffset);
      gain.gain.setValueAtTime(0.0001, now + startOffset);
      gain.gain.exponentialRampToValueAtTime(peakGain, now + startOffset + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + startOffset + duration);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + startOffset);
      osc.stop(now + startOffset + duration + 0.02);
    };

    if (isOnline) {
      // Pleasant ascending chime (C5 -> G5)
      playTone(523.25, 0.0, 0.16, 0.09);
      playTone(783.99, 0.1, 0.24, 0.11);
    } else {
      // Soft descending chime (G4 -> D4)
      playTone(392.0, 0.0, 0.16, 0.07);
      playTone(293.66, 0.1, 0.24, 0.06);
    }
  } catch (_) {}
}

export type TransferSoundKind =
  | 'request'
  | 'started'
  | 'completed'
  | 'paused'
  | 'cancelled'
  | 'failed';

export function playTransferSound(kind: TransferSoundKind) {
  if (!get(presenceSoundEnabled)) return;
  try {
    const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioCtx) return;
    if (!sharedAudioCtx) {
      sharedAudioCtx = new AudioCtx();
    }
    if (sharedAudioCtx.state === 'suspended') {
      sharedAudioCtx.resume().catch(() => {});
    }
    const ctx = sharedAudioCtx;
    const now = ctx.currentTime;

    const playTone = (
      freq: number,
      startOffset: number,
      duration: number,
      peakGain: number,
      type: OscillatorType = 'sine'
    ) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = type;
      osc.frequency.setValueAtTime(freq, now + startOffset);
      gain.gain.setValueAtTime(0.0001, now + startOffset);
      gain.gain.exponentialRampToValueAtTime(peakGain, now + startOffset + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + startOffset + duration);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + startOffset);
      osc.stop(now + startOffset + duration + 0.02);
    };

    switch (kind) {
      case 'request':
        playTone(880.0, 0.0, 0.14, 0.09);
        playTone(1108.73, 0.1, 0.22, 0.1);
        break;
      case 'started':
        playTone(659.25, 0.0, 0.12, 0.07);
        playTone(880.0, 0.08, 0.16, 0.08);
        break;
      case 'completed':
        playTone(523.25, 0.0, 0.14, 0.08);
        playTone(659.25, 0.09, 0.14, 0.09);
        playTone(783.99, 0.18, 0.28, 0.11);
        break;
      case 'paused':
        playTone(587.33, 0.0, 0.1, 0.06);
        playTone(587.33, 0.12, 0.1, 0.05);
        break;
      case 'cancelled':
        playTone(659.25, 0.0, 0.14, 0.07);
        playTone(523.25, 0.1, 0.2, 0.06);
        break;
      case 'failed':
        playTone(220.0, 0.0, 0.16, 0.09, 'triangle');
        playTone(174.61, 0.13, 0.26, 0.09, 'triangle');
        break;
    }
  } catch (_) {}
}

export type AppSoundKind =
  | 'chat'
  | 'meeting_invite'
  | 'meeting_join'
  | 'meeting_leave'
  | 'hand_raise'
  | 'remote_request'
  | 'remote_disconnect'
  | 'mesh_request'
  | 'torrent_done';

export function playAppSound(kind: AppSoundKind) {
  if (!get(presenceSoundEnabled)) return;
  try {
    const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioCtx) return;
    if (!sharedAudioCtx) {
      sharedAudioCtx = new AudioCtx();
    }
    if (sharedAudioCtx.state === 'suspended') {
      sharedAudioCtx.resume().catch(() => {});
    }
    const ctx = sharedAudioCtx;
    const now = ctx.currentTime;

    const playTone = (
      freq: number,
      startOffset: number,
      duration: number,
      peakGain: number,
      type: OscillatorType = 'sine'
    ) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = type;
      osc.frequency.setValueAtTime(freq, now + startOffset);
      gain.gain.setValueAtTime(0.0001, now + startOffset);
      gain.gain.exponentialRampToValueAtTime(peakGain, now + startOffset + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + startOffset + duration);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + startOffset);
      osc.stop(now + startOffset + duration + 0.02);
    };

    switch (kind) {
      case 'chat':
        playTone(740.0, 0.0, 0.09, 0.08);
        playTone(987.77, 0.06, 0.14, 0.09);
        break;
      case 'meeting_invite':
        playTone(659.25, 0.0, 0.14, 0.1);
        playTone(880.0, 0.14, 0.14, 0.1);
        playTone(1046.5, 0.28, 0.28, 0.11);
        break;
      case 'meeting_join':
        playTone(587.33, 0.0, 0.1, 0.06);
        playTone(783.99, 0.07, 0.14, 0.07);
        break;
      case 'meeting_leave':
        playTone(659.25, 0.0, 0.1, 0.05);
        playTone(493.88, 0.08, 0.16, 0.05);
        break;
      case 'hand_raise':
        playTone(880.0, 0.0, 0.12, 0.07);
        break;
      case 'remote_request':
        playTone(523.25, 0.0, 0.12, 0.09);
        playTone(659.25, 0.11, 0.12, 0.09);
        playTone(783.99, 0.22, 0.2, 0.1);
        break;
      case 'remote_disconnect':
        playTone(440.0, 0.0, 0.12, 0.07);
        playTone(329.63, 0.1, 0.2, 0.06);
        break;
      case 'mesh_request':
        playTone(587.33, 0.0, 0.12, 0.08);
        playTone(880.0, 0.1, 0.2, 0.09);
        break;
      case 'torrent_done':
        playTone(523.25, 0.0, 0.12, 0.08);
        playTone(659.25, 0.09, 0.12, 0.09);
        playTone(783.99, 0.18, 0.24, 0.1);
        break;
    }
  } catch (_) {}
}

let incomingRingTimer: ReturnType<typeof setInterval> | null = null;
let incomingRingTimeout: ReturnType<typeof setTimeout> | null = null;
let outgoingRingTimer: ReturnType<typeof setInterval> | null = null;
let outgoingRingTimeout: ReturnType<typeof setTimeout> | null = null;

function playDiscordRingPattern() {
  if (!get(presenceSoundEnabled)) return;
  try {
    const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioCtx) return;
    if (!sharedAudioCtx) sharedAudioCtx = new AudioCtx();
    if (sharedAudioCtx.state === 'suspended') {
      sharedAudioCtx.resume().catch(() => {});
    }
    const ctx = sharedAudioCtx;
    const now = ctx.currentTime;
    const notes = [
      { f: 659.25, t: 0.0, d: 0.13 },
      { f: 880.0, t: 0.13, d: 0.13 },
      { f: 987.77, t: 0.26, d: 0.13 },
      { f: 1318.51, t: 0.39, d: 0.28 },
      { f: 987.77, t: 0.72, d: 0.12 },
      { f: 1318.51, t: 0.85, d: 0.3 },
    ];
    for (const n of notes) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'triangle';
      osc.frequency.setValueAtTime(n.f, now + n.t);
      gain.gain.setValueAtTime(0.0001, now + n.t);
      gain.gain.exponentialRampToValueAtTime(0.11, now + n.t + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + n.t + n.d);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now + n.t);
      osc.stop(now + n.t + n.d + 0.02);
    }
  } catch (_) {}
}

function playOutgoingRingbackPattern() {
  if (!get(presenceSoundEnabled)) return;
  try {
    const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
    if (!AudioCtx) return;
    if (!sharedAudioCtx) sharedAudioCtx = new AudioCtx();
    if (sharedAudioCtx.state === 'suspended') {
      sharedAudioCtx.resume().catch(() => {});
    }
    const ctx = sharedAudioCtx;
    const now = ctx.currentTime;
    for (const freq of [440, 480]) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.setValueAtTime(freq, now);
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(0.035, now + 0.03);
      gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.65);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(now);
      osc.stop(now + 0.68);
    }
  } catch (_) {}
}

export function stopIncomingCallRingtone() {
  if (incomingRingTimer) {
    clearInterval(incomingRingTimer);
    incomingRingTimer = null;
  }
  if (incomingRingTimeout) {
    clearTimeout(incomingRingTimeout);
    incomingRingTimeout = null;
  }
}

export function startIncomingCallRingtone() {
  stopIncomingCallRingtone();
  playDiscordRingPattern();
  incomingRingTimer = setInterval(playDiscordRingPattern, 2100);
  incomingRingTimeout = setTimeout(stopIncomingCallRingtone, 45000);
}

export function stopOutgoingRingbackTone() {
  if (outgoingRingTimer) {
    clearInterval(outgoingRingTimer);
    outgoingRingTimer = null;
  }
  if (outgoingRingTimeout) {
    clearTimeout(outgoingRingTimeout);
    outgoingRingTimeout = null;
  }
}

export function startOutgoingRingbackTone() {
  stopOutgoingRingbackTone();
  playOutgoingRingbackPattern();
  outgoingRingTimer = setInterval(playOutgoingRingbackPattern, 2500);
  outgoingRingTimeout = setTimeout(stopOutgoingRingbackTone, 35000);
}

const prevTransferStatusMap = new Map<string, string>();

function pushToast(toast: PresenceToast, timeoutMs = 4500) {
  presenceToasts.update((prev) => [...prev.slice(-4), toast]);
  setTimeout(() => {
    removePresenceToast(toast.id);
  }, timeoutMs);
}

async function dispatchOsTransferNotification(title: string, body: string) {
  try {
    if (typeof document !== 'undefined' && document.hasFocus()) {
      return;
    }
    let granted = await isPermissionGranted();
    if (!granted) {
      const perm = await requestPermission();
      granted = perm === 'granted';
    }
    if (granted) {
      sendNotification({ title, body });
    }
  } catch (_) {}
}

export function notifyIncomingTransferRequest(evt: IncomingTransferEvent) {
  const now = Date.now();
  const title = evt.sender_alias || evt.sender_device_id;
  const subtitle = `Mengirim ${evt.files.length} file ke perangkat ini`;
  playTransferSound('request');
  pushToast({
    id: `xfer_req_${evt.session_id}_${now}`,
    deviceId: evt.sender_device_id,
    displayName: title,
    isOnline: true,
    subtitle,
    tone: 'cyan',
    timestamp: now,
  });
  dispatchOsTransferNotification(`Transfer Masuk dari ${title}`, subtitle);
}

export function notifyTransferState(rec: TransferRecord) {
  if (!rec?.id || !rec?.status) return;
  const prevStatus = prevTransferStatusMap.get(rec.id);
  if (prevStatus === rec.status) return;
  prevTransferStatusMap.set(rec.id, rec.status);

  const isDownload = rec.direction === 'incoming';
  const actionLabel = isDownload ? 'Download' : 'Transfer';
  const fileInfo = `${rec.total_files} file • ${rec.peer_alias}`;
  let subtitle = '';
  let tone: PresenceToast['tone'] = 'cyan';
  let soundKind: TransferSoundKind | null = null;
  let shouldOsNotify = false;

  switch (rec.status) {
    case 'in_progress':
      subtitle = prevStatus === 'paused' ? `${actionLabel} dilanjutkan (${fileInfo})` : `Mulai ${actionLabel.toLowerCase()} (${fileInfo})`;
      tone = 'cyan';
      soundKind = 'started';
      break;
    case 'completed':
      subtitle = `${actionLabel} selesai (${fileInfo})`;
      tone = 'emerald';
      soundKind = 'completed';
      shouldOsNotify = true;
      break;
    case 'paused':
      subtitle = `${actionLabel} dijeda (${fileInfo})`;
      tone = 'amber';
      soundKind = 'paused';
      break;
    case 'cancelled':
    case 'rejected':
      subtitle = `${actionLabel} dibatalkan (${fileInfo})`;
      tone = 'slate';
      soundKind = 'cancelled';
      shouldOsNotify = true;
      break;
    case 'failed':
      subtitle = `${actionLabel} gagal${rec.error_reason ? `: ${rec.error_reason}` : ` (${fileInfo})`}`;
      tone = 'rose';
      soundKind = 'failed';
      shouldOsNotify = true;
      break;
  }

  if (!subtitle || !soundKind) return;
  playTransferSound(soundKind);
  pushToast({
    id: `xfer_${rec.id}_${rec.status}_${Date.now()}`,
    deviceId: rec.peer_device_id,
    displayName: rec.peer_alias || rec.peer_device_id,
    isOnline: rec.status !== 'failed' && rec.status !== 'cancelled',
    subtitle,
    tone,
    timestamp: Date.now(),
  });
  if (shouldOsNotify) {
    dispatchOsTransferNotification(`${actionLabel} ${rec.status === 'completed' ? 'Selesai' : rec.status === 'failed' ? 'Gagal' : 'Dibatalkan'}`, subtitle);
  }
}

export function removePresenceToast(id: string) {
  presenceToasts.update((list) => list.filter((t) => t.id !== id));
}

async function dispatchOsPresenceNotification(displayName: string, isOnline: boolean) {
  try {
    if (typeof document !== 'undefined' && document.hasFocus()) {
      // When window is focused, the in-app floating toast + chime already notifies cleanly
      return;
    }
    let granted = await isPermissionGranted();
    if (!granted) {
      const perm = await requestPermission();
      granted = perm === 'granted';
    }
    if (granted) {
      sendNotification({
        title: isOnline ? `${displayName} is Online` : `${displayName} went Offline`,
        body: isOnline
          ? `${displayName} sekarang online di Raydrop.`
          : `${displayName} baru saja offline.`,
      });
    }
  } catch (_) {}
}

let initialized = false;
let initWatcherStartedAt = 0;
const prevOnlineMap = new Map<string, boolean>();
const lastNotifiedAt = new Map<string, number>();

export function initPresenceNotifications(): () => void {
  initWatcherStartedAt = Date.now();
  initialized = false;
  prevOnlineMap.clear();

  const unsub = contacts.subscribe((list: Contact[]) => {
    const myId = get(appSettings).device_id;
    const now = Date.now();

    // Seed baseline during initial 5 seconds after boot so startup sync doesn't spam toasts
    if (!initialized) {
      for (const c of list) {
        if (c.device_id !== myId) {
          prevOnlineMap.set(c.device_id, !!c.is_online);
        }
      }
      if (list.length > 0 && now - initWatcherStartedAt > 4000) {
        initialized = true;
      } else if (list.length > 0) {
        setTimeout(() => {
          initialized = true;
        }, 4500);
      }
      return;
    }

    for (const c of list) {
      if (!c.device_id || c.device_id === myId) continue;
      const isNowOnline = !!c.is_online;
      if (!prevOnlineMap.has(c.device_id)) {
        prevOnlineMap.set(c.device_id, isNowOnline);
        continue;
      }

      const wasOnline = prevOnlineMap.get(c.device_id)!;
      if (wasOnline === isNowOnline) continue;

      prevOnlineMap.set(c.device_id, isNowOnline);

      const lastTs = lastNotifiedAt.get(c.device_id) || 0;
      if (now - lastTs < 6000) continue;
      lastNotifiedAt.set(c.device_id, now);

      const displayName =
        c.custom_name?.trim() ||
        c.user_name?.trim() ||
        c.alias_name?.trim() ||
        c.device_id;

      const toastId = `presence_${c.device_id}_${now}`;
      presenceToasts.update((prev) => [
        ...prev.slice(-3),
        {
          id: toastId,
          deviceId: c.device_id,
          displayName,
          isOnline: isNowOnline,
          timestamp: now,
        },
      ]);

      playPresenceSound(isNowOnline);
      dispatchOsPresenceNotification(displayName, isNowOnline);

      setTimeout(() => {
        removePresenceToast(toastId);
      }, 4200);
    }
  });

  return unsub;
}
