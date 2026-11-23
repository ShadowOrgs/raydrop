import {
  updateInfo,
  updateModalOpen,
  updateChecking,
  updateErrorMessage,
  updateStatusMessage,
} from '../stores/update';
import { raydropApi } from './tauri-ipc';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

export const UPDATE_SNOOZE_KEY = 'raydrop_update_snooze_until';
const ONE_DAY_MS = 24 * 60 * 60 * 1000;

export async function checkForUpdates(manual: boolean = false): Promise<boolean> {
  try {
    updateChecking.set(true);
    updateErrorMessage.set('');

    const res = await raydropApi.checkAppUpdate();
    updateInfo.set(res);

    if (res.has_update) {
      if (!manual) {
        const snoozeUntil = Number(localStorage.getItem(UPDATE_SNOOZE_KEY) || '0');
        if (Date.now() < snoozeUntil) {
          return true;
        }
        localStorage.setItem(UPDATE_SNOOZE_KEY, String(Date.now() + ONE_DAY_MS));
      }

      updateModalOpen.set(true);

      // Trigger native OS notification
      try {
        let hasPermission = await isPermissionGranted();
        if (!hasPermission) {
          const perm = await requestPermission();
          hasPermission = perm === 'granted';
        }
        if (hasPermission) {
          sendNotification({
            title: `Raydrop Update: ${res.latest_version}`,
            body: `A new version of Raydrop is ready. Open the app to update now!`,
          });
        }
      } catch (err) {
        console.warn('Could not dispatch OS update notification:', err);
      }
      return true;
    } else {
      if (manual) {
        // Always open modal on manual check so user sees exact status (Up to Date or Compiling)
        updateModalOpen.set(true);
      }
      return false;
    }
  } catch (err: any) {
    console.warn('Update check failed:', err);
    if (manual) {
      const msg = typeof err === 'string' ? err : err?.message || 'Failed to check for updates';
      updateErrorMessage.set(msg);
      updateModalOpen.set(true);
    }
    return false;
  } finally {
    updateChecking.set(false);
  }
}

