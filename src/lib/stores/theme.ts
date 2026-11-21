import { writable } from 'svelte/store';

export type RaydropTheme = 'slate' | 'oled' | 'cyberpunk' | 'light';

export interface ThemeOption {
  id: RaydropTheme;
  name: string;
  description: string;
  previewBg: string;
  previewAccent: string;
}

export const RAYDROP_THEMES: ThemeOption[] = [
  {
    id: 'slate',
    name: 'Midnight Slate',
    description: 'Default dark studio theme with balanced contrast',
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
    const saved = localStorage.getItem(STORAGE_KEY) as RaydropTheme | null;
    if (saved && ['slate', 'oled', 'cyberpunk', 'light'].includes(saved)) {
      return saved;
    }
  }
  return 'slate';
}

export const currentTheme = writable<RaydropTheme>(getInitialTheme());

export function applyTheme(theme: RaydropTheme) {
  currentTheme.set(theme);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, theme);
  }
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-theme', theme);
  }
}

// Apply immediately on module load
if (typeof document !== 'undefined') {
  document.documentElement.setAttribute('data-theme', getInitialTheme());
}
