<script lang="ts">
  // Ajanda › Yeni plan / düzenle: ada içinde cam sayfa (eskiz paket 2 §3). Başlık, başlangıç (ya da gün boyu),
  // süre çipleri, renk, Tekrar (Hiç / Her gün / Hafta içi / Günler) ve gün seçici, "başlarken çentikte haber
  // ver". Solda altta "Bugünü her güne uygula": o günün bütün tekrarsız planlarını tek tuşla döngüye alır.
  // Ada odak almadığından metin kutuları yazma kipini açar (startTyping).
  import { LEAD_MINUTES, PLAN_COLORS, PLAN_MINUTES, parseDay, withRepeat } from '../../lib/agenda';
  import { onMount, untrack } from 'svelte';
  import { call } from '../../lib/ipc';
  import { startTyping } from '../../lib/store.svelte';
  import type { PhoneStatus, PlanForm, Repeat } from '../../lib/types';

  interface Props {
    form: PlanForm;
    editing: boolean;
    error: string | null;
    onsave: (form: PlanForm) => void;
    oncancel: () => void;
    onremove?: () => void;
    onloop: (date: string) => void;
  }
  let { form: initial, editing, error, onsave, oncancel, onremove, onloop }: Props = $props();

  // Form kendi kopyasında düzenlenir; Vazgeç hiçbir şeyi değiştirmez.
  let f = $state<PlanForm>(untrack(() => ({ ...initial })));
  let lastTime = untrack(() => initial.time) ?? '09:00';
  const allDay = $derived(f.time === null);
  const k = $derived(f.color || 'var(--accent)');
  const REPEATS: [Repeat, string][] = [['none', 'Hiç'], ['daily', 'Her gün'], ['weekdays', 'Hafta içi'], ['days', 'Günler']];
  const DAYS = ['Pt', 'Sa', 'Ça', 'Pe', 'Cu', 'Ct', 'Pz'];
  const LEADS = [0, LEAD_MINUTES, 15, 30];
  const dateLabel = $derived(parseDay(f.date).toLocaleDateString('tr-TR', { weekday: 'short', day: 'numeric', month: 'long' }));

  function toggleAllDay() {
    if (f.time === null) f.time = lastTime;
    else {
      lastTime = f.time;
      f.time = null;
    }
  }
  const setRepeat = (r: Repeat) => (f = withRepeat(f, r));
  const toggleDay = (i: number) => {
    const days = f.days ^ (1 << i);
    if (days) f.days = days;
  };
  function submit() {
    if (f.title.trim()) onsave({ ...f, title: f.title.trim() });
  }
  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') submit();
    else if (event.key === 'Escape') {
      event.stopPropagation();
      oncancel();
    }
  }
  // Telefon eşliyse "Telefona da hatırlat" satırı çıkar (yalnız bir kez sorulur; olay dinlenmez).
  let phoneName = $state<string | null>(null);
  onMount(() => {
    void call<PhoneStatus>('phone_status').then(s => (phoneName = s?.enabled && s.devices.length ? s.devices.map(d => d.name).join(', ') : null));
  });
  let custom = $state(untrack(() => !PLAN_MINUTES.includes(initial.minutes) && initial.minutes > 0));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={oncancel}></div>
<div class="sheet" role="dialog" aria-label={editing ? 'Planı düzenle' : 'Yeni plan'} style:--k={k}>
  <h3>
    <span class="tile"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={editing ? 'M5 19h4L19 9l-4-4L5 15zM13 7l4 4' : 'M12 5v14M5 12h14'} /></svg></span>
    {editing ? 'Planı düzenle' : 'Yeni plan'}
    <span class="grow"></span>
    <span class="muted">{dateLabel}</span>
  </h3>

  <div class="fld">
    <span class="lb">Başlık</span>
    <span class="inp">
      <span class="dot" style:--k={k}></span>
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={f.title} onfocus={startTyping} {onkeydown} placeholder="Sabah koşusu" maxlength="120" autofocus={!editing} aria-label="Başlık" />
    </span>
  </div>

  <div class="fld">
    <span class="lb">Başlangıç</span>
    {#if allDay}
      <span class="muted">Gün boyu</span>
    {:else}
      <input class="time tnum" type="time" bind:value={f.time} onfocus={startTyping} aria-label="Başlangıç saati" />
      <span class="lb mid">Süre</span>
      <span class="pills">
        {#each PLAN_MINUTES as m (m)}
          <button class="pl" class:on={!custom && f.minutes === m} onclick={() => ((custom = false), (f.minutes = m))}>{m}</button>
        {/each}
        {#if custom}
          <input class="own tnum" type="number" min="1" max="1440" bind:value={f.minutes} onfocus={startTyping} aria-label="Süre (dk)" />
        {:else}
          <button class="pl" onclick={() => (custom = true)} title="Kendi süren (dk)">✎</button>
        {/if}
      </span>
    {/if}
    <span class="grow"></span>
    <button class="pl" class:on={allDay} onclick={toggleAllDay}>Gün boyu</button>
  </div>

  <div class="fld">
    <span class="lb">Renk</span>
    <span class="row">
      {#each PLAN_COLORS as c (c)}
        <button class="sw-c" class:on={f.color === c} style:--k={c} aria-label="Renk {c}" onclick={() => (f.color = c)}></button>
      {/each}
    </span>
  </div>

  <div class="fld">
    <span class="lb">Tekrar</span>
    <span class="seg">
      {#each REPEATS as [r, t] (r)}
        <button class:on={f.repeat === r} onclick={() => setRepeat(r)}>{t}</button>
      {/each}
    </span>
  </div>
  {#if f.repeat === 'days' || f.repeat === 'weekdays'}
    <div class="fld">
      <span class="lb"></span>
      <span class="row">
        {#each DAYS as d, i (d)}
          {@const on = f.repeat === 'weekdays' ? i < 5 : (f.days & (1 << i)) !== 0}
          <button class="dd" class:on disabled={f.repeat === 'weekdays'} onclick={() => toggleDay(i)}>{d}</button>
        {/each}
      </span>
    </div>
  {/if}

  <div class="fld">
    <span class="lb">Haber ver</span>
    {#if f.notify && !allDay}
      <span class="pills">
        {#each LEADS as m (m)}
          <button class="pl" class:on={f.lead === m} onclick={() => (f.lead = m)}>{m ? `${m} dk önce` : 'Saatinde'}</button>
        {/each}
      </span>
    {:else}
      <span class="muted">{allDay ? 'Gün boyu planda haber yok' : 'Çentikte gösterilmez'}</span>
    {/if}
    <span class="grow"></span>
    {#if !allDay}
      <button class="sw" class:on={f.notify} role="switch" aria-checked={f.notify} aria-label="Başlarken çentikte göster" onclick={() => (f.notify = !f.notify)}></button>
    {/if}
  </div>

  {#if phoneName && !allDay}
    <div class="fld">
      <span class="lb">Telefona da hatırlat</span>
      <span class="muted">{phoneName} · bilgisayar kapalıyken de çalar</span>
      <span class="grow"></span>
      <button class="sw" class:on={f.phone} role="switch" aria-checked={f.phone} aria-label="Telefona da hatırlat" onclick={() => (f.phone = !f.phone)}></button>
    </div>
  {/if}

  {#if error}<p class="err">{error}</p>{/if}

  <div class="foot">
    <button class="btn ghost" onclick={() => onloop(f.date)} title="Bu günün tekrarsız planları her gün tekrarlansın">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 0 1 13.7-5.6L20 8.5M20 4v4.5h-4.5M20 12a8 8 0 0 1-13.7 5.6L4 15.5M4 20v-4.5h4.5" /></svg>
      Bugünü her güne uygula
    </button>
    <span class="grow"></span>
    {#if onremove}<button class="btn danger" onclick={onremove}>Sil</button>{/if}
    <button class="btn" onclick={oncancel}>Vazgeç</button>
    <button class="btn acc" disabled={!f.title.trim()} onclick={submit}>Kaydet</button>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--scrim);
    z-index: 4;
  }
  .sheet {
    position: absolute;
    left: 50%;
    top: 6px;
    transform: translateX(-50%);
    width: 580px;
    z-index: 5;
    padding: 14px 16px 12px;
    border-radius: 18px;
    background: linear-gradient(180deg, rgb(40 42 54 / 0.97), rgb(22 23 31 / 0.97));
    border: 1px solid var(--line-strong);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.08),
      0 18px 40px rgb(0 0 0 / 0.6);
    display: flex;
    flex-direction: column;
    gap: 9px;
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
  h3 {
    margin: 0 0 2px;
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: var(--fs-14);
    font-weight: 650;
  }
  .tile {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--k) 22%, transparent);
    color: var(--k);
  }
  .tile svg {
    width: 14px;
    height: 14px;
  }
  .grow {
    flex: 1;
  }
  .muted {
    font-size: var(--fs-12);
    font-weight: 400;
    color: var(--text-3);
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .fld {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
  }
  .lb {
    flex: none;
    width: 74px;
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .lb.mid {
    width: auto;
    margin-left: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .inp {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 10px;
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 55%, transparent);
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--k);
  }
  input {
    font: inherit;
    font-size: var(--fs-12-5);
    color: var(--text);
    background: none;
    border: 0;
    outline: none;
    min-width: 0;
    color-scheme: dark;
  }
  .inp input {
    flex: 1;
  }
  .time,
  .own {
    height: 28px;
    padding: 0 8px;
    border-radius: 9px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .time {
    width: 96px;
  }
  .own {
    width: 58px;
    text-align: center;
  }
  .pills {
    display: flex;
    gap: 5px;
  }
  .pl {
    white-space: nowrap;
    border-radius: 99px;
    padding: 3px 10px;
    font-size: var(--fs-12);
    color: var(--text-2);
    background: var(--surface-3);
  }
  .pl.on {
    color: var(--text);
    background: color-mix(in srgb, var(--k) 26%, var(--surface-3));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 60%, transparent);
  }
  .sw-c {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--k);
  }
  .sw-c.on {
    box-shadow:
      0 0 0 2px #15161e,
      0 0 0 3.5px var(--k);
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .seg button {
    padding: 3px 11px;
    border-radius: 8px;
    font-size: var(--fs-12);
    color: var(--text-3);
  }
  .seg button.on {
    background: color-mix(in srgb, var(--k) 26%, var(--surface-3));
    color: var(--text);
  }
  .dd {
    width: 28px;
    height: 24px;
    border-radius: 8px;
    font-size: var(--fs-11);
    color: var(--text-3);
    background: var(--surface-2);
  }
  .dd.on {
    color: var(--text);
    background: color-mix(in srgb, var(--k) 30%, var(--surface-3));
  }
  .dd:disabled {
    cursor: default;
  }
  .sw {
    position: relative;
    flex: none;
    width: 30px;
    height: 17px;
    border-radius: 99px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .sw::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: var(--text-3);
    transition: transform var(--t-fast) var(--ease);
  }
  .sw.on {
    background: color-mix(in srgb, var(--k) 70%, #000);
    box-shadow: none;
  }
  .sw.on::after {
    transform: translateX(13px);
    background: #fff;
  }
  .err {
    margin: 0;
    font-size: var(--fs-11-5);
    color: var(--amber);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--line);
    border-radius: 11px;
    padding: 5px 12px;
    font-size: var(--fs-12);
    background: var(--surface-2);
    color: var(--text-2);
  }
  .btn:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .btn svg {
    width: 13px;
    height: 13px;
  }
  .btn.ghost {
    background: none;
  }
  .btn.danger:hover {
    color: var(--red);
  }
  .btn.acc {
    background: var(--accent);
    border-color: transparent;
    color: var(--on-accent);
    font-weight: 650;
  }
  .btn.acc:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
