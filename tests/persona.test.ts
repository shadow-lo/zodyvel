import { describe, expect, it } from 'vitest';
import { gaze, lookFor, PLAYS, reactionFor, tapReaction, TRIPLE_MS } from '../src/mascot/persona';

describe('lookFor', () => {
  it('boşta hap gözler, üzerine gelince gülümser ve kızarır', () => {
    expect(lookFor('idle', null, false)).toEqual({ pill: true, face: null, cheek: false });
    expect(lookFor('idle', null, true)).toEqual({ pill: true, face: 'smile', cheek: true });
  });
  it('onayda ağız "o", çizgi yüzlü durumlar kendi yüzünü gösterir', () => {
    expect(lookFor('attention', null, false).face).toBe('o');
    expect(lookFor('done', null, false)).toEqual({ pill: false, face: 'done', cheek: true });
    expect(lookFor('sleep', null, false)).toEqual({ pill: false, face: 'sleep', cheek: false });
  });
  it('geçici ifade durumun önüne geçer; göz kırpmada bir hap göz kalır', () => {
    expect(lookFor('music', 'pout', false)).toEqual({ pill: false, face: 'pout', cheek: true });
    expect(lookFor('idle', 'wink', false).pill).toBe(true);
    expect(lookFor('idle', 'dizzy', false).pill).toBe(false);
  });
});

describe('reactionFor', () => {
  it('bitti zıplar, hata sarsılır, diğer geçişler yaylanır', () => {
    expect(reactionFor('working', 'done')).toBe(PLAYS.jump);
    expect(reactionFor('working', 'error')).toBe(PLAYS.shake);
    expect(reactionFor('idle', 'working')).toBe(PLAYS.pop);
    expect(reactionFor('idle', 'idle')).toBeNull();
  });
  it('ilk çizimde yalnız bitti ve hata tepki verir', () => {
    expect(reactionFor(undefined, 'done')).toBe(PLAYS.jump);
    expect(reactionFor(undefined, 'error')).toBe(PLAYS.shake);
    expect(reactionFor(undefined, 'idle')).toBeNull();
  });
});

describe('tapReaction', () => {
  it('tek tık ezilir, hızlı üçüncü tık döner ve sayaç sıfırlanır', () => {
    let s = tapReaction([], 0);
    expect(s.play).toBe(PLAYS.squash);
    s = tapReaction(s.taps, 200);
    expect(s.play).toBe(PLAYS.squash);
    s = tapReaction(s.taps, 400);
    expect(s.play).toBe(PLAYS.spin);
    expect(s.taps).toEqual([]);
  });
  it('yavaş tıklar dönmeye yetmez', () => {
    let s = tapReaction([], 0);
    s = tapReaction(s.taps, TRIPLE_MS);
    s = tapReaction(s.taps, TRIPLE_MS * 2);
    expect(s.play).toBe(PLAYS.squash);
  });
});

describe('gaze', () => {
  it('merkezde düz bakar, uzakta en uca, yakında kısmen', () => {
    expect(gaze(0, 0)).toEqual({ x: 0, y: 0 });
    expect(gaze(1000, 0)).toEqual({ x: 1, y: 0 });
    const near = gaze(0, 110);
    expect(near.x).toBeCloseTo(0);
    expect(near.y).toBeCloseTo(0.5);
  });
});
