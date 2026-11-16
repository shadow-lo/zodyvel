import { describe, expect, it } from 'vitest';
import {
  addDays,
  blocks,
  dayDiff,
  dayLooped,
  daySummary,
  emptyForm,
  monthGrid,
  nextPlan,
  occursOn,
  plansOn,
  repeatLabel,
  snapMinute,
  untilText,
  weekdayOf,
  withRepeat,
} from '../src/lib/agenda';
import { nextEvent } from '../src/lib/today';
import type { CalEvent } from '../src/lib/types';

const plan = (id: number, date: string, time: string | null, extra: Partial<CalEvent> = {}): CalEvent => ({ id, date, time, title: `p${id}`, ...extra });

describe('tekrar kuralları (today/agenda.rs ile aynı)', () => {
  it('9 Kasım 2026 Pazartesi; kurallar günleri seçer, başlangıçtan önce yok', () => {
    expect(weekdayOf('2026-11-09')).toBe(0);
    expect(weekdayOf('2026-11-15')).toBe(6);
    const once = plan(1, '2026-11-09', '08:00');
    expect(occursOn(once, '2026-11-09') && !occursOn(once, '2026-11-10')).toBe(true);
    const daily = { ...once, repeat: 'daily' as const };
    expect(occursOn(daily, '2026-12-25') && !occursOn(daily, '2026-11-08')).toBe(true);
    const wd = { ...once, repeat: 'weekdays' as const };
    expect(['2026-11-13', '2026-11-14', '2026-11-15', '2026-11-16'].map(d => occursOn(wd, d))).toEqual([true, false, false, true]);
    const days = { ...once, repeat: 'days' as const, days: 0b1000010 };
    expect(Array.from({ length: 7 }, (_, i) => occursOn(days, addDays('2026-11-09', i)))).toEqual([false, true, false, false, false, false, true]);
  });

  it('gün farkı ve ekleme ay ve yaz saati sınırında doğru', () => {
    expect(addDays('2026-10-31', 1)).toBe('2026-11-01');
    expect(dayDiff('2026-03-30', '2026-03-28')).toBe(2);
    expect(addDays('2026-01-01', -1)).toBe('2025-12-31');
  });
});

describe('bir günün planları ve sıradaki plan', () => {
  const events = [
    plan(1, '2026-11-01', '22:00', { repeat: 'weekdays', minutes: 90 }),
    plan(2, '2026-11-10', '09:00', { minutes: 60 }),
    plan(3, '2026-11-10', null),
    plan(4, '2026-11-01', '07:30', { repeat: 'daily', minutes: 45 }),
  ];
  it('gün boyu başta, sonra saate göre; tekrarlılar o güne girer', () => {
    expect(plansOn(events, '2026-11-10').map(e => e.id)).toEqual([3, 4, 2, 1]);
    expect(plansOn(events, '2026-11-14').map(e => e.id)).toEqual([4]);
  });
  it('sıradaki: bugünün kalanı, yoksa sonraki gün; süren plan istenirse verilir', () => {
    const now = new Date(2026, 10, 10, 9, 30);
    // Saat ekranı gibi: bugünün gün boyu planı da sayılır.
    expect(nextPlan(events, now)?.event.id).toBe(3);
    expect(nextPlan(events, now, { allDay: false })?.event.id).toBe(1);
    expect(nextPlan(events, now, { allDay: false, running: true })?.event.id).toBe(2);
    const late = new Date(2026, 10, 13, 23, 50);
    expect(nextPlan(events, late)).toMatchObject({ date: '2026-11-14', start: 450 });
    // Bugün.ts nextEvent tekrarlı planı sıradaki günüyle verir (Saat ekranı "Sıradaki").
    expect(nextEvent(events, late)).toMatchObject({ id: 4, date: '2026-11-14' });
    expect(nextPlan([], now)).toBeNull();
  });
  it('kalan süre metni', () => {
    const now = new Date(2026, 10, 10, 21, 12);
    expect(untilText({ event: events[0]!, date: '2026-11-10', start: 22 * 60 }, now)).toBe('48 dk sonra');
    expect(untilText({ event: events[0]!, date: '2026-11-10', start: 23 * 60 + 30 }, now)).toBe('2 sa 18 dk sonra');
    expect(untilText({ event: events[0]!, date: '2026-11-11', start: 450 }, now)).toBe('yarın 07:30');
  });
  it('özet ve döngü anahtarı', () => {
    const day = plansOn(events, '2026-11-10');
    expect(daySummary(day)).toEqual({ count: 4, minutes: 195, looped: 2 });
    expect(dayLooped(day)).toBe(false);
    expect(dayLooped(plansOn(events, '2026-11-11'))).toBe(true);
    expect(dayLooped([plan(9, '2026-11-10', null)])).toBe(false);
  });
});

describe('çizelge ve form', () => {
  it('çakışan blok alt şeride iner; süresiz plan en az 30 dk çizilir', () => {
    const b = blocks([plan(1, 'x', '09:00', { minutes: 120 }), plan(2, 'x', '10:00', { minutes: 30 }), plan(3, 'x', '11:00'), plan(4, 'x', null)]);
    expect(b.map(x => [x.event.id, x.lane, x.start, x.end])).toEqual([
      [1, 0, 540, 660],
      [2, 1, 600, 630],
      [3, 0, 660, 690],
    ]);
  });
  it('sürükleme 15 dakikaya oturur ve gün içinde kalır', () => {
    expect(snapMinute(547, 60)).toBe(540);
    expect(snapMinute(-20, 60)).toBe(0);
    expect(snapMinute(1430, 60)).toBe(1380);
  });
  it('ay ızgarası pazartesiden başlar (1 Kasım 2026 Pazar)', () => {
    const g = monthGrid(2026, 10);
    expect(g).toHaveLength(42);
    expect(g[0]).toBe('2026-10-26');
    expect(g[6]).toBe('2026-11-01');
  });
  it('tekrar etiketi ve "Günler"e geçiş', () => {
    expect(repeatLabel(plan(1, 'x', null, { repeat: 'days', days: 0b1000010 }))).toBe('Sal, Paz');
    expect(repeatLabel(plan(1, 'x', null, { repeat: 'weekdays' }))).toBe('Hafta içi');
    expect(repeatLabel(plan(1, 'x', null))).toBe('');
    const f = emptyForm('2026-11-11', new Date(2026, 10, 10, 21, 5));
    expect(f).toMatchObject({ time: '21:30', minutes: 60, notify: true, lead: 5, repeat: 'none' });
    expect(withRepeat(f, 'days').days).toBe(0b100);
    expect(withRepeat(withRepeat(f, 'days'), 'daily').days).toBe(0);
  });
});
