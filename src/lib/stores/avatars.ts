import { writable } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';

export const myAvatar = writable<string | null>(null);
export const peerAvatars = writable<Record<string, string>>({});
export const peerAvatarHashes = writable<Record<string, string>>({});

export async function initAvatarStore() {
  try {
    const mine = await raydropApi.getMyAvatar();
    if (mine) {
      myAvatar.set(mine);
    }
  } catch (_) {}

  // Listen for self avatar updates
  try {
    await raydropApi.listenEvent<{ hash: string; avatar_data: string }>(
      'my_avatar_updated',
      (payload) => {
        if (payload?.avatar_data) {
          myAvatar.set(payload.avatar_data);
        }
      }
    );
  } catch (_) {}

  // Listen for peer avatar updates
  try {
    await raydropApi.listenEvent<{ device_id: string; avatar_data: string; hash?: string }>(
      'peer_avatar_updated',
      (payload) => {
        if (payload?.device_id && payload?.avatar_data) {
          peerAvatars.update((map) => ({
            ...map,
            [payload.device_id]: payload.avatar_data,
          }));
          if (payload.hash) {
            peerAvatarHashes.update((map) => ({
              ...map,
              [payload.device_id]: payload.hash!,
            }));
          }
        }
      }
    );
  } catch (_) {}
}

const requestedAvatarPeers = new Set<string>();

export async function loadPeerAvatar(deviceId: string, expectedHash?: string | null): Promise<string | null> {
  let cached: string | null = null;
  let currentHash: string | null = null;
  peerAvatars.subscribe((m) => {
    cached = m[deviceId] || null;
  })();
  peerAvatarHashes.subscribe((m) => {
    currentHash = m[deviceId] || null;
  })();

  if (cached && (!expectedHash || currentHash === expectedHash)) {
    return cached;
  }

  try {
    const av = await raydropApi.getPeerAvatar(deviceId);
    if (av) {
      peerAvatars.update((m) => ({ ...m, [deviceId]: av }));
      if (expectedHash) {
        peerAvatarHashes.update((m) => ({ ...m, [deviceId]: expectedHash }));
      }
      return av;
    }
  } catch (_) {}

  // Not on disk or hash changed, request from peer over network
  const reqKey = `${deviceId}:${expectedHash || ''}`;
  if (!requestedAvatarPeers.has(reqKey)) {
    requestedAvatarPeers.add(reqKey);
    setTimeout(() => requestedAvatarPeers.delete(reqKey), 5000);
    try {
      await raydropApi.requestPeerAvatar(deviceId);
    } catch (_) {}
  }

  return null;
}
