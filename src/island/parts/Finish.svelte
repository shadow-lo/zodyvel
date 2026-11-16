<script lang="ts">
  // Bitiş bildirimi (Faz 4; eskiz design/eskiz-paket2.html §9): Zodymetre dolunca çentik büyür, halka
  // tamamlanır ve içinden 8 küçük parça bir kez dağılır (~900 ms, yalnız transform/opacity; sonra parçalar
  // DOM'dan kalkar). Konfeti değil: farklı biçim, açı ve uzaklıkta az sayıda parça. Mod Bitişi (Faz 5a) aynı
  // kalıpta modun renginde: kapattığı uygulamaları yeniden açmayı ya da 5 dk mola vermeyi sunar.
  import { onMount } from 'svelte';
  import type { FinishAction, Notice } from '../../lib/notice';
  import ModeGlyph from './ModeGlyph.svelte';
  import Ring from './Ring.svelte';

  interface Props {
    notice: Notice;
    reduce?: boolean;
    onopen: () => void;
    /** Zodymetre: 'add' +5 dk, 'again' aynı süreyle yeniden. Mod: 'reopen' kapattıklarını aç, 'break' 5 dk mola
     * (Zodymetre olarak). 'ok': kapat. */
    onaction: (action: FinishAction) => void;
  }
  let { notice, reduce = false, onopen, onaction }: Props = $props();

  const BURST_MS = 900;
  // [açı°, uzaklık px, boy px, köşe, renk]
  const PIECES: [number, number, number, string, string][] = [
    [12, 34, 6, '50%', 'var(--k)'],
    [58, 27, 4, '1px', '#fff'],
    [101, 38, 7, '50%', '#ffd98a'],
    [148, 25, 5, '1.5px', 'var(--k)'],
    [196, 36, 4, '50%', '#c9c1ff'],
    [237, 30, 6, '1.5px', 'var(--k)'],
    [283, 39, 5, '50%', '#fff'],
    [326, 26, 4, '1px', 'var(--k)'],
  ];
  let burst = $state(true);
  onMount(() => {
    if (reduce) return void (burst = false);
    const timer = setTimeout(() => (burst = false), BURST_MS + 100);
    return () => clearTimeout(timer);
  });
  const fin = $derived(notice.finish);
  const mode = $derived(fin?.kind === 'mode' ? fin : null);
  // Zodymetre yeşil (eskiz §9), mod kendi renginde.
  const k = $derived(mode ? mode.color : 'var(--green)');
  // Ek yazmamak için (Discord'u, Steam'i…) düğme kısa; hangi uygulamalar olduğu açıklama satırında.
  const reopenLabel = $derived(mode?.reopen.length === 1 ? 'Yeniden aç' : 'Hepsini aç');
</script>

<div class="fin" style:--k={k}>
  <span class="gl" aria-hidden="true"></span>
  <button class="hit" onclick={onopen} aria-label={mode ? 'Modlar\'ı aç' : 'Bugün\'ü aç'}></button>
  <span class="burst">
    <Ring ratio={1} size={50} stroke={4} color="var(--k)" />
    {#if mode}
      <span class="ck glyph"><ModeGlyph icon={mode.icon} color={mode.color} size={26} /></span>
    {:else}
      <svg class="ck" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
    {/if}
    {#if burst}
      {#each PIECES as [r, d, w, br, c], i (i)}
        <i class="pt" style:--r="{r}deg" style:--d="{d}px" style:--w="{w}px" style:--br={br} style:--c={c}></i>
      {/each}
    {/if}
  </span>
  <span class="txt">
    <small>{mode ? 'Mod' : 'Zodymetre'}</small>
    <b>{notice.title}</b>
    <span class="dt">{notice.detail}</span>
    <span class="btns">
      {#if mode}
        {#if mode.reopen.length}<button class="btn" onclick={() => onaction('reopen')}>{reopenLabel}</button>{/if}
        <button class="btn" onclick={() => onaction('break')}>Mola ver · 5 dk</button>
      {:else}
        <button class="btn" onclick={() => onaction('add')}>+5 dk</button>
        <button class="btn" onclick={() => onaction('again')}>Yeniden</button>
      {/if}
      <button class="btn ok" onclick={() => onaction('ok')}>Tamam</button>
    </span>
  </span>
  <span class="frame" aria-hidden="true"></span>
</div>

<style>
  .fin {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 4px 18px 10px 16px;
    border-radius: 0 0 22px 22px;
    overflow: hidden;
  }
  .gl {
    position: absolute;
    inset: 0;
    background: radial-gradient(55% 110% at 30% 120%, color-mix(in srgb, var(--k) 26%, transparent), transparent 70%);
    pointer-events: none;
  }
  .hit {
    position: absolute;
    inset: 0;
  }
  /* Çerçeve ışığı (çentikle aynı): soldan iner, alttan geçer, sağdan çıkar; üstte yok. */
  .frame {
    position: absolute;
    inset: 0;
    border: 2px solid var(--k);
    border-top: 0;
    border-radius: inherit;
    box-shadow: inset 0 -8px 16px -8px var(--k);
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 50%);
    mask-image: linear-gradient(to bottom, transparent, #000 50%);
    pointer-events: none;
    transform-origin: top;
  }
  .burst {
    position: relative;
    width: 58px;
    height: 58px;
    flex: none;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  .ck {
    position: absolute;
    width: 22px;
    height: 22px;
    fill: none;
    stroke: #fff;
    stroke-width: 2.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .ck.glyph {
    width: auto;
    height: auto;
    display: grid;
  }
  .pt {
    position: absolute;
    left: 50%;
    top: 50%;
    width: var(--w);
    height: var(--w);
    margin: calc(var(--w) / -2) 0 0 calc(var(--w) / -2);
    border-radius: var(--br);
    background: var(--c);
    opacity: 0;
    animation: fly 0.9s cubic-bezier(0.15, 0.7, 0.3, 1) both;
  }
  @keyframes fly {
    from {
      transform: rotate(var(--r)) translateY(-14px) scale(0.6);
      opacity: 1;
    }
    60% {
      opacity: 1;
    }
    to {
      transform: rotate(var(--r)) translateY(calc(var(--d) * -1.5));
      opacity: 0;
    }
  }
  .txt {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    pointer-events: none;
  }
  small,
  .dt {
    font-size: var(--fs-11-5);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  b {
    font-size: var(--fs-16);
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .btns {
    display: flex;
    gap: 6px;
    margin-top: 7px;
    pointer-events: auto;
  }
  .btn {
    font-size: var(--fs-12);
    padding: 4px 12px;
    border-radius: 10px;
    color: var(--text);
    background: var(--surface-2);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.08);
  }
  .btn:hover {
    background: var(--surface-3);
  }
  .btn.ok {
    background: linear-gradient(180deg, color-mix(in srgb, var(--k) 55%, #fff), var(--k));
    color: #04140b;
    font-weight: 650;
    box-shadow: 0 0 14px color-mix(in srgb, var(--k) 45%, transparent);
  }
  @media (prefers-reduced-motion: no-preference) {
    .burst {
      animation: pop var(--t-slow) var(--spring) both;
    }
    .txt {
      animation: slide var(--t-base) var(--ease) 140ms both;
    }
    .frame {
      animation: frame-in var(--t-slow) var(--ease) 80ms both;
    }
  }
</style>
