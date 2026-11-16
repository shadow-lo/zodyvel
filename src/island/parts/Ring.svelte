<script lang="ts">
  // Kalan süre halkası (çentik, ara boy, Bitiş). Geçiş animasyonu yok: oran saniyede bir değişse de
  // yalnız tek bir stroke-dashoffset yazılır.
  let { ratio, size = 20, stroke = 4, color = 'var(--accent)' }: { ratio: number; size?: number; stroke?: number; color?: string } = $props();
  const r = $derived((size - stroke) / 2);
  const len = $derived(2 * Math.PI * r);
</script>

<svg class="ring" width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
  <circle cx={size / 2} cy={size / 2} {r} stroke-width={stroke} class="track" />
  <circle
    cx={size / 2}
    cy={size / 2}
    {r}
    stroke-width={stroke}
    stroke={color}
    stroke-dasharray={len}
    stroke-dashoffset={len * (1 - Math.max(0, Math.min(1, ratio)))}
    transform="rotate(-90 {size / 2} {size / 2})"
  />
</svg>

<style>
  .ring {
    flex: none;
    display: block;
  }
  circle {
    fill: none;
    stroke-linecap: round;
  }
  .track {
    stroke: rgb(255 255 255 / 0.12);
  }
</style>
