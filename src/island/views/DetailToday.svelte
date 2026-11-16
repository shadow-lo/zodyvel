<script lang="ts">
  // Bugün ayrıntısı: üstte kısa saat ve Zodymetreler (en çok 3, Faz 5a), altta yapılacaklar ve bugünün planları.
  // Haftalık takvim Faz 5b'de Ajanda sekmesine taşındı (K3); burada yalnız bugünün planlarının kısa listesi kalır
  // (Ajanda modülü açıksa). Hava kartı yok, takvim yalnız Zodyvel içinde (K10).
  import { hm, isoDay, minuteOf, plansOn } from '../../lib/agenda';
  import { data, type IslandContext } from '../../lib/store.svelte';
  import ClockCard from '../parts/ClockCard.svelte';
  import Meters from '../parts/Meters.svelte';
  import TodoList from '../parts/TodoList.svelte';

  let { ctx }: { ctx: IslandContext } = $props();

  const agendaOn = $derived(data.settings.modules.agenda);
  const plans = $derived(ctx.today && agendaOn ? plansOn(ctx.today.events, isoDay(new Date())) : []);
  const nowMin = new Date().getHours() * 60 + new Date().getMinutes();
  const time = (t: string | null) => (t === null ? 'Gün boyu' : hm(minuteOf(t) ?? 0));
</script>

<div class="body">
  {#if ctx.today}
    <div class="grid" class:solo={!agendaOn}>
      <div class="clock"><ClockCard /></div>
      <div class="meters"><Meters meters={ctx.meters} /></div>
      <div class="todos"><TodoList todos={ctx.today.todos} /></div>
      {#if agendaOn}
        <section class="plans">
          <h4>Bugünün planı<small>{plans.length ? `${plans.length} plan` : 'boş'}</small></h4>
          <ul data-scroll>
            {#each plans as e (e.id)}
              {@const s = minuteOf(e.time)}
              <li style:--k={e.color || 'var(--accent)'} class:past={s !== null && s + (e.minutes || 0) <= nowMin}>
                <span class="t">{time(e.time)}</span><span class="n" title={e.title}>{e.title}</span>
              </li>
            {:else}
              <li class="rest">Ajanda'dan plan ekleyebilirsin.</li>
            {/each}
          </ul>
        </section>
      {/if}
    </div>
  {:else}
    <p class="none">Yükleniyor…</p>
  {/if}
</div>

<style>
  .body {
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
  }
  .grid {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: 150px 1fr 230px;
    grid-template-rows: auto 1fr;
    gap: 8px;
    min-height: 0;
  }
  .grid > :global(*) {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .grid > div > :global(*) {
    flex: 1;
  }
  .meters {
    grid-column: 2 / span 2;
  }
  .todos {
    grid-column: 1 / span 2;
  }
  .grid.solo .todos {
    grid-column: 1 / span 3;
  }
  .plans {
    grid-column: 3;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    padding: 10px 12px;
    gap: 6px;
  }
  h4 {
    margin: 0;
    font-size: var(--fs-12-5);
    font-weight: 650;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  h4 small {
    color: var(--text-3);
    font-weight: 500;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  li {
    display: flex;
    gap: 8px;
    padding: 3px 8px;
    border-radius: 8px;
    background: var(--surface-2);
    box-shadow: inset 2px 0 var(--k);
    font-size: var(--fs-12);
  }
  li.past {
    opacity: 0.55;
  }
  .t {
    flex: none;
    width: 58px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .n {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  li.rest {
    background: none;
    box-shadow: none;
    color: var(--text-3);
    font-size: var(--fs-11-5);
    padding-left: 0;
  }
  .none {
    margin: auto;
    color: var(--text-3);
  }
</style>
