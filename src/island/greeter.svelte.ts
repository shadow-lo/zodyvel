// Peek ve karşılamanın durumu ile zamanlayıcıları. Island.svelte yalnız şunları çağırır: üzerine gelince
// `enter`, ayrılınca/dışarı tıklanınca `cancel`, boyut için `size`, açılışta `start`. Kararlar lib/peek.ts'de.
// Maskottan bağımsız: çizim parts/Peek.svelte ve parts/Welcome.svelte, maskot mascot/index.ts'den gelir.

import { inTauri } from '../lib/ipc';
import { HELLO_MS, PEEK, peekPlan, startupGreeting, type Greeting } from '../lib/peek';
import { data, saveSettings } from '../lib/store.svelte';
import { MINI, type Size } from './levels';

export class Greeter {
  /** Şeritten kafasını çıkarmış maskot; `wave`: el sallıyor. */
  peek = $state<{ wave: boolean; key: number } | null>(null);
  /** Açılış selamı ya da tanıtım turu. */
  greeting = $state<Greeting | null>(null);

  #open: () => void;
  #ready = false;
  #lastWaveAt: number | null = null;
  #hoverTimer: ReturnType<typeof setTimeout> | undefined;
  #holdTimer: ReturnType<typeof setTimeout> | undefined;
  #helloTimer: ReturnType<typeof setTimeout> | undefined;

  /** `open`: mini açılır (Island'da `pointer('hover')`). */
  constructor(open: () => void) {
    this.#open = open;
  }

  /** Ayarlar yüklendikten sonra bir kez: ilk açılışta tur, sonra kısa selam. */
  start(): void {
    // Tarayıcı önizlemesinde ayarlar kaydedilmez; tur yalnız adres #karsilama ile, selam #selam ile görünür.
    if (!inTauri) {
      const hash = location.hash;
      data.settings.island.welcomed = !hash.includes('karsilama');
      if (!hash.includes('selam') && !hash.includes('karsilama')) data.settings.island.greet = false;
    }
    this.#ready = true;
    const greeting = startupGreeting(data.settings.island);
    if (greeting === 'hello') this.#hello();
    else this.greeting = greeting;
  }

  /** Ayarlardan "Turu yeniden göster" (welcomed=false geldi): tur açılır. */
  sync(welcomed: boolean): void {
    if (this.#ready && !welcomed && this.greeting !== 'tour') {
      this.cancel();
      this.greeting = 'tour';
    }
  }

  /**
   * Şeride gelindi (ada kapalı). true: peek ya da tur bu girişi üstlendi; false: Island her zamanki gibi
   * gecikmeden sonra mini açar (tıklama kipinde açmaz).
   */
  enter(hoverDelayMs: number, notice: boolean): boolean {
    if (this.greeting === 'tour') return true;
    const island = data.settings.island;
    const click = island.reveal === 'click';
    if (this.greeting === 'hello') {
      // Selam sürerken gelindi: maskot zaten dışarıda, kısa bekleyip mini (tıklama kipinde tıklanınca).
      this.#endHello();
      this.peek = { wave: false, key: Date.now() };
      if (!click) this.#hold(PEEK_HELLO_HOLD_MS);
      return true;
    }
    const plan = peekPlan({
      enabled: island.peek,
      click,
      reduceMotion: island.reduceMotion,
      notice,
      lastWaveAt: this.#lastWaveAt,
      now: Date.now(),
    });
    if (!plan.peek) return false;
    clearTimeout(this.#hoverTimer);
    this.#hoverTimer = setTimeout(() => {
      if (plan.wave) this.#lastWaveAt = Date.now();
      this.peek = { wave: plan.wave, key: Date.now() };
      if (plan.holdMs !== null) this.#hold(plan.holdMs);
    }, hoverDelayMs);
    return true;
  }

  /** Peek'e tıklandı: beklemeden mini. */
  click(): void {
    this.#endHello();
    this.#finish();
  }

  /** Fare ayrıldı, dışarı tıklandı ya da ada başka yoldan açıldı: maskot geri saklanır. */
  cancel(): void {
    clearTimeout(this.#hoverTimer);
    clearTimeout(this.#holdTimer);
    this.peek = null;
  }

  finishTour(): void {
    this.greeting = null;
    if (!data.settings.island.welcomed) {
      const next = structuredClone($state.snapshot(data.settings));
      next.island.welcomed = true;
      void saveSettings(next);
    }
  }

  /** Peek, selam ya da tur sırasında adanın boyu; null: Island kendi boyunu kullanır. */
  size(level: number, notice: boolean): Size | null {
    if (this.greeting === 'tour') return MINI;
    if (level > 0 || notice) return null;
    return this.peek || this.greeting === 'hello' ? PEEK : null;
  }

  #hold(ms: number) {
    clearTimeout(this.#holdTimer);
    this.#holdTimer = setTimeout(() => this.#finish(), ms);
  }

  #finish() {
    this.cancel();
    this.#open();
  }

  #hello() {
    this.greeting = 'hello';
    this.#lastWaveAt = Date.now();
    this.#helloTimer = setTimeout(() => this.#endHello(), HELLO_MS);
  }

  #endHello() {
    clearTimeout(this.#helloTimer);
    if (this.greeting === 'hello') this.greeting = null;
  }
}

/** Açılış selamı sürerken üzerine gelinirse mini bu kadar sonra açılır. */
const PEEK_HELLO_HOLD_MS = 200;
