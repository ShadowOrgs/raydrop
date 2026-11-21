import { raydropApi } from '../utils/tauri-ipc';

const STORAGE_KEY = 'raydrop_device_macs';

function loadMacMap(): Record<string, string> {
  if (typeof localStorage === 'undefined') return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

export function getSavedMac(deviceId: string): string {
  return loadMacMap()[deviceId] || '';
}

export function saveDeviceMac(deviceId: string, mac: string) {
  if (typeof localStorage === 'undefined') return;
  const map = loadMacMap();
  map[deviceId] = mac.trim();
  localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
}

export async function triggerWakeOnLan(deviceId: string, deviceName: string): Promise<void> {
  let mac = getSavedMac(deviceId);
  if (!mac) {
    const entered = window.prompt(
      `Enter the MAC address of "${deviceName}" to wake it via Wake-on-LAN (e.g. AA:BB:CC:DD:EE:FF):`
    );
    if (!entered || !entered.trim()) return;
    mac = entered.trim();
  }

  try {
    await raydropApi.sendWakeOnLan(mac);
    saveDeviceMac(deviceId, mac);
    alert(`⚡ Magic Packet sent to ${deviceName} (${mac})!`);
  } catch (err: any) {
    alert(`Failed to send Wake-on-LAN packet: ${err?.message || err}`);
  }
}
