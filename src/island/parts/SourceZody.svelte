<script lang="ts">
  // Kaynak Zody'si: ekran çerçeveli Zody, çerçevesi kaynağın renginde (TILE_COLORS); dış ince çizgi durumu söyler. Durağan;
  // yalnız yeni olay gelince (`bump` değişince) bir kez küçük sıçrama (transform). Boşta animasyon yok.
  import Zody from '../../mascot/Zody.svelte';
  import { TILE_COLORS, type TileKind, type TileTone } from '../../lib/sources';
  import type { ZodyMood } from '../../lib/types';

  interface Props {
    tile: TileKind;
    tone?: TileTone;
    mood?: ZodyMood;
    size?: number;
    /** Yeni olay anahtarı; değişince tek seferlik sıçrama. */
    bump?: number | null;
    alive?: boolean;
  }
  let { tile, tone = 'calm', mood = 'idle', size = 22, bump = null, alive = false }: Props = $props();
</script>

<span class="sz {tone}" style:--s="{size}px">
  {#key bump}
    <span class="body" class:hop={bump !== null}><Zody {mood} body={TILE_COLORS[tile]} {size} {alive} /></span>
  {/key}
</span>

<style>
  .sz {
    --ring: transparent;
    position: relative;
    flex: none;
    width: var(--s);
    height: var(--s);
    border-radius: 28%;
    box-shadow: 0 0 0 1.5px var(--ring);
  }
  .busy {
    --ring: color-mix(in srgb, var(--teal) 70%, transparent);
  }
  .attention {
    --ring: color-mix(in srgb, var(--amber) 80%, transparent);
  }
  .alert {
    --ring: color-mix(in srgb, var(--red) 80%, transparent);
  }
  .off {
    opacity: 0.55;
  }
  .body {
    display: block;
  }
  .hop {
    animation: hop var(--t-slow) var(--ease) 1;
  }
  @keyframes hop {
    0%,
    100% {
      transform: translateY(0) scale(1);
    }
    35% {
      transform: translateY(-22%) scale(1.06, 0.96);
    }
    65% {
      transform: translateY(0) scale(0.97, 1.03);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .hop {
      animation: none;
    }
  }
</style>
