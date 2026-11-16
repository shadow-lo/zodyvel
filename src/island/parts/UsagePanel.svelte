<script lang="ts">
  // Claude kullanım özeti: bugün, bu hafta, geçen hafta; son 7 gün çubukları; bu haftanın projeleri ve
  // modelleri; geçen haftanın çalışma özeti. Maliyet API liste fiyatıyla tahmindir (abonelikte ayrıca
  // ödenmez). Panel ekrandayken dakikada bir yenilenir; kapalıyken hiçbir şey okunmaz.
  import { data, watchUsage } from '../../lib/store.svelte';
  import { busiestDay, DAY_SHORT, hours, money, tokens } from '../../lib/usage';

  let { onclose }: { onclose?: () => void } = $props();

  $effect(() => {
    if (!data.settings.features.usage) return;
    return watchUsage();
  });

  const usage = $derived(data.usage);
  const s = $derived(usage?.summary ?? null);
  // Son 7 gün (14 günlük dizinin sonu); maliyet yoksa (bilinmeyen model) token'a göre.
  const week = $derived(
    usage ? usage.spans.days.slice(-7).map((span, i) => ({ span, day: usage.summary.days[i + 7] ?? { cost: 0, tokens: 0, activeMs: 0, sessions: 0 } })) : [],
  );
  const byCost = $derived(week.some(d => d.day.cost > 0));
  const peak = $derived(Math.max(1e-9, ...week.map(d => (byCost ? d.day.cost : d.day.tokens))));
  const busiest = $derived(usage ? busiestDay(usage.spans, usage.summary.days) : null);
  const tiles = $derived(s ? ([['Bugün', s.today], ['Bu hafta', s.week], ['Geçen hafta', s.lastWeek]] as const) : []);
</script>

<section class="usage" aria-label="Kullanım özeti">
  <header>
    <b>Kullanım</b>
    <small>tahmini API fiyatı · aboneliğinde ayrıca ödenmez</small>
    {#if onclose}<button class="x" onclick={onclose} aria-label="Kapat">×</button>{/if}
  </header>
  {#if !data.settings.features.usage}
    <p class="none">Kullanım özeti kapalı (Ayarlar → Özellikler).</p>
  {:else if !s}
    <p class="none">Claude kayıtları okunuyor…</p>
  {:else if !s.week.messages && !s.lastWeek.messages}
    <p class="none">Son iki haftada Claude Code kaydı yok.</p>
  {:else}
    <div class="tiles">
      {#each tiles as [label, t] (label)}
        <div class="tile">
          <small>{label}</small>
          <b class="num">{money(t.cost)}</b>
          <span>{tokens(t.tokens)} token · {t.sessions} oturum</span>
        </div>
      {/each}
    </div>
    <div class="mid">
      <div class="bars" aria-label="Son 7 gün">
        {#each week as { span, day }, i (span.from)}
          {@const value = byCost ? day.cost : day.tokens}
          <div class="bar" class:today={i === week.length - 1} title="{money(day.cost)} · {tokens(day.tokens)} token · {hours(day.activeMs)}">
            <span class="track"><span class="fill" style:transform="scaleY({value / peak})"></span></span>
            <small>{DAY_SHORT[new Date(span.from).getDay()]}</small>
          </div>
        {/each}
      </div>
      <div class="lists">
        <div>
          <small class="cap">Projeler · bu hafta</small>
          {#each s.week.projects.slice(0, 3) as p (p.name)}
            <span class="li"><span class="nm">{p.name}</span><span class="num">{byCost ? money(p.cost) : tokens(p.tokens)}</span></span>
          {:else}
            <span class="li dim">—</span>
          {/each}
        </div>
        <div>
          <small class="cap">Modeller</small>
          {#each s.week.models.slice(0, 2) as m (m.name)}
            <span class="li"><span class="nm">{m.name}</span><span class="num">{byCost ? money(m.cost) : tokens(m.tokens)}</span></span>
          {:else}
            <span class="li dim">—</span>
          {/each}
        </div>
      </div>
    </div>
    <footer>
      {#if s.lastWeek.messages}
        {@const w = s.lastWeek}
        <span><b>Geçen hafta</b> {hours(w.activeMs)} · {w.sessions} oturum · {w.files} dosya · <span class="add">+{w.added.toLocaleString('tr-TR')}</span>&nbsp;<span class="del">−{w.removed.toLocaleString('tr-TR')}</span>{#if busiest}{` · en yoğun ${busiest}`}{/if}</span>
      {:else}
        <span>Bu hafta {hours(s.week.activeMs)} aktif · {s.week.files} dosya değişti</span>
      {/if}
      {#if s.week.unpriced}<span class="dim">Fiyatı bilinmeyen modeller: {tokens(s.week.unpriced)} token, maliyete katılmadı.</span>{/if}
    </footer>
  {/if}
</section>

<style>
  .usage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  header b {
    font-size: var(--fs-14);
    font-weight: 700;
  }
  header small {
    flex: 1;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .x {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    color: var(--text-3);
    font-size: var(--fs-15);
    line-height: 1;
  }
  .x:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .none {
    margin: auto;
    color: var(--text-3);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 12px;
    background: var(--surface-2);
    min-width: 0;
  }
  .tile small,
  .cap {
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .tile b {
    font-size: var(--fs-16);
    font-weight: 700;
  }
  .tile span {
    color: var(--text-2);
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mid {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 14px;
  }
  .bars {
    flex: 1;
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .bar {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .track {
    flex: 1;
    width: 100%;
    max-width: 22px;
    border-radius: 6px;
    background: var(--surface-2);
    overflow: hidden;
    display: flex;
    align-items: flex-end;
  }
  .fill {
    width: 100%;
    height: 100%;
    background: color-mix(in srgb, var(--accent) 55%, transparent);
    transform-origin: bottom;
  }
  .bar.today .fill {
    background: var(--accent);
  }
  .bar small {
    color: var(--text-3);
    font-size: var(--fs-10-5);
  }
  .bar.today small {
    color: var(--text);
  }
  .lists {
    width: 46%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .lists > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .li {
    display: flex;
    gap: 8px;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .nm {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .li .num {
    color: var(--text);
  }
  .dim {
    color: var(--text-3);
  }
  footer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  footer b {
    color: var(--text);
    font-weight: 650;
  }
  footer .dim {
    font-size: var(--fs-11);
  }
  .add {
    color: var(--green);
  }
  .del {
    color: var(--red);
  }
</style>
