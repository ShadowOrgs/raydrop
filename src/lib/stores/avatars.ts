import { writable } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';

export const myAvatar = writable<string | null>(null);
export const peerAvatars = writable<Record<string, string>>({});

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
    await raydropApi.listenEvent<{ device_id: string; avatar_data: string }>(
      'peer_avatar_updated',
      (payload) => {
        if (payload?.device_id && payload?.avatar_data) {
          peerAvatars.update((map) => ({
            ...map,
            [payload.device_id]: payload.avatar_data,
          }));
        }
      }
    );
  } catch (_) {}
}

export async function loadPeerAvatar(deviceId: string): Promise<string | null> {
  let cached: string | null = null;
  peerAvatars.update((m) => {
    cached = m[deviceId] || null;
    return m;
  });
  if (cached) return cached;

  try {
    const av = await raydropApi.getPeerAvatar(deviceId);
    if (av) {
      peerAvatars.update((m) => ({ ...m, [deviceId]: av }));
      return av;
    }
  } catch (_) {}
  return null;
}
