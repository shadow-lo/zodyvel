// Claude plan limiti hapı: "Claude %73" (5 saatlik pencere). Haftalık pencere %80'i geçince yanına
// eklenir; ayrıntı (sıfırlanma saatleri) ipucunda. Saf; saat yerel takvime göre yazılır.
import type { LimitWindow, PlanLimits } from './types';

export type LimitTone = 'ok' | 'warn' | 'bad';

export interface LimitPill {
  text: string;
  tone: LimitTone;
  title: string;
}

const DAYS = ['Paz', 'Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt'];
const WEEK_SHOWN = 80;

/** Sıfırlanma: bir gün içindeyse "14:30", değilse "Paz 10:00". */
export function resetLabel(at: number, now: number): string {
  const d = new Date(at);
  const time = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  return at - now < 24 * 3_600_000 ? time : `${DAYS[d.getDay()]} ${time}`;
}

export function limitTone(pct: number): LimitTone {
  return pct >= 90 ? 'bad' : pct >= 75 ? 'warn' : 'ok';
}

/** Gösterilecek bilgi yoksa `null` (hap çizilmez). */
export function limitPill(limits: PlanLimits | null, now: number): LimitPill | null {
  const live = (w: LimitWindow | null) => (w && w.resetsAt > now ? w : null);
  const five = live(limits?.fiveHour ?? null);
  const week = live(limits?.sevenDay ?? null);
  if (!five && !week) return null;
  const pct = (w: LimitWindow) => Math.round(w.pct);
  const parts: string[] = [];
  if (five) parts.push(`Claude %${pct(five)}`);
  if (week && (!five || week.pct >= WEEK_SHOWN)) parts.push(five ? `hafta %${pct(week)}` : `Claude hafta %${pct(week)}`);
  const lines: string[] = [];
  if (five) lines.push(`5 saatlik limit: %${pct(five)} · ${resetLabel(five.resetsAt, now)} sıfırlanır`);
  if (week) lines.push(`Haftalık limit: %${pct(week)} · ${resetLabel(week.resetsAt, now)} sıfırlanır`);
  return { text: parts.join(' · '), tone: limitTone(Math.max(five?.pct ?? 0, week?.pct ?? 0)), title: lines.join('\n') };
}
