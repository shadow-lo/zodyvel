import { describe, expect, it } from 'vitest';
import { byUrgency, categoryOf, chipStatus, contextHot, contextPercent, loopOf, modeBadge, rowStatus, todoProgress } from '../src/lib/agent-view';
import type { AgentStep } from '../src/lib/types';

let id = 0;
const step = (kind: AgentStep['kind'], target: string, state: AgentStep['state']): AgentStep => ({ id: ++id, kind, target, label: '', at: 0, state, sub: false });

describe('categoryOf (Faz 7.1)', () => {
  it('sohbetin adı varsa klasör ayrı yazılır; ad zaten klasörse yazılmaz', () => {
    expect(categoryOf({ project: 'Naber', category: 'Ödev' })).toBe('Ödev');
    expect(categoryOf({ project: 'Ödev', category: 'Ödev' })).toBeNull();
    expect(categoryOf({ project: 'Ödev 2', category: 'Ödev' })).toBeNull();
    expect(categoryOf({ project: 'Naber' })).toBeNull();
  });
});

describe('todoProgress', () => {
  it('liste yoksa null', () => expect(todoProgress([])).toBeNull());
  it('biten ve süren işi sayar', () => {
    const p = todoProgress([
      { text: 'a', active: '', status: 'completed' },
      { text: 'b', active: 'B yapıyor', status: 'in_progress' },
      { text: 'c', active: '', status: 'pending' },
    ]);
    expect(p?.done).toBe(1);
    expect(p?.total).toBe(3);
    expect(p?.current?.text).toBe('b');
  });
});

describe('modeBadge', () => {
  it('olağan kipte rozet yok', () => {
    expect(modeBadge(null)).toBeNull();
    expect(modeBadge('default')).toBeNull();
  });
  it('tehlikeli kip kırmızı', () => expect(modeBadge('bypassPermissions')?.tone).toBe('danger'));
});

describe('loopOf', () => {
  it('aynı komut üç kez olmadıysa döngü', () => {
    const steps = [step('bash', 'pnpm test', 'failed'), step('bash', 'pnpm test', 'failed'), step('bash', 'pnpm test', 'failed')];
    expect(loopOf(steps)).toEqual({ target: 'pnpm test', count: 3, command: true });
  });
  it('süren son adım sayılmaz ama zinciri kesmez', () => {
    const steps = [step('bash', 'x', 'failed'), step('bash', 'x', 'failed'), step('bash', 'x', 'failed'), step('bash', 'x', 'now')];
    expect(loopOf(steps)?.count).toBe(3);
  });
  it('başarılı adım ya da farklı hedef zinciri keser', () => {
    expect(loopOf([step('bash', 'x', 'failed'), step('bash', 'x', 'done'), step('bash', 'x', 'failed'), step('bash', 'x', 'failed')])).toBeNull();
    expect(loopOf([step('bash', 'x', 'failed'), step('bash', 'y', 'failed'), step('bash', 'x', 'failed')])).toBeNull();
    expect(loopOf([step('read', 'a.ts', 'failed'), step('read', 'a.ts', 'failed'), step('read', 'a.ts', 'failed')])).toBeNull();
  });
});

describe('byUrgency', () => {
  it('önce onay, sonra hata, bekleyen, çalışan, biten; eşitse en yeni', () => {
    const list = byUrgency([
      { id: 'd', status: 'done' as const, lastSeen: 9 },
      { id: 'r1', status: 'running' as const, lastSeen: 1 },
      { id: 'r2', status: 'running' as const, lastSeen: 5 },
      { id: 'a', status: 'waiting_approval' as const, lastSeen: 0 },
    ]);
    expect(list.map(a => a.id)).toEqual(['a', 'r2', 'r1', 'd']);
  });
});

it('contextPercent', () => {
  expect(contextPercent(50_000, 200_000)).toBe(25);
  expect(contextPercent(1, 0)).toBe(0);
  expect(contextPercent(300_000, 200_000)).toBe(100);
});

it('contextHot', () => {
  expect(contextHot(79)).toBe(false);
  expect(contextHot(80)).toBe(true);
  expect(contextHot(100)).toBe(true);
});

it('chipStatus', () => {
  const todos = [{ text: 'a', active: '', status: 'completed' as const }, { text: 'b', active: '', status: 'pending' as const }];
  expect(chipStatus({ status: 'running', todos, statusSince: 0 })).toBe('1/2');
  expect(chipStatus({ status: 'running', todos: [], statusSince: 0 })).toBe('çalışıyor');
  expect(chipStatus({ status: 'done', todos: [], statusSince: 0 }, 120_000)).toBe('bitti · 2 dk');
  expect(chipStatus({ status: 'waiting_approval', todos: [], statusSince: 0 })).toBe('onay');
});

it('rowStatus', () => {
  const todos = [{ text: 'a', active: '', status: 'completed' as const }, { text: 'b', active: '', status: 'pending' as const }];
  expect(rowStatus({ status: 'running', todos, statusSince: 0 })).toBe('Çalışıyor · 1/2');
  expect(rowStatus({ status: 'running', todos: [], statusSince: 0 })).toBe('Çalışıyor');
  expect(rowStatus({ status: 'waiting_input', todos: [], statusSince: 0 }, 12 * 60_000)).toBe('Bekliyor · 12 dk');
  expect(rowStatus({ status: 'done', todos: [], statusSince: 0 }, 120_000)).toBe('Bitti · 2 dk');
  expect(rowStatus({ status: 'waiting_approval', todos: [], statusSince: 0 })).toBe('Onay bekliyor');
});
