// "Onaya git" ve "Terminale git" kısayollarının hedefi (saf fonksiyonlar; kısayolu Island.svelte işler).
//
// Onaya git: adada görünen ilk istek, yoksa katlanmış ilk istek (katlama kalkar). Terminale git: onay
// bekleyen oturum, yoksa seni bekleyen (en son bekleyeni), yoksa hata veren, yoksa seçili, o da yoksa en son
// olay gelen oturum. Penceresi bilinmeyen oturum (`app` boş) atlanır: öne getirilecek bir şey yok.

import type { AgentSession, ApprovalRequest } from './types';

/** Açılacak istek ve katlanmışsa açılması gerekip gerekmediği; bekleyen istek yoksa null. */
export function approvalJump(approvals: ApprovalRequest[], folded: ApprovalRequest[]): { id: string; unfold: boolean } | null {
  const shown = approvals[0];
  if (shown) return { id: shown.id, unfold: false };
  const hidden = folded[0];
  return hidden ? { id: hidden.id, unfold: true } : null;
}

/** Penceresi öne getirilecek oturum; uygun oturum yoksa null. */
export function terminalTarget(agents: AgentSession[], requests: ApprovalRequest[], selected: string | null): AgentSession | null {
  const reachable = agents.filter(a => a.app);
  const asking = requests.map(r => reachable.find(a => a.id === r.sessionId)).find(a => a !== undefined);
  if (asking) return asking;
  const latest = (status: AgentSession['status']) =>
    reachable.filter(a => a.status === status).sort((a, b) => b.statusSince - a.statusSince)[0];
  return (
    latest('waiting_approval') ??
    latest('waiting_input') ??
    latest('error') ??
    reachable.find(a => a.id === selected) ??
    [...reachable].sort((a, b) => b.lastSeen - a.lastSeen)[0] ??
    null
  );
}
