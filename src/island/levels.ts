import type { Level } from '../lib/island-machine';
import { isFixed, tabDef, TABS, type Size, type Tab } from '../lib/tabs';

export type { Size };

/** Ada penceresiyle aynı olmalı (src-tauri/src/island.rs). */
export const WINDOW: Size = { w: 740, h: 460 };

/** Boştaki siyah şerit: sekmelerin yalnız en üstündeki birkaç pikseli kaplar. */
export const NOTCH: Size = { w: 120, h: 6 };
export const MINI: Size = { w: 620, h: 190 };

/** Açık ada: AI minisi 620×190; sabit sekmeler (K11) ve AI ayrıntısı boylarını kayıttan (tabs.ts) alır.
 * Seviye 0'da `notch` verilirse çentik o boydadır (canlı çentik, ara boy, Bitiş; lib/live.ts). */
export function sizeFor(level: Level, tab: Tab, notch: Size | null = null): Size {
  if (level === 0) return notch ?? NOTCH;
  if (level === 1 && !isFixed(tab)) return MINI;
  return tabDef(tab).size;
}

/** En büyük açık boy: pencere (WINDOW) bunu taşma payıyla birlikte kapsamalı. */
export const LARGEST: Size = TABS.reduce((m, t) => ({ w: Math.max(m.w, t.size.w), h: Math.max(m.h, t.size.h) }), MINI);

/** Açılma/büyüme animasyonunun süresi (ms): yaylı eğri, sonunda hafif taşıp yerine oturur. */
export const MORPH_MS = 560;
/** Kapanma/küçülme süresi (ms): taşma yok, hızlı. */
export const SHRINK_MS = 260;
/** Yay eğrisinin en çok taşması (≈%5,4). Pencere bölgesi animasyon sürerken bu payı da kapsar. */
export const OVERSHOOT = { w: 16, h: 24 };

/** Alt köşe yarıçapı: boştaki şerit hafif, çentik yüksekliğin yarısı, açık ada 22 (Rust apply_region ile aynı
 * kural: yükseklik 10'dan azsa 3, değilse min(h/2, 22)). */
export function radiusFor(s: Size): number {
  return s.h <= 10 ? 3 : Math.min(s.h / 2, 22);
}

/** Ada pencerenin içinde ortalanmış bir kırpma dikdörtgeni olarak çizilir (clip-path: inset). */
export function insetFor(s: Size): { x: number; y: number } {
  return { x: Math.max(0, (WINDOW.w - s.w) / 2), y: Math.max(0, WINDOW.h - s.h) };
}

/** Animasyon sürerken bildirilecek alan: eski ve yeni alanın birleşimi, büyürken taşma payıyla. */
export function morphArea(from: Size, to: Size): Size {
  const grow = to.w > from.w || to.h > from.h;
  const w = Math.max(from.w, to.w) + (grow ? OVERSHOOT.w : 0);
  const h = Math.max(from.h, to.h) + (grow ? OVERSHOOT.h : 0);
  return { w: Math.min(WINDOW.w, w), h: Math.min(WINDOW.h, h) };
}
