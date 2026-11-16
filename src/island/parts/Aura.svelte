<script lang="ts" module>
  // Dalga yolu bir kez üretilir (periyot 200: %50 kaydırınca dikiş görünmez). Bütün örnekler paylaşır.
  function wave(top: number): string {
    let d = `M0 ${top}`;
    for (let x = 0; x < 1600; x += 200) d += ` Q${x + 50} ${top - 22} ${x + 100} ${top} T${x + 200} ${top}`;
    return `${d} V200 H0Z`;
  }
  const BACK = wave(70);
  const FRONT = wave(105);
  let uid = 0;
</script>

<script lang="ts">
  // Dalgalı ışık (Faz 1 kararı T4, md/faz1-tasarim-kararlari.md §2.2): adanın altından durum renginde yükselen
  // ışık. Altta yumuşak "bloom", üstünde iki dalga katmanı. Belirince bir kez yükselir (~900 ms); yalnız onay
  // beklerken (live) iki dalga ters yönlerde yavaşça kayar (translateX, compositor). Başka durumda durağan.
  // Kartlar yarı saydam cam olduğu için ışık kartların altından görünür. Ada dinlenmedeyken (.resting) ya da
  // "hareketi azalt" açıkken kayma durur.
  interface Props {
    /** Işığın rengi (durum rengi). */
    color: string;
    /** 0..1: onay 1, bitti 0.6, çalışıyor 0.42. */
    strength?: number;
    /** Yalnız onay beklerken true: dalgalar yavaşça kayar. */
    live?: boolean;
  }
  let { color, strength = 1, live = false }: Props = $props();
  const id = `aw${++uid}`;
</script>

<div class="aura" class:live style:--a={color} style:--s={strength} aria-hidden="true">
  <div class="bloom"></div>
  {#each [{ cls: 'back', d: BACK, op: 0.55 }, { cls: 'front', d: FRONT, op: 0.8 }] as w (w.cls)}
    <div class="w {w.cls}">
      <svg viewBox="0 0 1600 200" preserveAspectRatio="none" style:opacity={w.op}>
        <defs>
          <linearGradient id="{id}{w.cls}" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color={color} stop-opacity="0" />
            <stop offset=".35" stop-color={color} stop-opacity=".18" />
            <stop offset="1" stop-color={color} stop-opacity=".5" />
          </linearGradient>
        </defs>
        <path d={w.d} fill="url(#{id}{w.cls})" />
      </svg>
    </div>
  {/each}
</div>

<style>
  .aura {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: calc(62% * var(--s));
    pointer-events: none;
    overflow: hidden;
    animation: aura-in var(--t-slow) var(--ease) both;
  }
  .bloom {
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse 75% 95% at 50% 115%, color-mix(in srgb, var(--a) 70%, transparent), transparent 72%);
    opacity: calc(0.25 + 0.4 * var(--s));
  }
  .w {
    position: absolute;
    left: 0;
    bottom: 0;
    width: 200%;
    height: 100%;
  }
  .w svg {
    display: block;
    width: 100%;
    height: 100%;
  }
  .live .w {
    will-change: transform;
  }
  .live .back {
    animation: drift 16s linear infinite;
  }
  .live .front {
    animation: drift 11s linear infinite reverse;
  }
  @keyframes aura-in {
    from {
      transform: translateY(45%);
      opacity: 0;
    }
  }
  @keyframes drift {
    from {
      transform: translateX(0);
    }
    to {
      transform: translateX(-50%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .aura,
    .live .w {
      animation: none;
    }
  }
</style>
