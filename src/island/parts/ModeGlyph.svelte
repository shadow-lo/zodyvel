<script lang="ts">
  // Mod simgeleri (Faz 5a; eskiz paket 2 §4): renkli yuvarlak kare içinde ince çizgi simge. Modlar sekmesi,
  // çentikteki mod hapı ve Mod Bitişi aynı çizimi kullanır.
  import type { ModeIcon } from '../../lib/types';

  interface Props {
    icon: ModeIcon;
    color: string;
    size?: number;
  }
  let { icon, color, size = 22 }: Props = $props();
</script>

<span class="tile" style:--k={color} style:width="{size}px" style:height="{size}px" style:border-radius="{Math.round(size * 0.3)}px">
  <svg viewBox="0 0 24 24" aria-hidden="true" style:width="{Math.round(size * 0.52)}px" style:height="{Math.round(size * 0.52)}px">
    {#if icon === 'brief'}
      <rect x="3.5" y="7" width="17" height="12" rx="2.5" /><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7M3.5 12.5h17" />
    {:else if icon === 'book'}
      <path d="M4 5.5A1.5 1.5 0 0 1 5.5 4H11v16H5.5A1.5 1.5 0 0 1 4 18.5z" /><path d="M20 5.5A1.5 1.5 0 0 0 18.5 4H13v16h5.5a1.5 1.5 0 0 0 1.5-1.5z" />
    {:else if icon === 'moon'}
      <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />
    {:else if icon === 'game'}
      <rect x="2.5" y="7" width="19" height="11" rx="5.5" /><path d="M7 10.5v4M5 12.5h4M15.5 11.5h.01M18 13.5h.01" />
    {:else if icon === 'leaf'}
      <path d="M5 19c0-8 5-13 14-14-1 9-6 14-14 14z" /><path d="M5 19l7-7" />
    {:else}
      <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
    {/if}
  </svg>
</span>

<style>
  .tile {
    display: inline-grid;
    place-items: center;
    flex: none;
    background: linear-gradient(160deg, color-mix(in srgb, var(--k) 42%, #000), color-mix(in srgb, var(--k) 16%, #000));
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--k) 45%, transparent),
      0 0 12px color-mix(in srgb, var(--k) 28%, transparent);
  }
  svg {
    fill: none;
    stroke: color-mix(in srgb, var(--k) 35%, #fff);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
