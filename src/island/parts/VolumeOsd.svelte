<script lang="ts">
  // Ses göstergesi (Faz 10; eskiz design/eskiz-paket3.html ekran 4): çentik kapsülü simge + dolan çubuk + yüzde olur.
  // Hareket yalnız transform: çubuk `scaleX` ile hedef değere yumuşak geçer (160 ms), her tuşta yeniden çizim yok.
  import type { VolumeOsd } from '../../lib/types';

  let { osd }: { osd: VolumeOsd } = $props();
  const icon = $derived(osd.muted ? 'm' : osd.level < 34 ? 'l' : 'h');
</script>

<div class="vol" class:mute={osd.muted} role="status" aria-label={osd.muted ? 'Ses kapalı' : `Ses %${osd.level}`}>
  <svg class="i" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    <path d="M4 10v4h4l5 4V6L8 10z" />
    {#if icon === 'h'}
      <path d="M16.5 9a4 4 0 0 1 0 6M19 7a7 7 0 0 1 0 10" />
    {:else if icon === 'l'}
      <path d="M16.5 9a4 4 0 0 1 0 6" />
    {:else}
      <path d="M17 9.5l4 5M21 9.5l-4 5" />
    {/if}
  </svg>
  <span class="bar"><i style:--v={osd.muted ? 0 : osd.level / 100}></i></span>
  <span class="pc">{osd.muted ? '—' : osd.level}</span>
</div>

<style>
  .vol {
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 16px 0 12px;
    width: max-content;
    margin: 0 auto;
    background: var(--island);
    border-radius: 0 0 18px 18px;
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.05);
    color: #fff;
  }
  .i {
    width: 17px;
    height: 17px;
    flex: none;
  }
  .bar {
    position: relative;
    width: 132px;
    height: 6px;
    border-radius: 6px;
    background: rgb(255 255 255 / 0.13);
    overflow: hidden;
  }
  .bar i {
    position: absolute;
    inset: 0;
    border-radius: 6px;
    background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 70%, #fff), #fff);
    transform-origin: left;
    transform: scaleX(var(--v));
    transition: transform var(--t-fast) var(--ease);
    will-change: transform;
  }
  .mute .bar i {
    background: #4a4d58;
  }
  .pc {
    width: 30px;
    text-align: right;
    font-size: var(--fs-13-5);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .mute .pc {
    color: var(--text-3);
  }
  @media (prefers-reduced-motion: reduce) {
    .bar i {
      transition: none;
    }
  }
</style>
