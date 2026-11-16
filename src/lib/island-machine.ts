// Adanın kademeli açılma kuralları. Saf fonksiyon: zamanlayıcı ve pencere işi Island.svelte'de.
//
// 0 siyah şerit · 1 mini (üzerine gelince) · 2 ayrıntı (mininin ortasına tıklayınca)
// Üstteki simge çubuğu hangi bölümün açık olduğunu seçer (tab). Sekmeler `tabs.ts` kaydından gelir.
// K11: mini → ayrıntı iki aşaması yalnız AI Yöneticisi'nde. Sabit sekmeler (müzik, Bugün, Saat…)
// her zaman seviye 2'de, tek ekran olarak açılır; onlarda 'open' ve 'shrink' bir şey yapmaz. Sabit bir
// ekrandan AI sekmesine geçilince mini AI açılır.
// Dosya rafı bölümü ('shelf') yalnız rafta dosya varken ya da adaya dosya sürüklenirken görünür.
// Zody sohbeti ('zody', Yeni görünüm + özellik): yazarken ya da dinlerken fare çıksa da kapanmaz (Island.svelte).
// Ada kapalıyken gelen onay/soru adayı açmaz: çentik Zody'li bir bildirime dönüşür (bkz. notice.ts),
// üzerine gelince ya da tıklayınca ilgili bölüm açılır. Ada açıkken gelirse AI bölümüne geçilir.
// Yazma sahnesi (`write`): mini AI'nın yerinde açılır (✎ ya da kısayol). Fare uzaklaşınca kapanmaz
// (yazarken imleç kayabilir); Esc mini AI'ya döner, dışarı tıklama kapatır.

import { isFixed, type Tab } from './tabs';

export type { Tab };
export type Level = 0 | 1 | 2;

/** Bir sekmenin açıkken durduğu seviye: sabit sekme 2, AI bölümü mini (1) ya da `keep`. */
function levelFor(tab: Tab, keep: Level = 1): Level {
  return isFixed(tab) ? 2 : keep;
}

export interface IslandState {
  level: Level;
  tab: Tab;
  /** Yazma sahnesi açık (yalnız seviye 1, AI bölümü). Kapalıyken alan hiç yoktur. */
  write?: true;
}

export type IslandInput =
  /** `tab`: üzerine gelince açılacak bölüm (müzik çalıyorsa 'music'). */
  | { type: 'hover'; tab: Tab }
  /** Simge çubuğundan ya da yan alandan bölüm seçimi; seviye korunur (şeritteyse mini açılır). */
  | { type: 'tab'; tab: Tab }
  /** Mininin ortasına tıklama: ayrıntı. */
  | { type: 'open' }
  /** Ayrıntıdaki ‹ geri (Kaynaklar Mail/GitHub): miniye dön. Fare tekerleği adayı büyütüp küçültmez. */
  | { type: 'shrink' }
  | { type: 'leave' }
  | { type: 'outside' }
  | { type: 'escape' }
  /** Yeni onay isteği ya da soru geldi. */
  | { type: 'attention' }
  | { type: 'hide' }
  /** Genel kısayol: kapalıyken mini açar, açıkken küçültür. */
  | { type: 'toggle'; tab: Tab }
  /** ✎ ya da yazma kısayolu: yazma sahnesini açar; sahne açıkken kısayol adayı kapatır. */
  | { type: 'write' }
  /** Sahnedeki ‹ ya da gönderimden sonra: mini AI'ya dönüş. */
  | { type: 'back' };

export const initial: IslandState = { level: 0, tab: 'ai' };

const plain = ({ level, tab }: IslandState): IslandState => ({ level, tab });
const closed = (state: IslandState): IslandState => ({ ...plain(state), level: 0 });

export function step(state: IslandState, input: IslandInput): IslandState {
  if (state.write) return writing(state, input);
  switch (input.type) {
    case 'hover':
      return state.level === 0 ? { ...state, level: levelFor(input.tab), tab: input.tab } : state;
    case 'tab':
      // AI'nın kendi seviyesi korunur (mini ya da ayrıntı); sabit ekrandan ya da şeritten gelince mini.
      return { ...state, tab: input.tab, level: levelFor(input.tab, state.level === 0 || isFixed(state.tab) ? 1 : state.level) };
    case 'open':
      return state.level >= 1 ? { ...state, level: 2 } : state;
    case 'shrink':
      return state.level === 2 && !isFixed(state.tab) ? { ...state, level: 1 } : state;
    case 'leave':
    case 'outside':
    case 'escape':
    case 'hide':
      return closed(state);
    case 'attention':
      // Kapalıyken çentik bildirimi gösterilir; açıkken onay AI bölümünde görünür (seviye korunur:
      // sabit bir ekrandan gelince AI ayrıntısı, minideyken mini).
      return state.level === 0 ? state : { ...state, tab: 'ai' };
    case 'toggle':
      return state.level === 0 ? { ...state, level: levelFor(input.tab), tab: input.tab } : closed(state);
    case 'write':
      return { level: 1, tab: 'ai', write: true };
    case 'back':
      return state;
  }
}

function writing(state: IslandState, input: IslandInput): IslandState {
  switch (input.type) {
    case 'leave':
    case 'hover':
    case 'shrink':
    case 'attention':
      return state;
    case 'escape':
    case 'back':
      return { level: 1, tab: 'ai' };
    case 'tab':
      return { level: levelFor(input.tab), tab: input.tab };
    case 'open':
      return { level: 2, tab: 'ai' };
    case 'write':
    case 'toggle':
    case 'outside':
    case 'hide':
      return closed(state);
  }
}

/** Fare uzaklaşınca kapanmadan önceki bekleme; ayrıntı ekranında daha toleranslı. */
export function leaveDelay(level: Level): number {
  return level <= 1 ? 320 : 700;
}
