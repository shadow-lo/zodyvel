<script lang="ts">
  // Saat düzeninin ana ekranı (Faz 2b iskelet, Faz 3 kartlar + düzenleme kipi; eskiz design/yeni-rota.html).
  // Solda tarih, büyük saat (vurgu renginde degrade rakamlar) ve sıradaki etkinlik; sağda kullanıcının
  // seçtiği en çok 6 cam kart (lib/clock-cards.ts). Kart sayısı arttıkça kartlar küçülür; "Saat" kapalıysa
  // kartlar bütün genişliği alır. Modülü kapalı kart çizilmez, veri okumaz.
  // Düzenleme kipi (✎ ya da sağ tık): kartlara dokunarak seç (sıra = seçim sırası), "Saat" aç/kapa, Bitti (Esc: vazgeç).
  // Saat yalnız bu ekran görünürken ve dakikada bir, dakika başına hizalı güncellenir. Zodymetre kartı yalnız
  // zamanlayıcı çalışırken saniyede bir yenilenir; boşta hiçbir şey çalışmaz, döngülü animasyon yok.
  import { onMount } from 'svelte';
  import Zody from '../../mascot/Zody.svelte';
  import { agentColor } from '../../lib/format';
  import { data, musicControl, saveSettings, todayApply, type IslandContext } from '../../lib/store.svelte';
  import { sourceState } from '../../lib/sources.svelte';
  import { agentMood, pickLead } from '../../lib/mood';
  import { hhmm, leadMeter, nextEvent, whenLabel } from '../../lib/today';
  import { CARDS, MAX_CARDS, cardGrid, cleanCards, clockText, meterLeft, toggleCard, visibleCards, type CardAvailability, type CardId } from '../../lib/clock-cards';
  import type { AgentStatus, Settings } from '../../lib/types';
  import type { Tab } from '../../lib/tabs';

  interface Props {
    ctx: IslandContext;
    ontab: (tab: Tab) => void;
    /** Mail kartı: Kaynaklar'daki Mail ayrıntısını açar (Mail sekmesi Faz 5b). */
    onmail?: () => void;
  }
  let { ctx, ontab, onmail }: Props = $props();

  let now = $state(new Date());
  onMount(() => {
    let timer: ReturnType<typeof setTimeout>;
    const tick = () => {
      now = new Date();
      timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    timer = setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    return () => clearTimeout(timer);
  });

  const time = $derived(hhmm(now));
  const date = $derived(now.toLocaleDateString('tr-TR', { weekday: 'long', day: 'numeric', month: 'long' }));
  const next = $derived(ctx.today ? nextEvent(ctx.today.events, now) : null);
  const minutesTo = $derived.by(() => {
    if (!next?.time || whenLabel(next, now).split(' ')[0] !== 'Bugün') return null;
    const [h, m] = next.time.split(':').map(Number);
    return (h ?? 0) * 60 + (m ?? 0) - (now.getHours() * 60 + now.getMinutes());
  });
  const nextLine = $derived(next ? (minutesTo !== null && minutesTo < 120 ? `${minutesTo} dk` : whenLabel(next, now)) : null);

  const LABELS: Record<AgentStatus, string> = {
    running: 'Çalışıyor',
    waiting_input: 'Yanıt bekliyor',
    waiting_approval: 'Onay bekliyor',
    done: 'Bitti',
    error: 'Hata',
  };
  const lead = $derived(pickLead(ctx.agents, null));

  // Hangi kartın verisi var. Kapalı modülün kartı seçili kalsa da çizilmez.
  const available = $derived<CardAvailability>({
    music: data.settings.modules.music,
    meter: data.settings.modules.today,
    ai: data.settings.modules.agents,
    agenda: data.settings.modules.today,
    mail: data.settings.features.sources && data.settings.features.sourceMail,
    shelf: data.settings.features.shelf,
    note: true,
    pc: data.settings.modules.system,
  });

  // ---- Düzenleme kipi: taslak üzerinde çalışılır, Bitti'de tek seferde kaydedilir.
  let editing = $state(false);
  let draft = $state<CardId[]>([]);
  let draftTime = $state(true);
  function startEdit() {
    draft = cleanCards(data.settings.clock.cards);
    draftTime = data.settings.clock.showTime;
    editing = true;
  }
  async function finishEdit() {
    editing = false;
    await save(n => {
      n.clock.cards = [...draft];
      n.clock.showTime = draftTime;
    });
  }
  async function save(change: (next: Settings) => void) {
    const next = structuredClone($state.snapshot(data.settings)) as Settings;
    change(next);
    await saveSettings(next);
  }
  function onkey(e: KeyboardEvent) {
    if (!editing) return;
    if (e.key === 'Escape') editing = false;
    else if (e.key === 'Enter' && !(e.target as Element).closest('button')) void finishEdit();
  }

  const cards = $derived(editing ? [] : visibleCards(data.settings.clock.cards, available));
  const showTime = $derived(editing ? true : data.settings.clock.showTime);
  const grid = $derived(cardGrid(cards.length, showTime));

  // ---- Not kartı: yerinde yazılır, odaktan çıkınca kaydedilir.
  let noteEdit = $state(false);
  let noteDraft = $state('');
  function editNote() {
    noteDraft = data.settings.clock.note;
    noteEdit = true;
  }
  function focusNote(el: HTMLTextAreaElement) {
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }
  async function saveNote() {
    if (!noteEdit) return;
    noteEdit = false;
    const text = noteDraft.trim().slice(0, 280);
    if (text !== data.settings.clock.note) await save(n => (n.clock.note = text));
  }

  // ---- Zodymetre (bugünkü tek zamanlayıcı; çoklu adlı Zodymetre Faz 5a). Yalnız çalışırken saniyede bir.
  // Faz 5a: en çok üç Zodymetre; kartta öndeki (en yakın biten) görünür.
  const timer = $derived(leadMeter(ctx.meters));
  const meterRunning = $derived(timer?.endsAt != null);
  let tickNow = $state(Date.now());
  $effect(() => {
    if (!meterRunning || !cards.includes('meter')) return;
    tickNow = Date.now();
    const id = setInterval(() => (tickNow = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const meter = $derived(timer ? meterLeft(timer, meterRunning ? tickNow : 0) : null);
  const RING = 2 * Math.PI * 15;

  // ---- Mail (Kaynaklar'daki Gmail; okunmamış toplamı ve en yeni okunmamış satır).
  const mail = $derived(cards.includes('mail') ? sourceState('mail') : null);
  const mailTop = $derived(mail?.items.find(i => i.unread) ?? null);
  const mailLine = $derived.by(() => {
    if (!mail) return '';
    if (mail.status === 'error') return mail.error ?? 'Bağlantı hatası';
    if (mail.status === 'connecting') return 'Bağlanıyor…';
    if (mail.status !== 'ok') return 'Kurulum gerekiyor';
    return mailTop ? `${mailTop.title} · ${mailTop.summary}` : 'Okunmamış yok';
  });

  function go(id: CardId) {
    if (id === 'music') ontab('music');
    else if (id === 'ai') ontab('ai');
    else if (id === 'meter' || id === 'agenda') ontab('today');
    else if (id === 'shelf') ontab('shelf');
    else if (id === 'pc') ontab('pc');
    else if (id === 'mail') onmail?.();
    else if (id === 'note') editNote();
  }
  function cardKey(e: KeyboardEvent, id: CardId) {
    if (e.key === 'Enter' && e.target === e.currentTarget) go(id);
  }
  const reachable = (id: CardId) => id !== 'pc' || ctx.tabs.includes('pc');
</script>

<svelte:window onkeydown={onkey} />

{#snippet icon(id: CardId)}
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if id === 'music'}<path d="M9 18V5l11-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="17" cy="16" r="3" />
    {:else if id === 'meter'}<circle cx="12" cy="13" r="8" /><path d="M12 9v4l2.5 2M9.5 2.5h5" />
    {:else if id === 'ai'}<rect x="4" y="5" width="16" height="13" rx="4" /><path d="M10 10v3M14 10v3" />
    {:else if id === 'agenda'}<rect x="4" y="5" width="16" height="15" rx="3" /><path d="M4 10h16M9 3v4M15 3v4" />
    {:else if id === 'mail'}<rect x="3.5" y="5.5" width="17" height="13" rx="2.5" /><path d="m4 7 8 6 8-6" />
    {:else if id === 'shelf'}<path d="M4 13.5 6 6h12l2 7.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18z" /><path d="M4 13.5h4.5l1 2h5l1-2H20" />
    {:else if id === 'note'}<path d="M6 4h9l4 4v12H6z" /><path d="M14 4v5h5M9 13h6M9 16h4" />
    {:else}<rect x="3.5" y="4.5" width="17" height="11" rx="2" /><path d="M8 20h8M12 15.5V20" />{/if}
  </svg>
{/snippet}

{#snippet card(id: CardId)}
  <div class="card {id}" role="button" tabindex="0" class:dim={!reachable(id)} onclick={() => reachable(id) && go(id)} onkeydown={e => cardKey(e, id)}>
    {#if id === 'music'}
      <span class="art" class:empty={!ctx.music?.title}>{@render icon('music')}</span>
      <span class="tx"><b>{ctx.music?.title || 'Müzik'}</b><small>{ctx.music?.artist || 'Çalan bir şey yok'}</small></span>
      {#if ctx.music?.title}
        <button class="play" aria-label={ctx.music.playing ? 'Duraklat' : 'Oynat'} onclick={e => (e.stopPropagation(), musicControl('toggle'))}>
          {#if ctx.music.playing}
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14M16 5v14" /></svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true"><path class="fill" d="M8 5.5v13l10-6.5z" /></svg>
          {/if}
        </button>
      {/if}
    {:else if id === 'meter'}
      <span class="ring" aria-hidden="true">
        <svg viewBox="0 0 36 36">
          <circle class="track" cx="18" cy="18" r="15" />
          <circle class="arc" cx="18" cy="18" r="15" stroke-dasharray={RING} stroke-dashoffset={RING * (1 - (meter?.ratio ?? 1))} />
        </svg>
      </span>
      <span class="tx">
        <b class="num">{meter ? clockText(meter.left) : '--:--'}</b>
        <small>{timer?.name || 'Zodymetre'} · {!meterRunning && meter?.left === 0 ? 'doldu' : `${timer?.minutes ?? 0} dk`}</small>
      </span>
      {#if timer}
        <button class="play soft" aria-label={meterRunning ? 'Duraklat' : 'Başlat'} onclick={e => (e.stopPropagation(), void todayApply({ op: meterRunning ? 'timerPause' : 'timerStart', id: timer?.id }))}>
          {#if meterRunning}
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5v14M16 5v14" /></svg>
          {:else}
            <svg viewBox="0 0 24 24" aria-hidden="true"><path class="fill" d="M8 5.5v13l10-6.5z" /></svg>
          {/if}
        </button>
      {/if}
    {:else if id === 'ai'}
      <span class="ic zody"><Zody mood={lead ? agentMood(lead.status) : ctx.mood} body={lead ? agentColor(lead.tool) : undefined} size={30} /></span>
      <span class="tx">
        <b>{lead ? lead.project : 'AI Yöneticisi'}</b>
        <small class:warn={ctx.approvals.length > 0}>{ctx.approvals.length ? `${ctx.approvals.length} onay bekliyor` : lead ? LABELS[lead.status] : 'Açık oturum yok'}</small>
      </span>
    {:else if id === 'agenda'}
      <span class="ic">{@render icon('agenda')}</span>
      <span class="tx">
        <b>{next ? next.title : 'Ajanda'}</b>
        <small>{next ? whenLabel(next, now) : `${ctx.today?.todos.filter(t => !t.done).length ?? 0} yapılacak`}</small>
      </span>
    {:else if id === 'mail'}
      <span class="ic mailic">{@render icon('mail')}{#if mail && mail.badge > 0}<i class="badge">{mail.badge > 99 ? '99+' : mail.badge}</i>{/if}</span>
      <span class="tx">
        <b>{mail && mail.badge > 0 ? `${mail.badge} okunmamış` : 'Mail'}</b>
        <small class:err={mail?.status === 'error'}>{mailLine}</small>
      </span>
    {:else if id === 'shelf'}
      <span class="ic">{@render icon('shelf')}</span>
      <span class="tx">
        <b>{ctx.shelf.length ? `Raf · ${ctx.shelf.length} dosya` : 'Raf'}</b>
        <small>{ctx.shelf.length ? ctx.shelf.slice(0, 2).map(i => i.name).join(', ') : 'Dosyayı adaya sürükle'}</small>
      </span>
    {:else if id === 'note'}
      <span class="ic">{@render icon('note')}</span>
      {#if noteEdit}
        <!-- svelte-ignore a11y_autofocus -->
        <textarea
          class="note-in"
          maxlength="280"
          placeholder="Kısa bir not…"
          bind:value={noteDraft}
          use:focusNote
          onclick={e => e.stopPropagation()}
          onblur={saveNote}
          onkeydown={e => {
            e.stopPropagation();
            if (e.key === 'Enter' && !e.shiftKey) (e.preventDefault(), (e.currentTarget as HTMLTextAreaElement).blur());
            else if (e.key === 'Escape') noteEdit = false;
          }}
        ></textarea>
      {:else}
        <span class="tx"><b class="note-tx">{data.settings.clock.note || 'Not'}</b>{#if !data.settings.clock.note}<small>Yazmak için dokun</small>{/if}</span>
      {/if}
    {:else if id === 'pc'}
      <span class="ic">{@render icon('pc')}</span>
      <span class="tx">
        <b>Bilgisayar</b>
        {#if ctx.system}
          <small class="bars">
            <span>CPU {Math.round(ctx.system.cpu)}%</span>
            <span>RAM {ctx.system.ramUsedGb.toFixed(1)}/{Math.round(ctx.system.ramTotalGb)} GB</span>
          </small>
        {:else}
          <small>Ölçülüyor…</small>
        {/if}
      </span>
    {/if}
  </div>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="home" class:notime={!showTime} class:editing oncontextmenu={e => !editing && (e.preventDefault(), startEdit())}>
  {#if showTime}
    <div class="clock">
      <span class="date">{date}</span>
      <span class="time" aria-label="Saat {time}">{time}</span>
      {#if editing}
        <div class="edit-side">
          <button class="switch" role="switch" aria-checked={draftTime} onclick={() => (draftTime = !draftTime)}>
            <i class:on={draftTime}></i>Saat
          </button>
          <span class="count">{draft.length}/{MAX_CARDS} kart</span>
        </div>
      {:else if next}
        <span class="next">Sıradaki · {nextLine}<b>{next.title}</b></span>
      {:else if available.agenda}
        <span class="next">Bugün başka plan yok</span>
      {/if}
    </div>
  {/if}

  {#if editing}
    <div class="picker">
      {#each CARDS as c (c.id)}
        {@const pos = draft.indexOf(c.id)}
        <button
          class="tile"
          class:on={pos >= 0}
          class:off={!available[c.id]}
          aria-pressed={pos >= 0}
          disabled={pos < 0 && draft.length >= MAX_CARDS}
          title={available[c.id] ? c.title : c.off}
          onclick={() => (draft = toggleCard(draft, c.id))}
        >
          {#if pos >= 0}<i class="pos">{pos + 1}</i>{/if}
          <span class="ic">{@render icon(c.id)}</span>
          <span class="tname">{c.title}</span>
          {#if !available[c.id]}<small>Kapalı</small>{/if}
        </button>
      {/each}
      <div class="edit-foot">
        <button class="ghost" onclick={() => (editing = false)}>Vazgeç <kbd>Esc</kbd></button>
        <button class="done" onclick={finishEdit}>Bitti</button>
      </div>
    </div>
  {:else}
    <div class="cards" class:compact={grid.compact} style:--cols={grid.cols} style:--rows={grid.rows}>
      {#each cards as id (id)}
        {@render card(id)}
      {:else}
        <button class="card empty-add" onclick={startEdit}>
          <span class="ic"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg></span>
          <span class="tx"><b>Kart ekle</b><small>Müzik, Zodymetre, Mail…</small></span>
        </button>
      {/each}
    </div>
    <button class="edit-btn" aria-label="Saat ekranını düzenle" title="Düzenle" onclick={startEdit}>
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13.5 6.5 4 4" /></svg>
    </button>
  {/if}
</div>

<style>
  .home {
    position: relative;
    height: calc(100% - 36px);
    display: grid;
    grid-template-columns: 236px 1fr;
    gap: 12px;
    padding: 2px 14px 14px;
  }
  /* Saat kapalı: solda yalnız düzenle düğmesinin dar şeridi. */
  .home.notime {
    grid-template-columns: 22px 1fr;
    gap: 8px;
    padding-left: 8px;
  }
  .notime .edit-btn {
    position: static;
    align-self: center;
    grid-column: 1;
    grid-row: 1;
  }
  .notime .cards {
    grid-column: 2;
    grid-row: 1;
  }
  .clock {
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding-left: 14px;
    min-width: 0;
  }
  .date {
    color: var(--text-2);
    font-size: var(--fs-13-5);
  }
  .date::first-letter {
    text-transform: uppercase;
  }
  /* Büyük saat: tabular rakamlar, üstten beyaz alta vurgu rengine degrade. */
  .time {
    font-size: 74px;
    line-height: 1;
    font-weight: 300;
    letter-spacing: -2px;
    font-variant-numeric: tabular-nums;
    margin: 6px 0 8px -3px;
    background: linear-gradient(180deg, #fff 30%, color-mix(in srgb, var(--accent) 70%, #fff));
    background-clip: text;
    -webkit-background-clip: text;
    color: transparent;
  }
  .editing .time {
    opacity: 0.45;
  }
  .next {
    color: var(--text-2);
    font-size: var(--fs-12);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .next b {
    display: block;
    margin-top: 2px;
    color: var(--text);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ---- Kart ızgarası: sütun/satır sayısı kart sayısından (clock-cards.ts cardGrid). */
  .cards {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-template-rows: repeat(var(--rows), minmax(0, 1fr));
    gap: 10px;
    min-width: 0;
    min-height: 0;
  }
  .cards.compact {
    gap: 8px;
  }
  .card {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    min-height: 0;
    padding: 10px 12px;
    border-radius: 18px;
    text-align: left;
    cursor: pointer;
    background: linear-gradient(180deg, rgb(255 255 255 / 0.055), rgb(255 255 255 / 0.025));
    box-shadow:
      inset 0 0 0 1px rgb(255 255 255 / 0.08),
      inset 0 1px 0 rgb(255 255 255 / 0.06);
    transition: transform var(--t-fast) var(--spring);
  }
  .card:hover {
    transform: translateY(-1px);
  }
  .card.dim {
    cursor: default;
  }
  .compact .card {
    padding: 6px 10px;
    gap: 10px;
    border-radius: 15px;
  }
  .ic,
  .art,
  .ring {
    position: relative;
    width: 40px;
    height: 40px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 13px;
    color: var(--text-2);
    background: rgb(255 255 255 / 0.05);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
  }
  .compact .ic,
  .compact .art,
  .compact .ring {
    width: 32px;
    height: 32px;
    border-radius: 11px;
  }
  .art {
    color: #fff;
    background: linear-gradient(135deg, var(--coral), var(--accent));
  }
  .art.empty {
    color: var(--text-3);
    background: rgb(255 255 255 / 0.05);
  }
  .ic svg,
  .art svg {
    width: 20px;
    height: 20px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .compact .ic svg,
  .compact .art svg {
    width: 17px;
    height: 17px;
  }
  .compact .ic.zody :global(svg) {
    width: 26px;
    height: 26px;
  }
  /* Zodymetre halkası: kalan oran. Geçiş animasyonu yok; çalışırken saniyede bir yeni değer. */
  .ring {
    background: none;
    box-shadow: none;
  }
  .ring svg {
    width: 100%;
    height: 100%;
    transform: rotate(-90deg);
  }
  .ring circle {
    fill: none;
    stroke-width: 3.2;
  }
  .ring .track {
    stroke: rgb(255 255 255 / 0.09);
  }
  .ring .arc {
    stroke: var(--accent-2);
    stroke-linecap: round;
  }
  .mailic {
    color: var(--red);
  }
  .badge {
    position: absolute;
    top: -5px;
    right: -7px;
    min-width: 18px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    font-style: normal;
    font-size: var(--fs-xs);
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    color: var(--on-amber);
    background: var(--amber);
    box-shadow: 0 0 8px color-mix(in srgb, var(--amber) 55%, transparent);
  }
  .tx {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .tx b,
  .tx small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tx b {
    font-size: var(--fs-13-5);
    font-weight: 600;
    color: var(--text);
  }
  .tx b.num {
    font-variant-numeric: tabular-nums;
    font-size: var(--fs-15);
  }
  .tx b.note-tx {
    font-weight: 500;
    white-space: normal;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.3;
  }
  .compact .tx b.note-tx {
    -webkit-line-clamp: 1;
    line-clamp: 1;
  }
  .tx small {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .tx small.warn {
    color: var(--amber);
  }
  .tx small.err {
    color: var(--red);
  }
  .bars {
    display: flex;
    gap: 8px;
  }
  .note-in {
    flex: 1;
    min-width: 0;
    height: 100%;
    max-height: 64px;
    resize: none;
    border: 0;
    outline: 0;
    padding: 4px 6px;
    border-radius: 8px;
    font: inherit;
    font-size: var(--fs-12);
    color: var(--text);
    background: rgb(255 255 255 / 0.06);
  }
  .play {
    width: 34px;
    height: 34px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: #000;
    background: #fff;
  }
  .play.soft {
    color: var(--text);
    background: rgb(255 255 255 / 0.1);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.12);
  }
  .compact .play {
    width: 28px;
    height: 28px;
  }
  .play svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.4;
    stroke-linecap: round;
  }
  .play .fill {
    fill: currentColor;
    stroke: none;
  }
  .empty-add {
    border: 0;
    color: var(--text-2);
  }

  /* ---- Düzenle düğmesi: fare ekrana gelince belirir (yalnız opacity). */
  .edit-btn {
    position: absolute;
    left: 14px;
    bottom: 10px;
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 9px;
    color: var(--text-2);
    background: rgb(255 255 255 / 0.06);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease);
  }
  .home:hover .edit-btn,
  .edit-btn:focus-visible {
    opacity: 1;
  }
  .edit-btn svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.9;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  /* ---- Düzenleme kipi. */
  .edit-side {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
  }
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-12);
    color: var(--text);
  }
  .switch i {
    position: relative;
    width: 30px;
    height: 18px;
    border-radius: 9px;
    background: rgb(255 255 255 / 0.14);
    transition: background-color var(--t-fast) var(--ease);
  }
  .switch i::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform var(--t-fast) var(--spring);
  }
  .switch i.on {
    background: var(--accent);
  }
  .switch i.on::after {
    transform: translateX(12px);
  }
  .count {
    font-size: var(--fs-11);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .picker {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    grid-template-rows: 1fr 1fr auto;
    gap: 8px;
    min-width: 0;
    min-height: 0;
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-height: 0;
    border-radius: 15px;
    color: var(--text-2);
    background: linear-gradient(180deg, rgb(255 255 255 / 0.045), rgb(255 255 255 / 0.02));
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.07);
    transition: transform var(--t-fast) var(--spring);
  }
  .tile:hover:not(:disabled) {
    transform: translateY(-1px);
  }
  .tile .ic {
    width: 30px;
    height: 30px;
    border-radius: 10px;
  }
  .tile .ic svg {
    width: 16px;
    height: 16px;
  }
  .tile.on {
    color: var(--text);
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 22%, transparent), color-mix(in srgb, var(--accent) 8%, transparent));
    box-shadow:
      inset 0 0 0 1.5px color-mix(in srgb, var(--accent) 70%, transparent),
      0 0 14px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .tile.on .ic {
    color: var(--accent-soft);
  }
  .tile.off {
    opacity: 0.55;
  }
  .tile:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .tname {
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .tile small {
    font-size: var(--fs-10);
    color: var(--text-3);
  }
  .pos {
    position: absolute;
    top: 6px;
    right: 7px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    font-style: normal;
    font-size: var(--fs-xs);
    font-weight: 700;
    line-height: 17px;
    text-align: center;
    color: var(--on-accent);
    background: var(--accent);
  }
  .edit-foot {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .ghost,
  .done {
    height: 28px;
    padding: 0 14px;
    border-radius: 14px;
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .ghost {
    color: var(--text-2);
    background: rgb(255 255 255 / 0.06);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.08);
  }
  .ghost kbd {
    margin-left: 4px;
    font: inherit;
    font-size: var(--fs-10);
    color: var(--text-3);
  }
  .done {
    color: var(--on-accent);
    background: var(--accent);
  }
</style>
