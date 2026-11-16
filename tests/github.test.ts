import { describe, expect, it } from 'vitest';
import { buttons, firstTab, headline, inTab, ordered, tokenProblem } from '../src/lib/github';
import { idleState, type SourceItem, type SourceState } from '../src/lib/sources';

const item = (id: string, tag: SourceItem['tag'], timeMs: number, actions: SourceItem['actions'] = ['open']): SourceItem => ({
  id,
  title: id,
  summary: '',
  timeMs,
  unread: tag !== null,
  tag,
  url: 'https://github.com/a/b',
  actions,
});

const state = (items: SourceItem[]): SourceState => ({ ...idleState('github', 'ok'), items });

describe('github görünümü', () => {
  it('kırık CI önde, sonra inceleme, bildirim ve düzelen CI', () => {
    const rows = [item('n', 'notif', 9), item('ok', null, 10), item('rv', 'review', 1), item('ci', 'ci_failed', 0), item('n2', 'notif', 11)];
    expect(ordered(rows).map(r => r.id)).toEqual(['ci', 'rv', 'n2', 'n', 'ok']);
    expect(inTab(rows, 'ci').map(r => r.id)).toEqual(['ci', 'ok']);
    expect(inTab(rows, 'notif').map(r => r.id)).toEqual(['n2', 'n']);
    expect(firstTab(rows)).toBe('ci');
    expect(firstTab(rows, 2), 'kırık CI yerel değişiklikten önce').toBe('ci');
    expect(firstTab([item('rv', 'review', 1)])).toBe('review');
    expect(firstTab([item('rv', 'review', 1)], 1)).toBe('repos');
    expect(firstTab([])).toBe('repos');
    expect(inTab(rows, 'repos')).toEqual([]);
  });

  it('başlık rozeti en acil türü söyler', () => {
    expect(headline(state([item('a', 'ci_failed', 0), item('b', 'review', 0)]))).toEqual({ text: 'CI kırık', tone: 'red' });
    expect(headline(state([item('a', 'ci_failed', 0), item('b', 'ci_failed', 0)]))?.text).toBe('2 CI kırık');
    expect(headline(state([item('b', 'review', 0), item('c', 'notif', 0)]))).toEqual({ text: '1 inceleme', tone: 'amber' });
    expect(headline(state([]))).toBeNull();
    expect(headline({ ...state([item('a', 'ci_failed', 0)]), status: 'error' })).toBeNull();
  });

  it('düğmeler: onay isteyenler işaretli, önizleme yok', () => {
    const b = buttons(item('ci', 'ci_failed', 0, ['open', 'rerun', 'fix_with_claude', 'preview']));
    expect(b.map(x => x.label)).toEqual(['Günlüğü aç', 'Yeniden çalıştır', "Claude'a düzelttir"]);
    expect(b.map(x => x.confirm !== null)).toEqual([false, true, true]);
    expect(buttons(item('n', 'notif', 0, ['open', 'mark_read'])).map(x => x.label)).toEqual(['Aç', 'Okundu']);
  });

  it('token türü', () => {
    expect(tokenProblem('ghp_abc')).toBeNull();
    expect(tokenProblem('')).toBeNull();
    expect(tokenProblem('github_pat_11AA')).toContain('klasik');
    expect(tokenProblem('ghs_x')).toContain('uygulama');
    expect(tokenProblem('ghp_a b')).toContain('geçersiz');
  });
});
