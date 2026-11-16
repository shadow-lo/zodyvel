<script lang="ts">
  // Araç Zody'si: her araç aynı ekran çerçeveli Zody, çerçevesi kendi renginde (agentColor); durum gözlerde. Yazma sahnesinden başlatılan görevin Zody'sinde sol altta küçük numara. Durağan.
  import Zody from '../../mascot/Zody.svelte';
  import { agentColor } from '../../lib/format';
  import type { ZodyMood } from '../../lib/types';

  interface Props {
    tool: string;
    mood?: ZodyMood;
    size?: number;
    /** Yeni görev numarası (yalnız Yeni görünümde gösterilir; çağıran karar verir). */
    num?: number | null;
    alive?: boolean;
  }
  let { tool, mood = 'idle', size = 24, num, alive = false }: Props = $props();
</script>

<span class="tz" style:--s="{size}px">
  <Zody {mood} body={agentColor(tool)} {size} {alive} />
  {#if num}<span class="num" title="Yazma sahnesinden başlatılan {num}. görev">{num}</span>{/if}
</span>

<style>
  .tz {
    position: relative;
    display: inline-block;
    width: var(--s);
    height: var(--s);
    flex: none;
  }
  .num {
    position: absolute;
    left: 2%;
    bottom: 4%;
    min-width: max(12px, calc(var(--s) * 0.32));
    height: max(12px, calc(var(--s) * 0.32));
    padding: 0 2px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    font-size: max(8px, calc(var(--s) * 0.2));
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
    color: var(--text);
    background: var(--surface-3);
    box-shadow: 0 0 0 1.5px var(--island);
  }
</style>
