# Brainstorm: Raydrop Full Redesign & Brand Asset Update

## Goal
Redesign all pages/views of Raydrop (desktop/web app + landing page) with a cohesive, modern visual identity, starting immediately with:
1. Replacing the existing logo and favicon across the application and landing page with the newly attached Raydrop brand assets:
   - **Horizontal Logo** (`media_1795438238177_d60e11f3.png`): Droplet-"R" mark + "Raydrop" wordmark ("Ray" in white, "drop" in cyan) + tagline "FAST & SECURE P2P FILE TRANSFER".
   - **App Icon / Favicon** (`media_1795438238179_e28db61a.png`): Rounded dark-navy square with the cyan/blue/purple droplet-"R" mark.

## Constraints
- **Tech Stack**: Svelte 4 + Tailwind CSS + Vite + Tauri v2 (`src/`), plus static HTML/CSS/JS landing page (`landing/index.html`).
- **Multi-Theme Support**: App supports 5 themes (`azure`, `slate`, `oled`, `cyberpunk`, `light`) in `src/lib/stores/theme.ts` and `src/app.postcss`.
- **Contrast Constraint**: The new horizontal logo has white text for `"Ray"`. On dark backgrounds (`azure`, `slate`, `oled`, `cyberpunk`) it has high contrast, whereas on `light` theme surfaces it will need a dark pill/container or theme-aware treatment so `"Ray"` remains legible.
- **Tauri & Web Assets**: Favicons and icons live in multiple places (`public/`, `landing/`, `src-tauri/icons/`, `browser-extension/icons/`). `npx tauri icon` or a script is needed to regenerate `.ico` / `.icns` / multi-size PNGs cleanly without Pillow (not installed in Python).
- **Functional Preservation**: Redesign must preserve all existing P2P, WebRTC, LAN, chat, remote desktop, converter, notes, clipboard, and Tauri IPC functionality.

## Risks
- **Light Mode Contrast**: White `"Ray"` text in `logo.png` disappearing on light backgrounds in `Clean Light` theme.
- **Asset Drift**: Missing a copy of `favicon.ico`, `icon.png`, or `logo.png` in `public/`, `landing/`, or `src-tauri/icons/`, causing mixed old/new branding.
- **Scope Ambiguity**: User started a numbered list (`1. ganti logo dan favicon...`) — they may have specific design preferences or additional numbered items for the full page redesign.

## Acceptance Criteria
1. All `logo.png`, `icon.png`, `favicon.ico` (and Tauri app icons if included) reflect the new attached brand assets with zero old logos remaining.
2. `index.html`, `landing/index.html`, `Header.svelte`, `App.svelte` splash screen, and `AuthModal.svelte` display the updated logo/favicon cleanly without clipping or contrast issues.
3. All app views (`home`, `transfers`, `chat`, `meeting`, `remote`, `dashboard`, `clipboard`, `notes`, `converter`, `lan`/`devices`/`contacts`) and `landing/index.html` follow a unified, polished design system across all 5 themes and responsive breakpoints.
4. `npm run build` succeeds with zero errors.
