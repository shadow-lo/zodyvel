import { describe, expect, it } from 'vitest';
import { busiestDay, demoUsage, hours, money, tokens, usageSpans, weekKey, weeklyNote, type UsageSummary } from '../src/lib/usage';

describe('kullanım özeti', () => {
  it('yerel gün ve hafta sınırları Pazartesiden başlar', () => {
    const now = new Date(2026, 9, 15, 14, 30); // Perşembe
    const s = usageSpans(now);
    expect(new Date(s.today.from)).toEqual(new Date(2026, 9, 15));
    expect(new Date(s.today.to)).toEqual(new Date(2026, 9, 16));
    expect(new Date(s.week.from)).toEqual(new Date(2026, 9, 12));
    expect(new Date(s.lastWeek.from)).toEqual(new Date(2026, 9, 5));
    expect(s.lastWeek.to).toBe(s.week.from);
    expect(s.days).toHaveLength(14);
    expect(s.days[13]).toEqual(s.today);
    expect(new Date(s.days[0]!.from)).toEqual(new Date(2026, 9, 2));
    // Pazar: hafta önceki Pazartesiden.
    expect(new Date(usageSpans(new Date(2026, 9, 18, 23)).week.from)).toEqual(new Date(2026, 9, 12));
    expect(weekKey(new Date(2026, 9, 18, 23))).toBe('2026-10-12');
    expect(weekKey(new Date(2026, 9, 19, 0, 1))).toBe('2026-10-19');
  });

  it('biçimler', () => {
    expect(money(0)).toMatch(/0,00/);
    expect(money(0.004)).toBe('<$0,01');
    expect(money(12.5)).toMatch(/12,50/);
    expect(tokens(950)).toBe('950');
    expect(tokens(1_250)).toBe('1,3 B');
    expect(tokens(41_800_000)).toBe('42 M');
    expect(hours(45 * 60_000)).toBe('45 dk');
    expect(hours(150 * 60_000)).toBe('2 sa 30 dk');
  });

  it('geçen haftanın en yoğun günü ve Pazartesi notu', () => {
    const spans = usageSpans(new Date(2026, 9, 15, 10));
    const summary: UsageSummary = demoUsage();
    // Geçen hafta 5-11 Ekim: dizinin 3..9 elemanları; en uzun aktif gün 9 Ekim (Cuma).
    summary.days = summary.days.map((d, i) => ({ ...d, activeMs: i === 7 ? 9e6 : i === 12 ? 9e9 : 1000 }));
    expect(busiestDay(spans, summary.days)).toBe('Cuma');
    const note = weeklyNote(summary, spans);
    expect(note?.title).toMatch(/^Geçen hafta .* çalıştın$/);
    expect(note?.detail).toContain('en yoğun Cuma');
    expect(weeklyNote({ ...summary, lastWeek: { ...summary.lastWeek, messages: 0 } }, spans)).toBeNull();
  });
});

describe('Dalga B özellikleri', () => {
  it('varsayılanlar: plan kartı ve kullanım açık, Pazartesi özeti kapalı', async () => {
    const { defaultFeatures } = await import('../src/lib/types');
    const f = defaultFeatures();
    expect([f.planReview, f.usage, f.weeklyRecap]).toEqual([true, true, false]);
  });
});

describe('kullanım özelliği kapalıyken', () => {
  it('hiçbir şey okunmaz, eski özet atılır', async () => {
    const { data, loadUsage } = await import('../src/lib/store.svelte');
    data.settings.features.usage = true;
    await loadUsage();
    expect(data.usage).not.toBeNull(); // Tarayıcıda örnek veri.
    data.settings.features.usage = false;
    await loadUsage();
    expect(data.usage).toBeNull();
  });
});
