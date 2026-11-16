// Saat ana ekranının kart kaydı (Faz 3). Kimlikler Rust settings.rs CLOCK_CARDS ile aynı.
// Kullanıcı düzenleme kipinde en çok MAX_CARDS kart seçer; seçim sırası gösterim sırasıdır. Modülü ya da
// özelliği kapalı kart seçili kalsa bile çizilmez (veri okumaz) ve düzenleme kipinde soluk görünür.

export type CardId = 'music' | 'meter' | 'ai' | 'agenda' | 'mail' | 'shelf' | 'note' | 'pc';

export interface CardDef {
  id: CardId;
  title: string;
  /** Kapalıyken düzenleme kipinde gösterilen kısa neden. */
  off: string;
}

export const CARDS: CardDef[] = [
  { id: 'music', title: 'Müzik', off: 'Müzik modülü kapalı' },
  { id: 'meter', title: 'Zodymetre', off: 'Bugün modülü kapalı' },
  { id: 'ai', title: 'AI', off: 'AI modülü kapalı' },
  { id: 'agenda', title: 'Ajanda', off: 'Bugün modülü kapalı' },
  { id: 'mail', title: 'Mail', off: 'Ayarlar › Kaynaklar › Mail' },
  { id: 'shelf', title: 'Raf', off: 'Raf özelliği kapalı' },
  { id: 'note', title: 'Not', off: '' },
  { id: 'pc', title: 'Bilgisayar', off: 'Bilgisayar modülü kapalı' },
];

export const MAX_CARDS = 6;
export const DEFAULT_CARDS: CardId[] = ['music', 'ai', 'agenda', 'shelf'];

const IDS = new Set<string>(CARDS.map(c => c.id));

export function isCard(id: string): id is CardId {
  return IDS.has(id);
}

/** Ayardaki listeyi temizler: bilinmeyen ve tekrar atılır, en çok MAX_CARDS. */
export function cleanCards(list: readonly string[]): CardId[] {
  const out: CardId[] = [];
  for (const id of list) if (isCard(id) && !out.includes(id) && out.length < MAX_CARDS) out.push(id);
  return out;
}

/** Düzenleme kipinde karta dokununca: seçiliyse çıkar, değilse (yer varsa) sona ekle. */
export function toggleCard(list: readonly CardId[], id: CardId): CardId[] {
  if (list.includes(id)) return list.filter(c => c !== id);
  return list.length >= MAX_CARDS ? [...list] : [...list, id];
}

/** Hangi kartların verisi var (modül/özellik açık). */
export type CardAvailability = Record<CardId, boolean>;

/** Ekranda çizilecek kartlar: seçili ve kullanılabilir olanlar, seçim sırasıyla. */
export function visibleCards(list: readonly string[], available: CardAvailability): CardId[] {
  return cleanCards(list).filter(id => available[id]);
}

export interface Grid {
  cols: number;
  rows: number;
  /** Kartlar küçülsün mü (3 satır ya da dar sütunda alt satırı kısaltır). */
  compact: boolean;
}

/** Kart sayısına ve büyük saatin açık olmasına göre ızgara. Kart sayısı arttıkça kartlar küçülür. */
export function cardGrid(count: number, showTime: boolean): Grid {
  if (count <= 0) return { cols: 1, rows: 1, compact: false };
  if (showTime) {
    if (count <= 2) return { cols: 1, rows: count, compact: false };
    if (count <= 4) return { cols: 2, rows: 2, compact: false };
    return { cols: 2, rows: 3, compact: true };
  }
  if (count <= 3) return { cols: count, rows: 1, compact: false };
  if (count === 4) return { cols: 2, rows: 2, compact: false };
  return { cols: 3, rows: 2, compact: false };
}

/** Zodymetre kartı için kalan süre (ms) ve doluluk (0…1, kalan oran). */
export function meterLeft(timer: { minutes: number; endsAt: number | null; leftMs: number }, now: number): { left: number; ratio: number } {
  const full = Math.max(1, timer.minutes * 60_000);
  const left = timer.endsAt !== null ? Math.max(0, timer.endsAt - now) : Math.max(0, timer.leftMs);
  return { left, ratio: Math.min(1, left / full) };
}

/** "mm:ss" ya da bir saatten uzunsa "s:mm:ss". */
export function clockText(ms: number): string {
  const total = Math.ceil(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${h ? `${h}:` : ''}${pad(m)}:${pad(s)}`;
}
