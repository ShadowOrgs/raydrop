import { writable, derived } from 'svelte/store';
import { raydropApi, type Contact, type LanStatusInfo } from '../utils/tauri-ipc';

export const contacts = writable<Contact[]>([]);
export const lanStatusMap = writable<Record<string, LanStatusInfo>>({});

export async function refreshLanStatuses() {
  try {
    const map = await raydropApi.getLanConnectionStatuses();
    lanStatusMap.set(map || {});
  } catch (_) {}
}

export const lanContacts = derived(contacts, ($contacts) =>
  $contacts.filter((c) => c.network_type === 'lan')
);

export const ownDevicesContacts = derived(contacts, ($contacts) =>
  $contacts.filter((c) => c.is_own_device)
);

export const wanContacts = derived(contacts, ($contacts) =>
  $contacts.filter((c) => c.network_type === 'wan' && !c.is_own_device)
);
