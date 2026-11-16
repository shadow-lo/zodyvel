<script lang="ts">
  // Mail sekmesi (Faz 5b; eskiz design/eskiz-paket2.html §6): üstte hesap seçici (en çok 5 Gmail, her hesabın
  // kendi mail Zody'si ve okunmamış sayısı), "x dk önce yenilendi" ve Gmail ↗. Altında Birincil / Güncellemeler /
  // Tanıtımlar kartları: okunmamış sayısı ve en yeni üç okunmamış; ✓ okundu yapar. Birincil'de çok okunmamış varsa
  // kart vurgu rengine döner ve köşedeki Zody onay gözüyle bakar. Karta ya da bir maile basınca kategori listesi
  // açılır (en çok 15 ileti): solda liste, sağda seçilen mailin metni (BODY.PEEK, okundu yapmaz). Yanıtla ve
  // "Yeni mail" Gmail'i tarayıcıda açar (adadan gönderme yok). Mini karttaki satır `hub.mailFocus` ile buraya gelir.
  // Veri Rust'taki IMAP bağlantısından (Mail kaynağı açıkken, IDLE ile); bu ekran ağ isteği yapmaz, yalnız
  // "Yenile" ve satır eylemleri. Dalgalı ışık yok (yalnız onay, raf, AI ayrıntısı).
  import { ago, countText, findMail, HOT_PRIMARY, MAIL_SLOTS, mailCards, splitAddress, syncedText, type SourceItem, type SourceState } from '../../lib/sources';
  import { hub, sourceAction, sourceRefresh } from '../../lib/sources.svelte';
  import { openSettings } from '../../lib/store.svelte';
  import MailMessage from '../parts/MailMessage.svelte';
  import SourceZody from '../parts/SourceZody.svelte';

  let { state: src }: { state: SourceState } = $props();

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(id);
  });

  const accounts = $derived(src.accounts ?? []);
  let picked = $state<number | null>(null);
  const account = $derived(accounts.find(a => a.slot === picked) ?? accounts.find(a => a.status === 'ok') ?? accounts[0] ?? null);
  const cards = $derived(src.status === 'ok' && account?.status === 'ok' ? mailCards(src, account) : []);
  let error = $state<string | null>(null);

  // Kategori listesi ve okunan mail. Okunan mail anlık görüntüdür: okundu yapılıp listeden düşse de metin açık kalır.
  let listKey = $state<string | null>(null);
  let reading = $state<SourceItem | null>(null);
  const list = $derived(listKey ? (cards.find(c => c.key === listKey) ?? null) : null);
  function pickAccount(slot: number) {
    picked = slot;
    listKey = null;
    reading = null;
  }
  function showList(key: string, item: SourceItem | null = null) {
    listKey = key;
    reading = item;
  }
  // Mini karttan gelen mail: hesabını seç, kategorisini aç, maili göster.
  $effect(() => {
    const id = hub.mailFocus;
    if (!id) return;
    hub.mailFocus = null;
    const found = findMail(src, id);
    if (found) {
      picked = found.slot;
      showList(found.card, found.item);
      return;
    }
    // Kategori listesinde yoksa (okundu kalmış ya da daha eski): hesabının ilk kartı açılır, mail sağda okunur.
    const item = src.items.find(i => i.id === id);
    const owner = accounts.find(a => a.address === item?.account) ?? accounts[0];
    if (!item || !owner) return;
    picked = owner.slot;
    showList(mailCards(src, owner)[0]?.key ?? 'inbox', item);
  });

  async function act(id: string, action: 'open' | 'mark_read') {
    error = await sourceAction('mail', id, action);
  }
  const openCat = (key: string) => account && act(`${account.slot}:${key}`, 'open');
  const compose = () => account && act(`${account.slot}:compose`, 'open');
  const initials = (name: string) =>
    name
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map(w => w[0]?.toLocaleUpperCase('tr-TR'))
      .join('') || '?';
  // Gönderenin harf rozeti: addan türeyen sabit renk (her açılışta aynı).
  const HUES = ['#e0779b', '#5b9cff', '#2ec4b6', '#c2410c', '#1f9d74', '#8b7cff', 'var(--red)', '#a16207', '#64748b'];
  const hue = (s: string) => HUES[[...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7) % HUES.length];
  const CAT_ICON: Record<string, string> = {
    primary: 'M4 6.5h16v9.5a2 2 0 0 1-2 2H9l-4 3v-3.2A2 2 0 0 1 4 16z',
    updates: 'M12 8v5M12 16.5v.5M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17z',
    promotions: 'M3.5 12.5 11 5h8v8l-7.5 7.5zM15.5 8.5h.01',
    inbox: 'M4 13.5 6 6h12l2 7.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18zM4 13.5h4.5l1 2h5l1-2H20',
  };
  const hot = (key: string, unread: number) => key === 'primary' && unread >= HOT_PRIMARY;
</script>

<div class="body">
  {#if src.status === 'ok' || accounts.length}
    <div class="acc-row">
      {#each accounts as a (a.slot)}
        {@const [name, domain] = splitAddress(a.address)}
        <button class="accb" class:on={a.slot === account?.slot} class:bad={a.status === 'error'} onclick={() => pickAccount(a.slot)} title={a.status === 'error' ? (a.error ?? 'Bağlantıda sorun var') : a.address}>
          <SourceZody tile="mail" size={22} tone={a.status === 'error' ? 'alert' : a.badge ? 'attention' : 'calm'} mood={a.status === 'error' ? 'error' : a.status === 'connecting' ? 'working' : a.badge ? (a.slot === account?.slot ? 'attention' : 'idle') : 'sleep'} />
          {name}<span class="muted">{domain}</span>
          {#if a.status === 'ok' && a.badge}<span class="ct" class:dim={a.slot !== account?.slot}>{countText(a.badge)}</span>{/if}
        </button>
      {/each}
      {#if accounts.length < MAIL_SLOTS}
        <button class="accb plus" onclick={openSettings} title="Gmail hesabı ekle (Ayarlar › Kaynaklar)" aria-label="Gmail hesabı ekle">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
        </button>
      {/if}
      <span class="grow"></span>
      <span class="hdr">{account?.status === 'connecting' ? 'bağlanıyor…' : syncedText(account?.syncedMs, now)}</span>
      <button class="btn ghost" onclick={() => sourceRefresh('mail')} title="Yenile">Yenile</button>
      <button class="btn new" onclick={compose} disabled={!account} title="Gmail'de yeni mail yaz (tarayıcıda açılır)">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4" /></svg>Yeni mail
      </button>
      <button class="btn ghost" onclick={() => openCat('inbox')} disabled={!account} title="Gmail'de aç">
        Gmail <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 16 16 8M9.5 8H16v6.5" /></svg>
      </button>
    </div>

    {#if account?.status === 'error'}
      <p class="note err">{account.address}: {account.error ?? 'Bağlantıda sorun var.'} <button class="link" onclick={() => sourceRefresh('mail')}>Yeniden dene</button></p>
    {:else if account?.status === 'connecting' || !cards.length}
      <p class="note">Gmail'e bağlanıyor…</p>
    {:else if list}
      <div class="split">
        <section class="card lst">
          <div class="lh">
            <button class="back" onclick={() => ((listKey = null), (reading = null))} title="Kartlara dön" aria-label="Kartlara dön">‹</button>
            <b>{list.title}</b>
            <span class="muted tnum">{countText(list.unread)} okunmamış</span>
            <button class="gm" onclick={() => openCat(list.key)} title="{list.title} sekmesini Gmail'de aç">Gmail ↗</button>
          </div>
          <div class="rows" data-scroll>
            {#each list.all as m (m.id)}
              <button class="row" class:on={m.id === reading?.id} class:unr={m.unread} onclick={() => (reading = m)}>
                <span class="av" style:--k={hue(m.title)}>{initials(m.title)}</span>
                <span class="t"><b><span>{m.title}</span><em class="tnum">{ago(m.timeMs, now)}</em></b><small>{m.summary}</small></span>
              </button>
            {:else}
              <p class="clean">Okunmamış yok.</p>
            {/each}
            {#if list.unread > list.all.length}<p class="clean">Kalan {list.unread - list.all.length} mail Gmail'de.</p>{/if}
          </div>
        </section>
        <section class="card rdr">
          {#if reading}
            <MailMessage item={reading} {now} onback={() => (reading = null)} />
          {:else}
            <p class="note">Okumak için soldan bir mail seç.</p>
          {/if}
        </section>
      </div>
    {:else}
      <div class="cats" style:--n={cards.length}>
        {#each cards as c (c.key)}
          <div class="card cat" class:hot={hot(c.key, c.unread)} data-k={c.key}>
            <button class="ch" onclick={() => showList(c.key)} title="{c.title} maillerini göster">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d={CAT_ICON[c.key] ?? CAT_ICON.inbox} /></svg>{c.title}
            </button>
            {#if hot(c.key, c.unread)}
              <span class="corner"><SourceZody tile="mail" size={34} tone="attention" mood="attention" /></span>
            {/if}
            <div class="num tnum">{countText(c.unread)}<small>okunmamış</small></div>
            <div class="mls">
              {#each c.items as m (m.id)}
                <div class="mm" class:unr={m.unread}>
                  <button class="open" onclick={() => showList(c.key, m)} title="Maili oku">
                    <span class="av" style:--k={hue(m.title)}>{initials(m.title)}</span>
                    <span class="t"><b><span>{m.title}</span><em class="tnum">{ago(m.timeMs, now)}</em></b><small>{m.summary}</small></span>
                  </button>
                  {#if m.unread}
                    <button class="rd" onclick={() => act(m.id, 'mark_read')} title="Okundu yap" aria-label="Okundu yap">
                      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m5 12.5 4.5 4.5L19 7.5" /></svg>
                    </button>
                  {/if}
                </div>
              {:else}
                <p class="clean">Okunmamış yok.</p>
              {/each}
            </div>
            <button class="more" onclick={() => showList(c.key)}>Tümünü gör ›</button>
          </div>
        {/each}
      </div>
    {/if}
  {:else if src.status === 'error'}
    <p class="note err">{src.error ?? 'Gmail bağlantısında sorun var.'}</p>
  {:else if src.status === 'connecting'}
    <p class="note">Gmail'e bağlanıyor…</p>
  {:else}
    <p class="note">Gmail bağlı değil. <button class="link" onclick={openSettings}>Ayarlar › Kaynaklar</button>'dan bağlayabilirsin.</p>
  {/if}
  {#if error}<p class="toast" role="status">{error}</p>{/if}
</div>

<style>
  .body {
    position: relative;
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
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
    min-width: 0;
    flex: 0 1 auto;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hdr,
  .muted {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .acc-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
  }
  .accb {
    flex: none;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 9px 3px 4px;
    border-radius: 99px;
    border: 1px solid var(--line);
    background: var(--surface);
    font-size: var(--fs-12);
  }
  .accb:hover {
    border-color: var(--line-strong);
  }
  .accb.on {
    background: var(--surface-3);
    border-color: var(--line-strong);
  }
  .accb.bad {
    border-color: color-mix(in srgb, var(--red) 50%, transparent);
  }
  .accb .muted {
    margin-left: -4px;
  }
  .accb.plus {
    padding: 6px 9px;
    color: var(--text-3);
  }
  .accb.plus svg {
    width: 13px;
    height: 13px;
  }
  .ct {
    min-width: 20px;
    padding: 0 6px;
    border-radius: 99px;
    background: var(--red);
    color: #fff;
    font-size: var(--fs-10-5);
    font-weight: 700;
    text-align: center;
    line-height: 17px;
  }
  .ct.dim {
    background: #c9ceda;
    color: #15161e;
  }
  .btn {
    flex: none;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 10px;
    border: 1px solid var(--line);
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .btn:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .btn svg {
    width: 12px;
    height: 12px;
  }
  .btn.new {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 8px;
  }
  .lst,
  .rdr {
    display: flex;
    flex-direction: column;
    padding: 9px 10px;
    gap: 6px;
  }
  .rdr {
    padding: 11px 14px;
  }
  .lh {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-12-5);
    white-space: nowrap;
  }
  .lh b {
    font-weight: 650;
  }
  .back {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    font-size: 16px;
    line-height: 1;
    color: var(--text-3);
  }
  .back:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .gm {
    margin-left: auto;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .gm:hover {
    color: var(--text);
  }
  .rows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border-radius: 10px;
    text-align: left;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.on {
    background: var(--surface-3);
  }
  .row.unr .t b {
    color: var(--text);
    font-weight: 650;
  }
  .cats {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(var(--n), minmax(0, 1fr));
    gap: 8px;
  }
  .card {
    background: var(--glass), var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    min-width: 0;
    min-height: 0;
  }
  .cat {
    --c2: #7fb2ff;
    position: relative;
    padding: 11px 12px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;
  }
  .cat[data-k='primary'] {
    --c2: #c9c1ff;
  }
  .cat[data-k='promotions'] {
    --c2: #ffb08a;
  }
  /* Acil Birincil: kart vurgu renginde (durağan degrade, animasyon yok). */
  .cat.hot {
    background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 34%, transparent), color-mix(in srgb, var(--accent) 10%, transparent) 70%), var(--surface);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .ch {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--text-2);
  }
  .ch svg {
    width: 15px;
    height: 15px;
    color: var(--c2);
  }
  .ch:hover {
    color: var(--text);
  }
  .corner {
    position: absolute;
    top: 9px;
    right: 10px;
  }
  .num {
    font-size: 34px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: var(--c2);
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .num small {
    font-size: var(--fs-11);
    font-weight: 500;
    letter-spacing: 0;
    color: var(--text-3);
  }
  .mls {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .mm {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: 10px;
  }
  .mm:hover {
    background: var(--surface-2);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    text-align: left;
  }
  .av {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: var(--fs-10-5);
    font-weight: 700;
    color: #fff;
    background: var(--k);
  }
  .t {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .t b {
    display: flex;
    gap: 6px;
    font-size: var(--fs-12);
    font-weight: 500;
    color: var(--text-2);
  }
  .t b span {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .t em {
    flex: none;
    font-style: normal;
    font-size: var(--fs-10-5);
    color: var(--text-3);
  }
  .t small {
    font-size: var(--fs-11);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mm.unr .t b {
    color: var(--text);
    font-weight: 650;
  }
  .rd {
    position: absolute;
    right: 4px;
    top: 50%;
    width: 24px;
    height: 24px;
    margin-top: -12px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-3);
    background: var(--pop-bg);
    opacity: 0;
  }
  .mm:hover .rd {
    opacity: 1;
  }
  .rd:hover {
    color: var(--green);
  }
  .rd svg {
    width: 14px;
    height: 14px;
  }
  .clean {
    margin: 6px 0 0;
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .more {
    align-self: flex-start;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .more:hover {
    color: var(--text);
  }
  .note {
    margin: auto;
    font-size: var(--fs-12);
    color: var(--text-3);
    text-align: center;
  }
  .note.err {
    color: var(--amber);
  }
  .link {
    color: var(--accent-soft);
    text-decoration: underline;
  }
  .toast {
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
