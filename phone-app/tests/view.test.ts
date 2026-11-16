import { describe, expect, it } from 'vitest';
import { ageText, approvalFor, buildAnswers, groupAgenda, isPast, remainText, sortSessions, statusText, statusTone, timeLabel } from '../src/lib/view';
import type { AgendaItem, ApprovalInfo, SessionInfo } from '../src/lib/types';

const s = (id: string, status: string, started: number | null = 1000): SessionInfo => ({ id, tool: 'claude', name: id, folder: '', status, started, step: null, steps: null, reply: null });
const ag = (date: string, time: string | null, title = 'x'): AgendaItem => ({ id: 1, date, time, title, minutes: 30, lead: 10, notify: !!time });

describe('durum', () => {
  it('bilinmeyen durum boşta sayılır', () => {
    expect(statusText('???')).toBe('boşta');
    expect(statusTone('???')).toBe('idle');
    expect(statusTone('waiting_input')).toBe('amber');
    expect(statusText('done')).toBe('tamamlandı');
  });
  it('bekleyenler üstte, sonra çalışanlar', () => {
    const r = sortSessions([s('a', 'done', 5), s('b', 'running', 1), s('c', 'waiting_approval', 0), s('d', 'running', 9)]);
    expect(r.map((x) => x.id)).toEqual(['c', 'd', 'b', 'a']);
  });
  it('onay oturuma kimlikle bağlanır', () => {
    const ap: ApprovalInfo[] = [{ id: '1', session: 'b', session_id: 'x', kind: 'permission', action: null, questions: null }, { id: '2', session: 'a', session_id: 'a', kind: 'question', action: null, questions: null }];
    expect(approvalFor(s('a', 'waiting_input'), ap)?.id).toBe('2');
    expect(approvalFor(s('b', 'waiting_input'), ap)).toBeUndefined();
  });
});

describe('süre', () => {
  it('çalışanda geçen süre, diğerinde yaş', () => {
    expect(timeLabel(s('a', 'running', 1000), 1042_000)).toBe('0:42');
    expect(timeLabel(s('a', 'running', 0), 3_725_000)).toBe('1:02:05');
    expect(timeLabel(s('a', 'done', 0), 18 * 60_000)).toBe('18 dk');
    expect(timeLabel(s('a', 'done', null), 5)).toBe('');
    expect(ageText(30)).toBe('şimdi');
    expect(ageText(86400 + 5)).toBe('dün');
    expect(ageText(3 * 86400)).toBe('3 gün');
  });
  it('ayrıntı penceresi geri sayımı', () => {
    expect(remainText(null, 0)).toBeNull();
    expect(remainText(1000, 2000)).toBeNull();
    expect(remainText(84_000, 0)).toBe('1:24');
  });
});

describe('ajanda', () => {
  const now = new Date(2026, 10, 14, 12, 0).getTime();
  it('güne göre gruplar, Bugün/Yarın adlandırır', () => {
    const g = groupAgenda([ag('2026-11-16', '09:00'), ag('2026-11-14', '14:30'), ag('2026-11-14', null), ag('2026-11-15', '11:00')], now);
    expect(g.map((d) => d.title)).toEqual(['Bugün', 'Yarın', 'Pazartesi']);
    expect(g[0]?.sub).toBe('Cumartesi 14 Kasım');
    expect(g[2]?.sub).toBe('16 Kasım');
    expect(g[0]?.items.map((i) => i.time)).toEqual([null, '14:30']);
  });
  it('geçmiş saat soluk', () => {
    expect(isPast(ag('2026-11-14', '10:00'), now)).toBe(true);
    expect(isPast(ag('2026-11-14', '14:00'), now)).toBe(false);
    expect(isPast(ag('2026-11-14', null), now)).toBe(false);
  });
});

describe('soru cevabı', () => {
  it('serbest metin seçimin önüne geçer, çoklu seçim birleşir, boş soru gitmez', () => {
    const q = [{ q: 'A' }, { q: 'B' }, { q: 'C' }];
    expect(buildAnswers(q, [['x'], ['y', 'z'], []], ['', '', ''])).toEqual({ A: 'x', B: 'y, z' });
    expect(buildAnswers(q, [['x'], [], []], ['kendi', '', ''])).toEqual({ A: 'kendi' });
  });
});
