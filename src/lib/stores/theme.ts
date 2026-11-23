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
    name: 'Raydrop Obsidian',
    description: 'Pure monochrome black & white editorial contrast',
    previewBg: '#09090b',
    previewAccent: '#ffffff',
  },
  {
    id: 'slate',
    name: 'Nordic Graphite',
    description: 'Studio charcoal with understated glacier-teal contrast',
    previewBg: '#111318',
    previewAccent: '#2dd4bf',
  },
  {
    id: 'oled',
    name: 'Velvet Noir',
    description: 'True pitch-black OLED canvas with crisp hairline depth',
    previewBg: '#000000',
    previewAccent: '#38bdf8',
  },
  {
    id: 'cyberpunk',
    name: 'Royal Amethyst',
    description: 'Velvety midnight-plum with refined iris-violet glow',
    previewBg: '#0b0914',
    previewAccent: '#a78bfa',
  },
  {
    id: 'light',
    name: 'Porcelain Daylight',
    description: 'Crisp porcelain white with deep ink & cobalt accents',
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
