<script lang="ts">
  // Canlı çentik (Faz 4; eskiz design/eskiz-paket2.html §8–9, lib/live.ts; karar 9 Kasım 22:38, 22:50).
  // · Sağda geniş hap (olay, müzik ya da süren etkinlik), solunda ayrı küçük baloncuklar; her biri basınca
  //   kendi sekmesini açar, fareyle gelince (ara boy kipinde) o hap genişleyip ayrıntıyı yazar.
  // · Zody çentikte çerçevesiz, yalnız gözleriyle durur: hapın kendisi onun ekranıdır.
  // · Hapın çevresinde durum renginde çerçeve ışığı: soldan iner, alttan geçer, sağdan çıkar (üstte yok).
  //   AI çalışırken aracın renginde (Claude turuncu) şerit şerit soldan sağa akar; taze onay sarı, bitti
  //   yeşil aynı biçimde akar. Taze olmayan olayda ve diğer haplarda çerçeve durağandır.
  // Hareket yalnız transform/opacity. Boştaki Zody ve durağan haplar hiç kıpırdamaz.
  import type { Snippet } from 'svelte';
  import Zody from '../../mascot/Zody.svelte';
  import { agentColor } from '../../lib/format';
  import { MINI_SIZE, type LiveItem, type MiniKind, type NotchTarget, type NotchView } from '../../lib/live';
  import type { ZodyMood } from '../../lib/types';
  import ModeGlyph from './ModeGlyph.svelte';
  import Ring from './Ring.svelte';

  interface Props {
    view: NotchView;
    /** Olay taze mi (renkli akan çerçeve + yazı); değilse yalnız Zody gözleri. */
    fresh?: boolean;
    /** Genişlemiş (ara boy) hapın anahtarı: süren etkinliğin key'i ya da 'notice'. */
    peekKey?: string | null;
    /** Müzik kapağı (data: adresi); yoksa renkli kare. */
    art?: string | null;
    /** Baloncuklar dahil doğal genişlik (ölçülür; ada bu genişliğe göre kırpılır). */
    width?: number;
    reduce?: boolean;
    onopen: (target: NotchTarget) => void;
    /** Fare bir hapa geldi (null: çentikten çıktı). */
    onhover: (target: NotchTarget | null) => void;
    /** Ara boydaki Zodymetre/mod düğmeleri (hangi hap olduğu `item`'da: meterId ya da mod). */
    ontimer?: (op: 'start' | 'pause' | 'add', item: LiveItem) => void;
    onmusic?: (action: 'toggle' | 'next' | 'prev') => void;
    /** Açık hap mini pencere mi (Faz 9) ve boyu; içeriği `minipane` çizer (Island, NotchMini). */
    mini?: MiniKind | null;
    miniH?: number;
    minipane?: Snippet<[LiveItem | 'notice']>;
  }
  let { view, fresh = true, peekKey = null, art = null, width = $bindable(0), reduce = false, onopen, onhover, ontimer, onmusic, mini = null, miniH = 0, minipane }: Props = $props();
  const miniStyle = $derived(mini ? `width:${MINI_SIZE[mini].w}px;height:${miniH}px` : undefined);

  const MOOD_TONE: Partial<Record<ZodyMood, string>> = {
    attention: 'var(--amber)',
    done: 'var(--green)',
    error: 'var(--red)',
    idle: 'var(--accent)',
    sleep: 'var(--text-3)',
  };
  const tone = (item: LiveItem) =>
    item.kind === 'working'
      ? agentColor(item.tool ?? '')
      : item.kind === 'timer'
        ? item.color || 'var(--accent-soft)'
        : item.kind === 'mode'
          ? item.color || 'var(--accent)'
          : 'var(--coral)';
  const notice = $derived(view.notice);
  const lead = $derived(view.lead);
  const noticeTone = $derived(notice ? (MOOD_TONE[notice.mood] ?? 'var(--amber)') : 'transparent');
  /** Akan çerçeve: AI çalışırken; taze onay, soru, bitti ve hata. */
  const flowing = (item: LiveItem) => !reduce && item.kind === 'working';
  const noticeFlow = $derived(!reduce && fresh && notice !== null && ['attention', 'done', 'error'].includes(notice.mood));
  const LABEL: Record<LiveItem['kind'], string> = { timer: 'Zodymetre', mode: 'Modlar', working: 'AI ayrıntısı', music: 'Müzik' };
  const GO: Record<LiveItem['kind'], string> = { timer: 'Bugün', mode: 'Modlar', working: 'AI', music: 'Müzik' };
  /** Zodymetre ve mod ara boyda adının yanında süresini yazar, düğmeleri vardır. */
  const clocked = (item: LiveItem) => item.kind === 'timer' || item.kind === 'mode';
</script>

{#snippet frame(flow: boolean)}
  <span class="frame" class:flow aria-hidden="true">{#if flow}<span class="fade"><span class="stripes"></span></span>{/if}</span>
{/snippet}

{#snippet icon(item: LiveItem, size: number)}
  {#if item.kind === 'timer'}
    <Ring ratio={item.ratio ?? 0} {size} stroke={size > 30 ? 4 : 3.5} color={item.color || 'var(--accent)'} />
  {:else if item.kind === 'mode'}
    <ModeGlyph icon={item.icon ?? 'star'} color={item.color || 'var(--accent)'} {size} />
  {:else if item.kind === 'working'}
    <Zody mood="working" size={size + 6} bare />
  {:else}
    <span class="cover" style:width="{size}px" style:height="{size}px">{#if art}<img src={art} alt="" />{/if}</span>
  {/if}
{/snippet}

{#snippet eq()}
  <span class="eq" aria-hidden="true"><i style:height="7px"></i><i style:height="12px"></i><i style:height="5px"></i><i style:height="10px"></i></span>
{/snippet}

<!-- Ara boy: hapın yerine genişleyen panel. -->
{#snippet wide(over: string, title: string, detail: string, mono: boolean, time: string, go: string, big: Snippet, extra: Snippet | null)}
  <span class="big">{@render big()}</span>
  <span class="txt">
    {#if over && over !== title}<small>{over}</small>{/if}
    <b>{title}{#if detail}<span class:mono>{detail}</span>{/if}</b>
    {#if extra}{@render extra()}{/if}
  </span>
  <span class="rt">
    {#if time}<b class="tm">{time}</b>{/if}
    <span class="go">Tıkla: {go}</span>
  </span>
{/snippet}

{#snippet pill(item: LiveItem, wideMain: boolean)}
  {@const open = peekKey === item.key}
  <!-- Başka bir hap genişlemişken geniş hap da baloncuk boyuna iner: çentik pencereye sığar. -->
  {@const main = wideMain && !peekKey}
  {@const asMini = open && mini !== null && minipane !== undefined}
  <div class="pill" class:bub={!main && !open} class:open class:mini={asMini} class:short={main && !open && Boolean(item.short)} class:lit={flowing(item) && !asMini} style:--k={tone(item)} style={asMini ? miniStyle : undefined} role="presentation" onmouseenter={() => onhover(item)}>
    <button class="hit" onclick={() => onopen(item)} aria-label={LABEL[item.kind]} title={main || open ? undefined : `${LABEL[item.kind]} · ${item.title}`}></button>
    {#if asMini && minipane}
      {@render minipane(item)}
    {:else if open}
      {#snippet bigIcon()}{@render icon(item, 46)}{/snippet}
      {#snippet extra()}
        {#if item.kind === 'working' && item.steps?.length}
          <span class="steps">
            {#each item.steps as s, i (i)}<i class={s}></i>{/each}
            <span>{item.stepNo}. adım</span>
          </span>
        {:else if clocked(item) && ontimer}
          <span class="btns">
            <button class="btn" onclick={() => ontimer(item.running ? 'pause' : 'start', item)}>{item.running ? 'Duraklat' : 'Sürdür'}</button>
            {#if !item.endless}<button class="btn" onclick={() => ontimer('add', item)}>+5 dk</button>{/if}
          </span>
        {:else if item.kind === 'music'}
          <span class="steps">{@render eq()}</span>
          {#if onmusic}
            <span class="btns">
              <button class="btn" onclick={() => onmusic('toggle')}>{item.running ? 'Duraklat' : 'Oynat'}</button>
              <button class="btn" onclick={() => onmusic('next')}>Sonraki</button>
            </span>
          {/if}
        {/if}
      {/snippet}
      {@render wide(
        item.over,
        clocked(item) ? `${item.title} · ${item.time}` : item.title,
        item.kind === 'timer' ? '' : item.kind === 'mode' ? (item.detail === 'modu' ? '' : item.detail) : item.detail,
        item.mono,
        item.kind === 'working' ? item.time : '',
        GO[item.kind],
        bigIcon,
        extra,
      )}
    {:else}
      <span class="ic">{@render icon(item, 22)}</span>
      {#if main && item.short}
        <!-- Faz 7.3: tek kısa satır; dosya adı ve sanatçı fareyle açılan ara boyda. -->
        <b class="clip">{item.short}</b>
        {#if item.kind === 'music'}{@render eq()}{:else if item.time}<span class="tm">{item.time}</span>{/if}
      {:else if main}
        <b>{item.title}</b>
        {#if item.detail}<span class="dt" class:mono={item.mono}>{item.detail}</span>{/if}
        {#if item.kind === 'music'}{@render eq()}{:else if item.time}<span class="tm">{item.time}</span>{/if}
      {:else if clocked(item)}
        <span class="tm">{item.time}</span>
      {/if}
    {/if}
    {#if !asMini}{@render frame(flowing(item))}{/if}
  </div>
{/snippet}

<div class="notch">
  <div class="row" bind:clientWidth={width} role="presentation" onmouseleave={() => onhover(null)}>
    {#if lead && !notice}
      <!-- 11.ek: hap(lar) görünürken boştaki küçük Zody gözleri solda kalır; tıklayınca ana menü. Mini pencere açmaz. -->
      <div class="pill quiet" role="presentation">
        <button class="hit" onclick={() => onopen('idle')} aria-label="Ana menüyü aç" title="Ana menü"></button>
        <span class="eyes"><Zody mood="idle" size={34} bare /></span>
      </div>
    {/if}
    {#each view.bubbles as item (item.key)}
      {@render pill(item, false)}
    {/each}
    {#if notice}
      {@const open = peekKey === 'notice'}
      {@const asMini = open && mini !== null && minipane !== undefined}
      <div class="pill" class:open class:mini={asMini} class:quiet={!fresh && !open} class:lit={noticeFlow && !asMini} class:attn={notice.mood === 'attention'} style:--k={noticeTone} style={asMini ? miniStyle : undefined} role="presentation" onmouseenter={() => onhover('notice')}>
        <button class="hit" onclick={() => onopen('notice')} aria-label="Aç"></button>
        {#if asMini && minipane}
          {@render minipane('notice')}
        {:else if open}
          {#snippet bigZody()}<Zody mood={notice.mood} size={52} bare />{/snippet}
          {@render wide(notice.detail, notice.title, '', false, '', notice.tab === 'ai' ? 'AI' : notice.tab === 'today' ? 'Bugün' : notice.tab === 'modes' ? 'Modlar' : 'aç', bigZody, null)}
        {:else}
          <span class="ic"><Zody mood={notice.mood} size={28} bare /></span>
          {#if fresh && !peekKey}
            <b>{notice.title}</b>
            <span class="dt">{notice.detail}</span>
            {#if notice.more > 0}<span class="plus">+{notice.more}</span>{/if}
          {/if}
        {/if}
        {#if (fresh || open) && !asMini}{@render frame(noticeFlow)}{/if}
      </div>
    {:else if lead}
      {@render pill(lead, true)}
    {:else if view.idle}
      <!-- Her zaman açık ada: yalnız Zody'nin gözleri; hiç kıpırdamaz. -->
      <div class="pill quiet" role="presentation" onmouseenter={() => onhover('idle')}>
        <button class="hit" onclick={() => onopen('idle')} aria-label="Adayı aç"></button>
        <span class="eyes"><Zody mood="idle" size={34} bare /></span>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Hapların arkası saydam: ada çentikteyken zemini haplar kendisi çizer (Island .bare). */
  .notch {
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    justify-content: center;
  }
  .row {
    width: max-content;
    height: 100%;
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .pill {
    position: relative;
    height: 36px;
    display: flex;
    align-items: center;
    gap: 9px;
    max-width: 440px;
    padding: 0 16px 0 10px;
    white-space: nowrap;
    background: var(--island);
    border-radius: 0 0 18px 18px;
    box-shadow: 0 0 0 1px rgb(255 255 255 / 0.05);
    overflow: hidden;
  }
  /* AI çalışıyor / müzik hapı: en çok 300 px (PILL_MAX_W), taşan yazı "…" ile biter. */
  .pill.short {
    max-width: 300px;
  }
  .clip {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pill.bub {
    gap: 7px;
    padding: 0 12px;
  }
  /* Boştaki Zody: siyah alan gözlerle aynı boyda (34 px Zody'nin gözleri ~22×12 px). */
  .pill.quiet {
    height: 20px;
    width: 36px;
    padding: 0;
    justify-content: center;
    border-radius: 0 0 10px 10px;
  }
  .eyes {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    flex: none;
    margin-top: -4px;
  }
  .pill.open {
    height: 92px;
    width: 440px;
    gap: 12px;
    padding: 4px 18px 8px 14px;
    border-radius: 0 0 22px 22px;
  }
  /* Mini pencere (Faz 9): boyu satır içi stilden (MINI_SIZE); içerik NotchMini. */
  .pill.mini {
    max-width: none;
    padding: 0;
    align-items: stretch;
    border-radius: 0 0 22px 22px;
  }
  /* Akan çerçeve varken hapın altından hafif renk. */
  .pill.lit {
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.05),
      inset 0 -10px 14px -12px var(--k);
  }
  .bub:hover {
    background: color-mix(in srgb, var(--k) 10%, var(--island));
  }
  .hit {
    position: absolute;
    inset: 0;
  }
  .pill > :not(.hit, .frame) {
    position: relative;
    pointer-events: none;
  }

  /* Çerçeve ışığı: soldan iner, alttan geçer, sağdan çıkar; üst kenar yok, yukarı doğru söner.
     Gelişte bir kez yukarıdan aşağı uzar (scaleY + opacity). */
  .frame {
    position: absolute;
    inset: 0;
    border: 2px solid var(--k);
    border-top: 0;
    border-radius: inherit;
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 55%);
    mask-image: linear-gradient(to bottom, transparent, #000 55%);
    opacity: 0.9;
    pointer-events: none;
    transform-origin: top;
  }
  /* Akan çerçeve: kenar halkası maskeyle kesilir (üstte halka yok), içinde şeritler soldan sağa kayar.
     Yalnız transform; bir şerit aralığı (26 px) kayınca başa döner, dikiş görünmez. */
  .frame.flow {
    border: 0;
    padding: 0 2px 2px;
    -webkit-mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    -webkit-mask-composite: xor;
    mask:
      linear-gradient(#000 0 0) content-box,
      linear-gradient(#000 0 0);
    mask-composite: exclude;
    overflow: hidden;
  }
  .fade {
    position: absolute;
    inset: 0;
    overflow: hidden;
    -webkit-mask-image: linear-gradient(to bottom, transparent, #000 55%);
    mask-image: linear-gradient(to bottom, transparent, #000 55%);
  }
  .stripes {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -52px;
    right: 0;
    background: repeating-linear-gradient(90deg, var(--k) 0 16px, color-mix(in srgb, var(--k) 18%, transparent) 16px 26px);
    animation: run 0.9s linear infinite;
    will-change: transform;
  }
  @keyframes run {
    to {
      transform: translateX(26px);
    }
  }

  .ic,
  .big {
    flex: none;
    display: grid;
  }
  b {
    font-size: var(--fs-13-5);
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
  }
  .attn > b {
    color: var(--amber);
  }
  .dt {
    font-size: var(--fs-12-5);
    color: var(--text-2);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mono {
    font-family: 'Cascadia Code', Consolas, monospace;
    font-size: var(--fs-12);
  }
  .tm {
    font-size: var(--fs-13-5);
    font-weight: 600;
    font-style: normal;
    font-variant-numeric: tabular-nums;
    color: var(--k);
    margin-left: 4px;
  }
  .bub .tm {
    margin-left: 0;
  }
  .plus {
    font-size: var(--fs-11);
    font-weight: 700;
    color: var(--text-2);
    background: rgb(255 255 255 / 0.08);
    border-radius: 99px;
    padding: 1px 6px;
  }
  .cover {
    display: block;
    border-radius: 6px;
    overflow: hidden;
    background: var(--cover-fallback);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  /* Durağan ekolayzer: çalarken bile kıpırdamaz. */
  .eq {
    display: inline-flex;
    gap: 2px;
    align-items: flex-end;
    height: 12px;
    margin-left: 4px;
  }
  .eq i {
    width: 2.5px;
    border-radius: 2px;
    background: var(--k);
  }

  /* Ara boy */
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .txt small {
    font-size: var(--fs-11-5);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .txt b {
    font-size: var(--fs-15);
    font-weight: 650;
    display: flex;
    gap: 7px;
    align-items: baseline;
    min-width: 0;
  }
  .txt b span {
    font-weight: 400;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .steps {
    display: flex;
    gap: 5px;
    align-items: center;
    margin-top: 5px;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .steps > i {
    width: 18px;
    height: 4px;
    border-radius: 2px;
    background: rgb(255 255 255 / 0.12);
  }
  .steps > i.done {
    background: var(--k);
    opacity: 0.55;
  }
  .steps > i.now {
    background: var(--k);
    box-shadow: 0 0 6px var(--k);
  }
  .steps > span {
    margin-left: 4px;
  }
  .btns {
    display: flex;
    gap: 6px;
    margin-top: 6px;
    pointer-events: auto;
  }
  .btn {
    font-size: var(--fs-11-5);
    padding: 3px 10px;
    border-radius: 9px;
    color: var(--text);
    background: var(--surface-2);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.08);
  }
  .btn:hover {
    background: var(--surface-3);
  }
  .rt {
    align-self: stretch;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: flex-end;
    padding: 6px 0 2px;
  }
  .rt .tm {
    font-size: var(--fs-15);
  }
  .go {
    margin-top: auto;
    font-size: var(--fs-11);
    color: var(--text-3);
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: no-preference) {
    .ic,
    .big {
      animation: pop var(--t-slow) var(--spring) both;
    }
    .frame {
      animation: frame-in var(--t-slow) var(--ease) 80ms both;
    }
    .pill > b,
    .pill > .dt,
    .pill > .tm,
    .txt,
    .rt {
      animation: slide var(--t-base) var(--ease) 120ms both;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .stripes {
      animation: none;
    }
  }
</style>
