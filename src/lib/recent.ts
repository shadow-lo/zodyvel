// "En son hareketli bölümde aç" (Özellikler → Ada): ada açılırken en yeni etkinliği olan bölüm.
// Saf fonksiyonlar; etkinlik anlarını store (`data.activity`) tutar, Island.svelte `openTab` ile sorar.

import { isTab, type Tab } from './tabs';
import type { IslandContext } from './store.svelte';
import type { NowPlaying } from './types';

/** Bölümlerin en son etkinlik anı (Unix ms); yoksa 0 ya da hiç. */
export type Activity = Partial<Record<Tab, number>>;

/** Görünen bölümler arasında en yeni etkinliği olan; hiçbirinde etkinlik yoksa `fallback`. */
export function recentTab(activity: Activity, tabs: readonly Tab[], fallback: Tab): Tab {
  let best = fallback;
  let at = 0;
  for (const tab of tabs) {
    const when = activity[tab] ?? 0;
    if (when > at) {
      best = tab;
      at = when;
    }
  }
  return best;
}

/** Müzik etkinliği: çalmaya başladı ya da şarkı değişti. Windows'un zaman çizelgesi anı (`updatedAt`)
 *  çalarken sürekli yenilenebilir; onu kullanmak müziği hep "en yeni" yapardı. */
export function musicStarted(prev: NowPlaying | null, next: NowPlaying | null): boolean {
  if (!next?.playing) return false;
  return !prev?.playing || prev.title !== next.title || prev.artist !== next.artist || prev.app !== next.app;
}

const LAST_TAB = 'zodyvel.island.lastTab';

/** "Son bıraktığım" için kayıtlı bölüm (ada penceresinin localStorage'ı); yoksa null. */
export function savedTab(): Tab | null {
  try {
    const t = localStorage.getItem(LAST_TAB);
    return isTab(t) ? t : null;
  } catch {
    return null;
  }
}

export function saveTab(tab: Tab): void {
  try {
    localStorage.setItem(LAST_TAB, tab);
  } catch {
    // depolama kapalı: yalnız bu oturumda hatırlanır
  }
}

export interface OpenInput {
  /** Özellikler → Ada → En son hareketli bölümde aç. */
  on: boolean;
  /** Ayarlar › Ada › Üzerine gelince "Son bıraktığım": adanın en son açık kaldığı bölüm; değilse null. */
  last: Tab | null;
  ctx: Pick<IslandContext, 'approvals' | 'agents' | 'music' | 'tabs' | 'hoverTab'>;
  /** Store'un tuttuğu anlar: müziğin başlaması, zamanlayıcının dolması / etkinliğin saati. */
  activity: { music: number; today: number };
  /** Kaynaklar düzeni açıksa son yeni mail / kırık CI anı (AI bölümüne sayılır); değilse 0. */
  sourcesAt: number;
}

/** Ada açılırken gelecek bölüm. Bekleyen onay her zaman AI; "Son bıraktığım" seçiliyse en son açık kalan
 *  bölüm (hâlâ görünüyorsa); Saat düzeninde Saat; "En son hareketli" açıksa en yeni etkinlik; değilse her
 *  zamanki kural (hoverTab). */
export function openTab({ on, last, ctx, activity, sourcesAt }: OpenInput): Tab {
  if (ctx.approvals.length) return ctx.hoverTab;
  if (last && ctx.tabs.includes(last)) return last;
  // Saat düzeninde ana ekran her zaman Saat: kartları zaten en yeni etkinliği gösterir.
  if (!on || ctx.hoverTab === 'clock') return ctx.hoverTab;
  const ai = Math.max(0, sourcesAt, ...ctx.agents.map(a => Math.max(a.statusSince, a.turnEndedAt ?? 0)));
  const music = ctx.music?.playing ? activity.music : 0;
  return recentTab({ ai, music, today: activity.today }, ctx.tabs, ctx.hoverTab);
}
