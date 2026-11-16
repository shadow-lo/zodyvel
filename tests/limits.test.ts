import { describe, expect, it } from 'vitest';
import { limitPill, limitTone, resetLabel } from '../src/lib/limits';

const HOUR = 3_600_000;
const now = new Date(2026, 9, 20, 12, 0).getTime();

describe('plan limiti hapı', () => {
  it('5 saatlik yüzde başta, haftalık yalnız %80 üstünde', () => {
    const pill = limitPill({ fiveHour: { pct: 73.4, resetsAt: now + 2 * HOUR }, sevenDay: { pct: 41, resetsAt: now + 80 * HOUR }, at: now }, now);
    expect(pill?.text).toBe('Claude %73');
    expect(pill?.tone).toBe('ok');
    expect(pill?.title).toBe('5 saatlik limit: %73 · 14:00 sıfırlanır\nHaftalık limit: %41 · Cum 20:00 sıfırlanır');

    const busy = limitPill({ fiveHour: { pct: 20, resetsAt: now + HOUR }, sevenDay: { pct: 92, resetsAt: now + 30 * HOUR }, at: now }, now);
    expect(busy?.text).toBe('Claude %20 · hafta %92');
    expect(busy?.tone).toBe('bad');
  });

  it('süresi dolan pencere düşer; hiçbiri yoksa hap yok', () => {
    const week = limitPill({ fiveHour: { pct: 99, resetsAt: now - 1 }, sevenDay: { pct: 41, resetsAt: now + HOUR }, at: 0 }, now);
    expect(week?.text).toBe('Claude hafta %41');
    expect(limitPill({ fiveHour: { pct: 99, resetsAt: now }, sevenDay: null, at: 0 }, now)).toBeNull();
    expect(limitPill(null, now)).toBeNull();
  });

  it('ton ve sıfırlanma saati', () => {
    expect([limitTone(74), limitTone(75), limitTone(90)]).toEqual(['ok', 'warn', 'bad']);
    expect(resetLabel(now + 30 * 60_000, now)).toBe('12:30');
    expect(resetLabel(now + 48 * HOUR, now)).toBe('Per 12:00');
  });
});
