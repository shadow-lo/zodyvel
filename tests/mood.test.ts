import { describe, expect, it } from 'vitest';
import { agentMood, liveAgent, moodFor, pickLead, QUIET_MS, SLEEP_MS, type MoodInput } from '../src/lib/mood';
import type { AgentSession } from '../src/lib/types';

const calm: MoodInput = { approvals: 0, errors: false, working: 0, musicPlaying: false, done: false, asleep: false };

describe('moodFor', () => {
  it('boştayken idle', () => {
    expect(moodFor(calm)).toBe('idle');
  });

  it('öncelik sırası: onay > hata > çalışıyor > müzik > bitti', () => {
    const all = { approvals: 1, errors: true, working: 2, musicPlaying: true, done: true, asleep: false };
    expect(moodFor(all)).toBe('attention');
    expect(moodFor({ ...all, approvals: 0 })).toBe('error');
    expect(moodFor({ ...all, approvals: 0, errors: false })).toBe('working');
    expect(moodFor({ ...all, approvals: 0, errors: false, working: 0 })).toBe('music');
    expect(moodFor({ ...calm, done: true })).toBe('done');
  });

  it('yalnız boşta ve bitti uykuya geçer', () => {
    expect(moodFor({ ...calm, asleep: true })).toBe('sleep');
    expect(moodFor({ ...calm, done: true, asleep: true })).toBe('sleep');
    expect(moodFor({ ...calm, musicPlaying: true, asleep: true })).toBe('music');
    expect(moodFor({ ...calm, working: 1, asleep: true })).toBe('working');
    expect(moodFor({ ...calm, approvals: 1, asleep: true })).toBe('attention');
  });
});

describe('pickLead', () => {
  const agents = [
    { id: 'a', status: 'done' as const },
    { id: 'b', status: 'running' as const },
    { id: 'c', status: 'waiting_approval' as const },
  ];

  it('seçilen hâlâ varsa onu, yoksa en önemli oturumu verir', () => {
    expect(pickLead(agents, 'a')?.id).toBe('a');
    expect(pickLead(agents, 'gitti')?.id).toBe('c');
    expect(pickLead(agents, null)?.id).toBe('c');
    expect(pickLead(agents.slice(0, 2), null)?.id).toBe('b');
    expect(pickLead([], null)).toBeNull();
  });
});

describe('liveAgent', () => {
  const base: AgentSession = {
    id: 'a', tool: 'claude', project: 'Psrc', status: 'running', step: 'Düzenliyor', startedAt: 0, lastSeen: 0, demo: false,
    steps: [{ id: 1, kind: 'edit', target: 'a.ts', label: '', at: 0, state: 'done', sub: false }],
    app: null, statusSince: 0, turnStartedAt: null, turnEndedAt: null, turnFiles: 0,
    changed: [], todos: [], lastReply: null, mode: null, folder: null, subagents: 0, notes: [], canNote: false, canTask: false,
  };

  it('olay gelmeyen "çalışıyor" 5 dk sonra beklemeye döner, 10 dk sonra uyur', () => {
    expect(liveAgent(base, QUIET_MS - 1).status).toBe('running');
    const quiet = liveAgent(base, QUIET_MS + 1);
    expect(quiet.status).toBe('waiting_input');
    expect(quiet.asleep).toBe(false);
    expect(liveAgent(base, SLEEP_MS + 1).asleep).toBe(true);
  });

  it('süren bir adım varsa (uzun komut) çalışıyor kalır', () => {
    const busy = { ...base, steps: [{ ...base.steps[0]!, kind: 'bash' as const, state: 'now' as const }] };
    expect(liveAgent(busy, SLEEP_MS * 3).status).toBe('running');
  });

  it('bekleyen ya da bitmiş oturum 10 dk sessizse gezegeni uyur; onay bekleyen uyumaz', () => {
    expect(liveAgent({ ...base, status: 'done' }, SLEEP_MS + 1).asleep).toBe(true);
    expect(liveAgent({ ...base, status: 'waiting_input' }, SLEEP_MS - 1).asleep).toBeUndefined();
    expect(liveAgent({ ...base, status: 'waiting_approval' }, SLEEP_MS * 3).asleep).toBeUndefined();
    expect(agentMood('done', true)).toBe('sleep');
    expect(agentMood('done')).toBe('done');
  });
});
