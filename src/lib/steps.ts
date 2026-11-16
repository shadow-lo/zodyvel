// Adım geçmişinin Türkçe metinleri: tür + durum → fiil ("Okuyor" / "Okudu").

import type { AgentStep, StepKind } from './types';

const VERB: Record<StepKind, [now: string, done: string]> = {
  prompt: ['Yeni istek', 'Yeni istek'],
  read: ['Okuyor', 'Okudu'],
  edit: ['Düzenliyor', 'Düzenledi'],
  write: ['Yazıyor', 'Yazdı'],
  bash: ['Komut çalıştırıyor', 'Komut'],
  search: ['Arıyor', 'Aradı'],
  web: ["Web'de arıyor", "Web'de aradı"],
  agent: ['Alt ajan çalışıyor', 'Alt ajan'],
  plan: ['Planlıyor', 'Planı güncelledi'],
  question: ['Soru soruyor', 'Soru sordu'],
  approval: ['Onay istedi', 'Onay'],
  tool: ['Araç kullanıyor', 'Araç kullandı'],
  done: ['Tur tamamlandı', 'Tur tamamlandı'],
  error: ['Hata', 'Hata'],
};

/** Rust'ın verdiği etiket (onay kararı, tur özeti…) türün genel fiilinden önce gelir. */
const LABELLED: StepKind[] = ['prompt', 'approval', 'question', 'done', 'error'];

export function stepVerb(step: AgentStep): string {
  if (step.label && LABELLED.includes(step.kind)) return step.label;
  const verb = VERB[step.kind][step.state === 'now' ? 0 : 1];
  return step.state === 'failed' ? `${verb} · olmadı` : verb;
}

/** İçeriği gösterilebilen adımlar (dosya, fark, komut çıktısı). */
export const hasContent = (step: AgentStep) => ['read', 'edit', 'write', 'bash'].includes(step.kind);

/** Ayrıntıda varsayılan seçili adım: sürmekte olan, yoksa içeriği olan son adım, yoksa son adım. */
export function defaultStep(steps: AgentStep[]): AgentStep | null {
  const last = (test: (s: AgentStep) => boolean) => [...steps].reverse().find(test);
  return last(s => s.state === 'now') ?? last(hasContent) ?? steps[steps.length - 1] ?? null;
}
