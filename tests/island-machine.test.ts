import { describe, expect, it } from 'vitest';
import { initial, leaveDelay, step, type IslandState } from '../src/lib/island-machine';

const at = (level: IslandState['level'], tab: IslandState['tab'] = 'ai'): IslandState => ({ level, tab });

describe('ada seviyeleri', () => {
  it('üzerine gelince mini AI açılır, ortaya tıklayınca ayrıntı', () => {
    let s = step(initial, { type: 'hover', tab: 'ai' });
    expect(s).toEqual(at(1, 'ai'));
    s = step(s, { type: 'open' });
    expect(s).toEqual(at(2, 'ai'));
    expect(step(s, { type: 'open' })).toEqual(s);
  });

  it('K11: sabit sekmeler minisiz, doğrudan tek ekran (seviye 2) açılır', () => {
    for (const tab of ['music', 'today', 'pc', 'shelf', 'zody', 'clock'] as const) {
      expect(step(initial, { type: 'hover', tab })).toEqual(at(2, tab));
      expect(step(initial, { type: 'tab', tab })).toEqual(at(2, tab));
      expect(step(initial, { type: 'toggle', tab })).toEqual(at(2, tab));
      expect(step(at(1, 'ai'), { type: 'tab', tab })).toEqual(at(2, tab));
      // Basınca büyüme ya da geri küçülme yok.
      expect(step(at(2, tab), { type: 'open' })).toEqual(at(2, tab));
      expect(step(at(2, tab), { type: 'shrink' })).toEqual(at(2, tab));
    }
  });

  it('açıkken üzerine gelmek bölümü değiştirmez', () => {
    expect(step(at(2, 'pc'), { type: 'hover', tab: 'music' })).toEqual(at(2, 'pc'));
  });

  it('AI sekmesi kendi seviyesini korur; sabit ekrandan ya da şeritten gelince mini açar', () => {
    expect(step(at(2, 'music'), { type: 'tab', tab: 'ai' })).toEqual(at(1, 'ai'));
    expect(step(at(2, 'ai'), { type: 'tab', tab: 'ai' })).toEqual(at(2, 'ai'));
    expect(step(at(1, 'ai'), { type: 'tab', tab: 'ai' })).toEqual(at(1, 'ai'));
    expect(step(initial, { type: 'tab', tab: 'ai' })).toEqual(at(1, 'ai'));
  });

  it('şeritteyken açma ve geri bir şey yapmaz', () => {
    expect(step(initial, { type: 'open' })).toEqual(initial);
    expect(step(initial, { type: 'shrink' })).toEqual(initial);
  });

  it('ayrıntıdaki geri miniye döner, minide bir şey yapmaz', () => {
    expect(step(at(2, 'ai'), { type: 'shrink' })).toEqual(at(1, 'ai'));
    expect(step(at(1, 'ai'), { type: 'shrink' })).toEqual(at(1, 'ai'));
  });

  it('fare uzaklaşınca veya dışarı tıklanınca kapanır; bölüm hatırlanır', () => {
    expect(step(at(2, 'pc'), { type: 'leave' })).toEqual(at(0, 'pc'));
    expect(step(at(2, 'music'), { type: 'outside' })).toEqual(at(0, 'music'));
  });

  it('kapalıyken gelen onay adayı açmaz (çentik bildirimi); açıkken AI bölümüne geçer', () => {
    expect(step(at(0, 'music'), { type: 'attention' })).toEqual(at(0, 'music'));
    expect(step(at(2, 'pc'), { type: 'attention' })).toEqual(at(2, 'ai'));
    expect(step(at(2, 'music'), { type: 'attention' })).toEqual(at(2, 'ai'));
  });

  it('kısayol kapalıyken mini açar, açıkken küçültür', () => {
    expect(step(initial, { type: 'toggle', tab: 'ai' })).toEqual(at(1, 'ai'));
    expect(step(at(2, 'ai'), { type: 'toggle', tab: 'music' })).toEqual(at(0));
  });

  it('gizlenince kapanır', () => {
    expect(step(at(2, 'ai'), { type: 'hide' })).toEqual(at(0));
  });

  it('ayrıntı ekranı uzaklaşmaya daha toleranslıdır', () => {
    expect(leaveDelay(1)).toBeLessThan(leaveDelay(2));
  });
});

describe('yazma sahnesi', () => {
  const w: IslandState = { level: 1, tab: 'ai', write: true };

  it('her yerden mini AI üstünde açılır', () => {
    expect(step(initial, { type: 'write' })).toEqual(w);
    expect(step(at(2, 'music'), { type: 'write' })).toEqual(w);
  });

  it('fare uzaklaşınca, geri dönüşte ya da yeni onayda yerinde kalır', () => {
    for (const input of [{ type: 'leave' }, { type: 'shrink' }, { type: 'attention' }, { type: 'hover', tab: 'music' }] as const) {
      expect(step(w, input)).toEqual(w);
    }
  });

  it('Esc ve ‹ mini AI ya döner; dışarı tıklama ve kısayol kapatır', () => {
    expect(step(w, { type: 'escape' })).toEqual(at(1));
    expect(step(w, { type: 'back' })).toEqual(at(1));
    expect(step(w, { type: 'outside' })).toEqual(at(0));
    expect(step(w, { type: 'write' })).toEqual(at(0));
    expect(step(w, { type: 'toggle', tab: 'ai' })).toEqual(at(0));
  });

  it('bölüm seçimi ve ayrıntı sahneden çıkar', () => {
    expect(step(w, { type: 'tab', tab: 'today' })).toEqual(at(2, 'today'));
    expect(step(w, { type: 'open' })).toEqual(at(2));
  });

  it('sahne dışında geri dönüş bir şey yapmaz', () => {
    expect(step(at(1), { type: 'back' })).toEqual(at(1));
  });
});
