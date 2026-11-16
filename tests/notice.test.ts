import { describe, expect, it } from 'vitest';
import { agentEvents, batteryNote, dueReminder, noticeFor, updateNote, type AgentEvent } from '../src/lib/notice';
import type { AgentSession, AgentStatus, ApprovalRequest, Battery } from '../src/lib/types';

const request = (id: string, over: Partial<ApprovalRequest> = {}): ApprovalRequest => ({
  id,
  sessionId: 's',
  tool: 'claude',
  project: 'Psrc',
  kind: 'permission',
  action: 'Komut çalıştırmak istiyor',
  detail: 'pnpm test',
  questions: [],
  createdAt: 0,
  expiresAt: null,
  actionable: true,
  closedEarly: false,
  nativeOnly: false,
  files: [],
  always: null,
  plan: null,
  demo: false,
  ...over,
});

describe('çentik bildirimi', () => {
  it('istek yoksa bildirim yok', () => {
    expect(noticeFor({ approvals: [] })).toBeNull();
  });

  it('onay isteği: Zody dikkat hâlinde, AI bölümünü açar', () => {
    const n = noticeFor({ approvals: [request('a')] });
    expect(n).toMatchObject({ key: 'a', mood: 'attention', title: 'Onay bekleniyor', detail: 'Claude · Psrc', tab: 'ai', more: 0 });
  });

  it('soru ve araçta beklenen istek ayrı yazılır', () => {
    expect(noticeFor({ approvals: [request('q', { kind: 'question' })] })?.title).toBe('Soru soruyor');
    expect(noticeFor({ approvals: [request('c', { tool: 'codex', actionable: false })] })).toMatchObject({
      title: 'Araçta onay bekliyor',
      detail: 'Codex · Psrc',
    });
  });

  it('adadan karar verilebilen istek öne çıkar; diğerleri sayılır', () => {
    const n = noticeFor({ approvals: [request('info', { actionable: false }), request('live')] });
    expect(n).toMatchObject({ key: 'live', more: 1 });
  });

  it('kısa olay: bitti yeşil, hata kırmızı Zody; bekleyen istek önce gelir', () => {
    const done: AgentEvent = { key: 'k', session: 's1', kind: 'done', tool: 'codex', project: 'Psrc', turnMs: 252_000, files: 3 };
    expect(noticeFor({ approvals: [], flash: done })).toMatchObject({ mood: 'done', title: 'Bitti', detail: 'Codex · Psrc', tab: 'ai' });
    expect(noticeFor({ approvals: [], flash: { ...done, kind: 'error' } })).toMatchObject({ mood: 'error', title: 'Hata verdi' });
    expect(noticeFor({ approvals: [request('a')], flash: done })?.key).toBe('a');
  });

  it('Bitti düşük öncelikli notları iter: hatırlatma, odak aşaması, kısa not (Faz 7.2)', () => {
    const done: AgentEvent = { key: 'k', session: 's1', kind: 'done', tool: 'claude', project: 'Naber', turnMs: null, files: 0 };
    const reminder = { key: 'r', tool: 'claude' as const, project: 'Psrc', minutes: 12 };
    const toast = { key: 't', mood: 'idle' as const, title: 'Sesler kapalı', detail: '', tab: 'pc' as const };
    expect(noticeFor({ approvals: [], flash: done, reminder })?.key).toBe('k');
    expect(noticeFor({ approvals: [], flash: done, toast })?.key).toBe('k');
    expect(noticeFor({ approvals: [], flash: null, reminder })?.key).toBe('remind:r');
  });

  it('dolan zamanlayıcı Bugün bölümünü açar; istekten sonra, kısa olaydan önce gelir', () => {
    const timerDone = { key: 9, minutes: 25 };
    const flash: AgentEvent = { key: 'k', session: 's1', kind: 'done', tool: 'claude', project: 'Psrc', turnMs: null, files: 0 };
    expect(noticeFor({ approvals: [], flash, timerDone })).toMatchObject({ key: 'timer:9', mood: 'done', title: 'Zodymetre bitti · 25 dk', detail: 'Süre doldu', tab: 'today', finish: { kind: 'meter', minutes: 25 } });
    const meter = { id: 3, name: 'Okuma', color: '#2ec4b6', minutes: 25, rounds: 3, totalMs: 75 * 60_000 };
    expect(noticeFor({ approvals: [], timerDone: { key: 9, minutes: 25, meter } })).toMatchObject({
      title: 'Okuma bitti · 25 dk',
      detail: 'Bugün 3. tur · toplam 1 sa 15 dk',
      finish: { kind: 'meter', meterId: 3, name: 'Okuma', color: '#2ec4b6', rounds: 3 },
    });
    expect(noticeFor({ approvals: [request('a')], timerDone })?.key).toBe('a');
  });

  it('saati gelen etkinlik adını gösterir, Bugün bölümünü açar; istekten sonra, zamanlayıcıdan önce gelir', () => {
    const eventDue = { key: 4, id: 7, title: 'Toplantı', time: '00:04', more: 1 };
    const timerDone = { key: 9, minutes: 25 };
    expect(noticeFor({ approvals: [], eventDue, timerDone })).toMatchObject({
      key: 'event:4',
      title: 'Toplantı',
      detail: 'Plan · 00:04',
      tab: 'agenda',
      more: 1,
    });
    expect(noticeFor({ approvals: [request('a')], eventDue })?.key).toBe('a');
  });

  it('kısa not (sustur) en düşük öncelikte: yalnız başka bir şey yokken görünür', () => {
    const toast = { key: 'sound:1', mood: 'sleep' as const, title: 'Sesler kapalı', detail: 'Ctrl+Alt+M ile değiştir', tab: 'music' as const };
    expect(noticeFor({ approvals: [], toast })).toMatchObject({ ...toast, more: 0 });
    expect(noticeFor({ approvals: [request('a')], toast })?.key).toBe('a');
    expect(noticeFor({ approvals: [], timerDone: { key: 2, minutes: 5 }, toast })?.key).toBe('timer:2');
  });
});

const session = (id: string, status: AgentStatus, over: Partial<AgentSession> = {}): AgentSession => ({
  id, tool: 'claude', project: 'Psrc', status, step: '', startedAt: 0, lastSeen: 0, demo: false, steps: [], app: null,
  statusSince: 0, turnStartedAt: null, turnEndedAt: null, turnFiles: 0,
  changed: [], todos: [], lastReply: null, mode: null, folder: null, subagents: 0, notes: [], canNote: false, canTask: false,
  ...over,
});

describe('ajan olayları', () => {
  it("yalnız bitti/hata'ya yeni geçiş olay üretir", () => {
    const before = new Map<string, AgentStatus>([['a', 'running'], ['b', 'running'], ['c', 'done']]);
    const events = agentEvents(before, [session('a', 'done'), session('b', 'waiting_input'), session('c', 'done')], 7);
    expect(events).toEqual([{ key: 'a:done:7', session: 'a', kind: 'done', tool: 'claude', project: 'Psrc', turnMs: null, files: 0 }]);
    expect(agentEvents(new Map([['a', 'running']]), [session('a', 'error')], 1)[0]?.kind).toBe('error');
    // Kullanıcının durdurduğu tur (mini AI › Durdur) bitti bildirimi çıkarmaz (Faz 9).
    expect(agentEvents(new Map([['a', 'running']]), [session('a', 'done', { step: 'Durduruldu' })], 1)).toEqual([]);
  });

  it('önceki durumu bilinmeyen oturum olay üretmez (ilk yükleme)', () => {
    expect(agentEvents(new Map(), [session('x', 'done')], 1)).toEqual([]);
  });
});

const MIN = 60_000;

describe('akıllı uyarılar', () => {
  it('bitti özeti: tur süresi başlıkta, dosya sayısı ayrıntıda; kapalıyken eski yazı', () => {
    const done: AgentEvent = { key: 'k', session: 's1', kind: 'done', tool: 'claude', project: 'Psrc', turnMs: 252_000, files: 3 };
    expect(noticeFor({ approvals: [], flash: done, doneSummary: true })).toMatchObject({ title: 'Bitti · 4 dk 12 sn', detail: 'Claude · Psrc · 3 dosya' });
    expect(noticeFor({ approvals: [], flash: done })).toMatchObject({ title: 'Bitti', detail: 'Claude · Psrc' });
    // Süre bilinmiyor, dosya değişmedi: yalnız bilinen yazılır.
    expect(noticeFor({ approvals: [], flash: { ...done, kind: 'error', turnMs: null, files: 0 }, doneSummary: true })).toMatchObject({
      title: 'Hata verdi',
      detail: 'Claude · Psrc',
    });
  });

  it('dosya sayısı yazan Bitti bildirimi o oturumun Değişenler sekmesini açar', () => {
    const done: AgentEvent = { key: 'k', session: 's1', kind: 'done', tool: 'claude', project: 'Psrc', turnMs: 1_000, files: 4 };
    expect(noticeFor({ approvals: [], flash: done, doneSummary: true })?.open).toEqual({ session: 's1', section: 'changed' });
    // Dosya sayısı yazmıyorsa (özet kapalı ya da dosya yok) yalnız oturum seçilir.
    expect(noticeFor({ approvals: [], flash: done })?.open).toEqual({ session: 's1', section: null });
    expect(noticeFor({ approvals: [], flash: { ...done, files: 0 }, doneSummary: true })?.open).toEqual({ session: 's1', section: null });
    expect(noticeFor({ approvals: [request('a')], flash: done, doneSummary: true })?.open).toBeUndefined();
  });

  it('tur süresi ve dosya sayısı oturumdan gelir', () => {
    const before = new Map<string, AgentStatus>([['a', 'running']]);
    const a = { ...session('a', 'done'), turnStartedAt: 1_000, turnEndedAt: 43_000, turnFiles: 2 };
    expect(agentEvents(before, [a], 9)[0]).toMatchObject({ turnMs: 42_000, files: 2 });
  });

  it('uzun bekleyen onay: eşikten sonra başlık süreyi söyler, anahtar bir kez değişir', () => {
    const r = request('a', { createdAt: 0 });
    expect(noticeFor({ approvals: [r], lateAfterMin: 10, now: 9 * MIN })).toMatchObject({ key: 'a', title: 'Onay bekleniyor' });
    expect(noticeFor({ approvals: [r], lateAfterMin: 10, now: 12 * MIN + 5 })).toMatchObject({ key: 'a:late', title: "12 dk'dır onay bekliyor" });
    expect(noticeFor({ approvals: [r], lateAfterMin: 10, now: 15 * MIN })?.key).toBe('a:late');
    expect(noticeFor({ approvals: [{ ...r, kind: 'question' }], lateAfterMin: 5, now: 6 * MIN })?.title).toBe("6 dk'dır cevap bekliyor");
    // Özellik kapalı: hiç değişmez.
    expect(noticeFor({ approvals: [r], lateAfterMin: null, now: 60 * MIN })).toMatchObject({ key: 'a', title: 'Onay bekleniyor' });
  });

  it('yanıt bekleyen oturum bir kez hatırlatılır; örnek ve çalışan oturum hatırlatılmaz', () => {
    const waiting = { ...session('w', 'waiting_input'), statusSince: 0 };
    expect(dueReminder([waiting], 9 * MIN, 10, new Set())).toBeNull();
    const due = dueReminder([waiting], 12 * MIN, 10, new Set());
    expect(due).toMatchObject({ key: 'w:0', minutes: 12, project: 'Psrc' });
    expect(dueReminder([waiting], 13 * MIN, 10, new Set(['w:0']))).toBeNull();
    // Yeni bekleme (durum yeniden başladı) yeni hatırlatmadır.
    expect(dueReminder([{ ...waiting, statusSince: 20 * MIN }], 31 * MIN, 10, new Set(['w:0']))?.key).toBe(`w:${20 * MIN}`);
    expect(dueReminder([{ ...waiting, demo: true }, { ...session('r', 'running'), statusSince: 0 }], 60 * MIN, 10, new Set())).toBeNull();
    expect(noticeFor({ approvals: [], reminder: due })).toMatchObject({ key: 'remind:w:0', title: "12 dk'dır yanıtını bekliyor", tab: 'ai' });
    expect(noticeFor({ approvals: [request('a')], reminder: due })?.key).toBe('a');
  });
});

describe('pil notu', () => {
  const bat = (over: Partial<Battery> = {}): Battery => ({ percent: 50, plugged: false, charging: false, saver: false, minutesLeft: null, ...over });

  it('ilk okuma ve değişmeyen durum not üretmez', () => {
    expect(batteryNote(null, bat(), 1)).toBeNull();
    expect(batteryNote(bat(), bat({ percent: 49 }), 1)).toBeNull();
    expect(batteryNote(bat(), null, 1)).toBeNull();
  });

  it('şarja takma ve çıkarma', () => {
    expect(batteryNote(bat(), bat({ plugged: true, charging: true }), 1)).toMatchObject({ title: 'Şarj oluyor', detail: 'Pil %50', tab: 'pc' });
    expect(batteryNote(bat({ plugged: true }), bat({ minutesLeft: 135 }), 1)).toMatchObject({ title: 'Pilden çalışıyor', detail: 'Pil %50 · yaklaşık 2 sa 15 dk' });
  });

  it('%20 ve %10 eşikleri yalnız aşağı geçişte, prizdeyken değil', () => {
    expect(batteryNote(bat({ percent: 21 }), bat({ percent: 20 }), 1)).toMatchObject({ title: 'Pil azaldı', mood: 'attention', detail: '%20 · şarja tak' });
    expect(batteryNote(bat({ percent: 20 }), bat({ percent: 19 }), 1)).toBeNull();
    expect(batteryNote(bat({ percent: 11 }), bat({ percent: 10 }), 1)).toMatchObject({ title: 'Pil çok azaldı', mood: 'error' });
    expect(batteryNote(bat({ percent: 21, plugged: true }), bat({ percent: 20, plugged: true }), 1)).toBeNull();
  });
});

describe('Ajanda planı haberi (Faz 5b)', () => {
  it('"5 dk önce" haberinde başlama saati ve kalan dakika', () => {
    const eventDue = { key: 5, id: 8, title: 'Ders', time: '22:00', more: 0, lead: 5, color: '#ff7a59' };
    expect(noticeFor({ approvals: [], eventDue })).toMatchObject({ title: 'Ders', detail: "22:00'da başlıyor · 5 dk kaldı", tab: 'agenda' });
  });
});

describe('updateNote', () => {
  it('önceki sürüm varsa ve farklıysa "güncellendi" notu verir', () => {
    const note = updateNote('0.7.0', '0.7.1', 5);
    expect(note?.title).toBe("0.7.1'ye güncellendi");
    expect(note?.detail).toContain('0.7.0');
  });
  it('ilk açılışta (önceki boş) ya da aynı sürümde not yok', () => {
    expect(updateNote('', '0.7.1', 5)).toBeNull();
    expect(updateNote('0.7.1', '0.7.1', 5)).toBeNull();
  });
});
