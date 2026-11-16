// Claude kullanım özeti (Özellikler → Kullanım ve maliyet özeti): yerel takvim aralıkları, biçimler,
// haftalık özet notu. Veriyi Rust okur (usage_summary); burada yalnız gün sınırları ve metin var.

import type { Toast } from './notice';

export interface UsagePart {
  name: string;
  cost: number;
  tokens: number;
}

export interface UsageTotals {
  input: number;
  output: number;
  cacheRead: number;
  tokens: number;
  /** Fiyatı bilinen mesajların tahmini API maliyeti (USD). */
  cost: number;
  /** Fiyatı bilinmeyen modellerin token'ları. */
  unpriced: number;
  messages: number;
  sessions: number;
  activeMs: number;
  files: number;
  added: number;
  removed: number;
  projects: UsagePart[];
  models: UsagePart[];
}

export interface UsageDay {
  cost: number;
  tokens: number;
  activeMs: number;
  sessions: number;
}

export interface UsageSummary {
  today: UsageTotals;
  week: UsageTotals;
  lastWeek: UsageTotals;
  /** `spans.days` ile aynı sırada: 13 gün önce … bugün. */
  days: UsageDay[];
  filesRead: number;
}

export interface Span {
  from: number;
  to: number;
}

export interface UsageSpans {
  today: Span;
  week: Span;
  lastWeek: Span;
  days: Span[];
}

const DAY_NAMES = ['Pazar', 'Pazartesi', 'Salı', 'Çarşamba', 'Perşembe', 'Cuma', 'Cumartesi'];
export const DAY_SHORT = ['Paz', 'Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt'];

function midnight(date: Date, addDays = 0): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + addDays);
}

/** Yerel gün sınırları (yaz saati geçişi dahil): bugün, bu hafta (Pazartesiden), geçen hafta, son 14 gün. */
export function usageSpans(now: Date): UsageSpans {
  const today = midnight(now);
  const tomorrow = midnight(now, 1);
  const sinceMonday = (today.getDay() + 6) % 7;
  const monday = midnight(now, -sinceMonday);
  const lastMonday = midnight(now, -sinceMonday - 7);
  const days = Array.from({ length: 14 }, (_, i) => ({ from: +midnight(now, i - 13), to: +midnight(now, i - 12) }));
  return {
    today: { from: +today, to: +tomorrow },
    week: { from: +monday, to: +tomorrow },
    lastWeek: { from: +lastMonday, to: +monday },
    days,
  };
}

/** Pazartesi tarihi (yerel, YYYY-MM-DD): haftalık özetin hangi hafta için gösterildiği. */
export function weekKey(now: Date): string {
  const monday = midnight(now, -((now.getDay() + 6) % 7));
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${monday.getFullYear()}-${pad(monday.getMonth() + 1)}-${pad(monday.getDate())}`;
}

export function money(usd: number): string {
  if (usd > 0 && usd < 0.01) return '<$0,01';
  return new Intl.NumberFormat('tr-TR', { style: 'currency', currency: 'USD', maximumFractionDigits: usd >= 100 ? 0 : 2 }).format(usd);
}

export function tokens(n: number): string {
  if (n < 1000) return String(n);
  const units: [number, string][] = [
    [1e9, 'Mr'],
    [1e6, 'M'],
    [1e3, 'B'],
  ];
  const [size, unit] = units.find(([s]) => n >= s) ?? [1e3, 'B'];
  const value = n / size;
  return `${value.toLocaleString('tr-TR', { maximumFractionDigits: value < 10 ? 1 : 0 })} ${unit}`;
}

export function hours(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 60) return `${minutes} dk`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m ? `${h} sa ${m} dk` : `${h} sa`;
}

/** Geçen haftanın en yoğun günü (aktif süreye göre); `days` 14 günlük dizi. */
export function busiestDay(spans: UsageSpans, days: UsageDay[]): string | null {
  let best: { at: number; ms: number } | null = null;
  spans.days.forEach((span, i) => {
    const day = days[i];
    if (!day || span.from < spans.lastWeek.from || span.from >= spans.lastWeek.to) return;
    if (day.activeMs > 0 && (!best || day.activeMs > best.ms)) best = { at: span.from, ms: day.activeMs };
  });
  const found = best as { at: number; ms: number } | null;
  return found ? (DAY_NAMES[new Date(found.at).getDay()] ?? null) : null;
}

/** Haftanın ilk açılışında bir kez: geçen haftanın özeti. Geçen hafta hiç çalışılmadıysa not yok. */
export function weeklyNote(summary: UsageSummary, spans: UsageSpans): Toast | null {
  const w = summary.lastWeek;
  if (!w.messages) return null;
  const busiest = busiestDay(spans, summary.days);
  const parts = [`${w.sessions} oturum`, w.files ? `${w.files} dosya` : null, busiest ? `en yoğun ${busiest}` : null].filter(Boolean);
  return {
    key: `weekly:${spans.lastWeek.from}`,
    mood: 'done',
    title: `Geçen hafta ${hours(w.activeMs)} çalıştın`,
    detail: parts.join(' · '),
    tab: 'ai',
  };
}

/** Tarayıcı önizlemesi için örnek özet. */
export function demoUsage(): UsageSummary {
  const totals = (scale: number): UsageTotals => ({
    input: Math.round(41_200_000 * scale),
    output: Math.round(612_000 * scale),
    cacheRead: Math.round(38_900_000 * scale),
    tokens: Math.round(41_812_000 * scale),
    cost: 31.4 * scale,
    unpriced: 0,
    messages: Math.round(820 * scale),
    sessions: Math.max(1, Math.round(9 * scale)),
    activeMs: Math.round(6.4 * 3_600_000 * scale),
    files: Math.round(46 * scale),
    added: Math.round(2140 * scale),
    removed: Math.round(860 * scale),
    projects: [
      { name: 'Psrc', cost: 19.8 * scale, tokens: Math.round(26e6 * scale) },
      { name: 'zodyvel-site', cost: 7.1 * scale, tokens: Math.round(9e6 * scale) },
      { name: 'notlar', cost: 4.5 * scale, tokens: Math.round(6e6 * scale) },
    ],
    models: [
      { name: 'Opus 5.5', cost: 27.9 * scale, tokens: Math.round(36e6 * scale) },
      { name: 'Haiku 5.5', cost: 3.5 * scale, tokens: Math.round(5.8e6 * scale) },
    ],
  });
  const shape = [0.4, 0.9, 0.2, 0, 1.2, 0.7, 0.3, 0.8, 1.1, 0.5, 0, 0.9, 1.3, 0.6];
  return {
    today: totals(0.18),
    week: totals(0.62),
    lastWeek: totals(1),
    days: shape.map(f => ({ cost: 4.6 * f, tokens: Math.round(6e6 * f), activeMs: Math.round(f * 1.1 * 3_600_000), sessions: Math.ceil(f * 2) })),
    filesRead: 12,
  };
}
