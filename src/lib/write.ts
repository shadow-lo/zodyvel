// Yazma sahnesinin kuralları (saf fonksiyonlar; görünüm WriteScene.svelte'de).
//
// Hedef açık bir oturumsa ne olacağı oturumun durumuna bağlıdır: çalışırken yazılan sıraya girer, tur
// bitince iletilir ("Hemen ilet" bir sonraki araç çağrısında araya girer); bitmiş ve köprüsü bekleyen
// oturumda hemen yeni tur olur; köprü beklemiyorsa metin panoya kopyalanır ve oturumun penceresi öne gelir.
// Codex'e adadan yazılamaz: şeritte soluk durur.

import type { AgentSession } from './types';

export type WriteMode = 'queue' | 'task' | 'copy' | 'blocked';

export function writeMode(agent: AgentSession): WriteMode {
  const busy = agent.status === 'running' || agent.status === 'waiting_approval';
  if (busy) return agent.canNote ? 'queue' : 'blocked';
  if (agent.canTask) return 'task';
  // Pencereye yazı enjekte edilmez; kopyalanır. Codex'te de yalnız bu yol var ama adadan yazılmaz sayılır.
  return agent.tool === 'claude' ? 'copy' : 'blocked';
}

/** Gönder düğmesinin yazısı. */
export function sendLabel(mode: WriteMode): string {
  return mode === 'queue' ? 'Sıraya koy' : mode === 'task' ? 'Gönder' : mode === 'copy' ? 'Kopyala' : 'Gönder';
}

/** Şeritte sığan oturumlar: seçili olan her zaman görünür, sıra bozulmaz. */
export function stripAgents(agents: AgentSession[], selected: string | null, max = 3): { shown: AgentSession[]; more: number } {
  if (agents.length <= max) return { shown: agents, more: 0 };
  const head = agents.slice(0, max);
  const pick = agents.find(a => a.id === selected);
  const shown = pick && !head.includes(pick) ? [...head.slice(0, max - 1), pick] : head;
  return { shown, more: agents.length - shown.length };
}

/** İlk hedef: istenen oturum, yoksa yazılabilen ilk oturum, o da yoksa yeni görev. */
export function firstTarget(agents: AgentSession[], wanted: string | null, newTask: boolean): string | 'new' | null {
  const own = agents.find(a => a.id === wanted);
  if (own && writeMode(own) !== 'blocked') return own.id;
  const open = agents.find(a => writeMode(a) !== 'blocked');
  if (open) return open.id;
  return newTask ? 'new' : null;
}

/** Yeni görevle başlayacak oturumun numarası (Zody rozeti): açık görevlerin en büyüğünden bir fazla. */
export function nextTaskNo(agents: AgentSession[]): number {
  return agents.reduce((n, a) => Math.max(n, a.taskNo ?? 0), 0) + 1;
}

/** "Dün 22:14", "Pzt 17:40", "12 Eki"; bugünse yalnız saat. */
export function whenLabel(at: number, now = Date.now()): string {
  const d = new Date(at);
  const today = new Date(now);
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(today) - day(d)) / 86_400_000);
  const time = d.toLocaleTimeString('tr-TR', { hour: '2-digit', minute: '2-digit' });
  if (days <= 0) return time;
  if (days === 1) return `Dün ${time}`;
  if (days < 7) return `${d.toLocaleDateString('tr-TR', { weekday: 'short' })} ${time}`;
  return d.toLocaleDateString('tr-TR', { day: 'numeric', month: 'short' });
}

/** Raftan "Claude'a sor" ile yazma sahnesine eklenen dosya ya da klasör. */
export interface Attachment {
  path: string;
  name: string;
  dir: boolean;
}

/** Eklerin yolları metnin sonuna yazılır: dosyanın kendisi gönderilmez, Claude yolu alıp kendisi okur. */
export function withAttachments(text: string, attached: Attachment[]): string {
  if (attached.length === 0) return text;
  if (attached.length === 1) {
    const [a] = attached as [Attachment];
    return `${text}\n\n${a.dir ? 'Ekli klasör' : 'Ekli dosya'}: ${a.path}`;
  }
  return `${text}\n\nEkler:\n${attached.map(a => `- ${a.path}`).join('\n')}`;
}

/** Yeni görevin klasörü: ek, son klasörlerden birinin içindeyse o (en derin eşleşen); değilse null. */
export function folderFor(path: string, projects: { path: string }[]): string | null {
  const norm = (p: string) => p.replace(/[\\/]+$/, '').toLowerCase().replace(/\//g, '\\');
  const file = norm(path);
  const inside = projects.filter(p => file.startsWith(`${norm(p.path)}\\`)).sort((a, b) => b.path.length - a.path.length);
  return inside[0]?.path ?? null;
}
