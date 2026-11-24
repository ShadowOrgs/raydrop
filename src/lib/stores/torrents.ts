import { writable, derived } from 'svelte/store';
import {
  raydropApi,
  type TorrentTaskInfo,
  type HttpDownloadTaskInfo,
} from '../utils/tauri-ipc';
import { playAppSound, playTransferSound } from './presence';

export const torrentTasks = writable<TorrentTaskInfo[]>([]);
export const httpTasks = writable<HttpDownloadTaskInfo[]>([]);
export const downloadModalOpen = writable<boolean>(false);
export const prefilledDownloadUrl = writable<string>('');
export const browserExtensionModalOpen = writable<boolean>(false);

// Backward compatibility alias
export const magnetModalOpen = downloadModalOpen;
export const prefilledMagnetUrl = prefilledDownloadUrl;

export async function initDownloadStore() {
  // 1. Initial Load Torrents
  try {
    const list = await raydropApi.listTorrentTasks();
    if (list && Array.isArray(list)) {
      torrentTasks.set(list);
    }
  } catch (err) {
    console.warn('Failed to load initial torrent tasks:', err);
  }

  // 2. Initial Load HTTP Downloads
  try {
    const list = await raydropApi.listHttpDownloads();
    if (list && Array.isArray(list)) {
      httpTasks.set(list);
    }
  } catch (err) {
    console.warn('Failed to load initial HTTP downloads:', err);
  }

  // 3. Check for any magnet link passed from CLI / browser protocol on startup
  try {
    const pending = await raydropApi.getPendingMagnetLink();
    if (pending) {
      prefilledDownloadUrl.set(pending);
      downloadModalOpen.set(true);
    }
  } catch (_) {}

  // 4. Listen for real-time background BitTorrent stats broadcast
  const prevTorrentStatuses = new Map<string, string>();
  try {
    await raydropApi.listenEvent<TorrentTaskInfo[]>('torrent_stats_update', (tasks) => {
      if (tasks && Array.isArray(tasks)) {
        for (const t of tasks) {
          const prev = prevTorrentStatuses.get(t.id);
          if (prev && prev !== t.status) {
            if ((t.status === 'seeding' || t.status === 'completed') && prev === 'downloading') {
              playAppSound('torrent_done');
            } else if (t.status === 'error') {
              playTransferSound('failed');
            }
          }
          prevTorrentStatuses.set(t.id, t.status);
        }
        torrentTasks.set(tasks);
      }
    });
  } catch (err) {
    console.warn('Failed to register torrent_stats_update listener:', err);
  }

  // 5. Listen for real-time background HTTP download stats broadcast
  try {
    await raydropApi.listenEvent<HttpDownloadTaskInfo[]>('http_download_stats_update', (tasks) => {
      if (tasks && Array.isArray(tasks)) {
        httpTasks.set(tasks);
      }
    });
  } catch (err) {
    console.warn('Failed to register http_download_stats_update listener:', err);
  }

  // 6. Listen for external browser magnet clicks received by secondary instance
  try {
    await raydropApi.listenEvent<string>('open_magnet_link', (magnetUrl) => {
      if (magnetUrl) {
        prefilledDownloadUrl.set(magnetUrl);
        downloadModalOpen.set(true);
      }
    });
  } catch (err) {
    console.warn('Failed to register open_magnet_link listener:', err);
  }
}

// Backward compatibility alias
export const initTorrentStore = initDownloadStore;

export const activeTorrents = derived(torrentTasks, ($tasks) =>
  $tasks.filter((t) => t.status === 'downloading' || t.status === 'fetching_metadata')
);

export const activeHttpDownloads = derived(httpTasks, ($tasks) =>
  $tasks.filter((t) => t.status === 'downloading' || t.status === 'starting')
);
