import { open, type OpenDialogOptions } from '@tauri-apps/plugin-dialog';
import { get } from 'svelte/store';
import { appSettings } from '../stores/state';

const LAST_PICKED_DIR_KEY = 'raydrop_last_picked_dir';

function extractParentDir(filePath: string): string | null {
  const idx = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'));
  if (idx > 0) {
    return filePath.slice(0, idx);
  }
  return null;
}

export function getDefaultPickerPath(): string | undefined {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(LAST_PICKED_DIR_KEY);
    if (saved && saved.trim()) {
      return saved;
    }
  }
  const dl = get(appSettings)?.download_dir;
  if (dl && dl.trim()) {
    return dl;
  }
  return undefined;
}

export function saveLastPickedPath(picked: string | string[] | null, isDir = false) {
  if (!picked || typeof localStorage === 'undefined') return;
  const first = Array.isArray(picked) ? picked[0] : picked;
  if (!first || typeof first !== 'string') return;
  const dir = isDir ? first : extractParentDir(first);
  if (dir && dir.trim()) {
    localStorage.setItem(LAST_PICKED_DIR_KEY, dir);
  }
}

export async function openWithLastDir(options: OpenDialogOptions = {}) {
  const defaultPath = options.defaultPath ?? getDefaultPickerPath();
  const res = await open({
    ...options,
    ...(defaultPath ? { defaultPath } : {}),
  });
  if (res) {
    saveLastPickedPath(res as string | string[], !!options.directory);
  }
  return res;
}
