import { writable } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';

const LEGACY_MAC_KEY = 'raydrop_device_macs';
const CONFIG_STORAGE_KEY = 'raydrop_device_wol_configs';

export interface WolConfig {
  mac: string;
  wanHost: string;
  port: number;
}

export const wolModalTarget = writable<{
  deviceId: string;
  deviceName: string;
} | null>(null);

function loadConfigMap(): Record<string, WolConfig> {
  if (typeof localStorage === 'undefined') return {};
  try {
    const raw = localStorage.getItem(CONFIG_STORAGE_KEY);
    if (raw) {
      return JSON.parse(raw);
    }
    const legacyRaw = localStorage.getItem(LEGACY_MAC_KEY);
    if (legacyRaw) {
      const legacy: Record<string, string> = JSON.parse(legacyRaw);
      const migrated: Record<string, WolConfig> = {};
      for (const [id, mac] of Object.entries(legacy)) {
        migrated[id] = { mac, wanHost: '', port: 9 };
      }
      localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(migrated));
      return migrated;
    }
  } catch {}
  return {};
}

export function getWolConfig(deviceId: string): WolConfig {
  return loadConfigMap()[deviceId] || { mac: '', wanHost: '', port: 9 };
}

export function getSavedMac(deviceId: string): string {
  return getWolConfig(deviceId).mac;
}

export function saveWolConfig(deviceId: string, cfg: WolConfig) {
  if (typeof localStorage === 'undefined') return;
  const map = loadConfigMap();
  map[deviceId] = {
    mac: cfg.mac.trim(),
    wanHost: cfg.wanHost.trim(),
    port: Number(cfg.port) > 0 && Number(cfg.port) <= 65535 ? Number(cfg.port) : 9,
  };
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(map));
}

export async function executeWakeOnLan(
  deviceId: string,
  _deviceName: string,
  cfg: WolConfig
): Promise<string> {
  saveWolConfig(deviceId, cfg);
  return await raydropApi.sendWakeOnLanAuto({
    targetDeviceId: deviceId,
    macAddress: cfg.mac.trim(),
    targetHost: cfg.wanHost.trim() || null,
    port: cfg.port || 9,
  });
}

export function triggerWakeOnLan(deviceId: string, deviceName: string): void {
  wolModalTarget.set({ deviceId, deviceName });
}
