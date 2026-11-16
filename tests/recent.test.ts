import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { demoAgents, demoApproval } from '../src/lib/demo';
import { initial, step, type Tab } from '../src/lib/island-machine';
import { musicStarted, openTab, recentTab, savedTab, saveTab } from '../src/lib/recent';
import { applyMusic, context, data, timerFinished } from '../src/lib/store.svelte';
import { defaultSettings, type NowPlaying } from '../src/lib/types';

describe('recentTab', () => {
  it('picks the visible tab with the newest activity', () => {
    expect(recentTab({ ai: 100, music: 300, today: 200 }, ['ai', 'music', 'today'], 'ai')).toBe('music');
    expect(recentTab({ ai: 500, music: 300 }, ['ai', 'music'], 'music')).toBe('ai');
  });

  it('ignores hidden tabs and falls back when nothing happened', () => {
    expect(recentTab({ music: 900, today: 10 }, ['ai', 'today'], 'ai')).toBe('today');
    expect(recentTab({}, ['ai', 'music'], 'music')).toBe('music');
    expect(recentTab({ ai: 0 }, ['ai', 'pc'], 'pc')).toBe('pc');
  });
});

describe('musicStarted', () => {
  const s: NowPlaying = { title: 'A', artist: 'B', app: 'x', playing: true, positionMs: 0, durationMs: 1, updatedAt: 0, artId: null, canToggle: true, canNext: true, canPrev: true, canSeek: true, demo: false };
  it('counts start and song change, not timeline refreshes', () => {
    expect(musicStarted(null, s)).toBe(true);
    expect(musicStarted({ ...s, playing: false }, s)).toBe(true);
    expect(musicStarted(s, { ...s, positionMs: 5000, updatedAt: 99 })).toBe(false);
    expect(musicStarted(s, { ...s, title: 'C' })).toBe(true);
    expect(musicStarted(s, { ...s, playing: false })).toBe(false);
  });
});

describe('openTab wiring (setting → store → island)', () => {
  const song: NowPlaying = { title: 'A', artist: 'B', app: 'Spotify', playing: true, positionMs: 0, durationMs: 200_000, updatedAt: 0, artId: null, canToggle: true, canNext: true, canPrev: true, canSeek: true, demo: false };
  const agent = (statusSince: number) => ({ ...demoAgents()[0]!, id: 'x', status: 'done' as const, statusSince, turnEndedAt: statusSince, asleep: false });
  let lastTab: Tab = 'ai';
  const hoverOpen = () => step(initial, { type: 'hover', tab: openTab({ on: data.settings.features.recentTab, last: data.settings.island.hover === 'last' ? lastTab : null, ctx: context(), activity: data.activity, sourcesAt: 0 }) }).tab;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_800_000_000_000);
    data.settings = structuredClone(defaultSettings);
    data.approvals = [];
    data.music = null;
    data.activity = { music: 0, today: 0 };
    data.timerDone = null;
    data.eventDue = null;
  });
  afterEach(() => vi.useRealTimers());

  it('opens on the newest activity only when the switch is on', () => {
    // Şarkı 5 dk önce başladı, Windows zaman çizelgesi her saniye yenileniyor; Claude 1 dk önce bitti.
    vi.setSystemTime(Date.now() - 5 * 60_000);
    applyMusic({ now: { ...song, updatedAt: Date.now() }, art: null });
    vi.setSystemTime(Date.now() + 5 * 60_000);
    applyMusic({ now: { ...song, positionMs: 300_000, updatedAt: Date.now() }, art: null });
    data.agents = [agent(Date.now() - 60_000)];
    expect(hoverOpen()).toBe('music'); // kapalı: müzik çalıyorsa müzik
    data.settings.features.recentTab = true;
    expect(hoverOpen()).toBe('ai');
    // Yeni şarkı başladı: artık müzik daha yeni.
    applyMusic({ now: { ...song, title: 'C', updatedAt: Date.now() }, art: null });
    expect(hoverOpen()).toBe('music');
  });

  it('remembers a finished timer after its notice is gone and skips hidden tabs', () => {
    data.settings.features.recentTab = true;
    data.agents = [agent(Date.now() - 60_000)];
    timerFinished(25);
    data.timerDone = null;
    expect(hoverOpen()).toBe('today');
    data.settings.modules.today = false;
    expect(hoverOpen()).toBe('ai');
  });

  it('opens on the last tab left open when "Son bıraktığım" is chosen', () => {
    data.agents = [agent(Date.now() - 60_000)];
    // Kullanıcı adayı açtı, Bugün'e geçti, fare ayrıldı (ada kapandı): bölüm durumda kalır.
    const s = step(step(step(initial, { type: 'hover', tab: 'ai' }), { type: 'tab', tab: 'today' }), { type: 'leave' });
    expect(s).toEqual({ level: 0, tab: 'today' });
    lastTab = s.tab;
    expect(hoverOpen()).toBe('ai');
    data.settings.island.hover = 'last';
    expect(hoverOpen()).toBe('today');
    // Bölüm gizlendiyse her zamanki kural.
    data.settings.modules.today = false;
    expect(hoverOpen()).toBe('ai');
    data.settings.modules.today = true;
    data.approvals = [demoApproval(agent(0))];
    expect(hoverOpen()).toBe('ai');
    lastTab = 'ai';
  });

  it('Saat düzeninde Saat ana ekranı açılır (onay bekliyorsa AI)', () => {
    data.settings.features.recentTab = true;
    data.settings.island.hover = 'auto';
    data.settings.appearance.layout = 'clock';
    data.approvals = [];
    expect(context().tabs[0]).toBe('clock');
    expect(hoverOpen()).toBe('clock');
    data.approvals = [demoApproval(agent(0))];
    expect(hoverOpen()).toBe('ai');
    data.approvals = [];
    data.settings.appearance.layout = 'zody';
    expect(context().tabs).not.toContain('clock');
  });

  it('remembers the last tab across restarts', () => {
    const store = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v) });
    expect(savedTab()).toBe(null);
    saveTab('today');
    expect(savedTab()).toBe('today');
    store.set('zodyvel.island.lastTab', 'bogus');
    expect(savedTab()).toBe(null);
    vi.unstubAllGlobals();
  });

  it('always opens AI while an approval waits', () => {
    data.settings.features.recentTab = true;
    data.agents = [agent(Date.now() - 60_000)];
    timerFinished(25);
    data.approvals = [demoApproval(agent(0))];
    expect(hoverOpen()).toBe('ai');
  });
});
