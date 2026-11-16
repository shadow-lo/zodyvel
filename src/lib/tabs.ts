// Sekme kaydı (Faz 2b): adanın bütün sekmeleri tek yerde. `Tab` tipi, simge çubuğunun sırası, ipuçları,
// boylar ve seviye kuralı (island-machine) buradan okunur. Yeni bir sekme buraya bir satır ve
// `src/island/tab-views.ts`'e bir yükleyici ekleyerek doğar (ikincisi `satisfies Record<Tab, …>` ile
// zorunlu: eksik kalırsa derleme durur).
//
// K11 (karar, 9 Kasım): mini → ayrıntı iki aşaması yalnız AI Yöneticisi'nde kalır. Diğer sekmeler
// tek, sabit ekrandır (`fixed`): sekmeye girince doğrudan tam ekran açılır, basınca büyüme yoktur.
// Saf veri: Svelte ya da bileşen içe aktarmaz (testler ve Rust'la ortak boylar için).

export interface Size {
  w: number;
  h: number;
}

export interface TabDef {
  id: string;
  /** Simge çubuğundaki ipucu ve ekran okuyucu adı. */
  title: string;
  /** true: tek sabit ekran (K11). false: mini → ayrıntı (yalnız AI Yöneticisi). */
  fixed: boolean;
  /** Sabit ekranın (ya da AI ayrıntısının) boyu. Altta boşluk kalmasın diye içeriğe göre kısıldı. */
  size: Size;
}

export const TABS = [
  // Saat düzeninin ana ekranı: yalnız Görünüm › Düzen "Saat" iken görünür, en solda.
  { id: 'clock', title: 'Saat', fixed: true, size: { w: 720, h: 252 } },
  { id: 'ai', title: 'AI Yöneticisi', fixed: false, size: { w: 720, h: 440 } },
  { id: 'music', title: 'Müzik ve kontroller', fixed: true, size: { w: 720, h: 296 } },
  { id: 'today', title: 'Bugün', fixed: true, size: { w: 720, h: 344 } },
  // Ajanda (Faz 5b, eskiz paket 2 §1–3): Sıradaki + Gün/Ay, saat çizelgesi ya da ay ızgarası, haftalık şerit.
  { id: 'agenda', title: 'Ajanda', fixed: true, size: { w: 720, h: 440 } },
  // Modlar (Faz 5a, eskiz paket 2 §4–5): solda mod listesi, sağda süre/eylemler ya da açık modun halkası.
  { id: 'modes', title: 'Modlar', fixed: true, size: { w: 720, h: 360 } },
  // Mail (Faz 5b, eskiz paket 2 §6): hesap seçici, Birincil / Güncellemeler / Tanıtımlar kartları.
  { id: 'mail', title: 'Mail', fixed: true, size: { w: 720, h: 372 } },
  { id: 'pc', title: 'Bilgisayar', fixed: true, size: { w: 720, h: 244 } },
  { id: 'shelf', title: 'Dosya rafı', fixed: true, size: { w: 720, h: 360 } },
  { id: 'zody', title: 'Zody ile konuş', fixed: true, size: { w: 720, h: 360 } },
] as const satisfies readonly TabDef[];

export type Tab = (typeof TABS)[number]['id'];

/** Simge çubuğundaki sıra. */
export const TAB_IDS: readonly Tab[] = TABS.map(t => t.id);

const BY_ID = new Map<Tab, TabDef>(TABS.map(t => [t.id, t]));

export function tabDef(id: Tab): TabDef {
  return BY_ID.get(id) ?? TABS[1];
}

/** Sabit ekran mı (mini yok, doğrudan tam ekran)? */
export function isFixed(id: Tab): boolean {
  return tabDef(id).fixed;
}

export function isTab(value: unknown): value is Tab {
  return typeof value === 'string' && BY_ID.has(value as Tab);
}

/** Görünen sekmeler, kayıttaki sırayla. Kapalı modülün sekmesi listede yoktur. */
export function visibleTabs(enabled: Partial<Record<Tab, boolean>>): Tab[] {
  return TAB_IDS.filter(id => enabled[id]);
}
