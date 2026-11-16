// Zody'nin kişiliği: hangi yüz, hangi tepki, nereye bakar (saf fonksiyonlar, testli).
// Çizim ve zamanlayıcılar Zody.svelte'de; eskiz design/zody-kisilik.html.

import type { ZodyMood } from '../lib/types';

/** Kısa süreli yüz ifadeleri: tık → küskün, üç tık → sersem, selam → göz kırpma, hata → ay. */
export type Expression = 'pout' | 'dizzy' | 'wink' | 'ouch';
/** Çizgiyle çizilen yüzler (hap gözler dışındakiler). */
export type Face = Expression | 'done' | 'error' | 'music' | 'sleep' | 'smile' | 'o';
/** Tek seferlik gövde hareketleri. */
export type Reaction = 'pop' | 'jump' | 'shake' | 'squash' | 'spin' | 'selam';

/** Bu durumlarda gözler hap (göz kırpar, büyür, takip eder); diğerlerinde yüz çizgidir. */
const PILL: ReadonlySet<ZodyMood> = new Set(['idle', 'working', 'attention']);

export interface Look {
  /** Hap gözler görünür mü (göz kırpma, büyüme bunlarda çalışır). */
  pill: boolean;
  /** Çizgi yüz; null: yalnız hap gözler. */
  face: Face | null;
  /** Yanaklar kızarır. */
  cheek: boolean;
}

export function lookFor(mood: ZodyMood, exp: Expression | null, hover: boolean): Look {
  if (exp) return { pill: exp === 'wink', face: exp, cheek: exp === 'pout' || exp === 'wink' || hover };
  if (PILL.has(mood)) {
    const face = hover && mood === 'idle' ? 'smile' : mood === 'attention' ? 'o' : null;
    return { pill: true, face, cheek: hover };
  }
  return { pill: false, face: mood as Face, cheek: hover || mood === 'done' || mood === 'music' };
}

export interface Play {
  reaction: Reaction;
  /** Tepkinin süresi (ms): bitince sınıf kalkar. */
  ms: number;
  exp?: Expression;
  /** İfade ne zaman başlar ve ne kadar sürer (ms). */
  expFrom?: number;
  expMs?: number;
}

export const PLAYS = {
  pop: { reaction: 'pop', ms: 450 },
  jump: { reaction: 'jump', ms: 1150 },
  shake: { reaction: 'shake', ms: 700, exp: 'ouch', expFrom: 0, expMs: 900 },
  squash: { reaction: 'squash', ms: 550, exp: 'pout', expFrom: 0, expMs: 1200 },
  spin: { reaction: 'spin', ms: 1000, exp: 'dizzy', expFrom: 0, expMs: 2000 },
  selam: { reaction: 'selam', ms: 1600, exp: 'wink', expFrom: 550, expMs: 1000 },
} satisfies Record<Reaction, Play>;

/**
 * Durum değişince: bitti → sevinç zıplaması, hata → sarsılma, diğerleri kısa yaylanma.
 * İlk çizimde (prev yok) yalnız bitti ve hata kendini gösterir (çentik bildirimi gibi).
 */
export function reactionFor(prev: ZodyMood | undefined, next: ZodyMood): Play | null {
  if (prev === next) return null;
  if (next === 'done') return PLAYS.jump;
  if (next === 'error') return PLAYS.shake;
  return prev === undefined ? null : PLAYS.pop;
}

/** Üç tık bu süre içinde gelirse Zody başı dönene kadar döner. */
export const TRIPLE_MS = 650;

/** Tık zamanlarını günceller; üçüncü hızlı tıkta sayaç sıfırlanır ve 'spin' döner. */
export function tapReaction(taps: number[], now: number): { taps: number[]; play: Play } {
  const recent = [...taps.filter(t => now - t < TRIPLE_MS), now];
  return recent.length >= 3 ? { taps: [], play: PLAYS.spin } : { taps: recent, play: PLAYS.squash };
}

/** İmleç bu uzaklıkta (px) ve ötesinde gözler en uca bakar. */
export const LOOK_REACH = 220;

/** Merkeze göre imleç farkından bakış yönü: her eksen -1…1, uzaklıkla yumuşakça artar. */
export function gaze(dx: number, dy: number): { x: number; y: number } {
  const d = Math.hypot(dx, dy);
  if (d < 1) return { x: 0, y: 0 };
  const k = Math.min(1, d / LOOK_REACH);
  return { x: (dx / d) * k, y: (dy / d) * k };
}
