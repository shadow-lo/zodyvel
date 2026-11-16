import { describe, expect, it } from 'vitest';
import {
  accountLabel,
  countText,
  HOT_PRIMARY,
  mailCards,
  findMail,
  splitAddress,
  syncedText,
  ago,
  brokenAccounts,
  featured,
  idleState,
  PICK_HOLD_MS,
  tiles,
  type AccountLine,
  type AgentLite,
  type HubInput,
  type SourceItem,
  type SourceState,
} from '../src/lib/sources';

const NOW = 1_800_000_000_000;

function src(id: 'mail' | 'github', urgency: number, badge: number, extra: Partial<SourceState> = {}): SourceState {
  return { ...idleState(id, 'ok'), urgency, badge, ...extra };
}

function agent(id: string, status: AgentLite['status'], project = 'Psrc', tool = 'claude'): AgentLite {
  return { id, tool, project, status };
}

function input(over: Partial<HubInput> = {}): HubInput {
  return { agents: [], waiting: [], sources: [], enabled: ['mail', 'github'], selected: null, pick: null, urgentAt: 0, now: NOW, ...over };
}

describe('featured', () => {
  it('follows the priority order', () => {
    const full = input({
      agents: [agent('a', 'running'), agent('b', 'waiting_approval', 'web', 'codex')],
      waiting: ['b'],
      sources: [src('github', 3, 1), src('mail', 2, 4)],
    });
    expect(featured(full)).toBe('s:b');
    expect(featured({ ...full, waiting: [] })).toBe('s:a');
    expect(featured({ ...full, waiting: [], agents: [] })).toBe('github');
    expect(featured({ ...full, waiting: [], agents: [], sources: [src('mail', 2, 4)] })).toBe('mail');
    expect(featured(input())).toBe('zody');
  });

  it('prefers the selected one among running sessions', () => {
    const two = input({ agents: [agent('a', 'running'), agent('b', 'running')] });
    expect(featured(two)).toBe('s:a');
    expect(featured({ ...two, selected: 'b' })).toBe('s:b');
  });

  it('ignores disabled sources, idle agents and sources that are not ok', () => {
    expect(featured(input({ agents: [agent('a', 'done')] }))).toBe('zody');
    expect(featured(input({ sources: [src('github', 3, 1, { status: 'error' })] }))).toBe('zody');
    expect(featured(input({ sources: [src('mail', 2, 0)] }))).toBe('zody');
    expect(featured(input({ sources: [src('mail', 2, 4)], enabled: ['github'] }))).toBe('zody');
  });

  it('keeps the user pick for two minutes or until a new urgent event', () => {
    const busy = input({ agents: [agent('a', 'running')], pick: { id: 'mail', at: NOW - 60_000 } });
    expect(featured(busy)).toBe('mail');
    expect(featured({ ...busy, now: NOW - 60_000 + PICK_HOLD_MS })).toBe('s:a');
    expect(featured({ ...busy, urgentAt: NOW - 1000 })).toBe('s:a');
    expect(featured({ ...busy, enabled: [] })).toBe('s:a');
    // Oturum seçimi; oturum kapandıysa seçim düşer.
    const picked = input({ agents: [agent('a', 'running'), agent('b', 'done')], pick: { id: 's:b', at: NOW - 1000 } });
    expect(featured(picked)).toBe('s:b');
    expect(featured({ ...picked, agents: [agent('a', 'running')] })).toBe('s:a');
  });
});

describe('tiles', () => {
  it('keeps Mail and GitHub first, then one tile per session', () => {
    const t = tiles({
      agents: [agent('a', 'running'), agent('b', 'waiting_input', 'web')],
      waiting: [],
      sources: [src('mail', 2, 3), src('github', 3, 1, { items: [{ id: '1', title: 'r', summary: 's', timeMs: 0, unread: true, tag: 'ci_failed', url: null, actions: [] }] })],
      enabled: ['github', 'mail'],
    });
    expect(t.map(x => x.id)).toEqual(['mail', 'github', 's:a', 's:b']);
    expect(t.map(x => x.count || x.note)).toEqual([3, 'CI ✕', 'çalışıyor', 'bekliyor']);
    expect(t.map(x => x.label)).toEqual(['Mail', 'GitHub', 'Psrc', 'web']);
    expect(t.find(x => x.id === 'github')?.tone).toBe('alert');
  });

  it('numbers sessions of the same project and folds the overflow into +N', () => {
    const agents = ['a', 'b', 'c', 'd', 'e', 'f'].map(id => agent(id, 'running'));
    const t = tiles({ agents, waiting: ['f'], sources: [], enabled: ['mail'] });
    expect(t.map(x => x.id)).toEqual(['mail', 's:a', 's:b', 's:c', 's:d', 'more']);
    expect(t.map(x => x.label)).toEqual(['Mail', 'Psrc', 'Psrc 2', 'Psrc 3', 'Psrc 4', '+2 oturum']);
    expect(t[5]?.tone).toBe('attention');
    expect(tiles({ agents: agents.slice(0, 2), waiting: [], sources: [], enabled: [] }).map(x => x.kind)).toEqual(['claude', 'claude']);
  });

  it('describes sources that are not connected and waiting sessions', () => {
    const t = tiles({ agents: [agent('a', 'waiting_approval', 'x', 'codex')], waiting: ['a'], sources: [idleState('mail', 'needs_setup')], enabled: ['mail', 'github'] });
    expect(t.map(x => [x.count, x.note, x.tone])).toEqual([
      [0, 'bağla', 'off'],
      [0, 'kapalı', 'off'],
      [0, 'onay', 'attention'],
    ]);
    expect(t[2]?.kind).toBe('codex');
  });
});

describe('ago', () => {
  it('reads short Turkish times', () => {
    expect([0, 9, 60, 23 * 60, 25 * 60, 72 * 60].map(m => ago(NOW - m * 60_000, NOW))).toEqual(['şimdi', '9 dk', '1 sa', '23 sa', 'dün', '3 g']);
  });
});

describe('mail accounts', () => {
  const acc = (slot: number, address: string, status: AccountLine['status'] = 'ok'): AccountLine => ({ slot, address, status, error: status === 'error' ? 'Giriş reddedildi.' : null, badge: 0 });
  const row = (account: string): SourceItem => ({ id: '0:1', title: 'A', summary: 'B', timeMs: 0, unread: true, tag: null, url: null, actions: [], account });

  it('labels rows only when several accounts are linked', () => {
    const one = src('mail', 2, 1, { accounts: [acc(0, 'ad@gmail.com')] });
    expect(accountLabel(one, row('ad@gmail.com'))).toBeNull();
    const two = src('mail', 2, 1, { accounts: [acc(0, 'ad@gmail.com'), acc(1, 'is@gmail.com')] });
    expect(accountLabel(two, row('is@gmail.com'))).toBe('is');
    expect(accountLabel(two, { ...row('x'), account: null })).toBeNull();
    // Aynı ön ad iki hesapta: tam adres.
    const clash = src('mail', 2, 1, { accounts: [acc(0, 'ad@gmail.com'), acc(1, 'ad@okul.edu')] });
    expect(accountLabel(clash, row('ad@okul.edu'))).toBe('ad@okul.edu');
  });

  it('lists broken accounts while others work', () => {
    const s = src('mail', 2, 1, { accounts: [acc(0, 'ad@gmail.com'), acc(1, 'is@gmail.com', 'error')] });
    expect(brokenAccounts(s).map(a => a.address)).toEqual(['is@gmail.com']);
    expect(brokenAccounts({ ...s, status: 'error' })).toEqual([]);
  });
});

describe('Mail sekmesi (Faz 5b)', () => {
  const item = (id: string, account: string) => ({ id, title: 'A', summary: 'B', timeMs: 0, unread: true, tag: null, url: null, actions: [], account });
  const base = { id: 'mail' as const, status: 'ok' as const, error: null, account: null, badge: 0, urgency: 0, repos: [] };
  it('kategoriler sırasıyla kart olur, en çok üç ileti', () => {
    const cats = [
      { key: 'primary' as const, unread: 12, items: [item('0:1', 'a'), item('0:2', 'a'), item('0:3', 'a'), item('0:4', 'a')] },
      { key: 'updates' as const, unread: 0, items: [] },
    ];
    const acc = { slot: 0, address: 'a@gmail.com', status: 'ok' as const, error: null, badge: 12, cats };
    const cards = mailCards({ ...base, items: [] }, acc);
    expect(cards.map(c => [c.title, c.unread, c.items.length])).toEqual([
      ['Birincil', 12, 3],
      ['Güncellemeler', 0, 0],
    ]);
    expect(HOT_PRIMARY).toBeLessThanOrEqual(12);
    expect(cards[0]?.all.map(i => i.id)).toEqual(['0:1', '0:2', '0:3', '0:4']);
  });
  it('mini karttaki mail Mail sekmesinde hesabı ve kategorisiyle bulunur', () => {
    const a = { slot: 0, address: 'a@gmail.com', status: 'ok' as const, error: null, badge: 1, cats: [{ key: 'primary' as const, unread: 1, items: [item('0:1', 'a@gmail.com')] }] };
    const b = { slot: 2, address: 'b@gmail.com', status: 'ok' as const, error: null, badge: 1, cats: [{ key: 'primary' as const, unread: 0, items: [] }, { key: 'updates' as const, unread: 5, items: [item('2:9', 'b@gmail.com')] }] };
    const state = { ...base, items: [], accounts: [a, b] };
    expect(findMail(state, '2:9')).toMatchObject({ slot: 2, card: 'updates', item: { id: '2:9' } });
    expect(findMail(state, '0:1')).toMatchObject({ slot: 0, card: 'primary' });
    expect(findMail(state, '9:9')).toBeNull();
  });
  it('Gmail süzgeci yoksa tek "Gelen kutusu" kartı, yalnız o hesabın satırları', () => {
    const acc = { slot: 1, address: 'b@x.com', status: 'ok' as const, error: null, badge: 2 };
    const cards = mailCards({ ...base, items: [item('1:1', 'b@x.com'), item('0:1', 'a@gmail.com')] }, acc);
    expect(cards).toHaveLength(1);
    expect(cards[0]).toMatchObject({ key: 'inbox', title: 'Gelen kutusu', unread: 2 });
    expect(cards[0]?.items.map(i => i.id)).toEqual(['1:1']);
    expect(mailCards({ ...base, items: [] }, null)).toEqual([]);
  });
  it('sayı, adres ve yenileme metni', () => {
    expect(countText(128)).toBe('99+');
    expect(countText(7)).toBe('7');
    expect(splitAddress('kisisel@gmail.com')).toEqual(['kisisel', '@gmail']);
    expect(syncedText(1_000_000 - 90_000, 1_000_000)).toBe('1 dk önce yenilendi');
    expect(syncedText(1_000_000, 1_000_000)).toBe('şimdi yenilendi');
    expect(syncedText(undefined, 1)).toBe('');
  });
});
