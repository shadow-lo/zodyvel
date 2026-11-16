<script lang="ts">
  // Ajanda'nın altındaki haftalık şerit (K3: eski Bugün WeekCard'ı buraya taşındı). Seçili günün haftası;
  // her günde o günün planları kadar (en çok 4) renkli çubuk, uzunluğu planın süresiyle. Güne basınca o gün
  // çizelgede açılır. Bugün vurgu renginde çerçeveli.
  import { addDays, minuteOf, plansOn, weekdayOf } from '../../lib/agenda';
  import type { CalEvent } from '../../lib/types';

  interface Props {
    events: CalEvent[];
    sel: string;
    today: string;
    onpick: (date: string) => void;
  }
  let { events, sel, today, onpick }: Props = $props();

  const NAMES = ['Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt', 'Paz'];
  const days = $derived(Array.from({ length: 7 }, (_, i) => addDays(sel, i - weekdayOf(sel))));
  const bars = (date: string) =>
    plansOn(events, date)
      .slice(0, 4)
      .map(e => ({ id: e.id, k: e.color || 'var(--accent)', w: e.time ? Math.max(34, Math.min(100, ((e.minutes || 30) / 180) * 100)) : 100, timed: minuteOf(e.time) !== null }));
</script>

<div class="week">
  {#each days as d, i (d)}
    {@const list = bars(d)}
    <button class="day" class:today={d === today} class:sel={d === sel} onclick={() => onpick(d)} aria-label="{NAMES[i]} {Number(d.slice(8))}">
      <span class="dh">{NAMES[i]}<b>{Number(d.slice(8))}</b></span>
      <span class="bars">
        {#each list as b (b.id)}<i style:--k={b.k} style:width="{b.w}%" class:allday={!b.timed}></i>{:else}<em>boş</em>{/each}
      </span>
    </button>
  {/each}
</div>

<style>
  .week {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 6px;
    min-height: 0;
  }
  .day {
    font: inherit;
    color: inherit;
    cursor: pointer;
    border: 1px solid var(--line);
    background: var(--glass), var(--surface);
    box-shadow: var(--card-depth);
    border-radius: 13px;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    text-align: left;
  }
  .day:hover {
    border-color: var(--line-strong);
  }
  .day.sel {
    background: var(--surface-2);
    border-color: var(--line-strong);
  }
  .day.today {
    box-shadow: inset 0 0 0 1.5px var(--accent-soft);
  }
  .dh {
    display: flex;
    align-items: baseline;
    gap: 5px;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .dh b {
    font-size: var(--fs-13-5);
    color: var(--text);
    font-weight: 600;
  }
  .day.today .dh b {
    color: var(--accent-soft);
  }
  .bars {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .bars i {
    display: block;
    height: 4px;
    border-radius: 3px;
    background: var(--k);
    opacity: 0.85;
  }
  .bars i.allday {
    opacity: 0.45;
  }
  .bars em {
    font-style: normal;
    font-size: var(--fs-10-5);
    color: var(--text-3);
    opacity: 0.6;
  }
</style>
