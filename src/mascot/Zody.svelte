<script lang="ts">
  // Zody: ekran çerçeveli, iki gözlü maskot (Faz 1 kararı T2, çizim design/goz-zody.js biçim "ekran";
  // md/faz1-tasarim-kararlari.md §2.1). Çerçeve aracın renginde (`body`), gözler durum renginde parlayan dik
  // kapsüller; göz biçimi durumu söyler (çalışırken kısık, onayda iri, bitince mutlu yay, hatada çarpı, uykuda çizgi).
  // Hareket yalnız transform/opacity ve yalnız olayla: fare gelince göz kırpma, tıklayınca zıplama, imleci izleyen
  // gözler, durum değişince göz kırpma + tepki (persona.ts). Tek sürekli döngü çalışırken gözlerin steps() ile
  // taraması. Boşta hiçbir şey kıpırdamaz; hapta (alive=false) zamanlayıcı ya da dinleyici kurulmaz.
  import { onMount, tick, untrack } from 'svelte';
  import type { ZodyMood } from '../lib/types';
  import { gaze, PLAYS, reactionFor, tapReaction, type Expression, type Play, type Reaction } from './persona';

  interface Props {
    mood?: ZodyMood;
    size?: number;
    /** Çerçeve rengi: her araç kendi renginde. Boşsa temanın vurgu rengi (Zody moru). */
    body?: string;
    /** Yalnız ada açıkken canlı; seviye 0'da Zody durağandır. */
    alive?: boolean;
    /** Tıklamaya tepki verir (zıplama, üç tıkta dönme). Tıklanabilir bir kartın içindeyse kapalı kalmalı. */
    interactive?: boolean;
    /** Belirince bir kez selam verir (peek, karşılama). */
    greet?: boolean;
    /** Yalnız gözler: ekran çerçevesi, parlama ve işaret çizilmez (çentikteki "her zaman açık" Zody, Faz 4). */
    bare?: boolean;
  }

  let { mood = 'idle', size = 22, body, alive = false, interactive = false, greet = false, bare = false }: Props = $props();

  /** Bu boydan küçük Zody imleci izlemez (çentik, liste satırları): fark edilmez. */
  const TRACK_MIN = 30;
  /** Göz takibi saniyede en çok 10 kez güncellenir. */
  const TRACK_EVERY_MS = 100;
  /** Bakış bundan az değişirse güncellenmez (eksen başına, -1…1 ölçeğinde). */
  const GAZE_STEP = 0.12;
  /** Sağ üstteki işaret bu boydan küçükte çizilmez (16 px liste Zody'si sade kalır). */
  const BADGE_MIN = 22;

  /** Göz renkleri (durum). Müzik eskizde yoktu: mercan, mutlu yay. */
  const EYE: Record<ZodyMood, string> = {
    idle: 'var(--mint)',
    working: 'var(--teal)',
    attention: 'var(--amber)',
    done: 'var(--green)',
    error: 'var(--red)',
    music: 'var(--coral)',
    sleep: 'var(--text-3)',
  };
  const SPARKS = [22, 67, 112, 157, 202, 247, 292, 337];
  const L = 37;
  const R = 63;
  const CY = 50;

  // Her örneğin kendi degrade kimlikleri (aynı sayfada çok Zody var).
  const uid = `z${Math.random().toString(36).slice(2, 8)}`;

  let el: HTMLDivElement;
  let hover = $state(false);
  let exp = $state<Expression | null>(null);
  let act = $state<Reaction | null>(null);
  let blinkKey = $state(0);
  let gx = $state(0);
  let gy = $state(0);

  const eye = $derived(EYE[mood]);
  const frame = $derived(body ?? 'var(--accent)');
  /** Gözün biçimi: kısa süreli ifade durumun önüne geçer. */
  type Shape = 'pill' | 'arc' | 'cross' | 'line' | 'pout' | 'dizzy' | 'wink';
  const shape = $derived<Shape>(
    exp === 'ouch'
      ? 'cross'
      : exp
        ? exp
        : mood === 'done' || mood === 'music'
          ? 'arc'
          : mood === 'error'
            ? 'cross'
            : mood === 'sleep'
              ? 'line'
              : 'pill',
  );
  // Kapsül boyları (eskizdeki ölçüler): çalışırken kısık, onayda iri.
  const pw = $derived(mood === 'attention' ? 14 : 13);
  const ph = $derived(mood === 'working' ? 11 : mood === 'attention' ? 24 : 22);
  const tracking = $derived(alive && size >= TRACK_MIN && mood !== 'working' && mood !== 'sleep');
  const lx = $derived(tracking ? gx * 4 : 0);
  const ly = $derived(tracking ? gy * 3 : 0);

  /** Hareket serbest mi: canlı, sistemde ve ayarlarda "hareketi azalt" kapalı, ada dinlenmede değil. */
  function moving(): boolean {
    if (!alive || !el) return false;
    if (el.closest('.reduce-motion, .resting')) return false;
    return !matchMedia('(prefers-reduced-motion: reduce)').matches;
  }

  function blink() {
    if (moving()) blinkKey++;
  }

  let actTimer: ReturnType<typeof setTimeout> | undefined;
  let expTimer: ReturnType<typeof setTimeout> | undefined;

  async function play(p: Play) {
    if (!moving()) return;
    clearTimeout(actTimer);
    clearTimeout(expTimer);
    exp = null;
    // Aynı tepki üst üste gelirse animasyon baştan başlasın: sınıfı kaldır, stili hesaplat, geri koy.
    act = null;
    await tick();
    void el?.offsetWidth;
    act = p.reaction;
    actTimer = setTimeout(() => (act = null), p.ms);
    const e = p.exp;
    if (e) {
      expTimer = setTimeout(() => {
        exp = e;
        expTimer = setTimeout(() => (exp = null), p.expMs ?? p.ms);
      }, p.expFrom ?? 0);
    }
  }

  // Durum değişince bir göz kırpma ve tepki (bitti → zıplama, hata → sarsılma, diğerleri yaylanma).
  let prev: ZodyMood | undefined;
  $effect(() => {
    const next = mood;
    const p = reactionFor(prev, next);
    const changed = prev !== undefined && prev !== next;
    prev = next;
    untrack(() => {
      if (changed) blink();
      if (p) void play(p);
    });
  });

  onMount(() => {
    if (greet) void play(PLAYS.selam);
    return () => {
      clearTimeout(actTimer);
      clearTimeout(expTimer);
    };
  });

  // Göz takibi: fare adanın üzerindeyken (pencere yalnız oradayken olay alır) gözler imlece kayar.
  $effect(() => {
    if (!alive || size < TRACK_MIN) return;
    let last = 0;
    const move = (e: PointerEvent) => {
      const now = performance.now();
      if (now - last < TRACK_EVERY_MS || !moving()) return;
      last = now;
      const r = el.getBoundingClientRect();
      const g = gaze(e.clientX - (r.left + r.width / 2), e.clientY - (r.top + r.height / 2));
      // Küçük kıpırdamalar bakışı değiştirmez: gereksiz çizim yok.
      if (Math.abs(g.x - gx) < GAZE_STEP && Math.abs(g.y - gy) < GAZE_STEP) return;
      gx = g.x;
      gy = g.y;
    };
    const leave = () => (gx = gy = 0);
    window.addEventListener('pointermove', move, { passive: true });
    document.documentElement.addEventListener('pointerleave', leave);
    return () => {
      window.removeEventListener('pointermove', move);
      document.documentElement.removeEventListener('pointerleave', leave);
      gx = gy = 0;
    };
  });

  // Üzerine gelince bir göz kırpma ve gözler biraz büyür.
  $effect(() => {
    if (!alive) return;
    const enter = () => {
      hover = true;
      blink();
    };
    const leave = () => (hover = false);
    el.addEventListener('pointerenter', enter);
    el.addEventListener('pointerleave', leave);
    return () => {
      el.removeEventListener('pointerenter', enter);
      el.removeEventListener('pointerleave', leave);
      hover = false;
    };
  });

  // Tık: zıplar; hızlı üç tık: döner, başı döner.
  $effect(() => {
    if (!interactive) return;
    let taps: number[] = [];
    const click = () => {
      const r = tapReaction(taps, performance.now());
      taps = r.taps;
      void play(r.play.reaction === 'spin' ? r.play : PLAYS.pop);
      blink();
    };
    el.addEventListener('click', click);
    return () => el.removeEventListener('click', click);
  });
</script>

{#snippet pill(cx: number, w: number, h: number)}
  <rect x={cx - w / 2} y={CY - h / 2} width={w} height={h} rx={Math.min(w, h) / 2} fill="url(#{uid}f)" />
  <rect x={cx - w * 0.22} y={CY - h / 2 + h * 0.13} width={w * 0.2} height={Math.max(2, h * 0.26)} rx={w * 0.1} fill="#fff" opacity=".6" />
{/snippet}

{#snippet one(cx: number, right: boolean)}
  {#if shape !== 'line'}<ellipse cx={cx} cy={CY} rx="15" ry="19" fill="url(#{uid}h)" />{/if}
  {#if shape === 'pill' || (shape === 'wink' && !right)}
    {@render pill(cx, pw, ph)}
  {:else if shape === 'arc' || shape === 'wink'}
    <path d="M{cx - 7} {CY + 3} Q{cx} {CY - 9} {cx + 7} {CY + 3}" fill="none" stroke="url(#{uid}f)" stroke-width="6" stroke-linecap="round" />
  {:else if shape === 'cross'}
    <path d="M{cx - 5.5} {CY - 5.5}L{cx + 5.5} {CY + 5.5}M{cx + 5.5} {CY - 5.5}L{cx - 5.5} {CY + 5.5}" stroke={eye} stroke-width="5" stroke-linecap="round" />
  {:else if shape === 'pout'}
    <path d={right ? `M${cx + 5} ${CY - 6}L${cx - 4} ${CY}L${cx + 5} ${CY + 6}` : `M${cx - 5} ${CY - 6}L${cx + 4} ${CY}L${cx - 5} ${CY + 6}`} fill="none" stroke={eye} stroke-width="4.5" stroke-linecap="round" stroke-linejoin="round" />
  {:else if shape === 'dizzy'}
    <circle cx={cx} cy={CY} r="6.5" fill="none" stroke={eye} stroke-width="3.2" stroke-dasharray="30 11" />
    <circle cx={cx} cy={CY} r="1.8" fill={eye} />
  {:else}
    <rect x={cx - 6.5} y={CY - 2} width="13" height="4" rx="2" fill={eye} />
  {/if}
{/snippet}

<div
  bind:this={el}
  class="zody {mood} {act ? `act-${act}` : ''}"
  class:alive
  class:hover
  class:tap={interactive}
  style:--s="{size}px"
  style:--zb={frame}
  style:--ze={eye}
  role="img"
  aria-label="Zody"
>
  <div class="layer jump">
    <svg class="layer" viewBox="0 0 100 100" aria-hidden="true">
      <defs>
        <radialGradient id="{uid}h"><stop offset="0" stop-color={eye} stop-opacity=".55" /><stop offset=".55" stop-color={eye} stop-opacity=".16" /><stop offset="1" stop-color={eye} stop-opacity="0" /></radialGradient>
        <linearGradient id="{uid}f" x1="0" y1="0" x2="0" y2="1"><stop offset="0" class="eye-top" /><stop offset="1" stop-color={eye} /></linearGradient>
        <linearGradient id="{uid}b" x1="0" y1="0" x2="0" y2="1"><stop offset="0" class="frame-top" /><stop offset="1" class="frame-bottom" /></linearGradient>
        <radialGradient id="{uid}s" cx=".5" cy=".62" r=".7"><stop offset="0" stop-color={eye} stop-opacity={mood === 'sleep' ? 0 : 0.2} /><stop offset="1" stop-color={eye} stop-opacity="0" /></radialGradient>
      </defs>
      {#if !bare}<g class="frame">
        <rect x="7" y="15" width="86" height="70" rx="15" fill="url(#{uid}b)" />
        <rect x="7.6" y="15.6" width="84.8" height="68.8" rx="14.4" fill="none" stroke="#fff" stroke-opacity=".22" stroke-width="1.2" />
        <rect x="13.5" y="21.5" width="73" height="57" rx="9.5" fill="#040507" />
        <rect x="13.5" y="21.5" width="73" height="57" rx="9.5" fill="url(#{uid}s)" />
      </g>{/if}
      <g class="look" style:transform="translate({lx.toFixed(1)}px, {ly.toFixed(1)}px)">
        {#key blinkKey}
          <g class="eyes" class:blink={blinkKey > 0}>
            {@render one(L, false)}
            {@render one(R, true)}
          </g>
        {/key}
      </g>
      {#if !bare}<path class="glare" d="M18 26 L40 26 L26 46 L18 46 Z" />{/if}
    </svg>
  </div>

  <!-- Sağ üstteki küçük işaret: onayda ünlem, bitince yıldızcık, müzikte nota, uykuda "z". -->
  {#if size >= BADGE_MIN && !bare}
    {#if mood === 'attention'}
      <svg class="layer mark" viewBox="0 0 100 100" aria-hidden="true">
        <circle cx="90" cy="12" r="7.5" fill={EYE.attention} />
        <rect x="88.7" y="7.4" width="2.6" height="5.6" rx="1.2" fill="#2a1d05" />
        <circle cx="90" cy="15.4" r="1.4" fill="#2a1d05" />
      </svg>
    {:else if mood === 'done'}
      <svg class="layer mark" viewBox="0 0 100 100" aria-hidden="true">
        <path d="M90 4.5Q91.1 10.9 97.5 12Q91.1 13.1 90 19.5Q88.9 13.1 82.5 12Q88.9 10.9 90 4.5Z" fill={EYE.done} />
      </svg>
    {:else if mood === 'music'}
      <svg class="layer mark" viewBox="0 0 100 100" aria-hidden="true">
        <ellipse cx="87" cy="16" rx="3.6" ry="2.8" transform="rotate(-20 87 16)" fill={EYE.music} />
        <rect x="89.6" y="3" width="1.8" height="13" fill={EYE.music} />
        <path d="M91.4 3q5 1.8 4.2 6.8q-1-3.2-4.2-3.5Z" fill={EYE.music} />
      </svg>
    {:else if mood === 'sleep'}
      <svg class="layer mark" viewBox="0 0 100 100" aria-hidden="true">
        <text x="85" y="16" font-size="14" font-weight="700" font-family="Segoe UI, system-ui, sans-serif" fill={EYE.sleep}>z</text>
      </svg>
    {/if}
  {/if}

  {#if act === 'jump'}
    <div class="layer fx" aria-hidden="true">
      {#each SPARKS as a (a)}<i style:--a="{a}deg"></i>{/each}
    </div>
  {/if}
</div>

<style>
  .zody {
    position: relative;
    width: var(--s);
    height: var(--s);
    flex: none;
  }
  .tap {
    cursor: pointer;
  }
  .layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .jump {
    transform-origin: 50% 85%;
  }
  /* Çerçeve degradesi aracın renginden: üstte biraz açık, altta koyu (goz-zody.js mix(+.18 / −.38)). */
  .frame-top {
    stop-color: color-mix(in srgb, var(--zb) 82%, #fff);
  }
  .frame-bottom {
    stop-color: color-mix(in srgb, var(--zb) 62%, #000);
  }
  .eye-top {
    stop-color: color-mix(in srgb, var(--ze) 30%, #fff);
  }
  .sleep .frame {
    opacity: 0.82;
  }
  .glare {
    fill: #fff;
    opacity: 0.045;
  }
  .look {
    transition: transform 240ms steps(3);
  }
  .eyes {
    transform-box: fill-box;
    transform-origin: center;
    transition: transform 160ms var(--ease);
  }
  .hover:is(.idle, .attention) .eyes {
    transform: scale(1.06, 1.1);
  }
  .fx {
    font-size: calc(var(--s) / 10);
    pointer-events: none;
  }
  .fx i {
    position: absolute;
    left: calc(50% - 0.18em);
    top: calc(50% - 0.18em);
    width: 0.36em;
    height: 0.36em;
    border-radius: 50%;
    background: var(--ze);
    opacity: 0;
  }
  .fx i:nth-child(even) {
    left: calc(50% - 0.13em);
    top: calc(50% - 0.13em);
    width: 0.26em;
    height: 0.26em;
    background: var(--zb);
  }

  @media (prefers-reduced-motion: no-preference) {
    /* Tek sürekli döngü: çalışırken gözler adım adım sağa sola bakar (saniyede ~1 adım, CPU'suz). */
    .alive.working .eyes {
      animation: scan 2.4s steps(1, end) infinite;
    }
    /* Olayla gelen göz kırpma: blinkKey değişince <g> yeniden kurulur, animasyon bir kez oynar. */
    .alive .eyes.blink {
      animation: blink 0.2s ease-in-out;
    }
    .alive.working .eyes.blink {
      animation:
        blink 0.2s ease-in-out,
        scan 2.4s steps(1, end) 0.2s infinite;
    }
    .alive.act-pop .jump {
      animation: pop 0.45s var(--spring);
    }
    .alive.act-squash .jump {
      animation: squash 0.55s var(--ease);
    }
    .alive.act-spin .jump {
      animation: spin 1s cubic-bezier(0.2, 0.7, 0.2, 1);
    }
    .alive.act-jump .jump {
      animation: jump 1.1s var(--ease);
    }
    .alive.act-jump .fx i {
      animation: burst 0.8s var(--ease) 0.42s;
    }
    .alive.act-shake .jump {
      animation: shake 0.7s linear;
    }
    .alive.act-selam .jump {
      animation: rise 0.7s var(--spring) both;
    }
  }

  @keyframes scan {
    0% {
      transform: translateX(0);
    }
    33% {
      transform: translateX(-3px);
    }
    66% {
      transform: translateX(3px);
    }
  }
  @keyframes blink {
    50% {
      transform: scaleY(0.08);
    }
  }
  @keyframes pop {
    0% {
      transform: scale(0.88);
    }
    60% {
      transform: translateY(-6%) scale(1.05);
    }
    100% {
      transform: scale(1);
    }
  }
  @keyframes squash {
    0%,
    100% {
      transform: scale(1);
    }
    18% {
      transform: scale(1.16, 0.82);
    }
    45% {
      transform: scale(0.94, 1.08);
    }
    70% {
      transform: scale(1.03, 0.98);
    }
  }
  @keyframes spin {
    0% {
      transform: rotate(0) scale(1);
    }
    15% {
      transform: rotate(-20deg) scale(1.05, 0.95);
    }
    100% {
      transform: rotate(720deg) scale(1);
    }
  }
  @keyframes jump {
    0%,
    100% {
      transform: translateY(0) scale(1);
    }
    16% {
      transform: translateY(0) scale(1.14, 0.84);
    }
    42% {
      transform: translateY(-34%) scale(0.92, 1.1);
    }
    56% {
      transform: translateY(-38%) scale(1) rotate(-5deg);
    }
    76% {
      transform: translateY(0) scale(1.12, 0.88);
    }
    88% {
      transform: translateY(-3%) scale(0.98, 1.03);
    }
  }
  @keyframes burst {
    from {
      transform: rotate(var(--a)) translateY(-2.2em) scale(1);
      opacity: 1;
    }
    to {
      transform: rotate(var(--a)) translateY(-6.4em) scale(0.3);
      opacity: 0;
    }
  }
  @keyframes shake {
    0%,
    100% {
      transform: translateX(0);
    }
    12% {
      transform: translateX(-7%) rotate(-4deg);
    }
    28% {
      transform: translateX(7%) rotate(4deg);
    }
    44% {
      transform: translateX(-5%) rotate(-2deg);
    }
    60% {
      transform: translateX(4%) rotate(2deg);
    }
    76% {
      transform: translateX(-2%);
    }
  }
  @keyframes rise {
    0% {
      transform: translateY(85%) scale(0.9, 1.1);
      opacity: 0;
    }
    30% {
      opacity: 1;
    }
    70% {
      transform: translateY(-7%) scale(0.97, 1.03);
    }
    100% {
      transform: translateY(0) scale(1);
      opacity: 1;
    }
  }
</style>
