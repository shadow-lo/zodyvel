// Ayarlar sayfaları: menü ve arama dizini aynı listeden beslenir. Yeni sayfa eklerken dosya adını `SOURCES`
// eşlemesine de yaz (tests/search.test.ts eksik olanı yakalar).

import type { IconName } from './Icon.svelte';
import type { Page } from './state.svelte';

export const PAGES: { id: Page; name: string; icon: IconName }[] = [
  { id: 'genel', name: 'Genel bakış', icon: 'home' },
  { id: 'gorunum', name: 'Görünüm', icon: 'palette' },
  { id: 'ada', name: 'Ada', icon: 'island' },
  { id: 'moduller', name: 'Modüller', icon: 'grid' },
  { id: 'ozellikler', name: 'Özellikler', icon: 'spark' },
  { id: 'kaynaklar', name: 'Kaynaklar', icon: 'grid' },
  { id: 'zody', name: 'Zody asistanı', icon: 'ai' },
  { id: 'bildirim', name: 'Bildirimler', icon: 'bell' },
  { id: 'baglanti', name: 'Bağlantılar', icon: 'link' },
  { id: 'telefon', name: 'Telefon', icon: 'phone' },
  { id: 'kisayol', name: 'Kısayollar', icon: 'key' },
  { id: 'baslangic', name: 'Başlangıç', icon: 'power' },
  { id: 'hakkinda', name: 'Hakkında', icon: 'info' },
];

/** Yeni görünümde menü üç gruba ayrılır; sıra PAGES ile aynı (tests/search.test.ts her sayfanın bir kez geçtiğini denetler). */
export const GROUPS: { title: string; pages: Page[] }[] = [
  { title: 'Genel', pages: ['genel', 'gorunum'] },
  { title: 'Ada', pages: ['ada', 'moduller', 'ozellikler', 'kaynaklar', 'zody', 'bildirim'] },
  { title: 'Sistem', pages: ['baglanti', 'telefon', 'kisayol', 'baslangic', 'hakkinda'] },
];

/** Sayfa kimliği → pages/ klasöründeki dosya. */
export const SOURCES: Record<Page, string> = {
  genel: 'Overview',
  gorunum: 'Appearance',
  ada: 'IslandPage',
  moduller: 'Modules',
  ozellikler: 'Features',
  kaynaklar: 'Sources',
  zody: 'ZodyAssistant',
  bildirim: 'Notifications',
  baglanti: 'Connections',
  telefon: 'Telefon',
  kisayol: 'Shortcuts',
  baslangic: 'Startup',
  hakkinda: 'About',
};
