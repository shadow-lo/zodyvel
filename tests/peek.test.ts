import { describe, expect, it } from 'vitest';
import { PEEK_SHORT_MS, PEEK_WAVE_MS, WAVE_COOLDOWN_MS, peekPlan, startupGreeting, tourSteps } from '../src/lib/peek';

const base = { enabled: true, click: false, reduceMotion: false, notice: false, lastWaveAt: null, now: 1_000_000 };

describe('peek', () => {
  it('ilk gelişte el sallar, mini selam bitince açılır', () => {
    expect(peekPlan(base)).toEqual({ peek: true, wave: true, holdMs: PEEK_WAVE_MS });
  });

  it('yakın zamanda selam verdiyse yalnız kafa görünür, mini çabuk açılır', () => {
    expect(peekPlan({ ...base, lastWaveAt: base.now - 10_000 })).toEqual({ peek: true, wave: false, holdMs: PEEK_SHORT_MS });
    expect(peekPlan({ ...base, lastWaveAt: base.now - WAVE_COOLDOWN_MS })).toMatchObject({ peek: true, wave: true });
  });

  it('ayar kapalı, hareket azaltılmış ya da şerit bildirimdeyse doğrudan mini', () => {
    expect(peekPlan({ ...base, enabled: false })).toEqual({ peek: false });
    expect(peekPlan({ ...base, reduceMotion: true })).toEqual({ peek: false });
    expect(peekPlan({ ...base, notice: true })).toEqual({ peek: false });
  });
});

describe('tıklayınca aç kipi', () => {
  it('peek kendiliğinden mini açmaz; selam ayarı kapalı ya da hareket azaltılmış olsa da Zody çıkar', () => {
    const click = { ...base, click: true };
    expect(peekPlan(click)).toEqual({ peek: true, wave: true, holdMs: null });
    expect(peekPlan({ ...click, enabled: false, lastWaveAt: base.now - 10_000 })).toEqual({ peek: true, wave: false, holdMs: null });
    expect(peekPlan({ ...click, reduceMotion: true })).toEqual({ peek: true, wave: false, holdMs: null });
  });

  it('bildirim varken peek yok: şeridin kendisine tıklanır', () => {
    expect(peekPlan({ ...base, click: true, notice: true })).toEqual({ peek: false });
  });
});

describe('karşılama', () => {
  it('ilk açılışta tur, sonra kısa selam; hareket azaltılmışsa selam yok ama tur var', () => {
    expect(startupGreeting({ welcomed: false, greet: true, reduceMotion: false })).toBe('tour');
    expect(startupGreeting({ welcomed: false, greet: false, reduceMotion: true })).toBe('tour');
    expect(startupGreeting({ welcomed: true, greet: true, reduceMotion: false })).toBe('hello');
    expect(startupGreeting({ welcomed: true, greet: true, reduceMotion: true })).toBeNull();
    expect(startupGreeting({ welcomed: true, greet: false, reduceMotion: false })).toBeNull();
  });

  it('tur kullanıcının kısayollarını anlatır; boş kısayol cümlesi atlanır', () => {
    const steps = tourSteps({ toggle: 'Ctrl+Alt+Z', approve: 'Ctrl+Alt+Enter', deny: 'Ctrl+Alt+Backspace' });
    expect(steps).toHaveLength(5);
    expect(steps[2]?.text).toContain('Ctrl+Alt+Enter onaylar, Ctrl+Alt+Backspace reddeder.');
    expect(steps[4]?.text.startsWith('Ctrl+Alt+Z adayı')).toBe(true);
    const bare = tourSteps({ toggle: '', approve: '', deny: '' });
    expect(bare[2]?.text).not.toContain('onaylar');
    expect(bare[4]?.text.startsWith('Tema')).toBe(true);
  });

  it('turda yardımcı özellikler yalnız açıksa anılır; hiçbiri açık değilse adım yok', () => {
    const keys = { toggle: '', approve: '', deny: '' };
    const both = tourSteps(keys, { shelf: true, focus: true })[3]!;
    expect(both.title).toBe('Küçük yardımcılar');
    expect(both.text).toContain('sürükle');
    expect(both.text).toContain('Odak');
    expect(both.text).toContain('Özellikler');
    const onlyShelf = tourSteps(keys, { shelf: true, focus: false })[3]!.text;
    expect(onlyShelf).toContain('sürükle');
    expect(onlyShelf).not.toContain('Odak');
    expect(tourSteps(keys, { shelf: false, focus: true })[3]!.text).not.toContain('sürükle');
    const none = tourSteps(keys, { shelf: false, focus: false });
    expect(none).toHaveLength(4);
    expect(none.map(s => s.title)).not.toContain('Küçük yardımcılar');
  });

  it('her adımın Zody ruh hâli var; tur kısa kalır', () => {
    const steps = tourSteps({ toggle: 'Ctrl+Alt+Z', approve: 'Ctrl+Alt+Enter', deny: 'Ctrl+Alt+Backspace' });
    expect(steps.every(s => s.mood)).toBe(true);
    expect(steps.every(s => s.text.length <= 190)).toBe(true);
  });
});
