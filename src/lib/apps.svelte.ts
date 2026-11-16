// Sekme çubuğundaki uygulama kısayolları (Faz 2b): simge önbelleği, ekleme, açma, çıkarma.
// Liste ayarlarda durur (Settings.apps); Rust (apps.rs) yalnız kayıtlı hedefleri açar. Simge yalnız
// çubuk ilk kez çizilince bir kez istenir; adres kısayolunda simge yok, harfli kutucuk çizilir.

import { call, callOrThrow, inTauri } from './ipc';
import { data, saveSettings } from './store.svelte';
import type { AppShortcut } from './types';

const icons = $state<Record<string, string | null>>({});
const asked = new Set<string>();

export function isWebApp(app: AppShortcut): boolean {
  return /^https?:\/\//i.test(app.target);
}

/** Kısayolun simgesi (data URL); henüz yoksa ister ve null döner, gelince yeniden çizilir. */
export function appIcon(app: AppShortcut): string | null {
  const key = `${app.id}|${app.target}`;
  if (!asked.has(key) && !isWebApp(app)) {
    asked.add(key);
    void call<string | null>('apps_icon', { id: app.id }).then(url => (icons[key] = url ?? null));
  }
  return icons[key] ?? null;
}

/** Harfli kutucuğun rengi: addan sabit bir ton (her açılışta aynı). */
export function appHue(app: AppShortcut): number {
  let h = 0;
  for (const ch of app.name || app.target) h = (h * 31 + ch.charCodeAt(0)) % 360;
  return h;
}

export function appLetter(app: AppShortcut): string {
  return (app.name.trim()[0] ?? '?').toLocaleUpperCase('tr');
}

/** "+": dosya seçici açılır, seçilen uygulama eklenir. Hata metni döner (yoksa null). */
export async function pickApp(): Promise<string | null> {
  if (!inTauri) return null;
  try {
    await callOrThrow<AppShortcut | null>('apps_pick');
    return null;
  } catch (error) {
    return String(error);
  }
}

/** Adres (ya da yol) ekler. Hata metni döner (yoksa null). */
export async function addApp(target: string, name: string): Promise<string | null> {
  if (!inTauri) return 'Önizlemede eklenemez';
  try {
    await callOrThrow<AppShortcut>('apps_add', { target, name });
    return null;
  } catch (error) {
    return String(error);
  }
}

export async function launchApp(app: AppShortcut): Promise<string | null> {
  if (!inTauri) return null;
  try {
    await callOrThrow('apps_launch', { id: app.id });
    return null;
  } catch (error) {
    return String(error);
  }
}

export function removeApp(id: string): void {
  const next = $state.snapshot(data.settings);
  next.apps = next.apps.filter(a => a.id !== id);
  void saveSettings(next);
}

/** Kaldırılan kısayolu eski yerine geri koyar ("Geri al"). */
export function restoreApp(app: AppShortcut, index: number): void {
  const next = $state.snapshot(data.settings);
  if (next.apps.some(a => a.id === app.id)) return;
  next.apps.splice(Math.min(index, next.apps.length), 0, app);
  void saveSettings(next);
}

/** Sırayı değiştirir (Ayarlar'daki ↑/↓). */
export function moveApp(id: string, by: -1 | 1): void {
  const next = $state.snapshot(data.settings);
  const i = next.apps.findIndex(a => a.id === id);
  const j = i + by;
  const [a, b] = [next.apps[i], next.apps[j]];
  if (i < 0 || !a || !b) return;
  next.apps[i] = b;
  next.apps[j] = a;
  void saveSettings(next);
}
