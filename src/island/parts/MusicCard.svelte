<script lang="ts">
  import { clock } from '../../lib/format';
  import { data, musicControl, musicSeek } from '../../lib/store.svelte';
  import type { NowPlaying } from '../../lib/types';
  import Vinyl from './Vinyl.svelte';

  interface Props {
    music: NowPlaying;
    /** large: mini oynatıcı · bar: Kontroller ekranının altındaki şerit */
    variant?: 'large' | 'bar';
    /** Sanatçı ve uygulamanın yanında gösterilecek ek bilgi (ör. ses çıkışı). */
    extra?: string | null;
  }
  let { music, variant = 'large', extra = null }: Props = $props();

  const SKIP_MS = 10_000;

  // Uygulamalar konumu seyrek bildirir; çalarken son ölçümden ileri sayılır.
  // Sayaç yalnız kart ekrandayken ve müzik çalarken saniyede bir çalışır.
  let now = $state(Date.now());
  $effect(() => {
    if (!music.playing || music.durationMs <= 0) return;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const position = $derived(
    Math.min(music.durationMs, music.positionMs + (music.playing ? Math.max(0, now - music.updatedAt) : 0)),
  );

  // Sürüklerken gösterilen konum (0–1); bırakınca oraya gidilir.
  let scrub = $state<number | null>(null);
  const progress = $derived(scrub ?? (music.durationMs > 0 ? position / music.durationMs : 0));
  const shown = $derived(scrub === null ? position : scrub * music.durationMs);
  // Kapak boyu base.css'teki --art-large / --art-bar jetonlarından (Yeni görünümde biraz büyük).
  const art = $derived(variant === 'bar' ? 56 : 96);
  // Sığmayan başlık: yalnız kartın üzerine gelince ölçülür ve (Yeni görünümde) yavaşça kayar; boşta iş yok.
  let titleEl: HTMLElement | undefined = $state();
  let overflow = $state(0);
  function measureTitle() {
    overflow = titleEl ? Math.max(0, titleEl.scrollWidth - titleEl.clientWidth) : 0;
  }
  const subtitle = $derived([music.artist, music.app, extra].filter(Boolean).join(' · '));

  function fraction(event: PointerEvent): number {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.min(1, Math.max(0, (event.clientX - box.left) / box.width));
  }
  function grab(event: PointerEvent) {
    if (!music.canSeek || event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    scrub = fraction(event);
  }
  function drag(event: PointerEvent) {
    if (scrub !== null) scrub = fraction(event);
  }
  function drop(event: PointerEvent) {
    if (scrub === null) return;
    const target = fraction(event) * music.durationMs;
    scrub = null;
    musicSeek(target);
  }
  const skip = (delta: number) => musicSeek(position + delta);
</script>

{#snippet buttons()}
  <div class="controls" data-no-open>
    <button class="skip" aria-label="10 saniye geri" title="10 sn geri" disabled={!music.canSeek} onclick={() => skip(-SKIP_MS)}>−10</button>
    <button aria-label="Önceki" title={music.canPrev ? 'Önceki parça' : 'Bu uygulamada önceki parça yok'} disabled={!music.canPrev} onclick={() => musicControl('prev')}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3" y="3" width="2" height="10" rx="1" /><path d="M13 3.6v8.8a.6.6 0 0 1-.9.5L6 8.5a.6.6 0 0 1 0-1l6.1-4.4a.6.6 0 0 1 .9.5Z" /></svg>
    </button>
    <button class="play" aria-label={music.playing ? 'Duraklat' : 'Oynat'} disabled={!music.canToggle} onclick={() => musicControl('toggle')}>
      {#if music.playing}
        <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="3" width="3" height="10" rx="1" /><rect x="9" y="3" width="3" height="10" rx="1" /></svg>
      {:else}
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 3.5v9a.6.6 0 0 0 .9.5l7.2-4.5a.6.6 0 0 0 0-1L5.9 3a.6.6 0 0 0-.9.5Z" /></svg>
      {/if}
    </button>
    <button aria-label="Sonraki" title={music.canNext ? 'Sonraki parça' : 'Bu uygulamada sonraki parça yok'} disabled={!music.canNext} onclick={() => musicControl('next')}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="11" y="3" width="2" height="10" rx="1" /><path d="M3 3.6v8.8a.6.6 0 0 0 .9.5L10 8.5a.6.6 0 0 0 0-1L3.9 3.1a.6.6 0 0 0-.9.5Z" /></svg>
    </button>
    <button class="skip" aria-label="10 saniye ileri" title="10 sn ileri" disabled={!music.canSeek} onclick={() => skip(SKIP_MS)}>+10</button>
  </div>
{/snippet}

{#snippet timeline()}
  {#if music.durationMs > 0}
    <div class="seek">
      <span class="time">{clock(shown)}</span>
      <!-- Fare ile sürüklenir; ada odak almadığı için klavye yok (±10 düğmeleri var). -->
      <div
        class="track"
        class:playing={music.playing && scrub === null}
        class:seekable={music.canSeek}
        class:dragging={scrub !== null}
        data-no-open
        role="slider"
        tabindex="-1"
        aria-label="Konum"
        aria-valuemin={0}
        aria-valuemax={Math.round(music.durationMs / 1000)}
        aria-valuenow={Math.round(shown / 1000)}
        aria-disabled={!music.canSeek}
        title={music.canSeek ? undefined : 'Bu uygulama sarmaya izin vermiyor'}
        onpointerdown={grab}
        onpointermove={drag}
        onpointerup={drop}
        onpointercancel={() => (scrub = null)}
      >
        <span class="played" style:width="{progress * 100}%"><span class="wave"></span></span>
        <span class="knob" style:left="{progress * 100}%"></span>
      </div>
      <span class="time end">{clock(music.durationMs)}</span>
    </div>
  {/if}
{/snippet}

<section class="music {variant}" aria-label="Çalan müzik" onpointerenter={measureTitle}>
  <div class="art">
    {#if data.artUrl}
      <img src={data.artUrl} alt="" draggable="false" />
    {:else}
      <Vinyl playing={music.playing} size={Math.round(art * 0.62)} />
    {/if}
  </div>
  <div class="info">
    <div class="titles">
      <span
        class="title"
        class:long={overflow > 0}
        title={music.title}
        bind:this={titleEl}
        style:--shift="-{overflow + 12}px"
        style:--run="{Math.max(2.5, overflow / 28)}s"><span class="run">{music.title}</span></span
      >
      <span class="artist">{subtitle}</span>
    </div>
    {@render timeline()}
    {#if variant === 'large'}
      {@render buttons()}
    {/if}
  </div>
  {#if variant === 'bar'}
    {@render buttons()}
  {/if}
</section>

<style>
  .music {
    display: flex;
    gap: 18px;
    align-items: center;
    min-width: 0;
  }
  .bar {
    gap: 14px;
  }
  .art {
    flex: none;
    width: var(--art-large);
    height: var(--art-large);
    border-radius: 12px;
    display: grid;
    place-items: center;
    overflow: hidden;
    background: var(--cover-fallback);
  }
  .bar .art {
    width: var(--art-bar);
    height: var(--art-bar);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .info {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 4px;
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: var(--fs-16);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar .title {
    font-size: var(--fs-13-5);
  }
  /* Yeni görünüm: sığmayan başlık üzerine gelince kenarları solarak kayar (yalnız transform, yalnız hover). */
  :global(:root) .music:hover .title.long {
    text-overflow: clip;
    mask-image: linear-gradient(90deg, transparent, #000 6px, #000 calc(100% - 14px), transparent);
  }
  /* Boştayken satır içi (Klasik'teki "…" korunur); kayarken blok olur ki transform işlesin. */
  :global(:root) .music:hover .title.long .run {
    display: inline-block;
  }
  @media (prefers-reduced-motion: no-preference) {
    :global(:root) .music:hover .title.long .run {
      animation: marquee var(--run) ease-in-out 0.5s infinite alternate;
    }
  }
  @keyframes marquee {
    0%,
    12% {
      transform: none;
    }
    88%,
    100% {
      transform: translateX(var(--shift));
    }
  }
  .artist {
    font-size: var(--fs-11-5);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .seek {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .time {
    font-size: var(--fs-10-5);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    width: 30px;
    flex: none;
  }
  .time.end {
    text-align: right;
  }
  /* Görünen çizgi 2 px; tutma alanı 16 px yüksek. */
  .track {
    position: relative;
    flex: 1;
    height: 16px;
    background: linear-gradient(var(--surface-3), var(--surface-3)) center / 100% 2px no-repeat;
    touch-action: none;
  }
  .track.seekable {
    cursor: pointer;
  }
  .played {
    position: absolute;
    left: 0;
    top: 4px;
    height: 8px;
    overflow: hidden;
  }
  /* Dalga 24 px'lik tekrar eden bir desen: çizelge ne kadar uzun olursa olsun düğmeye kadar uzanır.
     Çalarken bir desen boyu sola kayar (yalnız transform; sağdaki fazlalık sayesinde boşluk kalmaz). */
  .wave {
    position: absolute;
    inset: 0 -24px 0 0;
    background: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='8'%3E%3Cpath d='M0 4Q6 0 12 4T24 4' fill='none' stroke='%23ff8f7a' stroke-width='1.8'/%3E%3C/svg%3E") left center / 24px 8px repeat-x;
  }
  .track:not(.playing) .wave {
    background: linear-gradient(var(--coral), var(--coral)) left center / 100% 2px no-repeat;
  }
  @media (prefers-reduced-motion: no-preference) {
    /* Sürekli animasyonlar saniyede ~12 adımla: değişmeyen kareler çizilmez (bkz. Vinyl.svelte). */
    .playing .wave {
      animation: flow 1.4s steps(17) infinite;
    }
  }
  @keyframes flow {
    to {
      transform: translateX(-24px);
    }
  }
  .knob {
    position: absolute;
    top: 50%;
    width: 8px;
    height: 8px;
    margin: -4px 0 0 -4px;
    border-radius: 50%;
    background: #fff;
    transition: transform var(--t-fast);
  }
  .track.seekable:hover .knob,
  .dragging .knob {
    transform: scale(1.5);
  }
  .track:not(.seekable) .knob {
    background: var(--text-3);
  }
  .controls {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
  }
  .controls button {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 8px;
    color: var(--text-2);
  }
  .controls button:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-2);
  }
  .controls button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .controls svg {
    width: 14px;
    height: 14px;
    fill: currentColor;
  }
  .controls .skip {
    width: 34px;
    font-size: var(--fs-10-5);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .controls .play {
    width: 42px;
    margin: 0 4px;
    background: var(--coral);
    color: #2b0f08;
  }
  .controls .play:hover:not(:disabled) {
    color: #2b0f08;
    background: #ffa594;
  }
  /* Yeni görünüm: oynat düğmesi kapağın rengini alır (Kapaktan renk kapalıysa ya da kapak yoksa mercan). */
  :global(:root) .controls .play {
    background: var(--cover, var(--coral));
    color: var(--cover-ink, #2b0f08);
  }
  :global(:root) .controls .play:hover:not(:disabled) {
    background: color-mix(in srgb, var(--cover, var(--coral)) 80%, #fff);
    color: var(--cover-ink, #2b0f08);
  }
  .bar .controls {
    flex: none;
  }
</style>
