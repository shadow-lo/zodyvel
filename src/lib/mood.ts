// Zody'nin hâli: adada olan bitenden tek bir duruma indirgenir (saf fonksiyon, testli).

import type { AgentSession, AgentStatus, ZodyMood } from './types';

/** Bu süre boyunca hiçbir şey değişmezse Zody uyur; ada açılınca ya da bir olay gelince uyanır. */
export const SLEEP_MS = 10 * 60_000;

export interface MoodInput {
  approvals: number;
  errors: boolean;
  working: number;
  musicPlaying: boolean;
  done: boolean;
  asleep: boolean;
}

/** Öncelik: onay > hata > çalışıyor > müzik > bitti > boşta. Yalnız boşta ve "bitti" uykuya geçer. */
export function moodFor(i: MoodInput): ZodyMood {
  if (i.approvals > 0) return 'attention';
  if (i.errors) return 'error';
  if (i.working > 0) return 'working';
  if (i.musicPlaying) return 'music';
  if (i.asleep) return 'sleep';
  return i.done ? 'done' : 'idle';
}

/** "Çalışıyor" görünüp bu süre hiç olay gelmeyen oturum aslında bekliyordur (süren bir komut yoksa). */
export const QUIET_MS = 5 * 60_000;

/**
 * Görünen durum. Claude Code, kullanıcı turu yarıda kesince ya da soruyu kendi penceresinde kapatınca
 * olay göndermez; oturum "çalışıyor"da takılı kalmasın. Bekleyen ya da bitmiş oturum SLEEP_MS boyunca
 * sessizse gezegeni uyur. Süren bir adım (komut, düzenleme) varsa uzun sürse de çalışıyor sayılır.
 */
export function liveAgent(agent: AgentSession, now: number): AgentSession {
  const quiet = now - agent.lastSeen;
  const busy = agent.steps.at(-1)?.state === 'now';
  if (agent.status === 'running' && !busy && quiet > QUIET_MS) {
    return {
      ...agent,
      status: 'waiting_input',
      step: 'Bir süredir hareket yok · seni bekliyor olabilir',
      statusSince: agent.lastSeen,
      asleep: quiet > SLEEP_MS,
    };
  }
  const resting = agent.status === 'waiting_input' || agent.status === 'done';
  return resting && quiet > SLEEP_MS ? { ...agent, asleep: true } : agent;
}

/** Ajan kartlarındaki gezegenin ifadesi. */
export function agentMood(status: AgentStatus, asleep = false): ZodyMood {
  if (asleep) return 'sleep';
  switch (status) {
    case 'running':
      return 'working';
    case 'waiting_approval':
      return 'attention';
    case 'done':
      return 'done';
    case 'error':
      return 'error';
    case 'waiting_input':
      return 'idle';
  }
}

/** Mini AI'da solda büyük kartta kim duracak: seçilen (hâlâ varsa), yoksa en önemli oturum. */
export function pickLead<T extends { id: string; status: AgentStatus }>(agents: T[], selected: string | null): T | null {
  const chosen = selected ? agents.find(a => a.id === selected) : undefined;
  if (chosen) return chosen;
  const rank: Record<AgentStatus, number> = { waiting_approval: 0, error: 1, running: 2, waiting_input: 3, done: 4 };
  return [...agents].sort((a, b) => rank[a.status] - rank[b.status])[0] ?? null;
}
