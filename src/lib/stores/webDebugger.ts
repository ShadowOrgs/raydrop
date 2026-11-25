import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { activeMainTab } from './state';

export interface DebugFixItem {
  category: string;
  severity: 'critical' | 'warning' | 'info';
  title: string;
  target: string;
  suggestion: string;
}

export interface DebugScores {
  overall: number;
  network: number;
  speed: number;
  ui: number;
  security: number;
}

export interface ResponsiveViewportResult {
  label: string;
  width: number;
  height: number;
  scrollWidth?: number;
  hasOverflow?: boolean;
  overflowPx?: number;
  offenderCount?: number;
  error?: string;
}

export interface WebAuditReport {
  id: string;
  url: string;
  title: string;
  isHttps: boolean;
  timestamp: number;
  navigation: {
    dns: number;
    tcp: number;
    tls: number;
    ttfb: number;
    download: number;
    domInteractive: number;
    loadTotal: number;
    protocol: string;
    transferSize: number;
    encodedBodySize: number;
    decodedBodySize: number;
  } | null;
  vitals: {
    fcp: number | null;
    lcp: number | null;
    cls: number;
  };
  resources: {
    totalRequests: number;
    totalTransfer: number;
    cachedCount: number;
    byType: Record<string, { count: number; size: number; duration: number }>;
    slowest: Array<{
      url: string;
      type: string;
      duration: number;
      size: number;
      cached: boolean;
    }>;
  };
  ui: {
    viewportWidth: number;
    viewportHeight: number;
    scrollWidth: number;
    hasOverflow: boolean;
    overflowPx: number;
    overflowElements: Array<{ selector: string; width: number; overflowPx: number }>;
    viewportMeta: string | null;
    brokenImages: Array<{ selector: string; src: string }>;
    oversizedImages: Array<{ selector: string; natural: string; rendered: string }>;
    missingAltCount: number;
    smallTextCount: number;
    smallTapTargets: Array<{ selector: string; size: string }>;
    mixedContent: string[];
    totalImages: number;
    domNodes: number;
  };
  ping: {
    statusCode: number;
    headers: Record<string, string>;
    min: number | null;
    avg: number | null;
    max: number | null;
    jitter: number | null;
    samples: Array<number | null>;
  };
  responsiveMatrix: ResponsiveViewportResult[];
  scores: DebugScores;
  fixes: DebugFixItem[];
}

export interface LiveNetworkEvent {
  timestamp: number;
  url: string;
  method: string;
  type: string;
  statusCode: number;
  duration: number;
  ttfb: number;
  ip: string | null;
  fromCache: boolean;
  error: string | null;
}

export interface LiveConsoleEvent {
  timestamp: number;
  level: string;
  message: string;
  source: string;
}

export interface SpeedtestResult {
  url: string;
  statusCode: number;
  minMs: number;
  avgMs: number;
  maxMs: number;
  jitterMs: number;
  samples: number[];
  bytes: number;
  downloadMs: number;
  mbps: number;
}

export const debugSession = writable<{
  active: boolean;
  tabId: number | null;
  url: string;
  title: string;
  startedAt: number | null;
}>({
  active: false,
  tabId: null,
  url: '',
  title: '',
  startedAt: null,
});

export const currentAudit = writable<WebAuditReport | null>(null);
export const auditHistory = writable<WebAuditReport[]>([]);
export const liveRequests = writable<LiveNetworkEvent[]>([]);
export const liveErrors = writable<LiveConsoleEvent[]>([]);
export const liveCls = writable<number>(0);
export const speedtestResult = writable<SpeedtestResult | null>(null);
export const extensionUpdateInfo = writable<{
  installedVersion: string;
  latestVersion: string;
} | null>(null);

let listenersInitialized = false;
const unlisteners: UnlistenFn[] = [];

export async function initWebDebuggerListeners() {
  if (listenersInitialized) return;
  listenersInitialized = true;

  unlisteners.push(
    await listen<any>('web_debug_session', (event) => {
      const p = event.payload || {};
      if (p.action === 'start') {
        liveRequests.set([]);
        liveErrors.set([]);
        liveCls.set(0);
        debugSession.set({
          active: true,
          tabId: p.tabId ?? null,
          url: p.url || '',
          title: p.title || '',
          startedAt: p.timestamp || Date.now(),
        });
        activeMainTab.set('webdebugger');
      } else if (p.action === 'stop') {
        debugSession.update((s) => ({ ...s, active: false }));
      }
    }),
  );

  unlisteners.push(
    await listen<WebAuditReport>('web_debug_audit', (event) => {
      const report = event.payload;
      if (!report) return;
      currentAudit.set(report);
      auditHistory.update((hist) => [report, ...hist.slice(0, 9)]);
      activeMainTab.set('webdebugger');
    }),
  );

  unlisteners.push(
    await listen<any>('web_debug_events', (event) => {
      const batch = event.payload?.events;
      if (!Array.isArray(batch)) return;

      const newReqs: LiveNetworkEvent[] = [];
      const newErrs: LiveConsoleEvent[] = [];

      for (const item of batch) {
        if (item.kind === 'network_request' && item.data) {
          newReqs.push({ timestamp: item.timestamp || Date.now(), ...item.data });
        } else if (item.kind === 'console_error' && item.data) {
          newErrs.push({ timestamp: item.timestamp || Date.now(), ...item.data });
        } else if (item.kind === 'vital_update' && item.data?.cls !== undefined) {
          liveCls.set(item.data.cls);
        }
      }

      if (newReqs.length > 0) {
        liveRequests.update((prev) => [...newReqs.reverse(), ...prev].slice(0, 250));
      }
      if (newErrs.length > 0) {
        liveErrors.update((prev) => [...newErrs.reverse(), ...prev].slice(0, 100));
      }
    }),
  );

  unlisteners.push(
    await listen<{ installedVersion: string; latestVersion: string }>(
      'extension_update_available',
      (event) => {
        if (event.payload) {
          extensionUpdateInfo.set(event.payload);
        }
      },
    ),
  );
}

export function buildMarkdownReport(audit: WebAuditReport): string {
  const lines = [
    `# Raydrop Web Audit Report`,
    `- **URL:** ${audit.url}`,
    `- **Title:** ${audit.title}`,
    `- **Date:** ${new Date(audit.timestamp).toLocaleString()}`,
    ``,
    `## Scores`,
    `- **Overall:** ${audit.scores.overall}/100`,
    `- **Network & Latency:** ${audit.scores.network}/100`,
    `- **Performance & Speed:** ${audit.scores.speed}/100`,
    `- **UI & Responsive:** ${audit.scores.ui}/100`,
    `- **Console & Security:** ${audit.scores.security}/100`,
    ``,
    `## Actionable Fixes (${audit.fixes.length})`,
  ];

  for (const fix of audit.fixes) {
    lines.push(`### [${fix.severity.toUpperCase()}] (${fix.category}) ${fix.title}`);
    lines.push(`- **Target:** \`${fix.target}\``);
    lines.push(`- **Saran:** ${fix.suggestion}`);
    lines.push('');
  }
  return lines.join('\n');
}
