const STORAGE_KEY = 'raydrop_account_device_names';

function loadMap(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export function getAccountDeviceName(email?: string | null): string | null {
  const key = (email || '').trim().toLowerCase();
  if (!key) return null;
  const val = loadMap()[key];
  return typeof val === 'string' && val.trim() ? val.trim() : null;
}

export function saveAccountDeviceName(email?: string | null, deviceName?: string | null): void {
  const key = (email || '').trim().toLowerCase();
  const name = (deviceName || '').trim();
  if (!key || !name) return;
  try {
    const map = loadMap();
    map[key] = name;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
  } catch {
    // Ignore storage quota errors
  }
}
