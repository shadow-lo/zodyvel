// AI panelinin saf hesapları: yapılacaklar ilerlemesi, izin kipi rozeti, döngü uyarısı, oturum sırası.

import { since } from './format';
import type { AgentSession, AgentStatus, AgentStep, AgentTodo } from './types';

/** Yapılacaklar ilerlemesi: biten / toplam ve sürmekte olan iş. Liste yoksa null. */
export function todoProgress(todos: AgentTodo[]): { done: number; total: number; current: AgentTodo | null } | null {
  if (todos.length === 0) return null;
  const done = todos.filter(t => t.status === 'completed').length;
  const current = todos.find(t => t.status === 'in_progress') ?? null;
  return { done, total: todos.length, current };
}

/** `short`: mini görünümdeki dar kart için. */
export type ModeBadge = { text: string; short: string; tone: 'plan' | 'auto' | 'danger' };

/** İzin kipinin rozeti; olağan kipte rozet yok. */
export function modeBadge(mode: string | null): ModeBadge | null {
  switch (mode) {
    case 'plan':
      return { text: 'Plan modu', short: 'Plan', tone: 'plan' };
    case 'acceptEdits':
      return { text: 'Düzenlemeler otomatik', short: 'Oto düzenleme', tone: 'auto' };
    case 'bypassPermissions':
      return { text: 'Her şeye izinli', short: 'Her şeye izinli', tone: 'danger' };
    default:
      return null;
  }
}

/** Döngü sayılması için aynı adımın üst üste tekrarı. */
export const LOOP_REPEAT = 3;

/**
 * Ajan aynı komutu ya da aynı dosyadaki düzenlemeyi üst üste deniyor ve her seferinde olmuyorsa döngü.
 * Yalnız son adımlara bakar; başarılı bir adım sayacı sıfırlar.
 */
export function loopOf(steps: AgentStep[]): { target: string; count: number; command: boolean } | null {
  let count = 0;
  let key = '';
  let target = '';
  for (let i = steps.length - 1; i >= 0; i--) {
    const step = steps[i]!;
    if (step.state === 'now' && i === steps.length - 1) continue;
    if (step.state !== 'failed' || !['bash', 'edit', 'write'].includes(step.kind) || !step.target) break;
    const k = `${step.kind}:${step.target}`;
    if (count === 0) [key, target] = [k, step.target];
    else if (k !== key) break;
    count += 1;
  }
  return count >= LOOP_REPEAT ? { target, count, command: key.startsWith('bash:') } : null;
}

const URGENCY: Record<AgentStatus, number> = { waiting_approval: 0, error: 1, waiting_input: 2, running: 3, done: 4 };

/** Ayrıntıdaki oturum listesi: önce beklemede olan, sonra en son hareket eden. */
export function byUrgency<T extends Pick<AgentSession, 'status' | 'lastSeen'>>(agents: T[]): T[] {
  return [...agents].sort((a, b) => URGENCY[a.status] - URGENCY[b.status] || b.lastSeen - a.lastSeen);
}

/** Bağlam doluluğu yüzdesi (0–100). */
export const contextPercent = (tokens: number, window: number) => (window > 0 ? Math.min(100, Math.round((tokens / window) * 100)) : 0);

/** Bağlam bu yüzdeye varınca ayrıntı başlığında "yakında sıkıştırılacak" ipucu çıkar. */
export const CONTEXT_HOT = 80;
export const contextHot = (percent: number) => percent >= CONTEXT_HOT;

/** Küçük oturum çipinin sağındaki kısa durum: "onay", "3/7", "bitti · 2 dk". */
export function chipStatus(agent: Pick<AgentSession, 'status' | 'todos' | 'statusSince'>, now = Date.now()): string {
  switch (agent.status) {
    case 'waiting_approval':
      return 'onay';
    case 'waiting_input':
      return 'bekliyor';
    case 'error':
      return 'hata';
    case 'done':
      return `bitti · ${since(agent.statusSince, now)}`;
    case 'running': {
      const p = todoProgress(agent.todos);
      return p ? `${p.done}/${p.total}` : 'çalışıyor';
    }
  }
}

/** Durumun rengi (CSS değişkeni): Yeni görünümdeki durum çizgisi, hale ve satır yazısı. */
export const statusTone: Record<AgentStatus, string> = {
  running: 'var(--blue)',
  waiting_input: 'var(--text-2)',
  waiting_approval: 'var(--amber)',
  done: 'var(--green)',
  error: 'var(--red)',
};

/** Yeni görünümdeki oturum satırının ikinci satırı: "Çalışıyor · 2/5", "Bekliyor · 12 dk", "Bitti · 2 dk". */
export function rowStatus(agent: Pick<AgentSession, 'status' | 'todos' | 'statusSince'>, now = Date.now()): string {
  switch (agent.status) {
    case 'waiting_approval':
      return 'Onay bekliyor';
    case 'waiting_input':
      return `Bekliyor · ${since(agent.statusSince, now)}`;
    case 'error':
      return 'Hata';
    case 'done':
      return `Bitti · ${since(agent.statusSince, now)}`;
    case 'running': {
      const p = todoProgress(agent.todos);
      return p ? `Çalışıyor · ${p.done}/${p.total}` : 'Çalışıyor';
    }
  }
}

/** Ayrı gösterilecek kategori (Faz 7.1): sohbetin kendi adı varsa klasör/proje adı ("Ödev"). Ad zaten
 * klasör adıysa (aynı klasördeki ikinci oturumun "Ödev 2"si dahil) tekrar yazılmaz. */
export function categoryOf(agent: { project: string; category?: string }): string | null {
  const category = agent.category?.trim();
  if (!category) return null;
  const base = agent.project.replace(/ \d+$/, '');
  return category === agent.project || category === base ? null : category;
}
