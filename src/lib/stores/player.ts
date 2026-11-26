import { writable } from 'svelte/store';
import {
  raydropApi,
  type MediaPlayerState,
  type MediaPlaylist,
  type MediaPlaylistItem,
  type MediaTrack,
} from '../utils/tauri-ipc';

export const winampPlayerOpen = writable<boolean>(false);

export const defaultMediaPlayerState: MediaPlayerState = {
  current_track: null,
  queue: [],
  current_index: null,
  is_playing: false,
  position_secs: 0,
  duration_secs: 0,
  volume: 0.85,
  muted: false,
  shuffle: false,
  repeat_mode: 'off',
  video_mode: false,
  active_playlist_id: null,
};

export const mediaPlayerState = writable<MediaPlayerState>({ ...defaultMediaPlayerState });
export const mediaPlaylists = writable<MediaPlaylist[]>([]);
export const selectedPlaylistId = writable<string | null>(null);
export const selectedPlaylistItems = writable<MediaPlaylistItem[]>([]);

export async function refreshMediaPlaylists() {
  try {
    const list = await raydropApi.mediaListPlaylists();
    mediaPlaylists.set(list);
  } catch (err) {
    console.warn('Failed to load playlists:', err);
  }
}

export async function loadPlaylistItems(playlistId: string) {
  selectedPlaylistId.set(playlistId);
  try {
    const items = await raydropApi.mediaListPlaylistItems(playlistId);
    selectedPlaylistItems.set(items);
  } catch (err) {
    console.warn('Failed to load playlist items:', err);
  }
}

export function playlistItemToTrack(item: MediaPlaylistItem): MediaTrack {
  return {
    id: item.id,
    source_type: item.source_type,
    title: item.title,
    artist: item.artist,
    duration_secs: item.duration_secs,
    uri_or_id: item.uri_or_id,
    thumbnail_url: item.thumbnail_url ?? null,
    is_video: item.is_video,
    stream_url: null,
  };
}
