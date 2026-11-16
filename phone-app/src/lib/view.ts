// Ekranların saf yardımcıları (testli: tests/view.test.ts). Hiçbiri zamanlayıcı ya da durum tutmaz.
import type { AgendaItem, ApprovalInfo, SessionInfo } from './types';

export type Tone = 'accent' | 'amber' | 'green' | 'red' | 'idle';

/** Bilinen durumlar; bilinmeyen "idle" sayılır. */
export function statusOf(s: string): 'running' | 'waiting_input' | 'waiting_approval' | 'done' | 'error' | 'idle' {
  return s === 'running' || s === 'waiting_input' || s === 'waiting_approval' || s === 'done' || s === 'error' ? s : 'idle';
}

export function statusText(s: string): string {
  switch (statusOf(s)) {
    case 'running': return 'çalışıyor';
    case 'waiting_approval': return 'onay bekliyor';
    case 'waiting_input': return 'soru soruyor';
    case 'done': return 'tamamlandı';
    case 'error': return 'hata verdi';
    default: return 'boşta';
  }
}

export function statusTone(s: string): Tone {
  switch (statusOf(s)) {
    case 'running': return 'accent';
    case 'waiting_approval':
    case 'waiting_input': return 'amber';
    case 'done': return 'green';
    case 'error': return 'red';
    default: return 'idle';
  }
}

export const needsMe = (s: SessionInfo) => s.status === 'waiting_approval' || s.status === 'waiting_input';

/** Onay/soru bekleyenler üstte, sonra çalışanlar, sonra geri kalanlar; her grupta yeni olan önce. */
export function sortSessions(list: SessionInfo[]): SessionInfo[] {
  const rank = (s: SessionInfo) => (needsMe(s) ? 0 : statusOf(s.status) === 'running' ? 1 : 2);
  return [...list].sort((a, b) => rank(a) - rank(b) || (b.started ?? 0) - (a.started ?? 0));
}

/** Oturumun bekleyen onayı/sorusu (önce oturum kimliği, yoksa ad ile eşleşir). */
export function approvalFor(s: SessionInfo, approvals: ApprovalInfo[]): ApprovalInfo | undefined {
  return approvals.find((a) => a.session_id === s.id) ?? approvals.find((a) => !a.session_id && a.session === s.name);
}

/** Çalışan oturumda geçen süre "0:42" / "1:05:10"; diğerlerinde kısa yaş "şimdi", "18 dk", "3 sa", "dün", "4 gün". */
export function timeLabel(s: SessionInfo, nowMs: number): string {
  if (s.started == null) return '';
  const sec = Math.max(0, Math.floor(nowMs / 1000 - s.started));
  if (statusOf(s.status) === 'running') {
    const h = Math.floor(sec / 3600), m = Math.floor((sec % 3600) / 60), x = sec % 60;
    const pad = (n: number) => String(n).padStart(2, '0');
    return h ? `${h}:${pad(m)}:${pad(x)}` : `${m}:${pad(x)}`;
  }
  return ageText(sec);
}

export function ageText(sec: number): string {
  if (sec < 60) return 'şimdi';
  if (sec < 3600) return `${Math.floor(sec / 60)} dk`;
  if (sec < 86400) return `${Math.floor(sec / 3600)} sa`;
  const d = Math.floor(sec / 86400);
  return d === 1 ? 'dün' : `${d} gün`;
}

/** Ayrıntı penceresinin kalan süresi "1:24"; kapalıysa null. */
export function remainText(until: number | null, nowMs: number): string | null {
  if (until == null || until <= nowMs) return null;
  const s = Math.ceil((until - nowMs) / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export const toolName = (t: string) => (t === 'claude' ? 'Claude' : t === 'codex' ? 'Codex' : 'AI');

/** "HH:MM" yerel saat. */
export function clock(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

const GUN = ['Pazar', 'Pazartesi', 'Salı', 'Çarşamba', 'Perşembe', 'Cuma', 'Cumartesi'];
const AY = ['Ocak', 'Şubat', 'Mart', 'Nisan', 'Mayıs', 'Haziran', 'Temmuz', 'Ağustos', 'Eylül', 'Ekim', 'Kasım', 'Aralık'];

/** "2026-11-14" → yerel gece yarısı (ms). Geçersizse NaN. */
function dayMs(date: string): number {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])).getTime() : NaN;
}

export function isoDay(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export interface AgendaDay {
  date: string;
  /** "Bugün" | "Yarın" | "Pazartesi" */
  title: string;
  /** "Cumartesi 14 Kasım" ya da "16 Kasım" */
  sub: string;
  items: AgendaItem[];
}

/** Olayları güne göre gruplar (tarih, sonra gün boyu önce, sonra saat sırası). */
export function groupAgenda(items: AgendaItem[], nowMs: number): AgendaDay[] {
  const today = isoDay(nowMs);
  const tomorrow = isoDay(dayMs(today) + 36 * 3600 * 1000);
  const sorted = [...items].sort((a, b) => a.date.localeCompare(b.date) || (a.time ?? '').localeCompare(b.time ?? ''));
  const out: AgendaDay[] = [];
  for (const it of sorted) {
    let g = out[out.length - 1];
    if (!g || g.date !== it.date) {
      const ms = dayMs(it.date);
      const d = new Date(ms);
      const named = Number.isNaN(ms) ? it.date : `${d.getDate()} ${AY[d.getMonth()]}`;
      const wd = Number.isNaN(ms) ? '' : GUN[d.getDay()] ?? '';
      const title = it.date === today ? 'Bugün' : it.date === tomorrow ? 'Yarın' : wd || it.date;
      const sub = it.date === today || it.date === tomorrow ? `${wd} ${named}` : named;
      g = { date: it.date, title, sub, items: [] };
      out.push(g);
    }
    g.items.push(it);
  }
  return out;
}

/** Bugünün saati geçmiş olayı mı (soluk gösterilir)? */
export function isPast(it: AgendaItem, nowMs: number): boolean {
  const today = isoDay(nowMs);
  if (it.date < today) return true;
  if (it.date > today || !it.time) return false;
  return it.time < clock(nowMs);
}

/** Soru formu cevabı: tek seçim, çoklu seçim (", " ile) ya da serbest metin (seçimlerin önüne geçer). */
export function buildAnswers(questions: { q: string }[], picked: string[][], free: string[]): Record<string, string> {
  const out: Record<string, string> = {};
  questions.forEach((x, i) => {
    const f = (free[i] ?? '').trim();
    const v = f || (picked[i] ?? []).join(', ');
    if (v) out[x.q] = v;
  });
  return out;
}

export const MAX_TEXT = 2000;
