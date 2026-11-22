import { writable } from 'svelte/store';
import { raydropApi } from '../utils/tauri-ipc';

export type RaydropTheme = 'azure' | 'slate' | 'oled' | 'cyberpunk' | 'light';

export interface ThemeOption {
  id: RaydropTheme;
  name: string;
  description: string;
  previewBg: string;
  previewAccent: string;
}

export const RAYDROP_THEMES: ThemeOption[] = [
  {
    id: 'azure',
    name: 'Carbon Azure',
    description: 'Default matte obsidian carbon with vibrant electric blue accents',
    previewBg: '#0a0a0c',
    previewAccent: '#0078e8',
  },
  {
    id: 'slate',
    name: 'Midnight Slate',
    description: 'Deep studio navy-slate with electric cyan contrast',
    previewBg: '#0f172a',
    previewAccent: '#06b6d4',
  },
  {
    id: 'oled',
    name: 'OLED True Black',
    description: 'Pure pitch-black canvas for OLED displays',
    previewBg: '#000000',
    previewAccent: '#22d3ee',
  },
  {
    id: 'cyberpunk',
    name: 'Cyberpunk Neon',
    description: 'Deep synthwave violet with neon cyan & magenta glow',
    previewBg: '#110924',
    previewAccent: '#d946ef',
  },
  {
    id: 'light',
    name: 'Clean Light',
    description: 'Crisp high-visibility daylight mode',
    previewBg: '#f8fafc',
    previewAccent: '#0284c7',
  },
];

const STORAGE_KEY = 'raydrop_theme';

function getInitialTheme(): RaydropTheme {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem(STORAGE_KEY) as string | null;
    if (saved === 'noir') return 'azure';
    if (saved && ['azure', 'slate', 'oled', 'cyberpunk', 'light'].includes(saved)) {
      return saved as RaydropTheme;
    }
  }
  return 'azure';
}

export const currentTheme = writable<RaydropTheme>(getInitialTheme());

export function applyTheme(theme: RaydropTheme, persistToBackend: boolean = true) {
  currentTheme.set(theme);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, theme);
  }
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', theme);
  }
  if (persistToBackend) {
    raydropApi.setUiTheme(theme).catch(() => {});
  }
}

// Apply immediately on module load
if (typeof document !== 'undefined') {
  document.documentElement.setAttribute('data-theme', getInitialTheme());
}
