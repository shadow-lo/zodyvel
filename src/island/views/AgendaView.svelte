<script lang="ts">
  // Ajanda (Faz 5b, K3; eskiz design/eskiz-paket2.html §1–3): üstte "Sıradaki" kartı ve Gün/Ay, tarih, + Plan,
  // Döngü; ortada günün yatay saat çizelgesi (ya da ay ızgarası), altta haftalık şerit (eski WeekCard).
  // Planlar Bugün'le aynı Rust deposunda (today.json); tekrar kuralları ve hatırlatma Rust'ta (today/agenda.rs).
  // Animasyon yok: şu anki blok dakikada bir dolar, "şimdi" çizgisi dakikada bir yer değiştirir.
  import { blocks, dayDiff, dayLooped, daySummary, emptyForm, formOf, hm, isoDay, looped, nextPlan, parseDay, plansOn, repeatLabel, untilText } from '../../lib/agenda';
  import { todayApply, type IslandContext } from '../../lib/store.svelte';
  import { minutesText } from '../../lib/today';
  import type { CalEvent, PlanForm } from '../../lib/types';
  import AgendaMonth from '../parts/AgendaMonth.svelte';
  import AgendaSheet from '../parts/AgendaSheet.svelte';
  import AgendaTimeline from '../parts/AgendaTimeline.svelte';
  import WeekCard from '../parts/WeekCard.svelte';

  let { ctx }: { ctx: IslandContext } = $props();

  // Dakikada bir: sıradaki plan, şimdi çizgisi. Ekran kapalıyken (sekme değişince) durur.
  let now = $state(new Date());
  $effect(() => {
    let id: ReturnType<typeof setInterval> | undefined;
    const first = setTimeout(() => {
      now = new Date();
      id = setInterval(() => (now = new Date()), 60_000);
    }, 60_000 - (Date.now() % 60_000));
    return () => {
      clearTimeout(first);
      clearInterval(id);
    };
  });

  const events = $derived(ctx.today?.events ?? []);
  const today = $derived(isoDay(now));
  let view = $state<'day' | 'month'>('day');
  let picked = $state<string | null>(null);
  const sel = $derived(picked ?? today);
  const plans = $derived(plansOn(events, sel));
  const next = $derived(nextPlan(events, now, { running: true, allDay: false }));
  const loopOn = $derived(dayLooped(plans));

  let sheet = $state<{ id: number | null; form: PlanForm } | null>(null);
  let error = $state<string | null>(null);

  async function apply(op: Parameters<typeof todayApply>[0]): Promise<boolean> {
    error = await todayApply(op);
    return !error;
  }

  function shift(by: number) {
    const d = parseDay(sel);
    picked = isoDay(view === 'day' ? new Date(d.getFullYear(), d.getMonth(), d.getDate() + by) : new Date(d.getFullYear(), d.getMonth() + by, 1));
  }
  const dateTitle = $derived.by(() => {
    const d = parseDay(sel);
    if (view === 'month') return d.toLocaleDateString('tr-TR', { month: 'long' }) + (d.getFullYear() !== now.getFullYear() ? ` ${d.getFullYear()}` : '');
    return d.toLocaleDateString('tr-TR', { day: 'numeric', month: 'long' });
  });
  const dayName = $derived.by(() => {
    const diff = dayDiff(sel, today);
    return diff === 0 ? 'Bugün' : diff === 1 ? 'Yarın' : diff === -1 ? 'Dün' : parseDay(sel).toLocaleDateString('tr-TR', { weekday: 'long' });
  });

  // Sıradaki kartı: süren plan "Şimdi", yoksa bir sonraki. Büyük rakam: dakika (100'den azsa) ya da saat.
  const nextStart = $derived(next?.start ?? null);
  const running = $derived(
    next !== null && next.date === today && nextStart !== null && nextStart <= now.getHours() * 60 + now.getMinutes(),
  );
  const big = $derived.by(() => {
    if (!next || next.date !== today || nextStart === null) return null;
    const minute = now.getHours() * 60 + now.getMinutes();
    const left = running ? nextStart + (next.event.minutes ?? 0) - minute : nextStart - minute;
    return left < 100 ? { n: String(Math.max(0, left)), u: 'dk' } : { n: String(Math.round(left / 60)), u: 'sa' };
  });
  const range = (e: CalEvent, start: number | null) =>
    start === null ? 'Gün boyu' : e.minutes ? `${hm(start)} – ${hm(start + e.minutes)}` : hm(start);

  function newPlan(date = sel, time: string | null = null) {
    const form = emptyForm(date, now);
    sheet = { id: null, form: time ? { ...form, time } : form };
  }
  const edit = (e: CalEvent) => (sheet = { id: e.id, form: { ...formOf(e), date: e.date } });

  async function save(form: PlanForm) {
    const ok = sheet?.id != null ? await apply({ op: 'editEvent', id: sheet.id, plan: form }) : await apply({ op: 'addEvent', ...form });
    if (ok) sheet = null;
  }
  async function remove(id: number) {
    if (await apply({ op: 'removeEvent', id })) sheet = null;
  }
  async function loopDay(date: string, on: boolean) {
    await apply({ op: 'loopDay', date, on });
  }
  // Çizelgede sürükleme: saat değişir (tekrarlı planın her günü birlikte kayar).
  const move = (e: CalEvent, start: number) => apply({ op: 'editEvent', id: e.id, plan: { ...formOf(e), time: hm(start) } });

  const sum = $derived(daySummary(plans));
  const sumText = $derived(
    sum.count ? [`${sum.count} plan`, sum.minutes ? `${minutesText(sum.minutes)} dolu` : '', sum.looped ? `döngüde ${sum.looped}` : ''].filter(Boolean).join(' · ') : 'Plan yok',
  );
</script>

<div class="body">
  <div class="ag" class:month={view === 'month'}>
    <div class="top">
      {#if next}
        {@const e = next.event}
        <div class="card next" style:--k={e.color || 'var(--accent)'}>
          <span class="bar"></span>
          <button class="tx" onclick={() => edit(e)} title="Planı düzenle">
            <span class="t1"><b>{running ? 'Şimdi' : 'Sıradaki'}</b> · {running ? 'sürüyor' : untilText(next, now)}</span>
            <span class="t2">{e.title}</span>
            <span class="t3 tnum">
              {range(e, next.start)}
              {#if looped(e)}<span class="rep"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 0 1 13.7-5.6L20 8.5M20 4v4.5h-4.5M20 12a8 8 0 0 1-13.7 5.6L4 15.5M4 20v-4.5h4.5" /></svg>{repeatLabel(e)}</span>{/if}
            </span>
          </button>
          {#if big}
            <span class="cd tnum"><b>{big.n}<small>{big.u}</small></b><em>{running ? 'kaldı' : 'başlamasına'}</em></span>
          {/if}
        </div>
      {:else}
        <div class="card next empty">
          <span class="tx"><span class="t1"><b>Sıradaki</b></span><span class="t2">Plan yok</span><span class="t3">"+ Plan" ile gününü saat saat kur.</span></span>
        </div>
      {/if}
      <div class="card ctrl">
        <div class="row">
          <span class="seg" role="tablist" aria-label="Görünüm">
            <button role="tab" aria-selected={view === 'day'} class:on={view === 'day'} onclick={() => (view = 'day')}>Gün</button>
            <button role="tab" aria-selected={view === 'month'} class:on={view === 'month'} onclick={() => (view = 'month')}>Ay</button>
          </span>
          <span class="dn">
            <button aria-label="Geri" onclick={() => shift(-1)}>‹</button>
            <button class="dt" title="Bugüne dön" onclick={() => (picked = null)}>{dateTitle}</button>
            <button aria-label="İleri" onclick={() => shift(1)}>›</button>
          </span>
          <span class="grow"></span>
          <button class="add" onclick={() => newPlan()}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>Plan
          </button>
        </div>
        <button class="loop" role="switch" aria-checked={loopOn} disabled={!plans.some(p => p.time)} onclick={() => loopDay(sel, !loopOn)}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 0 1 13.7-5.6L20 8.5M20 4v4.5h-4.5M20 12a8 8 0 0 1-13.7 5.6L4 15.5M4 20v-4.5h4.5" /></svg>
          <span class="grow">Döngü: {dayName === 'Bugün' ? 'bugünün' : 'bu günün'} düzeni her gün</span>
          <span class="sw" class:on={loopOn}></span>
        </button>
      </div>
    </div>

    {#if view === 'day'}
      <div class="card tl">
        <div class="hd">
          <b>{dayName}</b>
          <span class="hdr">
            {sumText}
          </span>
        </div>
        <AgendaTimeline
          blocks={blocks(plans)}
          allDay={plans.filter(p => !p.time)}
          date={sel}
          {today}
          nowMinute={now.getHours() * 60 + now.getMinutes()}
          onedit={edit}
          onmove={move}
          onadd={minute => newPlan(sel, hm(minute))}
        />
      </div>
      <WeekCard {events} {sel} {today} onpick={d => (picked = d)} />
    {:else}
      <AgendaMonth
        {events}
        {sel}
        {today}
        onpick={d => (picked = d)}
        onday={d => ((picked = d), (view = 'day'))}
        onedit={edit}
      />
    {/if}
  </div>

  {#if error && !sheet}<p class="err" role="status">{error}</p>{/if}

  {#if sheet}
    <AgendaSheet
      form={sheet.form}
      editing={sheet.id !== null}
      {error}
      onsave={save}
      oncancel={() => ((sheet = null), (error = null))}
      onremove={sheet.id !== null ? () => remove(sheet?.id ?? 0) : undefined}
      onloop={date => loopDay(date, true)}
    />
  {/if}
</div>

<style>
  .body {
    position: relative;
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
  }
  .ag {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-rows: 92px minmax(0, 1fr) 74px;
    gap: 8px;
    min-height: 0;
  }
  .ag.month {
    grid-template-rows: 92px minmax(0, 1fr);
  }
  .card {
    background: var(--glass), var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    min-width: 0;
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
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .grow {
    flex: 1;
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .hdr {
    font-size: var(--fs-11);
    color: var(--text-3);
  }

  .top {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 8px;
    min-height: 0;
  }
  .next {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px 10px 20px;
    overflow: hidden;
  }
  .next .bar {
    position: absolute;
    left: 9px;
    top: 14px;
    bottom: 14px;
    width: 4px;
    border-radius: 4px;
    background: var(--k);
    box-shadow: 0 0 10px color-mix(in srgb, var(--k) 60%, transparent);
  }
  .next .tx {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    text-align: left;
  }
  .t1 {
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .t1 b {
    color: var(--k, var(--text-2));
    font-weight: 650;
  }
  .t2 {
    font-size: 19px;
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .t3 {
    font-size: var(--fs-12);
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rep {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: var(--fs-11);
    color: var(--text-3);
    padding: 1px 7px;
    border-radius: 99px;
    background: var(--surface-2);
  }
  .rep svg {
    width: 11px;
    height: 11px;
  }
  .cd {
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
  }
  /* Geri sayım: planın renginden beyaza degrade rakam (durağan). */
  .cd b {
    font-size: 40px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.02em;
    background: linear-gradient(180deg, #fff, color-mix(in srgb, var(--k) 70%, #fff));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .cd small {
    font-size: 15px;
    margin-left: 2px;
  }
  .cd em {
    font-style: normal;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .next.empty .t2 {
    color: var(--text-2);
  }

  .ctrl {
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .seg button {
    padding: 3px 10px;
    border-radius: 8px;
    font-size: var(--fs-12);
    color: var(--text-3);
  }
  .seg button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .dn {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: var(--fs-12);
  }
  .dn button {
    padding: 2px 5px;
    border-radius: 7px;
    color: var(--text-3);
  }
  .dn button:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .dn .dt {
    color: var(--text);
    font-weight: 600;
    min-width: 64px;
    white-space: nowrap;
  }
  .add {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 5px 11px 5px 8px;
    border-radius: 11px;
    font-size: var(--fs-12);
    font-weight: 650;
    background: var(--accent);
    color: var(--on-accent);
  }
  .add svg {
    width: 14px;
    height: 14px;
    stroke-width: 2.4;
  }
  .loop {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 8px;
    border-radius: 10px;
    background: var(--surface-2);
    font-size: var(--fs-11-5);
    color: var(--text-2);
    text-align: left;
  }
  .loop:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .loop svg {
    width: 14px;
    height: 14px;
    color: var(--text-3);
  }
  .sw {
    position: relative;
    flex: none;
    width: 26px;
    height: 15px;
    border-radius: 99px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .sw::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--text-3);
    transition: transform var(--t-fast) var(--ease);
  }
  .sw.on {
    background: color-mix(in srgb, var(--accent) 70%, #000);
    box-shadow: none;
  }
  .sw.on::after {
    transform: translateX(11px);
    background: #fff;
  }

  .tl {
    padding: 9px 12px 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hd {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: var(--fs-13-5);
  }
  .hd b {
    font-weight: 600;
  }
  .err {
    position: absolute;
    left: 50%;
    bottom: 14px;
    transform: translateX(-50%);
    margin: 0;
    padding: 4px 12px;
    border-radius: 99px;
    background: rgb(0 0 0 / 0.85);
    border: 1px solid var(--line-strong);
    font-size: var(--fs-11-5);
    color: var(--amber);
  }
</style>
