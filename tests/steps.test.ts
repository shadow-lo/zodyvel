import { describe, expect, it } from 'vitest';
import { defaultStep, stepVerb } from '../src/lib/steps';
import type { AgentStep } from '../src/lib/types';

const step = (id: number, kind: AgentStep['kind'], state: AgentStep['state'], label = ''): AgentStep => ({
  id,
  kind,
  state,
  label,
  target: '',
  at: 0,
  sub: false,
});

describe('adım metinleri', () => {
  it('tür ve duruma göre fiil', () => {
    expect(stepVerb(step(1, 'read', 'now'))).toBe('Okuyor');
    expect(stepVerb(step(1, 'read', 'done'))).toBe('Okudu');
    expect(stepVerb(step(1, 'edit', 'failed'))).toBe('Düzenledi · olmadı');
  });

  it('onay ve tur özeti gibi adımlarda Rust etiketi kullanılır', () => {
    expect(stepVerb(step(1, 'approval', 'done', 'Onaylandı'))).toBe('Onaylandı');
    expect(stepVerb(step(1, 'approval', 'now'))).toBe('Onay istedi');
    // Okuma gibi adımlarda etiket fiili ezmez.
    expect(stepVerb(step(1, 'read', 'done', 'x'))).toBe('Okudu');
  });

  it('varsayılan seçili adım: sürmekte olan, yoksa içeriği olan son adım', () => {
    expect(defaultStep([step(1, 'read', 'done'), step(2, 'bash', 'now'), step(3, 'plan', 'done')])?.id).toBe(2);
    expect(defaultStep([step(1, 'read', 'done'), step(2, 'edit', 'done'), step(3, 'done', 'done')])?.id).toBe(2);
    expect(defaultStep([step(1, 'prompt', 'done')])?.id).toBe(1);
    expect(defaultStep([])).toBeNull();
  });
});
