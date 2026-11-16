import { describe, expect, it } from 'vitest';
import { replyFor, rulesBrain, think, type World } from '../src/lib/chat';
import { parse } from '../src/lib/commands';

const NOW = new Date(2026, 9, 22, 9, 0);
const world = (over: Partial<World> = {}): World => ({
  now: NOW,
  agents: [],
  approvals: [],
  music: null,
  today: {
    version: 2,
    events: [],
    todos: [],
    meters: [{ id: 1, name: 'Zodymetre', color: '', sound: true, minutes: 25, endsAt: null, leftMs: 25 * 60_000, day: '', rounds: 0, doneMs: 0 }],
    modes: [],
    mode: null,
    reopen: [],
    nextId: 1,
  },
  battery: null,
  modules: { agents: true, music: true, today: true, system: true },
  ...over,
});
const music = (playing: boolean) => ({ title: 'Parça', artist: 'Sanatçı', app: 'Spotify', playing, positionMs: 0, durationMs: 1, updatedAt: 0, artId: null, canToggle: true, canNext: true, canPrev: true, canSeek: true, demo: true });
const r = (text: string, w = world()) => replyFor(parse(text, NOW), w);

describe('kural beyni', () => {
  it('zamanlayıcı kurar ve başlatır', () => {
    expect(r('25 dakika zamanlayıcı')?.steps).toEqual([{ action: 'today.timer_set', args: { minutes: 25 } }, { action: 'today.timer_start' }]);
  });
  it('takvim eklemesi onay cümlesiyle gelir (eylem kaydı kart ister)', () => {
    const reply = r("yarın 10'da toplantı ekle");
    expect(reply?.confirmText).toBe('Yarın 10:00 · Toplantı. Ekleyeyim mi?');
    expect(reply?.steps?.[0]).toEqual({ action: 'today.event_add', args: { title: 'Toplantı', date: '2026-10-23', time: '10:00' } });
  });
  it('"durdur" çalan müziği duraklatır, müzik yoksa zamanlayıcıyı', () => {
    expect(r('durdur', world({ music: music(true) }))?.steps).toEqual([{ action: 'music.control', args: { action: 'toggle' } }]);
    const w = world();
    w.today!.meters[0]!.endsAt = NOW.getTime() + 60_000;
    expect(r('durdur', w)?.steps).toEqual([{ action: 'today.timer_pause' }]);
    expect(r('müziği durdur', world({ music: music(false) }))?.steps).toBeUndefined();
  });
  it('ses düzeyi okunan değere göre', () => {
    expect(r('sesi kıs', world({ volume: { level: 40, muted: false, device: null } }))?.steps).toEqual([{ action: 'controls.volume', args: { level: 30 } }]);
  });
  it('kapalı modül söylenir, eylem yok', () => {
    const reply = r('25 dakika zamanlayıcı', world({ modules: { agents: true, music: true, today: false, system: true } }));
    expect(reply?.steps).toBeUndefined();
    expect(reply?.tone).toBe('warn');
  });
  it('"evet" hiçbir eylem çalıştırmaz', () => {
    expect(r('evet')?.steps).toBeUndefined();
  });
  it('anlaşılmayan: zincir "anlamadım" ve öneriler döner', async () => {
    const reply = await think([rulesBrain], { text: 'bana bir şiir yaz', via: 'text', world: world() });
    expect(reply.mood).toBe('confused');
    expect(reply.chips?.length).toBeGreaterThan(0);
  });
  it('sonraki beyin yalnız kurallar anlamazsa sorulur', async () => {
    const model = { id: 'm', label: 'Model', reply: () => ({ text: 'model cevabı' }) };
    expect((await think([rulesBrain, model], { text: 'bana bir şiir yaz', via: 'text', world: world() })).text).toBe('model cevabı');
    expect((await think([rulesBrain, model], { text: 'saat kaç', via: 'text', world: world() })).text).toBe('Saat 09:00.');
  });
});
