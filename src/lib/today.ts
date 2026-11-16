// Bugün bölümünün saf yardımcıları: yerel tarih, hafta, sıradaki etkinlik, Zodymetre ve modların kalanı.
// Tarihler yerel saatle "2026-10-14" biçiminde tutulur (Rust tarafı saat dilimi bilmez).

import { nextPlan } from './agenda';
import type { CalEvent, FocusPhase, FocusState, FocusStep, MeterDone, MeterState, ModeDef, ModeRun, TimerState } from './types';

const pad = (n: number) => String(n).padStart(2, '0');

export function isoDate(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function hhmm(d: Date): string {
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Pazartesiden başlayan hafta; `offset` hafta ileri/geri. */
export function weekOf(d: Date, offset = 0): Date[] {
  const monday = new Date(d.getFullYear(), d.getMonth(), d.getDate() - ((d.getDay() + 6) % 7) + offset * 7);
  return Array.from({ length: 7 }, (_, i) => new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + i));
}

const month = (d: Date) => d.toLocaleDateString('tr-TR', { month: 'long' });

/** "12–18 Ekim" ya da ay değişiyorsa "27 Ekim – 2 Kasım". */
export function weekLabel(days: Date[]): string {
  const first = days[0];
  const last = days[days.length - 1];
  if (!first || !last) return '';
  return first.getMonth() === last.getMonth()
    ? `${first.getDate()}–${last.getDate()} ${month(last)}`
    : `${first.getDate()} ${month(first)} – ${last.getDate()} ${month(last)}`;
}

/** Şu andan sonraki ilk etkinlik (Ajanda planı; tekrarlıysa sıradaki günüyle). Bugünün gün boyu etkinliği de
 * sayılır. */
export function nextEvent(events: CalEvent[], now: Date): CalEvent | null {
  const next = nextPlan(events, now);
  return next ? { ...next.event, date: next.date } : null;
}

/** "Bugün 14:00", "Yarın", "Cuma 09:30", "3 Kasım". */
export function whenLabel(event: CalEvent, now: Date): string {
  const [y, m, d] = event.date.split('-').map(Number);
  const day = new Date(y ?? 0, (m ?? 1) - 1, d ?? 1);
  const diff = Math.round((day.getTime() - new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()) / 86_400_000);
  const name =
    diff === 0
      ? 'Bugün'
      : diff === 1
        ? 'Yarın'
        : diff > 1 && diff < 7
          ? day.toLocaleDateString('tr-TR', { weekday: 'long' })
          : `${day.getDate()} ${month(day)}`;
  return event.time ? `${name} ${event.time}` : name;
}

const SHORT_DAYS = ['Paz', 'Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt'];

/** Haftalık listedeki gün etiketi: "Bugün", "Yarın", "Dün", yoksa "Cum 23". */
export function dayLabel(date: string, now: Date): string {
  const [y, m, d] = date.split('-').map(Number);
  const day = new Date(y ?? 0, (m ?? 1) - 1, d ?? 1);
  const diff = Math.round((day.getTime() - new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()) / 86_400_000);
  if (diff === 0) return 'Bugün';
  if (diff === 1) return 'Yarın';
  if (diff === -1) return 'Dün';
  return `${SHORT_DAYS[day.getDay()]} ${day.getDate()}`;
}

/** Haftanın bütün etkinlikleri, gün ve saate göre (gün boyu olanlar günün başında). */
export function weekEvents(events: CalEvent[], days: Date[]): CalEvent[] {
  const first = days[0];
  const last = days[days.length - 1];
  if (!first || !last) return [];
  const [from, to] = [isoDate(first), isoDate(last)];
  return events
    .filter(e => e.date >= from && e.date <= to)
    .sort((a, b) => a.date.localeCompare(b.date) || (a.time ?? '').localeCompare(b.time ?? ''));
}

/** Zamanlayıcıda kalan süre (ms). */
export function timerLeft(timer: TimerState, now: number): number {
  return timer.endsAt === null ? timer.leftMs : Math.max(0, timer.endsAt - now);
}

/** Odak döngüsü (today/focus.rs ile aynı): 25 dk odak, 5 dk mola; 4. odaktan sonra 15 dk uzun mola. */
export const FOCUS_MIN: Record<FocusPhase, number> = { work: 25, break: 5, longBreak: 15 };
export const FOCUS_ROUNDS = 4;

/** Bir aşama dolunca sıradaki; null: döngü bitti. */
export function nextFocus(focus: FocusState): FocusState | null {
  const round = Math.min(FOCUS_ROUNDS, Math.max(1, focus.round));
  if (focus.phase === 'work') return { phase: round >= FOCUS_ROUNDS ? 'longBreak' : 'break', round };
  if (focus.phase === 'break') return { phase: 'work', round: round + 1 };
  return null;
}

/** Zamanlayıcı kartındaki etiket: "Odak 2/4", "Mola", "Uzun mola". */
export function focusLabel(focus: FocusState): string {
  if (focus.phase === 'work') return `Odak ${focus.round}/${FOCUS_ROUNDS}`;
  return focus.phase === 'break' ? 'Mola' : 'Uzun mola';
}

/** Aşama dolunca çentikteki not. */
export function focusStepText(step: FocusStep): { title: string; detail: string } {
  const next = step.next;
  if (!next) return { title: 'Odak döngüsü tamam', detail: `${FOCUS_ROUNDS} × ${FOCUS_MIN.work} dk odak` };
  if (next.phase === 'longBreak') return { title: `${FOCUS_ROUNDS} odak tamam`, detail: `${FOCUS_MIN.longBreak} dk uzun mola` };
  if (next.phase === 'break') return { title: 'Odak bitti, mola', detail: `${FOCUS_MIN.break} dk · ${next.round}/${FOCUS_ROUNDS}` };
  return { title: 'Mola bitti, odak', detail: `${FOCUS_MIN.work} dk · ${next.round}/${FOCUS_ROUNDS}` };
}

/** Zodymetreye süre ekler (today.rs Op::TimerAdd): çalışırken bitiş ileri alınır, duraklatılmışken kalan
 * artar; dolmuşsa yalnız eklenen süreyle hemen başlar. Toplam süre en çok MAX_TIMER_MIN. */
export const MAX_TIMER_MIN = 180;
export function timerAdd<T extends TimerState>(timer: T, minutes: number, now: number): T {
  const add = Math.max(1, Math.min(60, Math.round(minutes))) * 60_000;
  if (timer.endsAt === null && timer.leftMs === 0) return { ...timer, minutes: add / 60_000, endsAt: now + add, leftMs: add };
  const total = Math.min(MAX_TIMER_MIN, timer.minutes + add / 60_000);
  if (timer.endsAt !== null) return { ...timer, minutes: total, endsAt: timer.endsAt + add };
  return { ...timer, minutes: total, leftMs: timer.leftMs + add };
}

// --- Zodymetre (K8, Faz 5a) ---------------------------------------------------------------------------

/** Aynı anda en çok Zodymetre (today.rs MAX_METERS). */
export const MAX_METERS = 3;
/** Zodymetre renk seçenekleri; boş: vurgu rengi. */
export const METER_COLORS = ['', '#5b9cff', '#2ec4b6', '#4fcf86', '#ffb020', '#ff7a59', '#f472b6'];
/** Kartta hızlı süre seçimi (dk). */
export const METER_PRESETS = [5, 10, 25, 45, 60];

/** Zodymetre başlamış ama bitmemiş mi (çalışıyor ya da yarıda duraklatılmış). */
export function meterActive(m: TimerState, now: number): boolean {
  const left = timerLeft(m, now);
  return left > 0 && (m.endsAt !== null || left < m.minutes * 60_000);
}

/** Tek yer varsa gösterilecek Zodymetre (Saat ekranı kartı): en yakın biten çalışan, yoksa yarıda
 * duraklatılmış, yoksa ilki. */
export function leadMeter(meters: MeterState[]): MeterState | null {
  const running = meters.filter(m => m.endsAt !== null).sort((a, b) => (a.endsAt ?? 0) - (b.endsAt ?? 0));
  return running[0] ?? meters.find(m => m.leftMs > 0 && m.leftMs < m.minutes * 60_000) ?? meters[0] ?? null;
}

/** today.rs::finish_meter karşılığı (önizleme): durur, bugünkü tur sayısı artar. */
export function finishMeter(m: MeterState, day: string): { meter: MeterState; done: MeterDone } {
  const fresh = m.day !== day;
  const rounds = (fresh ? 0 : m.rounds) + 1;
  const doneMs = (fresh ? 0 : m.doneMs) + m.minutes * 60_000;
  const meter = { ...m, endsAt: null, leftMs: 0, day, rounds, doneMs };
  return { meter, done: { id: m.id, name: m.name, color: m.color, minutes: m.minutes, rounds, totalMs: doneMs } };
}

/** "1 sa 15 dk", "25 dk", "2 sa"; 0: "Süresiz". */
export function minutesText(minutes: number): string {
  if (minutes <= 0) return 'Süresiz';
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (!h) return `${m} dk`;
  return m ? `${h} sa ${m} dk` : `${h} sa`;
}

// --- Modlar (Faz 5a, K4) ------------------------------------------------------------------------------

/** Mod ekranındaki süre seçenekleri (dk; 0 süresiz). */
export const MODE_DURATIONS = [25, 50, 90, 120, 0];
export const MAX_MODE_MIN = 600;
export const MAX_MODES = 8;

/** İlk açılışta gelen dört mod (today/modes.rs default_modes ile aynı; önizleme ve testler için). */
export function defaultModes(): ModeDef[] {
  const base = { onlyAi: false, music: true, quiet: false, focus: false, closeApps: [], openApps: [] };
  return [
    { ...base, id: 'work', name: 'Çalışma', icon: 'brief', color: '#5b9cff', minutes: 50, onlyAi: true, focus: true },
    { ...base, id: 'study', name: 'Ders', icon: 'book', color: '#2ec4b6', minutes: 90, onlyAi: true, music: false, quiet: true },
    { ...base, id: 'sleep', name: 'Uyku', icon: 'moon', color: '#9d7bff', minutes: 480, onlyAi: true, music: false, quiet: true },
    { ...base, id: 'game', name: 'Oyun', icon: 'game', color: '#ff7a59', minutes: 0, quiet: true },
  ];
}

/** Modun başlarken yaptığı eylemler, ekranda ve listede sayılır. */
export function modeActions(d: ModeDef, focusOn = true): string[] {
  return [
    d.onlyAi ? 'Bildirimler: yalnız AI' : '',
    d.music ? '' : 'Müzik duraklatılır',
    ...d.closeApps.map(a => `${a.name} kapanır`),
    ...d.openApps.map(a => `${a.name} açılır`),
    d.quiet ? 'Zody sessiz' : '',
    d.focus && focusOn ? '25/5 odak döngüsü' : '',
  ].filter(Boolean);
}

/** Listedeki alt satır: "50 dk · 5 eylem", "Süresiz · 2 eylem". */
export function modeSummary(d: ModeDef, focusOn = true): string {
  const n = modeActions(d, focusOn).length;
  return n ? `${minutesText(d.minutes)} · ${n} eylem` : minutesText(d.minutes);
}

/** Açık modun kalanı (ms); süresizde null. */
export function modeLeft(run: ModeRun, now: number): number | null {
  if (run.minutes <= 0) return null;
  return run.endsAt === null ? run.leftMs : Math.max(0, run.endsAt - now);
}

/** Odak aşamasının kalanı (ms); döngü yoksa null. */
export function phaseLeft(run: ModeRun, now: number): number | null {
  if (!run.focus) return null;
  return run.phaseEndsAt === null ? run.phaseLeftMs : Math.max(0, run.phaseEndsAt - now);
}

/** Süre gösterimi için açık modun zamanlayıcı görünümü (süresizse geçen süre). */
export function modeTimer(run: ModeRun): TimerState {
  return { minutes: run.minutes, endsAt: run.endsAt, leftMs: run.leftMs };
}

/** Odak döngüsü çubuğu: 4 odak + aralarda 3 mola; her parça 'done' | 'now' | 'todo'. */
export function cycleBar(focus: FocusState): { kind: 'work' | 'break'; state: 'done' | 'now' | 'todo' }[] {
  const out: { kind: 'work' | 'break'; state: 'done' | 'now' | 'todo' }[] = [];
  for (let r = 1; r <= FOCUS_ROUNDS; r++) {
    const workState = r < focus.round || (r === focus.round && focus.phase !== 'work') ? 'done' : r === focus.round ? 'now' : 'todo';
    out.push({ kind: 'work', state: workState });
    if (r === FOCUS_ROUNDS) break;
    const breakState = r < focus.round ? 'done' : r === focus.round && focus.phase === 'break' ? 'now' : 'todo';
    out.push({ kind: 'break', state: breakState });
  }
  return out;
}

/** Odak döngüsünün sıradaki aşaması yazısı: "sonra 5 dk mola". */
export function nextPhaseText(focus: FocusState): string {
  const next = nextFocus(focus) ?? { phase: 'work', round: 1 };
  if (next.phase === 'work') return `sonra ${FOCUS_MIN.work} dk odak`;
  return next.phase === 'break' ? `sonra ${FOCUS_MIN.break} dk mola` : `sonra ${FOCUS_MIN.longBreak} dk uzun mola`;
}

/** today/modes.rs::start karşılığı (önizleme). */
export function startMode(d: ModeDef, minutes: number | undefined, now: number, focusOn: boolean): ModeRun {
  const m = Math.min(MAX_MODE_MIN, minutes ?? d.minutes);
  const focus: FocusState | null = d.focus && focusOn ? { phase: 'work', round: 1 } : null;
  const phase = focus ? FOCUS_MIN.work * 60_000 : 0;
  return {
    id: d.id,
    startedAt: now,
    minutes: m,
    endsAt: m > 0 ? now + m * 60_000 : null,
    leftMs: m * 60_000,
    paused: false,
    focus,
    phaseEndsAt: focus ? now + phase : null,
    phaseLeftMs: phase,
    closed: [],
  };
}

export function pauseMode(run: ModeRun, now: number): ModeRun {
  if (run.paused) return run;
  return {
    ...run,
    paused: true,
    endsAt: null,
    leftMs: run.endsAt !== null ? Math.max(0, run.endsAt - now) : run.leftMs,
    phaseEndsAt: null,
    phaseLeftMs: run.phaseEndsAt !== null ? Math.max(0, run.phaseEndsAt - now) : run.phaseLeftMs,
  };
}

export function resumeMode(run: ModeRun, now: number): ModeRun {
  if (!run.paused) return run;
  return {
    ...run,
    paused: false,
    endsAt: run.minutes > 0 ? now + run.leftMs : null,
    phaseEndsAt: run.focus ? now + run.phaseLeftMs : null,
  };
}
