// Zody beyni (Faz 12) için saf mantık: model seçimi, durum metinleri, cevabın sohbet cevabına çevrilmesi.
// Beyin bu fazda hiçbir eylem yapmaz: cevap yalnız metin (adım yok), öneriler "şunu yapmamı ister misin" diye gelir.

import { normalize } from './commands';
import type { Reply } from './chat';

export type ModelPref = 'auto' | 'fast' | 'long';
export type ModelId = 'fast' | 'long';

export const MODEL_LABELS: Record<ModelPref, { label: string; note: string }> = {
  auto: { label: 'Otomatik', note: 'Kısa sorular Haiku, uzun işler Sonnet' },
  fast: { label: 'Hızlı', note: 'Haiku: kısa ve çabuk cevaplar' },
  long: { label: 'Uzun işler', note: 'Sonnet: uzun, ayrıntılı cevaplar' },
};

/** Bu uzunluktan ya da şu sözcüklerden biri varsa iş "uzun" sayılır. */
const LONG_CHARS = 160;
const LONG_WORDS = ['yaz', 'planla', 'analiz', 'özetle', 'karşılaştır', 'açıkla', 'tasarla', 'düzelt', 'kod', 'çevir', 'fikir'];

export function pickModel(text: string, pref: ModelPref | undefined): ModelId {
  if (pref === 'fast' || pref === 'long') return pref;
  if (text.length > LONG_CHARS) return 'long';
  const words = normalize(text).split(' ');
  return words.some(w => LONG_WORDS.some(l => w === l || w.startsWith(l))) ? 'long' : 'fast';
}

/** Rust'tan gelen `brain_status` (brain/mod.rs Durumu). */
export interface BrainStatus {
  enabled: boolean;
  installed: boolean;
  version: string | null;
  running: boolean;
  name: string;
}

/** `brain_check_login`: "yes" | "no" | "unknown:<neden>". */
export type LoginState = 'yes' | 'no' | 'unknown' | 'untested';

export function parseLogin(raw: string | undefined): { state: LoginState; detail: string } {
  if (raw === 'yes') return { state: 'yes', detail: '' };
  if (raw === 'no') return { state: 'no', detail: '' };
  if (raw?.startsWith('unknown:')) return { state: 'unknown', detail: raw.slice(8) };
  return { state: 'unknown', detail: '' };
}

export type Tone = 'ok' | 'warn' | 'bad' | 'idle';
export interface Line {
  text: string;
  tone: Tone;
}

export function installLine(s: BrainStatus | null): Line {
  if (!s) return { text: 'Bakılıyor…', tone: 'idle' };
  if (!s.installed) return { text: 'Claude Code kurulu değil. Kurulum: claude.com/claude-code', tone: 'bad' };
  return { text: s.version ? `Kurulu (${s.version})` : 'Kurulu', tone: 'ok' };
}

export function loginLine(s: BrainStatus | null, login: { state: LoginState; detail: string }): Line {
  if (!s?.installed) return { text: 'Önce Claude Code kurulmalı.', tone: 'idle' };
  switch (login.state) {
    case 'yes':
      return { text: 'Giriş yapılmış', tone: 'ok' };
    case 'no':
      return { text: 'Giriş yapılmamış: bir terminalde "claude" yazıp /login ile giriş yap.', tone: 'bad' };
    case 'unknown':
      return { text: login.detail ? `Anlaşılamadı: ${login.detail}` : 'Anlaşılamadı, yeniden dene.', tone: 'warn' };
    default:
      return { text: 'Denenmedi. Deneme küçük bir istek gönderir.', tone: 'idle' };
  }
}

const MAX_ANSWER = 2000;

/** Beynin cevabı sohbete düşer: yalnız metin, adım yok (bu fazda beyin eylem yapmaz). */
export function answerReply(answer: string): Reply {
  const text = answer.trim();
  return { text: text.length > MAX_ANSWER ? `${text.slice(0, MAX_ANSWER).trimEnd()}…` : text || 'Bir şey diyemedim.', tone: 'info', mood: 'happy' };
}

export function failureReply(message: string): Reply {
  return { text: `Asistana ulaşamadım: ${message}`, tone: 'warn', mood: 'confused' };
}
