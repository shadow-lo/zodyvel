import { describe, expect, it } from 'vitest';
import { baseName, clip, meterItem, meterItems, MINI_SIZE, MINI_TABS_H, miniFor, miniHeight, modeItem, musicItem, notchShown, notchView, notchWidth, NOTCH_MAX_W, NOTCH_MIN_W, noticeStays, peekable, targetKey, workingItem } from '../src/lib/live';
import type { Notice } from '../src/lib/notice';
import { defaultModes, startMode, timerAdd } from '../src/lib/today';
import { defaultSettings, type AgentSession, type AgentStatus, type AgentStep, type MeterState, type NowPlaying } from '../src/lib/types';

const MIN = 60_000;
const prefs = defaultSettings.notch;

const step = (id: number, over: Partial<AgentStep> = {}): AgentStep => ({ id, kind: 'read', target: 'src/a.ts', label: '', at: 100, state: 'done', sub: false, ...over });

const session = (id: string, status: AgentStatus, over: Partial<AgentSession> = {}): AgentSession => ({
  id, tool: 'claude', project: 'Gezgin', status, step: '', startedAt: 0, lastSeen: 0, demo: false, steps: [], app: null,
  statusSince: 0, turnStartedAt: 0, turnEndedAt: null, turnFiles: 0,
  changed: [], todos: [], lastReply: null, mode: null, folder: null, subagents: 0, notes: [], canNote: false, canTask: false,
  ...over,
});

const song: NowPlaying = {
  title: 'Gece Yolu', artist: 'Deniz Aras', app: 'Spotify', playing: true, positionMs: 0, durationMs: 1, updatedAt: 0, artId: null,
  canToggle: true, canNext: true, canPrev: true, canSeek: false, demo: false,
};

const meter = (o: Partial<MeterState> = {}): MeterState => ({ id: 1, name: 'Zodymetre', color: '', sound: true, minutes: 25, endsAt: null, leftMs: 25 * MIN, day: '', rounds: 0, doneMs: 0, ...o });
const running = (left: number, minutes = 25, o: Partial<MeterState> = {}): MeterState => meter({ minutes, endsAt: 1_000 + left, leftMs: left, ...o });

const notice: Notice = { key: 'a', mood: 'attention', title: 'Onay bekleniyor', detail: 'Claude · Psrc', tab: 'ai', more: 0 };

describe('canlı çentik: süren etkinlikler', () => {
  it('Zodymetre: çalışırken halka ve kalan süre; duraklatılmış da görünür', () => {
    const t = meterItem(running(24 * MIN + 58_000, 25, { id: 7, name: 'Okuma', color: '#2ec4b6' }), 1_000);
    expect(t).toMatchObject({ kind: 'timer', key: 'timer:7', title: 'Okuma', time: '24:58', running: true, tab: 'today', meterId: 7, color: '#2ec4b6' });
    expect(t?.ratio).toBeCloseTo((24 * MIN + 58_000) / (25 * MIN));
    expect(meterItem(meter({ leftMs: 10 * MIN }), 0)).toMatchObject({ running: false, detail: 'duraklatıldı' });
  });

  it('başlatılmamış ya da dolmuş Zodymetre süren etkinlik değildir', () => {
    expect(meterItem(meter(), 0)).toBeNull();
    expect(meterItem(meter({ leftMs: 0 }), 0)).toBeNull();
  });

  it('en çok üç Zodymetre: en yakın biten önde, duraklatılmış sonda, başlamamış yok', () => {
    const list = [meter({ id: 1, leftMs: 3 * MIN }), running(9 * MIN, 25, { id: 2 }), running(2 * MIN, 25, { id: 3 }), meter({ id: 4 })];
    expect(meterItems(list, 1_000).map(i => i.meterId)).toEqual([3, 2, 1]);
  });

  it('mod: modun renginde, süreliyse kalan, süresizse geçen süre; odakta aşama', () => {
    const [work, , , game] = defaultModes();
    const m = modeItem(startMode(work!, undefined, 0, true), work!, 10 * MIN);
    expect(m).toMatchObject({ kind: 'mode', key: 'mode:work', title: 'Çalışma', detail: 'Odak 1/4', time: '40:00', tab: 'modes', color: '#5b9cff', icon: 'brief', endless: false });
    const g = modeItem(startMode(game!, undefined, 0, true), game!, 12 * MIN);
    expect(g).toMatchObject({ detail: 'modu', time: '12 dk', endless: true, over: 'Mod · süresiz' });
    expect(g.ratio).toBeUndefined();
  });

  it('AI çalışıyor: sürmekte olan adım fiil + kısa ad, kaç dakikadır', () => {
    const a = session('s1', 'running', {
      lastSeen: 5,
      turnStartedAt: 0,
      steps: [step(1, { kind: 'prompt', at: 0 }), step(2), step(3, { kind: 'edit', target: 'src/lib/auth.ts', state: 'now' })],
    });
    const w = workingItem([a], 4 * MIN + 10_000);
    expect(w).toMatchObject({ kind: 'working', title: 'Düzenliyor', detail: 'auth.ts', mono: true, time: '4 dk', session: 's1', tab: 'ai', over: 'Gezgin · Claude' });
    expect(w?.steps).toEqual(['done', 'now']);
    expect(w?.stepNo).toBe(2);
    // Faz 7.3: çentik hapında tek kısa satır; dosya adı yalnız ara boyda.
    expect(w?.short).toBe('Claude · Düzenliyor');
  });

  it('kısa hap: uzun şarkı adı 26 karaktere iner, Türkçe harfler bölünmez', () => {
    const long = { ...song, title: 'Çok Uzun Bir Şarkı Adı Gerçekten Çok Uzun Şöyle' };
    const m = musicItem(long)!;
    expect(Array.from(m.short!).length).toBe(26);
    expect(m.short!.endsWith('…')).toBe(true);
    expect(m.title).toBe(long.title);
    expect(musicItem(song)?.short).toBe('Gece Yolu');
    expect(clip('ığüşöçİĞÜŞÖÇ', 5)).toBe('ığüş…');
    expect(clip('  kısa  ', 26)).toBe('kısa');
  });

  it('birden çok çalışan oturum: en son hareket eden önde, sayısı üst satırda; örnekler sayılmaz', () => {
    const w = workingItem([session('a', 'running', { lastSeen: 1 }), session('b', 'running', { lastSeen: 9 }), session('d', 'running', { lastSeen: 99, demo: true })], 0);
    expect(w?.session).toBe('b');
    expect(w?.over).toContain('+1 oturum');
    expect(w?.time).toBe('şimdi');
    expect(workingItem([session('a', 'waiting_input')], 0)).toBeNull();
  });

  it('müzik yalnız çalarken', () => {
    expect(musicItem(song)).toMatchObject({ title: 'Gece Yolu', detail: 'Deniz Aras', tab: 'music' });
    expect(musicItem({ ...song, playing: false })).toBeNull();
    expect(musicItem(null)).toBeNull();
  });

  it('kısa ad: yol → dosya adı, komut olduğu gibi', () => {
    expect(baseName('src/lib/auth.ts')).toBe('auth.ts');
    expect(baseName('C:\\a\\b.rs')).toBe('b.rs');
    expect(baseName('npm test')).toBe('npm test');
  });
});

describe('canlı çentik: öncelik', () => {
  const agents = [session('s', 'running')];
  const meters = [running(10 * MIN)];

  it('olay geniş hapta; süren etkinlikler ve müzik solda baloncuk', () => {
    const v = notchView({ notice, prefs, agents, meters, mode: null, music: song, now: 1_000 });
    expect(v.notice).toBe(notice);
    expect(v.lead).toBeNull();
    expect(v.bubbles.map(i => i.kind)).toEqual(['timer', 'working', 'music']);
  });

  it('AI çalışırken müzik çalıyorsa: solda AI baloncuğu, sağda geniş müzik', () => {
    const v = notchView({ notice: null, prefs, agents, meters: [], mode: null, music: song, now: 0 });
    expect(v.lead?.kind).toBe('music');
    expect(v.bubbles.map(i => i.kind)).toEqual(['working']);
  });

  it('müzik yoksa Zodymetre › mod › AI çalışıyor', () => {
    const v = notchView({ notice: null, prefs, agents, meters, mode: null, music: null, now: 1_000 });
    expect(v.lead?.kind).toBe('timer');
    expect(v.bubbles.map(i => i.kind)).toEqual(['working']);
    const work = defaultModes()[0]!;
    const mode = { run: startMode(work, undefined, 0, true), def: work };
    expect(notchView({ notice: null, prefs, agents, meters: [], mode, music: null, now: 0 }).lead?.kind).toBe('mode');
    const all = notchView({ notice: null, prefs, agents, meters, mode, music: null, now: 1_000 });
    expect([all.lead?.kind, ...all.bubbles.map(i => i.kind)]).toEqual(['timer', 'mode', 'working']);
    expect(notchView({ notice: null, prefs: { ...prefs, mode: false }, agents: [], meters: [], mode, music: null, now: 0 }).lead).toBeNull();
  });

  it('"yalnız olunca" kipinde süren etkinlik gösterilmez', () => {
    const off = { ...prefs, live: false };
    expect(notchShown(notchView({ notice: null, prefs: off, agents, meters, mode: null, music: song, now: 0 }))).toBe(false);
    expect(notchView({ notice, prefs: off, agents, meters, mode: null, music: song, now: 0 }).bubbles).toEqual([]);
  });

  it('kapatılan etkinlik türü çentikte yer almaz', () => {
    const v = notchView({ notice: null, prefs: { ...prefs, timer: false, music: false }, agents, meters, mode: null, music: song, now: 0 });
    expect(v.lead?.kind).toBe('working');
    expect(v.bubbles).toEqual([]);
  });

  it('ara boy: süren etkinlik ve bilgi olayı genişler; onay, Bitiş ve boştaki Zody doğrudan açılır', () => {
    const item = workingItem(agents, 0)!;
    const done: Notice = { ...notice, mood: 'done', title: 'Bitti' };
    expect(peekable(item, null)).toBe(true);
    expect(peekable('notice', done)).toBe(true);
    expect(peekable('notice', notice)).toBe(false);
    expect(peekable('notice', { ...done, finish: { kind: 'meter', minutes: 25, meterId: 1, name: 'Zodymetre', color: '', rounds: 1, totalMs: 25 * MIN } })).toBe(false);
    expect(peekable('idle', null)).toBe(false);
    expect(targetKey(item)).toBe(item.key);
  });

  it('her zaman açık ada: hiçbir şey yokken yalnız Zody gözleri; kapalıyken siyah şerit', () => {
    expect(notchView({ notice: null, prefs, agents: [], meters: [], mode: null, music: null, now: 0 }).idle).toBe(true);
    const off = notchView({ notice: null, prefs: { ...prefs, live: false }, agents: [], meters: [], mode: null, music: null, now: 0 });
    expect(off.idle).toBe(false);
    expect(notchShown(off)).toBe(false);
    expect(notchView({ notice: null, prefs, agents, meters: [], mode: null, music: null, now: 0 }).idle).toBe(false);
  });

  it('tazeliği geçen olay: onay ve Bitiş kalır, bitti/hata kaybolur', () => {
    expect(noticeStays(notice)).toBe(true);
    expect(noticeStays({ ...notice, mood: 'done' })).toBe(false);
    expect(noticeStays({ ...notice, mood: 'done', finish: { kind: 'meter', minutes: 5, meterId: 1, name: 'Zodymetre', color: '', rounds: 1, totalMs: 5 * MIN } })).toBe(true);
  });

  it('ölçülen genişlik aralıkta tutulur', () => {
    expect(notchWidth(10)).toBe(NOTCH_MIN_W);
    expect(notchWidth(5000)).toBe(NOTCH_MAX_W);
    expect(notchWidth(301.4)).toBe(301);
  });
});

describe('Zodymetre: süre ekleme (today.rs Op::TimerAdd ile aynı)', () => {
  it('çalışırken bitiş ileri, duraklatılmışken kalan artar, dolmuşsa eklenen süreyle başlar', () => {
    expect(timerAdd(running(MIN), 5, 0)).toMatchObject({ minutes: 30, endsAt: 1_000 + 6 * MIN });
    expect(timerAdd({ minutes: 25, endsAt: null, leftMs: MIN }, 5, 0)).toMatchObject({ endsAt: null, leftMs: 6 * MIN });
    expect(timerAdd({ minutes: 25, endsAt: null, leftMs: 0, focus: null }, 5, 100)).toEqual({ minutes: 5, endsAt: 100 + 5 * MIN, leftMs: 5 * MIN, focus: null });
    expect(timerAdd({ minutes: 180, endsAt: 0, leftMs: 0, focus: null }, 5, 0).minutes).toBe(180);
  });
});

describe('mini pencereler (Faz 9)', () => {
  const mini = { ...prefs, hover: 'mini' as const };
  const work = workingItem([session('claude:a', 'running')], 0)!;
  const tune = musicItem(song)!;
  const clock = meterItem(running(10 * MIN), 1_000)!;
  const done: Notice = { key: 'd', mood: 'done', title: 'Bitti', detail: '', tab: 'ai', more: 0, open: { session: 'claude:a', section: null } };

  it('yalnız "Mini pencere" kipinde ve türün anahtarı açıksa açılır', () => {
    expect(miniFor(work, prefs, null)).toBeNull();
    expect([miniFor(work, mini, null), miniFor(tune, mini, null), miniFor(clock, mini, null)]).toEqual(['ai', 'music', 'timer']);
    expect(miniFor(tune, { ...mini, miniMusic: false }, null)).toBeNull();
    expect(miniFor(modeItem(startMode(defaultModes()[0]!, 25, 0, false), defaultModes()[0]!, 0), mini, null)).toBeNull();
  });

  it('bitti/hata bildirimi Mini AI olur; onay, Bitiş ve boştaki Zody olmaz', () => {
    expect(miniFor('notice', mini, done)).toBe('ai');
    expect(miniFor('notice', mini, { ...done, mood: 'error' })).toBe('ai');
    expect(miniFor('notice', mini, notice)).toBeNull();
    expect(miniFor('notice', mini, { ...done, open: undefined })).toBeNull();
    expect(miniFor('idle', mini, null)).toBeNull();
  });

  it('boylar eskizdeki gibi; çoklu Zodymetrede sekme satırı eklenir', () => {
    expect(MINI_SIZE).toEqual({ ai: { w: 540, h: 140 }, music: { w: 420, h: 112 }, timer: { w: 420, h: 128 } });
    expect(miniHeight('timer', 1)).toBe(128);
    expect(miniHeight('timer', 3)).toBe(128 + MINI_TABS_H);
    expect(miniHeight('ai', 3)).toBe(140);
  });

  it('Mini Oynatıcı açıkken duraklatılan şarkı çentikte kalır', () => {
    const paused = { ...song, playing: false };
    expect(musicItem(paused)).toBeNull();
    expect(musicItem(paused, true)?.key).toBe('music');
    expect(notchView({ notice: null, prefs, agents: [], meters: [], mode: null, music: paused, now: 0, hold: 'music' }).lead?.key).toBe('music');
  });
});
