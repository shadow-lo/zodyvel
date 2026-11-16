<script lang="ts">
  // Zodymetreler (Faz 5a, K8): bugünkü tek zamanlayıcının yerine adlı, süreli, renkli en çok üç sayaç.
  // Süreyi Rust tutar (ada kapalıyken de sürer); burada yalnız gösterilir. Saniyelik sayaç yalnız kart
  // ekrandayken ve en az biri çalışırken ilerler. ✎ ile ad, süre, renk ve ses düzenlenir; "+" yenisini ekler.
  // Odak döngüsü Modlar'a taşındı.
  import { clockText } from '../../lib/clock-cards';
  import { startTyping, todayApply } from '../../lib/store.svelte';
  import { MAX_METERS, MAX_TIMER_MIN, METER_COLORS, METER_PRESETS, timerLeft } from '../../lib/today';
  import type { MeterState } from '../../lib/types';
  import Ring from './Ring.svelte';

  let { meters }: { meters: MeterState[] } = $props();

  let now = $state(Date.now());
  const ticking = $derived(meters.some(m => m.endsAt !== null));
  $effect(() => {
    if (!ticking) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  // Düzenleme: mevcut bir Zodymetre (id) ya da yenisi ('new').
  let editing = $state<number | 'new' | null>(null);
  let name = $state('');
  let minutes = $state(25);
  let own = $state('');
  let color = $state('');
  let sound = $state(true);
  let error = $state<string | null>(null);

  function edit(m: MeterState | null) {
    editing = m ? m.id : 'new';
    name = m?.name ?? '';
    minutes = m?.minutes ?? 25;
    own = METER_PRESETS.includes(minutes) ? '' : String(minutes);
    color = m?.color ?? METER_COLORS[(meters.length + 1) % METER_COLORS.length] ?? '';
    sound = m?.sound ?? true;
    error = null;
  }

  function pickedMinutes(): number | null {
    if (!own.trim()) return minutes;
    const n = Number(/^(\d{1,3})\s*(dk|dakika)?$/i.exec(own.trim())?.[1] ?? NaN);
    return Number.isInteger(n) && n >= 1 && n <= MAX_TIMER_MIN ? n : null;
  }

  async function save() {
    const m = pickedMinutes();
    if (m === null) return void (error = `Süre 1–${MAX_TIMER_MIN} dk olmalı.`);
    const body = { name: name.trim(), minutes: m, color, sound };
    error = editing === 'new' ? await todayApply({ op: 'meterAdd', ...body }) : await todayApply({ op: 'meterEdit', id: editing as number, ...body });
    if (!error) editing = null;
  }

  async function remove() {
    if (typeof editing !== 'number') return;
    error = await todayApply({ op: 'meterRemove', id: editing });
    if (!error) editing = null;
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void save();
    } else if (event.key === 'Escape') {
      event.stopPropagation();
      editing = null;
    }
  }

  const meterView = (m: MeterState) => {
    const left = timerLeft(m, now);
    const running = m.endsAt !== null;
    const done = !running && m.leftMs === 0;
    return { left, running, done, ratio: done ? 1 : Math.min(1, left / Math.max(1, m.minutes * 60_000)), paused: !running && !done && left < m.minutes * 60_000 };
  };
</script>

<div class="meters">
  {#if editing !== null}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="form" style:--k={color || 'var(--accent)'} {onkeydown}>
      <div class="row">
        <input class="name" maxlength="24" placeholder="Ad (ör. Okuma)" aria-label="Zodymetre adı" bind:value={name} onfocus={startTyping} />
        <span class="swatches" role="radiogroup" aria-label="Renk">
          {#each METER_COLORS as c (c)}
            <button class="sw" class:on={color === c} style:--c={c || 'var(--accent)'} role="radio" aria-checked={color === c} aria-label={c || 'Vurgu rengi'} onclick={() => (color = c)}></button>
          {/each}
        </span>
        <button class="chip" class:on={sound} aria-pressed={sound} title="Dolunca ses" onclick={() => (sound = !sound)}>{sound ? 'Sesli' : 'Sessiz'}</button>
      </div>
      <div class="row">
        {#each METER_PRESETS as p (p)}
          <button class="chip" class:on={!own.trim() && minutes === p} onclick={() => ((minutes = p), (own = ''))}>{p} dk</button>
        {/each}
        <input class="own" class:on={own.trim() !== ''} inputmode="numeric" maxlength="7" placeholder="dk" aria-label="Kendi süren, dakika" bind:value={own} onfocus={startTyping} />
        <span class="grow"></span>
        {#if typeof editing === 'number'}<button class="plain danger" onclick={remove}>Sil</button>{/if}
        <button class="plain" onclick={() => (editing = null)}>Vazgeç</button>
        <button class="go" onclick={save}>{editing === 'new' ? 'Ekle' : 'Kaydet'}</button>
      </div>
      {#if error}<span class="err" role="alert">{error}</span>{/if}
    </div>
  {:else}
    {#each meters as m (m.id)}
      {@const s = meterView(m)}
      <div class="meter" class:done={s.done} style:--k={m.color || 'var(--accent)'}>
        <span class="dial">
          <Ring ratio={s.ratio} size={50} stroke={4} color="var(--k)" />
          <span class="t">{s.done ? '✓' : clockText(s.left)}</span>
        </span>
        <span class="info">
          <b title={m.name}>{m.name}</b>
          <small>{s.done ? 'doldu' : s.running ? `${m.minutes} dk` : s.paused ? 'duraklatıldı' : `${m.minutes} dk`}{m.rounds && m.day ? ` · ${m.rounds}. tur` : ''}</small>
          <span class="acts">
            <button class="go" aria-label={s.running ? `${m.name} duraklat` : `${m.name} başlat`} onclick={() => todayApply({ op: s.running ? 'timerPause' : 'timerStart', id: m.id })}>
              {s.running ? 'Duraklat' : s.done ? 'Yeniden' : s.paused ? 'Devam' : 'Başlat'}
            </button>
            {#if s.paused || s.running}
              <button class="ib" title="Sıfırla" aria-label="{m.name} sıfırla" onclick={() => todayApply({ op: 'timerReset', id: m.id })}>
                <svg viewBox="0 0 24 24"><path d="M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4" /></svg>
              </button>
            {/if}
            <button class="ib" title="Düzenle" aria-label="{m.name} düzenle" onclick={() => edit(m)}>
              <svg viewBox="0 0 24 24"><path d="M4 20h4L19 9l-4-4L4 16z" /></svg>
            </button>
          </span>
        </span>
      </div>
    {/each}
    {#if meters.length < MAX_METERS}
      <button class="add" title="Zodymetre ekle (en çok {MAX_METERS})" onclick={() => edit(null)}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
        {#if meters.length === 0}<span>Zodymetre ekle</span>{/if}
      </button>
    {/if}
  {/if}
</div>

<style>
  .meters {
    display: flex;
    gap: 6px;
    min-width: 0;
    height: 100%;
  }
  .meter,
  .form,
  .add {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
  }
  .meter {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 9px;
    background: linear-gradient(160deg, color-mix(in srgb, var(--k) 10%, var(--surface)), var(--surface) 60%);
  }
  .dial {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
  }
  .dial .t {
    position: absolute;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    color: var(--text);
  }
  .meter.done .t {
    color: var(--green);
    font-size: 15px;
  }
  .info {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .info b {
    font-size: var(--fs-12-5, 12.5px);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .info small {
    font-size: var(--fs-11, 11px);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 3px;
    margin-top: 4px;
  }
  button {
    font: inherit;
    cursor: pointer;
  }
  .go {
    border: 0;
    border-radius: 9px;
    padding: 3px 9px;
    font-size: var(--fs-11, 11px);
    font-weight: 600;
    color: #fff;
    background: color-mix(in srgb, var(--k, var(--accent)) 70%, #000);
  }
  .go:hover {
    background: var(--k, var(--accent));
  }
  .ib {
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 7px;
    display: grid;
    place-items: center;
    background: transparent;
    color: var(--text-3);
  }
  .ib:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .ib svg,
  .add svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .add {
    flex: none;
    min-width: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 10px;
    color: var(--text-3);
    border-style: dashed;
    background: transparent;
    box-shadow: none;
  }
  .add:only-child {
    flex: 1;
  }
  .add:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .form {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    padding: 7px 10px;
    position: relative;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .grow {
    flex: 1;
  }
  input {
    font: inherit;
    font-size: var(--fs-12, 12px);
    color: var(--text);
    background: var(--surface-3);
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 3px 8px;
    min-width: 0;
    outline: none;
  }
  input:focus {
    border-color: var(--k);
  }
  .name {
    flex: 1;
  }
  .own {
    width: 46px;
    text-align: center;
  }
  .own.on {
    border-color: var(--k);
  }
  .swatches {
    display: flex;
    gap: 3px;
  }
  .sw {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 0;
    padding: 0;
    background: var(--c);
    opacity: 0.55;
  }
  .sw.on {
    opacity: 1;
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 3.5px var(--c);
  }
  .chip {
    border: 0;
    border-radius: 8px;
    padding: 3px 7px;
    font-size: var(--fs-11, 11px);
    color: var(--text-3);
    background: var(--surface-3);
    white-space: nowrap;
  }
  .chip.on {
    color: var(--text);
    box-shadow: inset 0 0 0 1.5px var(--k);
  }
  .plain {
    border: 0;
    background: transparent;
    color: var(--text-3);
    font-size: var(--fs-11, 11px);
    padding: 3px 6px;
  }
  .plain:hover {
    color: var(--text);
  }
  .danger:hover {
    color: var(--red);
  }
  .err {
    position: absolute;
    right: 10px;
    top: 4px;
    font-size: 10.5px;
    color: var(--red);
  }
</style>
