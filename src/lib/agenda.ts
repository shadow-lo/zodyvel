// Ajanda (Faz 5b) saf yardımcıları: tekrar kuralları (today/agenda.rs ile aynı), bir günün planları,
// sıradaki plan, ay ızgarası, çizelge yerleşimi ve form. Tarihler yerel "2026-11-10" biçiminde.

import type { CalEvent, PlanForm, Repeat } from './types';

const pad = (n: number) => String(n).padStart(2, '0');

export function isoDay(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function parseDay(date: string): Date {
  const [y, m, d] = date.split('-').map(Number);
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1);
}

/** Gün farkı (yaz saatinden etkilenmez). */
export function dayDiff(a: string, b: string): number {
  const ua = Date.UTC(...(a.split('-').map(Number) as [number, number, number]));
  const ub = Date.UTC(...(b.split('-').map(Number) as [number, number, number]));
  return Math.round((ua - ub) / 86_400_000) || 0;
}

export function addDays(date: string, n: number): string {
  const d = parseDay(date);
  return isoDay(new Date(d.getFullYear(), d.getMonth(), d.getDate() + n));
}

/** 0 Pazartesi … 6 Pazar. */
export function weekdayOf(date: string): number {
  return (parseDay(date).getDay() + 6) % 7;
}

/** Plan bu günde var mı (Rust `agenda::occurs_on`). */
export function occursOn(e: CalEvent, date: string): boolean {
  if (date < e.date) return false;
  switch (e.repeat ?? 'none') {
    case 'none':
      return date === e.date;
    case 'daily':
      return true;
    case 'weekdays':
      return weekdayOf(date) < 5;
    case 'days':
      return ((e.days ?? 0) & (1 << weekdayOf(date))) !== 0;
  }
}

export function minuteOf(time: string | null): number | null {
  if (!time) return null;
  const [h, m] = time.split(':').map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
}

export function hm(minute: number): string {
  const m = ((minute % 1440) + 1440) % 1440;
  return `${pad(Math.floor(m / 60))}:${pad(m % 60)}`;
}

/** Bir günün planları: gün boyu olanlar başta, sonra saate göre. */
export function plansOn(events: CalEvent[], date: string): CalEvent[] {
  return events
    .filter(e => occursOn(e, date))
    .sort((a, b) => (minuteOf(a.time) ?? -1) - (minuteOf(b.time) ?? -1) || a.id - b.id);
}

export interface Occurrence {
  event: CalEvent;
  date: string;
  /** Başlangıç dakikası (gün boyunda null). */
  start: number | null;
}

/** Şu andan sonraki ilk plan; tekrarlarıyla, en çok `horizon` gün ileriye. `running`: şu an süren planı da ver
 * (Ajanda'daki "Şimdi" kartı). `allDay`: gün boyu planlar da sayılır (Saat ekranı; Ajanda'da sayılmaz). */
export function nextPlan(events: CalEvent[], now: Date, { horizon = 400, running = false, allDay = true } = {}): Occurrence | null {
  const today = isoDay(now);
  const minute = now.getHours() * 60 + now.getMinutes();
  // Tekrarsız planlar tarihe göre sıralı; tekrarlılar her gün olabilir. Önce yakın günler taranır.
  const last = events.reduce((m, e) => (e.repeat && e.repeat !== 'none' ? m : e.date > m ? e.date : m), today);
  const span = Math.min(horizon, Math.max(7, dayDiff(last, today) + 1));
  for (let i = 0; i <= span; i++) {
    const date = addDays(today, i);
    for (const event of plansOn(events, date)) {
      const start = minuteOf(event.time);
      if (start === null && !allDay) continue;
      if (i > 0 || start === null) return { event, date, start };
      const end = start + (event.minutes ?? 0);
      if (start >= minute || (running && end > minute)) return { event, date, start };
    }
  }
  return null;
}

/** Ay ızgarası: pazartesiden başlayan 6 hafta (42 gün). */
export function monthGrid(year: number, month: number): string[] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() + 6) % 7;
  return Array.from({ length: 42 }, (_, i) => isoDay(new Date(year, month, 1 - lead + i)));
}

/** "Her gün", "Hafta içi", "Sal, Paz"; tekrarsız ''. */
const SHORT = ['Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt', 'Paz'];
export function repeatLabel(e: CalEvent): string {
  switch (e.repeat ?? 'none') {
    case 'daily':
      return 'Her gün';
    case 'weekdays':
      return 'Hafta içi';
    case 'days':
      return SHORT.filter((_, i) => ((e.days ?? 0) & (1 << i)) !== 0).join(', ');
    default:
      return '';
  }
}

export const looped = (e: CalEvent) => (e.repeat ?? 'none') !== 'none';

/** Günün özeti: "8 plan · 11 sa 30 dk dolu · 4'ü döngüde". */
export function daySummary(plans: CalEvent[]): { count: number; minutes: number; looped: number } {
  return { count: plans.length, minutes: plans.reduce((m, e) => m + (e.time ? (e.minutes ?? 0) : 0), 0), looped: plans.filter(looped).length };
}

/** "Döngü" anahtarı açık mı: günün saatli planlarının hepsi her gün tekrarlanıyor (en az biri varsa). */
export function dayLooped(plans: CalEvent[]): boolean {
  const timed = plans.filter(e => e.time);
  return timed.length > 0 && timed.every(e => e.repeat === 'daily' || e.repeat === 'weekdays' || e.repeat === 'days');
}

export interface Block {
  event: CalEvent;
  start: number;
  end: number;
  /** 0 üst şerit, 1 alt şerit (çakışan bloklar). */
  lane: 0 | 1;
}

/** Çizelge blokları: süresiz plan 30 dk kadar çizilir; çakışan blok alt şeride iner. */
export const MIN_BLOCK = 30;
export function blocks(plans: CalEvent[]): Block[] {
  const out: Block[] = [];
  const ends: [number, number] = [-1, -1];
  for (const event of plans) {
    const start = minuteOf(event.time);
    if (start === null) continue;
    const end = Math.min(24 * 60, start + Math.max(event.minutes ?? 0, MIN_BLOCK));
    const lane: 0 | 1 = start >= ends[0] ? 0 : start >= ends[1] ? 1 : ends[0] <= ends[1] ? 0 : 1;
    ends[lane] = end;
    out.push({ event, start, end, lane });
  }
  return out;
}

/** Sürüklemede 15 dakikalık adım; gün içinde kalır. */
export function snapMinute(minute: number, length: number): number {
  return Math.max(0, Math.min(24 * 60 - Math.max(15, length), Math.round(minute / 15) * 15));
}

export const PLAN_COLORS = ['#2ec4b6', '#5b9cff', '#8b7cff', '#f472b6', '#ff7a59', '#4fcf86', '#9aa3b5'];
export const PLAN_MINUTES = [15, 30, 45, 60, 90];
export const LEAD_MINUTES = 5;

export function emptyForm(date: string, now: Date): PlanForm {
  // Sıradaki tam ya da yarım saat.
  const m = Math.min(23 * 60 + 30, Math.ceil((now.getHours() * 60 + now.getMinutes() + 1) / 30) * 30);
  return { date, time: hm(m), title: '', minutes: 60, color: PLAN_COLORS[0] ?? '', repeat: 'none', days: 0, notify: true, lead: LEAD_MINUTES, phone: true };
}

export function formOf(e: CalEvent): PlanForm {
  return {
    date: e.date,
    time: e.time,
    title: e.title,
    minutes: e.minutes ?? 0,
    color: e.color ?? '',
    repeat: e.repeat ?? 'none',
    days: e.days ?? 0,
    notify: e.notify ?? true,
    lead: e.lead ?? 0,
    phone: e.phone ?? true,
  };
}

/** Tekrar seçimi değişince gün maskesi: "Günler"e geçişte başlangıç gününün kendisi seçili gelir. */
export function withRepeat(form: PlanForm, repeat: Repeat): PlanForm {
  const days = repeat === 'days' ? form.days || 1 << weekdayOf(form.date) : 0;
  return { ...form, repeat, days };
}

/** "48 dk", "2 sa 5 dk", "yarın 08:00", "Cuma 09:30". */
export function untilText(o: Occurrence, now: Date): string {
  const days = dayDiff(o.date, isoDay(now));
  if (days === 0 && o.start !== null) {
    const left = o.start - (now.getHours() * 60 + now.getMinutes());
    if (left <= 0) return 'şimdi';
    const h = Math.floor(left / 60);
    return h ? `${h} sa ${left % 60} dk sonra` : `${left} dk sonra`;
  }
  if (days === 0) return 'bugün';
  if (days === 1) return o.start === null ? 'yarın' : `yarın ${hm(o.start)}`;
  const name = parseDay(o.date).toLocaleDateString('tr-TR', days < 7 ? { weekday: 'long' } : { day: 'numeric', month: 'long' });
  return o.start === null ? name : `${name} ${hm(o.start)}`;
}
