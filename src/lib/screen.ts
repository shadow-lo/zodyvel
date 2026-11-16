// Ada boyutu ve ekran seçimi: Ayarlar → Ada. Aralık Rust'taki display.rs ile aynı (tests/screen.test.ts).

import type { Screen } from './types';

/** Ada boyutu yüzdesi: kaydırıcının aralığı ve adımı. */
export const SIZE = { min: 85, max: 130, step: 5 } as const;

/** Seçilen ekran şu an bağlı değilse ada birincil ekranda durur; ayarda bunu söyleriz. */
export function missingScreen(screens: Screen[], id: string): boolean {
  return id !== '' && screens.length > 0 && !screens.some(s => s.id === id);
}

/** Tarayıcı önizlemesi (pnpm dev) için iki ekranlı örnek. */
export const DEMO_SCREENS: Screen[] = [
  { id: '\\\\.\\DISPLAY2', label: 'Ekran 2 · 2560×1440 · %150', primary: false },
  { id: '\\\\.\\DISPLAY1', label: 'Ekran 1 · 1920×1080 · %100', primary: true },
];
