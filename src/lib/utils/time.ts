/**
 * Formats a unix timestamp (in seconds or ms) and online state into a human-friendly string.
 */
export function formatLastOnline(unixSecs?: number | null, isOnline?: boolean): string {
  if (isOnline) {
    return 'Online';
  }
  if (!unixSecs || unixSecs <= 0) {
    return 'Offline';
  }

  // Handle case where unixSecs might be in milliseconds (> 1e11)
  const secs = unixSecs > 1e11 ? Math.floor(unixSecs / 1000) : unixSecs;
  const nowSecs = Math.floor(Date.now() / 1000);
  const diffSecs = Math.max(0, nowSecs - secs);

  if (diffSecs < 60) {
    return 'Last online just now';
  }
  if (diffSecs < 3600) {
    const mins = Math.floor(diffSecs / 60);
    return `Last online ${mins}m ago`;
  }
  if (diffSecs < 86400) {
    const hours = Math.floor(diffSecs / 3600);
    return `Last online ${hours}h ago`;
  }

  const date = new Date(secs * 1000);
  const now = new Date();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);

  const hours = String(date.getHours()).padStart(2, '0');
  const minutes = String(date.getMinutes()).padStart(2, '0');

  if (date.toDateString() === yesterday.toDateString()) {
    return `Last online yesterday, ${hours}:${minutes}`;
  }

  const dateStr = date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return `Last online ${dateStr}, ${hours}:${minutes}`;
}

export function formatLastOnlineCompact(unixSecs?: number | null, isOnline?: boolean): string {
  if (isOnline) {
    return 'Online';
  }
  if (!unixSecs || unixSecs <= 0) {
    return 'Offline';
  }

  const secs = unixSecs > 1e11 ? Math.floor(unixSecs / 1000) : unixSecs;
  const nowSecs = Math.floor(Date.now() / 1000);
  const diffSecs = Math.max(0, nowSecs - secs);

  if (diffSecs < 60) {
    return 'Just now';
  }
  if (diffSecs < 3600) {
    const mins = Math.floor(diffSecs / 60);
    return `${mins}m ago`;
  }
  if (diffSecs < 86400) {
    const hours = Math.floor(diffSecs / 3600);
    return `${hours}h ago`;
  }

  const date = new Date(secs * 1000);
  const now = new Date();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);

  const hours = String(date.getHours()).padStart(2, '0');
  const minutes = String(date.getMinutes()).padStart(2, '0');

  if (date.toDateString() === yesterday.toDateString()) {
    return `Yesterday, ${hours}:${minutes}`;
  }

  const dateStr = date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return `${dateStr}, ${hours}:${minutes}`;
}
