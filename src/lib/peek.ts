// "Peek" ara durumu ve karşılama. Şeride gelince ada doğrudan mini açılmaz: önce maskot şeridin
// altından kafasını çıkarır (ilk gelişte el sallar), fare kalırsa kısa süre sonra mini açılır, tıklayınca
// hemen açılır, fare giderse geri saklanır. Uygulama açılırken kısa bir selam; ilk açılışta tanıtım turu.
// "Tıklayınca aç" kipinde (reveal: click) peek kendiliğinden mini açmaz: Zody tıklanana kadar bekler.
// Saf fonksiyonlar: zamanlama kararları burada, çizim parts/Peek.svelte ve parts/Welcome.svelte'de.

import type { Size } from '../island/levels';
import type { ZodyMood } from './types';

/** Peek sırasında adanın boyu: maskot ve kısa bir "Selam!" sığar. */
export const PEEK: Size = { w: 172, h: 44 };

/** El sallayan peek'te mini açılmadan önceki bekleme (selam bitmeden açılmasın). */
export const PEEK_WAVE_MS = 700;
/** Yakın zamanda selam verdiyse yalnız kafa görünür, mini çabuk açılır. */
export const PEEK_SHORT_MS = 200;
/** El sallama bu kadar süre içinde bir kez (her üzerine gelişte selam bıkkınlık verir). */
export const WAVE_COOLDOWN_MS = 5 * 60_000;
/** Açılış selamının ekranda kalma süresi. */
export const HELLO_MS = 2200;

export interface PeekInput {
  /** Ayar: üzerine gelince önce selam. */
  enabled: boolean;
  /** Ayar: ada yalnız tıklayınca açılır; peek her zaman görünür, kendiliğinden mini açılmaz. */
  click: boolean;
  reduceMotion: boolean;
  /** Şerit bir bildirime dönüşmüşse (onay, bitti…) peek yok: üzerine gelince ilgili bölüm açılır. */
  notice: boolean;
  /** Son el sallama anı (ms); hiç olmadıysa null. */
  lastWaveAt: number | null;
  now: number;
}

/** `holdMs`: mini bu kadar sonra açılır; null: tıklanana kadar açılmaz. */
export type PeekPlan = { peek: false } | { peek: true; wave: boolean; holdMs: number | null };

export function peekPlan(input: PeekInput): PeekPlan {
  // Bildirim varken şeridin kendisi tıklanabilir haberdir; peek ona yer açmaz.
  if (input.notice) return { peek: false };
  // Tıklama kipinde tıklanacak bir şey gerekir: selam ayarı ya da hareketi azalt ne olursa olsun Zody çıkar.
  if (!input.click && (!input.enabled || input.reduceMotion)) return { peek: false };
  const wave = !input.reduceMotion && (input.lastWaveAt === null || input.now - input.lastWaveAt >= WAVE_COOLDOWN_MS);
  if (input.click) return { peek: true, wave, holdMs: null };
  return { peek: true, wave, holdMs: wave ? PEEK_WAVE_MS : PEEK_SHORT_MS };
}

export type Greeting = 'tour' | 'hello';

/**
 * Uygulama açılınca: ilk kez (ya da tur ayarlardan yeniden istendiyse) tanıtım turu; sonra kısa selam.
 * (Görünüm sorusu K2 ile kalktı: tek görünüm var.)
 */
export function startupGreeting(island: { welcomed: boolean; greet: boolean; reduceMotion: boolean }): Greeting | null {
  if (!island.welcomed) return 'tour';
  return island.greet && !island.reduceMotion ? 'hello' : null;
}

export interface TourStep {
  title: string;
  text: string;
  /** Zody'nin bu adımdaki ruh hâli. */
  mood: ZodyMood;
}

/** Turda anılacak özellikler: kapalı olan hiç anılmaz (kapalı özellik adada yer kaplamaz). */
export interface TourFeatures {
  shelf: boolean;
  focus: boolean;
}

/**
 * Tanıtım turu: dört kısa adım, açık yardımcı özellik varsa araya beşincisi. Kısayollar kullanıcının
 * ayarlarından gelir (boşsa o cümle atlanır).
 */
export function tourSteps(keys: { toggle: string; approve: string; deny: string }, features: TourFeatures = { shelf: true, focus: true }): TourStep[] {
  const decide =
    keys.approve && keys.deny
      ? ` ${keys.approve} onaylar, ${keys.deny} reddeder.`
      : keys.approve
        ? ` ${keys.approve} onaylar.`
        : '';
  const toggle = keys.toggle ? `${keys.toggle} adayı açıp kapatır. ` : '';
  const helpers = [
    features.shelf && 'Dosyayı şeride sürükle: rafta durur, başka uygulamaya bırakırsın.',
    features.focus && 'Zamanlayıcıdaki Odak düğmesi 25 dk çalışma, 5 dk mola kurar.',
  ].filter(Boolean);
  const extra: TourStep[] = helpers.length
    ? [{ title: 'Küçük yardımcılar', text: `${helpers.join(' ')} Hepsi Ayarlar → Özellikler'den kapanır.`, mood: 'working' }]
    : [];
  return [
    {
      title: 'Selam, ben Zody!',
      text: 'Ekranının üstündeki bu ince şeritte yaşıyorum. AI ajanlarını, müziği ve gününü buradan birlikte izleriz.',
      mood: 'idle',
    },
    {
      title: 'Üzerime gel',
      text: 'Fareyi şeride getirince kafamı çıkarırım, biraz bekleyince küçük görünüm açılır. Ortaya tıkla ya da tekerleği çevir: ayrıntı.',
      mood: 'idle',
    },
    {
      title: 'Onaylar şeritte',
      text: `Claude Code ya da Codex izin isteyince şerit küçük bir habere döner; üzerine gelip karar verirsin.${decide}`,
      mood: 'attention',
    },
    ...extra,
    {
      title: 'Hazırız',
      text: `${toggle}Tema, modüller ve bağlantılar tepsi simgesinden Ayarlar'da; aradığını Ctrl+K ile bulursun. Bu turu oradan yeniden açabilirsin.`,
      mood: 'done',
    },
  ];
}
