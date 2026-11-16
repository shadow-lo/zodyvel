<script lang="ts">
  // Ajanda › Gün: yatay saat çizelgesi (12 saatlik pencere, ‹ › ile 6'şar saat kayar). Geçmiş saatler kararır,
  // şu anki blok dolarak ilerler (ayrı ilerleme çubuğu yok), beyaz çizgi "şimdi". Bloğa basınca düzenleme,
  // sürükleyince saat 15 dakikalık adımlarla değişir (sürüklerken yalnız transform). Boş yere çift tıklama o
  // saate yeni plan açar.
  import { hm, snapMinute, type Block } from '../../lib/agenda';
  import type { CalEvent } from '../../lib/types';

  interface Props {
    blocks: Block[];
    allDay: CalEvent[];
    date: string;
    today: string;
    nowMinute: number;
    onedit: (e: CalEvent) => void;
    onmove: (e: CalEvent, start: number) => void;
    onadd: (minute: number) => void;
  }
  let { blocks, allDay, date, today, nowMinute, onedit, onmove, onadd }: Props = $props();

  const SPAN = 12;
  // Pencerenin ilk saati: bugünde şu andan 3 saat önce, başka günde ilk planın bir saat öncesi.
  function firstHour(): number {
    const from = date === today ? Math.floor(nowMinute / 60) - 3 : blocks[0] ? Math.floor(blocks[0].start / 60) - 1 : 8;
    return Math.max(0, Math.min(24 - SPAN, from));
  }
  let h0 = $state(0);
  let shownFor = '';
  $effect.pre(() => {
    if (date !== shownFor) {
      shownFor = date;
      h0 = firstHour();
    }
  });

  let width = $state(600);
  const ph = $derived(width / SPAN);
  const x = (minute: number) => (minute / 60 - h0) * ph;
  const before = $derived(blocks.filter(b => b.end <= h0 * 60).length);
  const after = $derived(blocks.filter(b => b.start >= (h0 + SPAN) * 60).length);
  const past = $derived(date < today ? width : date > today ? 0 : Math.max(0, Math.min(width, x(nowMinute))));
  const showNow = $derived(date === today && nowMinute >= h0 * 60 && nowMinute <= (h0 + SPAN) * 60);

  // Sürükleme: yalnız transform; bırakınca bir kez kaydedilir.
  let drag = $state<{ id: number; x0: number; dx: number; moved: boolean } | null>(null);
  function down(event: PointerEvent, b: Block) {
    if (event.button !== 0) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { id: b.event.id, x0: event.clientX, dx: 0, moved: false };
  }
  function moveTo(event: PointerEvent) {
    if (!drag) return;
    const dx = event.clientX - drag.x0;
    drag = { ...drag, dx, moved: drag.moved || Math.abs(dx) > 4 };
  }
  const dragged = (b: Block) => snapMinute(b.start + (drag && drag.id === b.event.id ? (drag.dx / ph) * 60 : 0), b.end - b.start);
  function up(b: Block) {
    if (!drag) return;
    const { moved } = drag;
    const start = dragged(b);
    drag = null;
    if (!moved) onedit(b.event);
    else if (start !== b.start) onmove(b.event, start);
  }
  function dbl(event: MouseEvent) {
    if ((event.target as Element).closest('.blk')) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    onadd(snapMinute(h0 * 60 + ((event.clientX - rect.left) / ph) * 60, 60));
  }
</script>

<div class="tl">
  <div class="nav">
    {#if allDay.length}
      <span class="allday">
        Gün boyu:
        {#each allDay as e (e.id)}<button class="chip" style:--k={e.color || 'var(--coral)'} onclick={() => onedit(e)}>{e.title}</button>{/each}
      </span>
    {/if}
    <span class="grow"></span>
    {#if h0 > 0}
      <button class="mv" onclick={() => (h0 = Math.max(0, h0 - 6))}>‹ {h0 <= 6 ? 'Sabah' : 'Önce'}{before ? `: ${before} plan` : ''}</button>
    {/if}
    {#if h0 < 24 - SPAN}
      <button class="mv" onclick={() => (h0 = Math.min(24 - SPAN, h0 + 6))}>{h0 + SPAN >= 18 ? 'Gece' : 'Sonra'}{after ? `: ${after} plan` : ''} ›</button>
    {/if}
    <span class="hint">· Bloğu sürükle, saatini değiştir</span>
  </div>
  <div class="ruler" aria-hidden="true">
    {#each { length: SPAN - 1 } as _, i (i)}
      <span style:left="{(i + 1) * ph}px">{String(h0 + i + 1).padStart(2, '0')}</span>
    {/each}
  </div>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="lanes" bind:clientWidth={width} ondblclick={dbl} style:--ph="{ph}px">
    <div class="past" style:width="{past}px"></div>
    {#each blocks as b (b.event.id)}
      {@const left = x(b.start)}
      {@const w = ((b.end - b.start) / 60) * ph - 3}
      {@const on = date === today && nowMinute >= b.start && nowMinute < b.end}
      {@const moving = drag?.id === b.event.id && drag.moved}
      {#if left + w > 0 && left < width}
        <button
          class="blk"
          class:now={on}
          class:low={b.lane === 1}
          class:moving
          style:--k={b.event.color || 'var(--accent)'}
          style:left="{left + 1}px"
          style:width="{w}px"
          style:transform={moving && drag ? `translateX(${drag.dx}px)` : undefined}
          title="{b.event.title} · {hm(b.start)}{b.event.repeat && b.event.repeat !== 'none' ? ' · tekrarlı: sürükleyince her günü kayar' : ''}"
          onpointerdown={e => down(e, b)}
          onpointermove={moveTo}
          onpointerup={() => up(b)}
          onpointercancel={() => (drag = null)}
        >
          {#if on}<span class="fill" style:width="{((nowMinute - b.start) / (b.end - b.start)) * 100}%"></span>{/if}
          {#if w > 40}<b>{b.event.title}</b><small class="tnum">{moving ? hm(dragged(b)) : hm(b.start)}</small>{/if}
          {#if b.event.repeat && b.event.repeat !== 'none' && w > 74}
            <svg class="rp" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 0 1 13.7-5.6L20 8.5M20 4v4.5h-4.5M20 12a8 8 0 0 1-13.7 5.6L4 15.5M4 20v-4.5h4.5" /></svg>
          {/if}
        </button>
      {/if}
    {/each}
    {#if showNow}<span class="nowl" style:left="{x(nowMinute) - 1}px" data-t={hm(nowMinute)}></span>{/if}
    {#if !blocks.length}<span class="empty">Boş gün · bir saate çift tıkla ya da "+ Plan"</span>{/if}
  </div>
</div>

<style>
  .tl {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  button {
    font: inherit;
    color: inherit;
    cursor: pointer;
    border: 0;
    background: none;
    padding: 0;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-11);
    color: var(--text-3);
    min-height: 18px;
  }
  .grow {
    flex: 1;
  }
  .allday {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .chip {
    padding: 1px 8px;
    border-radius: 99px;
    background: color-mix(in srgb, var(--k) 20%, transparent);
    color: var(--text);
    font-size: var(--fs-11);
  }
  .mv {
    color: var(--text-2);
    padding: 1px 5px;
    border-radius: 6px;
  }
  .mv:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .hint {
    white-space: nowrap;
  }
  .ruler {
    position: relative;
    height: 14px;
    font-size: var(--fs-10);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .ruler span {
    position: absolute;
    transform: translateX(-50%);
  }
  .lanes {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: 10px;
    overflow: hidden;
    /* Saat çizgileri: her saatte ince dikey çizgi (durağan arka plan). */
    background:
      repeating-linear-gradient(90deg, transparent 0 calc(var(--ph) - 1px), rgb(255 255 255 / 0.05) calc(var(--ph) - 1px) var(--ph)),
      rgb(255 255 255 / 0.02);
  }
  .past {
    position: absolute;
    inset: 0 auto 0 0;
    background: rgb(0 0 0 / 0.35);
    pointer-events: none;
  }
  .blk {
    position: absolute;
    top: 6px;
    height: calc(50% - 9px);
    border-radius: 9px;
    padding: 5px 8px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    text-align: left;
    background: color-mix(in srgb, var(--k) 22%, rgb(0 0 0 / 0.4));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 45%, transparent);
    touch-action: none;
    user-select: none;
  }
  .blk.low {
    top: calc(50% + 3px);
  }
  .blk:hover {
    background: color-mix(in srgb, var(--k) 32%, rgb(0 0 0 / 0.4));
  }
  /* Şu anki blok: kendi renginde parlar ve geçen kısmı dolu. */
  .blk.now {
    box-shadow:
      inset 0 0 0 1.5px var(--k),
      0 0 16px color-mix(in srgb, var(--k) 45%, transparent);
  }
  .blk.moving {
    z-index: 2;
    cursor: grabbing;
    box-shadow:
      inset 0 0 0 1.5px var(--k),
      0 6px 18px rgb(0 0 0 / 0.5);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    background: color-mix(in srgb, var(--k) 38%, transparent);
    pointer-events: none;
  }
  .blk b,
  .blk small {
    position: relative;
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .blk b {
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .blk small {
    font-size: var(--fs-10-5);
    color: var(--text-2);
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .rp {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 11px;
    height: 11px;
    fill: none;
    stroke: var(--text-3);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .nowl {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: #fff;
    box-shadow: 0 0 8px rgb(255 255 255 / 0.6);
    pointer-events: none;
    z-index: 1;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: var(--fs-11-5);
    color: var(--text-3);
    pointer-events: none;
  }
</style>
