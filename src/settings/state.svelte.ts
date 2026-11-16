// Ayarlar penceresinin ortak durumu: kaydetme, Windows ile başlat, bağlantı denetimi, açık sayfa.
// Sayfalar ayrı bileşenler; Genel bakış ve menü rozeti aynı veriye bakar.

import { call, callOrThrow } from '../lib/ipc';
import { data, saveSettings } from '../lib/store.svelte';
import type { Autostart, Inspection, Settings } from '../lib/types';

export type Page = 'genel' | 'gorunum' | 'ada' | 'moduller' | 'ozellikler' | 'kaynaklar' | 'bildirim' | 'baglanti' | 'telefon' | 'kisayol' | 'baslangic' | 'hakkinda' | 'zody';

const PAGE_KEY = 'zodyvel.settings.page';
function savedPage(): Page {
  try {
    return (localStorage.getItem(PAGE_KEY) as Page | null) ?? 'genel';
  } catch {
    return 'genel';
  }
}

export const ui = $state({
  page: savedPage(),
  saving: false,
  failed: false,
  /** Windows ile başlat: doğruluk kaynağı kayıt defteri (tepsiden de değişebilir). */
  autostart: null as Autostart | null,
  autostartError: null as string | null,
  connections: [] as Inspection[],
});

/** Aramadan gidilen ayarın adı; sayfa çizilince satır kısa süre vurgulanır (Settings.svelte). */
export const reveal = $state({ title: '', tick: 0 });

export function go(page: Page, title = ''): void {
  if (title) {
    reveal.title = title;
    reveal.tick++;
  }
  ui.page = page;
  try {
    localStorage.setItem(PAGE_KEY, page);
  } catch {
    // Depolama kapalıysa son sayfa hatırlanmaz; önemli değil.
  }
}

export async function update(change: (s: Settings) => void): Promise<void> {
  const next = structuredClone($state.snapshot(data.settings)) as Settings;
  change(next);
  ui.saving = true;
  ui.failed = false;
  try {
    await saveSettings(next);
  } catch {
    ui.failed = true;
  } finally {
    ui.saving = false;
  }
}

export async function loadAutostart(): Promise<void> {
  ui.autostart = (await call<Autostart>('autostart_get')) ?? null;
}

export async function setAutostart(enabled: boolean): Promise<void> {
  ui.autostartError = null;
  try {
    ui.autostart = await callOrThrow<Autostart>('autostart_set', { enabled });
  } catch (e) {
    ui.autostartError = String(e);
    await loadAutostart();
  }
}

export async function refreshConnections(): Promise<void> {
  const next = await call<Inspection[]>('connections_inspect');
  if (next) ui.connections = next;
}

/** Bağlantılarda kullanıcının bakması gereken sorun sayısı (menü rozeti). */
export function connectionIssues(items: Inspection[]): number {
  return items.filter(i => i.error || !i.hookReady || i.needsUpdate).length;
}
