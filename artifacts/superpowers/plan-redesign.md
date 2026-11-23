# Implementation Plan: Raydrop Full Redesign (Starting with Logo & Favicon)

## Phase 1: Logo & Favicon Replacement (Item 1)
1. **Update Brand Assets in `public/` and `landing/`**
   - Copy `media_1795438238177_d60e11f3.png` -> `public/logo.png`, `landing/logo.png` (and root `logo.png` if kept as repo asset).
   - Copy `media_1795438238179_e28db61a.png` -> `public/icon.png`, `landing/icon.png`.
   - Generate multi-resolution `favicon.ico` from `media_1795438238179_e28db61a.png` (using `npx tauri icon` or Node/PowerShell `.NET System.Drawing` script) -> `public/favicon.ico`, `landing/favicon.ico`.
   - Regenerate `src-tauri/icons/*` (and `browser-extension/icons/*` if desired) from `media_1795438238179_e28db61a.png` via `npx tauri icon`.
2. **Update Logo & Favicon References in Code**
   - Verify `index.html` favicon links (`/favicon.ico`, add `/icon.png` 32x32 & `apple-touch-icon` for parity with `landing/index.html`).
   - Update `src/lib/components/Header.svelte`, `src/App.svelte` (splash screen), `src/lib/components/AuthModal.svelte`, and `landing/index.html` to ensure proper sizing, aspect ratio, and dark-surface contrast (especially for `light` theme where `"Ray"` is white).

## Phase 2: Design System & Shell Redesign
3. **Unify Design Tokens & Theme Styles**
   - Update `tailwind.config.js` and `src/app.postcss` to harmonize surface, border, typography, and gradient tokens with the new Raydrop brand (deep navy `#0b1936`, obsidian `#0a0a0c`, electric blue-to-cyan-to-violet accents).
4. **Redesign App Shell (`src/App.svelte` & `src/lib/components/Header.svelte`)**
   - Refine top header bar, navigation tabs, status indicators, profile/theme menus, mobile home menu grid, and startup splash screen.

## Phase 3: Redesign All Application Pages & Modals
5. **Core Transfer & Network Views**: `ActiveTransfers.svelte`, `ContactList.svelte`, `DashboardView.svelte`.
6. **Communication & Remote Views**: `ChatView.svelte`, `MeetingView.svelte`, `RemoteDesktopView.svelte`.
7. **Utility Views**: `ClipboardHistoryView.svelte`, `NotesView.svelte`, `ConverterView.svelte`.
8. **Auth & Supporting Modals**: `AuthModal.svelte`, `TransferModal.svelte`, `TraySettings.svelte`, `UpdateModal.svelte`, and remaining feature modals.

## Phase 4: Redesign Landing Page
9. **Redesign `landing/index.html`**
   - Update hero, feature cards, screenshots section, download CTAs, and footer to match the new brand identity and app redesign.

## Verification Steps
1. **Asset & Reference Check**: Verify all `favicon.ico`, `icon.png`, and `logo.png` files are updated and non-empty.
2. **Build Check**: Run `npm run build` to confirm Vite + Svelte + Tailwind compile cleanly.
3. **Type Check**: Run `npx tsc --noEmit` to confirm zero TypeScript regressions.
4. **Visual / Runtime Check**: Start `npm run dev` and verify rendering across desktop & mobile viewports and all 5 themes (`azure`, `slate`, `oled`, `cyberpunk`, `light`).
