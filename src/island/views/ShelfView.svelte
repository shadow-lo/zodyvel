<script lang="ts">
  // Dosya rafı + Raf → AI (Faz 6, K5; design/eskiz-paket2.html ekran 7). Solda açık AI oturumlarının adları
  // ("Oturuma gönder"), sağda raf. Adaya sürüklenen dosya bir adın üstüne bırakılırsa o oturuma gider:
  // Zodyvel'in başlattığı görevde dosya gerçekten eklenir, diğerlerinde yol panoya kopyalanır ve pencere
  // öne gelir. Sürükleme sürerken imlecin yerini Rust bildirir (shelf://over); altındaki ad parlar ve hedef
  // seçilir. Raftaki kart tutulunca önce adanın içinde sürüklenir (adlara bırakılabilir); kart adadan
  // çıkınca Windows sürüklemesine geçer (Gezgin'den sürüklemek gibi; dosya kopyalanır, yerinden taşınmaz).
  // Çift tık: Gezgin'de göster. Sürekli animasyon yok; yalnız opacity/transform geçişi.
  import { untrack } from 'svelte';
  import ToolZody from '../parts/ToolZody.svelte';
  import { agentMood } from '../../lib/mood';
  import { dropText, folderLabel, kindLabel, outside, pastThreshold, sendChip, sentText, sessionSub } from '../../lib/shelf';
  import { data, shelfDragOut, shelfRemove, shelfReveal, shelfSend, shelfTarget, type IslandContext } from '../../lib/store.svelte';
  import type { ShelfItem } from '../../lib/types';

  interface Props {
    ctx: IslandContext;
    detail: boolean;
    /** Son bırakmada eklenen dosya sayısı (kısa not); null: not yok. */
    dropped: number | null;
    /** Dışarı sürükleme bitti (bırakıldı ya da vazgeçildi). */
    ondone: () => void;
    /** "Claude'a sor": yazma sahnesi bu dosya ekli açılır (özellik kapalıyken verilmez, düğme çıkmaz). */
    onask?: (item: ShelfItem) => void;
  }
  let { ctx, detail, dropped, ondone, onask }: Props = $props();

  const SENT_MS = 2600;

  const dragging = $derived(data.shelfDrag);
  const sessions = $derived(ctx.agents);
  /** İmlecin üstünde durduğu oturum (dışarıdan sürükleme ya da raftaki kart). */
  let hot = $state<string | null>(null);
  let body = $state<HTMLElement>();
  let press: { x: number; y: number; item: ShelfItem } | null = null;
  /** Raftaki kart adanın içinde sürükleniyor: hayalet kart imleci izler. */
  let inner = $state<{ item: ShelfItem; x: number; y: number } | null>(null);
  let note = $state<{ text: string; ok: boolean } | null>(null);
  let noteTimer: ReturnType<typeof setTimeout> | undefined;

  function sessionAt(x: number, y: number): string | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-session]');
    return el?.dataset.session ?? null;
  }

  // Dışarıdan dosya sürüklenirken: imlecin altındaki ad hedef olur (Rust bırakınca oraya gönderir).
  $effect(() => {
    const point = data.shelfOver;
    const id = point && dragging !== null ? sessionAt(point.x, point.y) : null;
    if (id === untrack(() => hot) || untrack(() => inner)) return;
    hot = id;
    shelfTarget(id);
  });

  // Gönderim sonucu: kısa not, sonra kaybolur.
  $effect(() => {
    const sent = data.shelfSent;
    if (!sent || Date.now() - sent.at > SENT_MS) return;
    const name = untrack(() => sessions.find(s => s.id === sent.session)?.project) ?? 'oturum';
    note = { text: sentText(sent, name), ok: !sent.error };
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = null), SENT_MS);
  });

  function onpointerdown(event: PointerEvent, item: ShelfItem) {
    if (event.button !== 0 || (event.target as Element).closest('.x, .ask')) return;
    press = { x: event.clientX, y: event.clientY, item };
  }

  function toWindows(item: ShelfItem) {
    inner = null;
    hot = null;
    void shelfDragOut([item]).finally(ondone);
  }

  function onpointermove(event: PointerEvent) {
    if (inner) {
      const rect = body?.getBoundingClientRect();
      if (rect && outside(event.clientX, event.clientY, rect)) {
        const item = inner.item;
        (event.currentTarget as Element).releasePointerCapture(event.pointerId);
        toWindows(item);
        return;
      }
      inner = { ...inner, x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
      hot = sessionAt(event.clientX, event.clientY);
      return;
    }
    if (!press) return;
    if (!(event.buttons & 1)) {
      press = null;
      return;
    }
    if (!pastThreshold(event.clientX - press.x, event.clientY - press.y)) return;
    const item = press.item;
    press = null;
    // Gönderilecek oturum yoksa eskisi gibi doğrudan Windows sürüklemesi.
    if (sessions.length === 0) {
      toWindows(item);
      return;
    }
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
    const rect = body?.getBoundingClientRect();
    inner = { item, x: event.clientX - (rect?.left ?? 0), y: event.clientY - (rect?.top ?? 0) };
  }

  function onpointerup() {
    press = null;
    if (!inner) return;
    const item = inner.item;
    const target = sessions.find(s => s.id === hot);
    inner = null;
    hot = null;
    if (target) void shelfSend(target, [item]);
  }

  function cancelInner() {
    if (!inner) return;
    inner = null;
    hot = null;
  }
</script>

<svelte:window onpointerup={() => (press = null)} />

<div class="body" class:detail class:split={sessions.length > 0} bind:this={body}>
  {#if sessions.length > 0}
    <section class="card send" aria-label="Oturuma gönder">
      <div class="tt">Oturuma gönder<small>Dosyayı bir adın üstüne bırak</small></div>
      <ul class="sessions" data-scroll>
        {#each sessions as s (s.id)}
          {@const on = hot === s.id && (dragging !== null || inner !== null)}
          <li class="ses" class:on data-session={s.id}>
            <ToolZody tool={s.tool} mood={agentMood(s.status)} size={30} />
            <span class="who"><b>{s.project}</b><small>{sessionSub(s)}</small></span>
            <span class="how" class:real={!!s.attach}>{sendChip(!!s.attach, on)}</span>
          </li>
        {/each}
      </ul>
      <p class="foot">Terminal ve masaüstü oturumlarında dosya yolu panoya kopyalanır, o pencere öne gelir; yapıştırman yeter.</p>
    </section>
  {/if}

  <section class="card shelf">
    {#if dragging !== null && !hot}
      <div class="drop" aria-live="polite">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11M7 10l5 5 5-5M5 19h14" /></svg>
        <b>Bırak: {dragging} dosya rafa</b>
        <small>{sessions.length ? 'Ya da soldaki bir oturumun adına bırak.' : 'Dosyalar yerinde kalır; raf yalnız yerlerini hatırlar.'}</small>
      </div>
    {:else}
      <header>
        <h4>Raf<small>{ctx.shelf.length} öğe</small></h4>
        <span class="hint">{dropped !== null ? dropText(dropped) : sessions.length ? 'Kartı bir oturuma ya da başka bir uygulamaya sürükle' : 'Tutup başka bir uygulamaya sürükle · çift tık: klasörde göster'}</span>
        {#if ctx.shelf.length}
          <button class="clear" onclick={() => shelfRemove()} title="Rafı boşalt (dosyalar silinmez)">Temizle</button>
        {/if}
      </header>
      {#if ctx.shelf.length === 0}
        <p class="none">Raf boş. Bir dosyayı ekranın üstündeki adaya sürükle.</p>
      {:else}
        <ul class="items" data-scroll>
          {#each ctx.shelf as item (item.id)}
            <li
              class="item"
              class:lift={inner?.item.id === item.id}
              title={item.path}
              draggable="false"
              onpointerdown={e => onpointerdown(e, item)}
              {onpointermove}
              {onpointerup}
              onlostpointercapture={cancelInner}
              ondblclick={() => shelfReveal(item)}
              ondragstart={e => e.preventDefault()}
            >
              <span class="kind" class:dir={item.dir}>
                {#if item.dir}
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" /></svg>
                {:else}
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 3.5h7l4.5 4.5v11a1.5 1.5 0 0 1-1.5 1.5h-10A1.5 1.5 0 0 1 5 19V5a1.5 1.5 0 0 1 1.5-1.5z" /><path d="M13.5 3.5V8H18" /></svg>
                {/if}
                <em>{kindLabel(item)}</em>
              </span>
              <span class="name">{item.name}</span>
              <small>{folderLabel(item.path)}</small>
              <button class="x" title="Raftan kaldır" aria-label="{item.name} raftan kaldır" onclick={() => shelfRemove(item.id)}>×</button>
              {#if onask}
                <button class="ask" title="Claude'a sor: yazma sahnesi bu {item.dir ? 'klasör' : 'dosya'} ekli açılır" aria-label="{item.name} için Claude'a sor" ondblclick={e => e.stopPropagation()} onclick={() => onask(item)}>✎ Sor</button>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>

  {#if inner}
    <div class="ghost" style:transform="translate({inner.x + 12}px, {inner.y + 10}px) rotate(-5deg)" aria-hidden="true">
      <em>{kindLabel(inner.item)}</em><span>{inner.item.name}</span>
    </div>
  {/if}

  {#if note}
    <div class="toast" class:bad={!note.ok} role="status">{note.text}</div>
  {/if}
</div>

<style>
  .body {
    position: relative;
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 10px;
  }
  .body.split {
    grid-template-columns: 270px minmax(0, 1fr);
  }
  .card {
    min-height: 0;
    min-width: 0;
    border-radius: 16px;
    background: var(--surface);
    background-image: var(--card-sheen);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
  }
  /* Oturuma gönder */
  .send {
    padding: 10px 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tt {
    padding: 0 6px 6px;
    font-size: var(--fs-13-5);
    font-weight: 650;
  }
  .tt small {
    display: block;
    font-size: var(--fs-11);
    color: var(--text-3);
    font-weight: 400;
  }
  .sessions {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .ses {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 4px;
    border-radius: 12px;
    border: 1px dashed transparent;
    transition:
      background-color var(--t-fast),
      border-color var(--t-fast);
  }
  .who {
    min-width: 0;
    flex: 1;
  }
  .who b {
    display: block;
    font-size: var(--fs-13);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who small {
    display: block;
    font-size: var(--fs-11);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .how {
    flex: none;
    font-size: var(--fs-10-5);
    padding: 2px 7px;
    border-radius: 99px;
    white-space: nowrap;
    color: var(--text-3);
    background: var(--surface);
  }
  .how.real {
    color: var(--green);
    background: color-mix(in srgb, var(--green) 12%, transparent);
  }
  .ses.on {
    background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 24%, transparent), color-mix(in srgb, var(--accent) 8%, transparent));
    border-color: color-mix(in srgb, var(--accent) 80%, transparent);
    box-shadow: 0 0 22px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .ses.on .how {
    color: #fff;
    background: var(--accent);
  }
  .foot {
    margin: 0;
    font-size: var(--fs-11);
    color: var(--text-3);
    line-height: 1.45;
    padding: 8px 6px 2px;
    border-top: 1px solid var(--line);
  }
  /* Raf */
  .shelf {
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 0 4px;
  }
  h4 {
    margin: 0;
    font-size: var(--fs-12-5);
    font-weight: 650;
  }
  h4 small {
    margin-left: 6px;
    color: var(--text-3);
    font-weight: 500;
    font-size: var(--fs-11);
  }
  .hint {
    flex: 1;
    min-width: 0;
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .clear {
    color: var(--text-2);
    font-size: var(--fs-11);
    padding: 2px 8px;
    border-radius: 7px;
  }
  .clear:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .none {
    flex: 1;
    margin: 0;
    display: grid;
    place-items: center;
    color: var(--text-3);
    font-size: var(--fs-12);
    text-align: center;
  }
  .items {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    grid-auto-rows: 108px;
    gap: 8px;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: none;
  }
  .item {
    position: relative;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    cursor: grab;
    user-select: none;
    touch-action: none;
    transition:
      border-color var(--t-fast),
      opacity var(--t-fast);
  }
  .item:hover {
    border-color: var(--line-strong);
  }
  .item:active {
    cursor: grabbing;
  }
  .item.lift {
    opacity: 0.35;
    border-style: dashed;
  }
  .item.lift .x,
  .item.lift .ask,
  .item.lift:hover .x,
  .item.lift:hover .ask {
    opacity: 0;
  }
  .kind {
    position: relative;
    width: 34px;
    height: 34px;
    margin-bottom: 4px;
    color: var(--text-2);
  }
  .kind svg {
    width: 34px;
    height: 34px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
    stroke-linejoin: round;
  }
  .kind.dir {
    color: var(--amber);
  }
  .kind.dir svg {
    fill: color-mix(in srgb, var(--amber) 18%, transparent);
  }
  .kind em,
  .ghost em {
    position: absolute;
    left: 14px;
    bottom: 2px;
    padding: 1px 4px;
    border-radius: 4px;
    font-style: normal;
    font-size: var(--fs-9);
    font-weight: 700;
    letter-spacing: 0.02em;
    white-space: nowrap;
    color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 18%, var(--surface));
  }
  .kind.dir em {
    display: none;
  }
  .name {
    font-size: var(--fs-12);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item small {
    color: var(--text-3);
    font-size: var(--fs-10-5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .x {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 22px;
    height: 22px;
    border-radius: 7px;
    color: var(--text-3);
    font-size: var(--fs-14);
    line-height: 1;
    opacity: 0;
    transition: opacity var(--t-fast);
  }
  .item:hover .x,
  .x:focus-visible {
    opacity: 1;
  }
  .x:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  /* "✎ Sor" yalnız Yeni görünümde verilir (Island.svelte); kartın üstüne gelince belirir. */
  .ask {
    position: absolute;
    top: 6px;
    right: 32px;
    height: 22px;
    padding: 0 7px;
    border-radius: 7px;
    color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 14%, var(--surface));
    font-size: var(--fs-10-5);
    font-weight: 600;
    opacity: 0;
    transition: opacity var(--t-fast);
  }
  .item:hover .ask,
  .ask:focus-visible {
    opacity: 1;
  }
  .ask:hover {
    background: color-mix(in srgb, var(--accent) 24%, var(--surface));
  }
  .drop {
    flex: 1;
    border: 1.5px dashed color-mix(in srgb, var(--accent) 55%, transparent);
    border-radius: 14px;
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    color: var(--text-2);
    text-align: center;
    animation: drop-in var(--t-fast) var(--ease) both;
  }
  .drop svg {
    width: 26px;
    height: 26px;
    fill: none;
    stroke: var(--accent-soft);
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .drop b {
    color: var(--text);
    font-size: var(--fs-13-5);
  }
  .drop small {
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  @keyframes drop-in {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
  }
  /* Raftaki kart adanın içinde sürüklenirken imleci izleyen küçük kart (yalnız transform). */
  .ghost {
    position: absolute;
    left: 0;
    top: 0;
    z-index: 8;
    width: 112px;
    padding: 22px 8px 8px;
    border-radius: 12px;
    background: var(--pop-bg);
    border: 1px solid color-mix(in srgb, var(--accent) 60%, transparent);
    box-shadow:
      0 18px 40px rgb(0 0 0 / 0.6),
      0 0 24px color-mix(in srgb, var(--accent) 35%, transparent);
    pointer-events: none;
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ghost em {
    left: 8px;
    top: 6px;
    bottom: auto;
  }
  .toast {
    position: absolute;
    z-index: 7;
    left: 50%;
    bottom: 16px;
    max-width: calc(100% - 40px);
    transform: translateX(-50%);
    padding: 7px 14px;
    border-radius: 99px;
    background: var(--pop-bg);
    border: 1px solid color-mix(in srgb, var(--green) 40%, transparent);
    box-shadow:
      0 0 22px color-mix(in srgb, var(--green) 25%, transparent),
      0 10px 30px rgb(0 0 0 / 0.6);
    font-size: var(--fs-12);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    animation: toast-in var(--t-base) var(--ease) both;
  }
  .toast.bad {
    border-color: color-mix(in srgb, var(--red) 45%, transparent);
    box-shadow: var(--pop-shadow);
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 10px);
    }
  }
</style>
