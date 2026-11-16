import { describe, expect, it } from 'vitest';
import { cardGrid, CARDS, cleanCards, clockText, DEFAULT_CARDS, MAX_CARDS, meterLeft, toggleCard, visibleCards, type CardAvailability } from '../src/lib/clock-cards';
import { defaultSettings } from '../src/lib/types';

const all: CardAvailability = { music: true, meter: true, ai: true, agenda: true, mail: true, shelf: true, note: true, pc: true };

describe('clock cards', () => {
  it('default matches settings default and Faz 2b 2×2', () => {
    expect(defaultSettings.clock.cards).toEqual(DEFAULT_CARDS);
    expect(defaultSettings.clock.showTime).toBe(true);
    expect(CARDS).toHaveLength(8);
  });

  it('cleans unknown, duplicate and extra ids', () => {
    expect(cleanCards(['ai', 'ai', 'hava', 'music', 'meter', 'mail', 'note', 'pc', 'shelf'])).toEqual(['ai', 'music', 'meter', 'mail', 'note', 'pc']);
  });

  it('toggles in selection order and respects the limit', () => {
    let list = toggleCard([], 'mail');
    list = toggleCard(list, 'ai');
    expect(list).toEqual(['mail', 'ai']);
    expect(toggleCard(list, 'mail')).toEqual(['ai']);
    const full = cleanCards(['music', 'meter', 'ai', 'agenda', 'mail', 'shelf']);
    expect(full).toHaveLength(MAX_CARDS);
    expect(toggleCard(full, 'note')).toEqual(full);
  });

  it('hides cards whose module is off', () => {
    expect(visibleCards(['music', 'mail', 'ai'], { ...all, mail: false })).toEqual(['music', 'ai']);
  });

  it('grid shrinks as cards grow', () => {
    expect(cardGrid(2, true)).toEqual({ cols: 1, rows: 2, compact: false });
    expect(cardGrid(4, true)).toEqual({ cols: 2, rows: 2, compact: false });
    expect(cardGrid(6, true)).toEqual({ cols: 2, rows: 3, compact: true });
    expect(cardGrid(3, false)).toEqual({ cols: 3, rows: 1, compact: false });
    expect(cardGrid(5, false)).toEqual({ cols: 3, rows: 2, compact: false });
  });

  it('meter remaining time', () => {
    expect(meterLeft({ minutes: 10, endsAt: 1_000 + 300_000, leftMs: 0 }, 1_000)).toEqual({ left: 300_000, ratio: 0.5 });
    expect(meterLeft({ minutes: 10, endsAt: null, leftMs: 600_000 }, 0).ratio).toBe(1);
    expect(meterLeft({ minutes: 10, endsAt: 5, leftMs: 0 }, 99).left).toBe(0);
    expect(clockText(65_000)).toBe('01:05');
    expect(clockText(3_725_000)).toBe('1:02:05');
    expect(clockText(400)).toBe('00:01');
  });
});
