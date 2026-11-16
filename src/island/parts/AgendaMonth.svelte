<script lang="ts">
  // Ajanda › Ay: yedi sütunlu ay ızgarası, her günde o günün planları kadar (en çok 4) renkli küçük çubuk.
  // Güne basınca sağda o günün listesi; "Gün görünümü" ya da çift tıklama çizelgeyi açar. Bugün vurgu renginde
  // çerçeveli, seçili gün dolgulu.
  import { daySummary, hm, minuteOf, monthGrid, parseDay, plansOn } from '../../lib/agenda';
  import type { CalEvent } from '../../lib/types';

  interface Props {
    events: CalEvent[];
    sel: string;
    today: string;
    onpick: (date: string) => void;
    onday: (date: string) => void;
    onedit: (e: CalEvent) => void;
  }
  let { events, sel, today, onpick, onday, onedit }: Props = $props();

  const d = $derived(parseDay(sel));
  const cells = $derived(monthGrid(d.getFullYear(), d.getMonth()));
  const month = $derived(sel.slice(0, 7));
  const list = $derived(plansOn(events, sel));
  const sum = $derived(daySummary(list));
  const title = $derived(d.toLocaleDateString('tr-TR', { weekday: 'long', day: 'numeric', month: 'long' }));
  const time = (e: CalEvent) => {
    const s = minuteOf(e.time);
    return s === null ? 'Gün boyu' : e.minutes ? `${hm(s)} – ${hm(s + e.minutes)}` : hm(s);
  };
</script>

<div class="month">
  <div class="card mg">
    {#each ['Pzt', 'Sal', 'Çar', 'Per', 'Cum', 'Cmt', 'Paz'] as w (w)}<span class="wd">{w}</span>{/each}
    {#each cells as c (c)}
      <button
        class="mc"
        class:dim={c.slice(0, 7) !== month}
        class:today={c === today}
        class:sel={c === sel}
        onclick={() => onpick(c)}
        ondblclick={() => onday(c)}
        aria-label={parseDay(c).toLocaleDateString('tr-TR', { day: 'numeric', month: 'long' })}
      >
        {Number(c.slice(8))}
        <span class="bars">
          {#each plansOn(events, c).slice(0, 4) as e (e.id)}<i style:--k={e.color || 'var(--accent)'}></i>{/each}
        </span>
      </button>
    {/each}
  </div>
  <div class="card dl">
    <h4>{title}<small>{sum.count ? `${sum.count} plan${sum.looped ? ` · döngüde ${sum.looped}` : ''}` : 'Plan yok'}</small></h4>
    <div class="items">
      {#each list as e (e.id)}
        <button class="it" style:--k={e.color || 'var(--accent)'} onclick={() => onedit(e)}>
          <i></i><span>{e.title}<small>{time(e)}</small></span>
        </button>
      {/each}
    </div>
    <button class="go" onclick={() => onday(sel)}>Gün görünümü ›</button>
  </div>
</div>

<style>
  .month {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 220px;
    gap: 8px;
    min-height: 0;
  }
  .card {
    background: var(--glass), var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    min-height: 0;
  }
  button {
    font: inherit;
    color: inherit;
    cursor: pointer;
    border: 0;
    background: none;
    padding: 0;
  }
  .mg {
    padding: 8px 10px;
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    grid-template-rows: 16px repeat(6, minmax(0, 1fr));
    gap: 3px;
  }
  .wd {
    font-size: var(--fs-10-5);
    color: var(--text-3);
    text-align: center;
  }
  .mc {
    border-radius: 9px;
    padding: 3px 6px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    font-size: var(--fs-12);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    min-height: 0;
  }
  .mc:hover {
    background: var(--surface-2);
  }
  .mc.dim {
    color: var(--text-3);
    opacity: 0.55;
    font-weight: 500;
  }
  .mc.today {
    box-shadow: inset 0 0 0 1.5px var(--accent-soft);
    color: var(--accent-soft);
  }
  .mc.sel {
    background: var(--surface-3);
  }
  .bars {
    display: flex;
    gap: 2px;
    width: 100%;
  }
  .bars i {
    flex: 1;
    max-width: 14px;
    height: 4px;
    border-radius: 2px;
    background: var(--k);
  }
  .dl {
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h4 {
    margin: 0;
    font-size: var(--fs-12-5);
    font-weight: 650;
    display: flex;
    flex-direction: column;
  }
  h4 small {
    font-weight: 400;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .items {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .it {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 7px;
    border-radius: 9px;
    background: var(--surface-2);
    text-align: left;
    font-size: var(--fs-12);
  }
  .it:hover {
    background: var(--surface-3);
  }
  .it i {
    flex: none;
    width: 4px;
    align-self: stretch;
    border-radius: 3px;
    background: var(--k);
  }
  .it span {
    min-width: 0;
    display: flex;
    flex-direction: column;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .it small {
    color: var(--text-3);
    font-size: var(--fs-10-5);
    font-variant-numeric: tabular-nums;
  }
  .go {
    align-self: flex-start;
    padding: 5px 11px;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .go:hover {
    color: var(--text);
  }
</style>
