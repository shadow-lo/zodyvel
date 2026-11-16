import { describe, expect, it } from 'vitest';
import { noticeFor } from '../src/lib/notice';
import {
  cycleBar,
  dayLabel,
  defaultModes,
  finishMeter,
  focusLabel,
  focusStepText,
  isoDate,
  leadMeter,
  meterActive,
  minutesText,
  modeActions,
  modeLeft,
  modeSummary,
  nextEvent,
  nextFocus,
  nextPhaseText,
  pauseMode,
  phaseLeft,
  resumeMode,
  startMode,
  timerAdd,
  timerLeft,
  weekEvents,
  weekLabel,
  weekOf,
  whenLabel,
} from '../src/lib/today';
import { defaultFeatures, defaultSettings, type CalEvent, type FocusState, type MeterState } from '../src/lib/types';

// 14 Ekim 2026 Çarşamba, 10:24 (yerel saat)
const now = new Date(2026, 9, 14, 10, 24);
const ev = (id: number, date: string, time: string | null): CalEvent => ({ id, date, time, title: `e${id}` });

describe('bugün', () => {
  it('haftalık liste: yalnız o haftanın etkinlikleri, gün ve saate göre', () => {
    const events = [ev(1, '2026-10-16', '09:00'), ev(2, '2026-10-14', '14:00'), ev(3, '2026-10-14', null), ev(4, '2026-10-19', null), ev(5, '2026-10-11', '10:00')];
    expect(weekEvents(events, weekOf(now)).map(e => e.id)).toEqual([3, 2, 1]);
    expect(weekEvents([], weekOf(now))).toEqual([]);
  });

  it('gün etiketi: Bugün, Yarın, Dün, yoksa kısa gün adı', () => {
    expect(dayLabel('2026-10-14', now)).toBe('Bugün');
    expect(dayLabel('2026-10-15', now)).toBe('Yarın');
    expect(dayLabel('2026-10-13', now)).toBe('Dün');
    expect(dayLabel('2026-10-16', now)).toBe('Cum 16');
    expect(dayLabel('2026-10-12', now)).toBe('Pzt 12');
  });

  it('hafta pazartesiden başlar', () => {
    const days = weekOf(now);
    expect(days.map(isoDate)).toEqual(['2026-10-12', '2026-10-13', '2026-10-14', '2026-10-15', '2026-10-16', '2026-10-17', '2026-10-18']);
    expect(weekLabel(days)).toBe('12–18 Ekim');
    expect(isoDate(weekOf(new Date(2026, 9, 18), 0)[0]!)).toBe('2026-10-12');
    expect(weekLabel(weekOf(now, 2))).toBe('26 Ekim – 1 Kasım');
  });

  it('sıradaki etkinlik: geçmiş saatler atlanır, bugünün gün boyu etkinliği sayılır', () => {
    const events = [ev(1, '2026-10-13', '15:00'), ev(2, '2026-10-14', '09:00'), ev(3, '2026-10-14', '14:00'), ev(4, '2026-10-15', null)];
    expect(nextEvent(events, now)?.id).toBe(3);
    expect(nextEvent([ev(5, '2026-10-14', null), ev(6, '2026-10-14', '12:00')], now)?.id).toBe(5);
    expect(nextEvent([ev(1, '2026-10-13', '15:00')], now)).toBeNull();
  });

  it('zaman etiketi', () => {
    expect(whenLabel(ev(1, '2026-10-14', '14:00'), now)).toBe('Bugün 14:00');
    expect(whenLabel(ev(1, '2026-10-15', null), now)).toBe('Yarın');
    expect(whenLabel(ev(1, '2026-10-16', '09:30'), now)).toBe('Cuma 09:30');
    expect(whenLabel(ev(1, '2026-11-03', null), now)).toBe('3 Kasım');
  });

  it('zamanlayıcıda kalan süre', () => {
    expect(timerLeft({ minutes: 5, endsAt: null, leftMs: 120_000 }, 0)).toBe(120_000);
    expect(timerLeft({ minutes: 5, endsAt: 10_000, leftMs: 0 }, 4_000)).toBe(6_000);
    expect(timerLeft({ minutes: 5, endsAt: 10_000, leftMs: 0 }, 12_000)).toBe(0);
  });
});

describe('odak modu (Faz 5a: modun içinde)', () => {
  it('4 odak, aralarda 5 dk mola, sonra 15 dk uzun mola', () => {
    let f: FocusState | null = { phase: 'work', round: 1 };
    const seen: string[] = [];
    while (f) {
      seen.push(focusLabel(f));
      f = nextFocus(f);
    }
    expect(seen).toEqual(['Odak 1/4', 'Mola', 'Odak 2/4', 'Mola', 'Odak 3/4', 'Mola', 'Odak 4/4', 'Uzun mola']);
  });

  it('aşama dolunca kısa not (Bitiş değil) ve Modlar açılır', () => {
    const step = { ended: 'work' as const, next: { phase: 'break' as const, round: 1 } };
    expect(focusStepText(step)).toEqual({ title: 'Odak bitti, mola', detail: '5 dk · 1/4' });
    const n = noticeFor({ approvals: [], timerDone: { key: 1, minutes: 25, focus: step } });
    expect(n).toMatchObject({ key: 'timer:1', mood: 'done', title: 'Odak bitti, mola', tab: 'modes' });
    expect(n?.finish).toBeUndefined();
    expect(defaultFeatures().focus).toBe(true);
  });

  it('döngü çubuğu ve sıradaki aşama', () => {
    const bar = (f: FocusState) => cycleBar(f).map(p => `${p.kind[0]}${p.state[0]}`).join(' ');
    expect(bar({ phase: 'work', round: 1 })).toBe('wn bt wt bt wt bt wt');
    expect(bar({ phase: 'break', round: 2 })).toBe('wd bd wd bn wt bt wt');
    expect(bar({ phase: 'longBreak', round: 4 })).toBe('wd bd wd bd wd bd wd');
    expect(nextPhaseText({ phase: 'work', round: 1 })).toBe('sonra 5 dk mola');
    expect(nextPhaseText({ phase: 'work', round: 4 })).toBe('sonra 15 dk uzun mola');
    expect(nextPhaseText({ phase: 'longBreak', round: 4 })).toBe('sonra 25 dk odak');
  });
});

describe('Zodymetre (Faz 5a)', () => {
  const meter = (o: Partial<MeterState> = {}): MeterState => ({ id: 1, name: 'Okuma', color: '', sound: true, minutes: 25, endsAt: null, leftMs: 25 * 60_000, day: '', rounds: 0, doneMs: 0, ...o });

  it('başlamış ama bitmemiş olan "süren"dir', () => {
    expect(meterActive(meter(), 0)).toBe(false);
    expect(meterActive(meter({ endsAt: 60_000 }), 0)).toBe(true);
    expect(meterActive(meter({ leftMs: 60_000 }), 0)).toBe(true);
    expect(meterActive(meter({ leftMs: 0 }), 0)).toBe(false);
  });

  it('dolunca bugünkü tur sayısı artar, gün değişince sıfırlanır', () => {
    const a = finishMeter(meter({ day: '2026-11-10', rounds: 2, doneMs: 50 * 60_000 }), '2026-11-10');
    expect(a.done).toEqual({ id: 1, name: 'Okuma', color: '', minutes: 25, rounds: 3, totalMs: 75 * 60_000 });
    expect(a.meter).toMatchObject({ endsAt: null, leftMs: 0 });
    expect(finishMeter(a.meter, '2026-11-11').done.rounds).toBe(1);
  });

  it('+5 dk adını ve rengini korur', () => {
    expect(timerAdd(meter({ endsAt: 1000 }), 5, 0)).toMatchObject({ name: 'Okuma', minutes: 30, endsAt: 301_000 });
    expect(timerAdd(meter({ leftMs: 0 }), 5, 10)).toMatchObject({ minutes: 5, endsAt: 300_010, leftMs: 300_000 });
  });

  it('Saat kartında öndeki: en yakın biten, yoksa yarıda kalan, yoksa ilki', () => {
    const list = [meter({ id: 1 }), meter({ id: 2, leftMs: 60_000 }), meter({ id: 3, endsAt: 9_000 }), meter({ id: 4, endsAt: 5_000 })];
    expect(leadMeter(list)?.id).toBe(4);
    expect(leadMeter(list.slice(0, 2))?.id).toBe(2);
    expect(leadMeter(list.slice(0, 1))?.id).toBe(1);
    expect(leadMeter([])).toBeNull();
  });

  it('süre yazısı', () => {
    expect([minutesText(25), minutesText(60), minutesText(75), minutesText(0)]).toEqual(['25 dk', '1 sa', '1 sa 15 dk', 'Süresiz']);
  });
});

describe('Modlar (Faz 5a)', () => {
  const [work, study, , game] = defaultModes();

  it('dört varsayılan mod ve eylem özeti', () => {
    expect(defaultModes().map(m => m.id)).toEqual(['work', 'study', 'sleep', 'game']);
    expect(modeActions(work!)).toEqual(['Bildirimler: yalnız AI', '25/5 odak döngüsü']);
    expect(modeActions(work!, false)).toEqual(['Bildirimler: yalnız AI']);
    expect(modeSummary(study!)).toBe('1 sa 30 dk · 3 eylem');
    expect(modeSummary(game!)).toBe('Süresiz · 1 eylem');
    const apps = { ...game!, quiet: false, closeApps: [{ name: 'Discord', path: 'C:\\D.exe' }], openApps: [{ name: 'Code', path: 'C:\\C.exe' }] };
    expect(modeActions(apps)).toEqual(['Discord kapanır', 'Code açılır']);
  });

  it('süreli mod duraklar, sürer; süresizde kalan yok', () => {
    let run = startMode(work!, undefined, 0, true);
    expect([run.minutes, run.endsAt, run.focus, run.phaseEndsAt]).toEqual([50, 50 * 60_000, { phase: 'work', round: 1 }, 25 * 60_000]);
    run = pauseMode(run, 60_000);
    expect([modeLeft(run, 9e9), phaseLeft(run, 9e9)]).toEqual([49 * 60_000, 24 * 60_000]);
    run = resumeMode(run, 120_000);
    expect([run.endsAt, run.phaseEndsAt]).toEqual([120_000 + 49 * 60_000, 120_000 + 24 * 60_000]);
    expect(startMode(work!, undefined, 0, false).focus).toBeNull();
    const endless = startMode(game!, undefined, 0, true);
    expect([endless.endsAt, modeLeft(endless, 1), phaseLeft(endless, 1)]).toEqual([null, null, null]);
  });

  it('mod Bitişi: kapattığı uygulamaları sorar, Modlar açılır', () => {
    const n = noticeFor({ approvals: [], modeDone: { key: 3, id: 'work', name: 'Çalışma', icon: 'brief', color: '#5b9cff', minutes: 50, reopen: ['Discord'] } });
    expect(n).toMatchObject({ key: 'mode:3', title: 'Çalışma modu bitti · 50 dk', detail: 'Discord kapatılmıştı, yeniden açılsın mı?', tab: 'modes' });
    expect(n?.finish).toMatchObject({ kind: 'mode', color: '#5b9cff', reopen: ['Discord'] });
    expect(defaultSettings.modules.modes).toBe(true);
    expect(defaultSettings.notch.mode).toBe(true);
  });
});
