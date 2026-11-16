<script lang="ts">
  // Üst simge çubuğu: yazı yok, yalnız simgeler. Hem minide hem ayrıntıda aynı yerde. Minide çubuğun boş
  // yerine tıklamak ayrıntıyı açar (Ayarlar → Ada; kullanıcı imleci en üste atıp basar, düğme aramaz).
  // Düzen (Faz 1 kararı T3): solda ses, ortada cam hap içinde sekmeler (sıra ve adlar sekme kaydından,
  // lib/tabs.ts), sağda uygulama kısayolları + "+" (Faz 2b, Rust apps.rs), ardından ✎ ve ⚙.
  import { tabDef, type Tab } from '../../lib/tabs';
  import { data, openSettings, readControls, setVolume, unfoldApprovals, type IslandContext } from '../../lib/store.svelte';
  import { hub, hubOn } from '../../lib/sources.svelte';
  import { appHue, appIcon, appLetter, isWebApp, launchApp, pickApp, removeApp, restoreApp } from '../../lib/apps.svelte';
  import { MAX_APPS, type AppShortcut } from '../../lib/types';

  interface Props {
    ctx: IslandContext;
    tab: Tab;
    detail: boolean;
    ontab: (tab: Tab) => void;
    /** Boş yere tıklama: ayrıntıyı aç. */
    onopen?: () => void;
    /** ✎ yazma sahnesi (özellik açıksa verilir). */
    onwrite?: () => void;
    /** Bir uygulama kısayolu açıldı (ada kapanabilir). */
    onlaunch?: () => void;
  }
  let { ctx, tab, detail, ontab, onopen, onwrite, onlaunch }: Props = $props();

  // Minide açık bölümün simgesine ikinci dokunuş ayrıntıyı açar.
  function tapTab(id: Tab) {
    if (!detail && tab === id && onopen) onopen();
    else ontab(id);
  }

  const openable = $derived(!detail && !!onopen && data.settings.island.barOpen);
  function onclick(event: MouseEvent) {
    if (openable && !(event.target as Element).closest("button")) onopen?.();
  }

  // Kaynaklar düzeninde (Özellikler → Kaynaklar) AI sekmesinin adı "Kaynaklar".
  const title = (id: Tab) => (id === 'ai' && hubOn() ? 'Kaynaklar' : tabDef(id).title);

  // Uygulama kısayolları: tıklayınca açılır, ada kapanır (onlaunch). Hata kısa süre ipucunda görünür.
  const apps = $derived(data.settings.apps ?? []);
  let appError = $state<string | null>(null);
  let errorTimer: ReturnType<typeof setTimeout> | undefined;
  function showError(message: string | null) {
    clearTimeout(errorTimer);
    appError = message;
    if (message) errorTimer = setTimeout(() => (appError = null), 4000);
  }
  async function open(app: AppShortcut) {
    const error = await launchApp(app);
    showError(error);
    if (!error) onlaunch?.();
  }
  // Sağ tık (11.ek2): Kaldır / Ayarlarda düzenle; kaldırınca 4 sn "Geri al".
  let menuApp = $state<AppShortcut | null>(null);
  let undo = $state<{ app: AppShortcut; index: number } | null>(null);
  let undoTimer: ReturnType<typeof setTimeout> | undefined;
  function openMenu(event: MouseEvent, app: AppShortcut) {
    event.preventDefault();
    event.stopPropagation();
    clearTimeout(undoTimer);
    undo = null;
    menuApp = app;
  }
  function remove(app: AppShortcut) {
    const index = apps.findIndex(a => a.id === app.id);
    removeApp(app.id);
    menuApp = null;
    undo = { app: $state.snapshot(app) as AppShortcut, index };
    clearTimeout(undoTimer);
    undoTimer = setTimeout(() => (undo = null), 4000);
  }
  function undoRemove() {
    if (!undo) return;
    restoreApp(undo.app, undo.index);
    clearTimeout(undoTimer);
    undo = null;
  }
  function editInSettings() {
    menuApp = null;
    openSettings();
  }
  // Menü açıkken başka yere basılırsa kapanır.
  function outside(event: PointerEvent) {
    if (menuApp && !(event.target as Element).closest('.pick')) menuApp = null;
  }
  async function add() {
    showError(await pickApp());
  }
  // Kaynaklarda kırık CI kırmızı, okunmamış mail sarı nokta.
  const sourceDot = $derived.by(() => {
    if (!hubOn()) return undefined;
    const live = hub.list.filter(s => s.status === 'ok');
    return live.some(s => s.urgency >= 3) ? 'var(--red)' : live.some(s => s.badge > 0 && s.urgency >= 2) ? 'var(--amber)' : undefined;
  });
  // Ses: durum arka planda izlenmez; yalnız tıklanınca okunur ve sessize alınır / açılır.
  let muted = $state<boolean | null>(null);
  let volBusy = false;
  async function toggleMute() {
    if (volBusy) return;
    volBusy = true;
    try {
      const now = muted ?? (await readControls())?.volume?.muted ?? false;
      await setVolume({ muted: !now });
      muted = !now;
    } finally {
      volBusy = false;
    }
  }

  // Simgedeki küçük nokta, o bölümde dikkat isteyen bir şey olduğunu söyler.
  const dot = $derived<Partial<Record<Tab, string>>>({
    ai: ctx.approvals.length || ctx.folded.length ? 'var(--amber)' : (sourceDot ?? (ctx.working ? 'var(--blue)' : undefined)),
    music: ctx.music?.playing ? 'var(--coral)' : undefined,
    today: ctx.timerDone?.meter ? 'var(--green)' : ctx.meters.some(m => m.endsAt !== null) ? 'var(--accent)' : undefined,
    // Ajanda: haberi gelen plan sarı. Mail: okunmamış Birincil sarı (Kaynaklar'daki kuralla aynı).
    agenda: ctx.eventDue ? 'var(--amber)' : undefined,
    mail: hub.list.some(s => s.id === 'mail' && s.status === 'ok' && s.badge > 0) ? 'var(--amber)' : undefined,
    // Açık mod: modun renginde nokta.
    modes: ctx.modeDone ? 'var(--green)' : (ctx.modeDef?.color ?? undefined),
    shelf: data.shelfDrag !== null ? 'var(--accent)' : undefined,
  });
</script>

<!-- Boş yer kısayolu yalnız fare için; klavye ve ekran okuyucu ayrıntıya kartın kendisinden gider. -->
<svelte:window onpointerdown={outside} />
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<nav class="bar" class:openable aria-label="Ada bölümleri" {onclick}>
  <button class="ico vol" class:off={muted} title={muted ? 'Sesi aç' : 'Sesi kapat'} aria-label={muted ? 'Sesi aç' : 'Sesi kapat'} aria-pressed={muted === true} onclick={toggleMute}>
    <svg viewBox="0 0 24 24" aria-hidden="true"
      ><path d="M4 9.5h3.2L12 5.5v13l-4.8-4H4z" />{#if muted}<path d="M16 9.5l5 5M21 9.5l-5 5" />{:else}<path d="M15.5 9a4.2 4.2 0 0 1 0 6M18.2 6.5a8 8 0 0 1 0 11" />{/if}</svg
    >
  </button>
  <span class="sp"></span>
  <span class="tabs">
  {#each ctx.tabs as id (id)}
    <button class="ico" class:on={tab === id} title={title(id)} aria-label={title(id)} aria-pressed={tab === id} onclick={() => tapTab(id)}>
      {#if id === 'ai'}
        <!-- AI logo-sekmesi: Zody'nin ekranı ve iki gözü (maskotla aynı dil). -->
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5" width="17" height="14" rx="4.2" /><path class="eye" d="M9.4 10.4v3.2M14.6 10.4v3.2" /></svg>
      {:else if id === 'clock'}
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></svg>
      {:else if id === 'music'}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V5l11-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="17" cy="16" r="3" /></svg>
      {:else if id === 'today'}
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5" width="17" height="15" rx="2.5" /><path d="M3.5 10h17M8 3v4M16 3v4" /></svg>
      {:else if id === 'agenda'}
        <!-- Ajanda: çizgili sayfa (Bugün'ün takviminden ayrı dursun). -->
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5" width="16" height="15" rx="3" /><path d="M8 11h8M8 15h5M9 3v4M15 3v4" /></svg>
      {:else if id === 'mail'}
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5.5" width="17" height="13" rx="3" /><path d="M4.5 7.5l7.5 5.5 7.5-5.5" /></svg>
      {:else if id === 'modes'}
        <!-- Modlar: yarım ay ve ışın (bir durumdan ötekine geçiş). -->
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.5a8.5 8.5 0 1 0 8.5 8.5A6.5 6.5 0 0 1 12 3.5z" /><path d="M17 3.5v3M15.5 5h3" /></svg>
      {:else if id === 'zody'}
        <!-- Konuşma balonunda Zody'nin iki gözü: sohbet sekmesi (AI'nın halkalı gezegeninden ayrı dursun). -->
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3.6c4.9 0 8.6 3.3 8.6 7.5s-3.7 7.5-8.6 7.5c-1 0-2-.1-2.9-.4L5 20.2l1-3.6c-1.6-1.4-2.6-3.3-2.6-5.5 0-4.2 3.7-7.5 8.6-7.5z" /><path class="eye" d="M9.4 10.2v1.6M14.6 10.2v1.6" /></svg>
      {:else if id === 'shelf'}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 13.5 6 6h12l2 7.5V18a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18z" /><path d="M4 13.5h4.5l1 2h5l1-2H20" /></svg>
      {:else}
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" /></svg>
      {/if}
      {#if dot[id]}<span class="dot" style:background={dot[id]} style:--glow={dot[id]}></span>{/if}
    </button>
  {/each}
  </span>
  <span class="sp"></span>
  {#if detail}<span class="hint">Esc: kapat</span>{/if}
  <!-- "Sonra ⌄" ile katlanan kartlar: istek beklemeye devam eder, buradan geri açılır. -->
  {#if ctx.folded.length}
    <button class="fold" onclick={() => (unfoldApprovals(), ontab('ai'))} title="Katlanan onay kartını geri aç">{ctx.folded.length} onay bekliyor · göster</button>
  {/if}
  <!-- Uygulama kısayolları (Faz 2b): kullanıcının eklediği en çok MAX_APPS uygulama ya da site. -->
  <span class="apps">
    {#if appError}<span class="app-error" role="status">{appError}</span>{/if}
    {#if menuApp}
      <span class="pick" role="menu" aria-label={`${menuApp.name} kısayolu`}>
        <b>{menuApp.name}</b>
        <button class="chip danger" role="menuitem" onclick={() => menuApp && remove(menuApp)}>Kaldır</button>
        <button class="chip" role="menuitem" onclick={editInSettings}>Ayarlarda düzenle</button>
      </span>
    {:else if undo}
      <span class="pick" role="status">
        <b>{undo.app.name}</b> kaldırıldı
        <button class="chip" onclick={undoRemove}>Geri al</button>
      </span>
    {:else}
    {#each apps as app (app.id)}
      {@const icon = appIcon(app)}
      <button class="app" class:web={isWebApp(app)} style:--hue={appHue(app)} title={app.name} aria-label={`${app.name} aç`} onclick={() => open(app)} oncontextmenu={event => openMenu(event, app)}>
        {#if icon}
          <img src={icon} alt="" draggable="false" />
        {:else if isWebApp(app)}
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8" /><path d="M4 12h16M12 4c2.4 2.4 2.4 13.6 0 16M12 4c-2.4 2.4-2.4 13.6 0 16" /></svg>
        {:else}
          <b>{appLetter(app)}</b>
        {/if}
      </button>
    {/each}
    {#if apps.length < MAX_APPS}
      <button class="slot" title="Uygulama kısayolu ekle (Ayarlar › Görünüm'den site de eklenir)" aria-label="Uygulama kısayolu ekle" onclick={add}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 7v10M7 12h10" /></svg>
      </button>
    {/if}
    {/if}
  </span>
  {#if onwrite}
    <button class="ico pen" title={data.settings.shortcuts.write ? `Yaz (${data.settings.shortcuts.write})` : 'Yaz'} aria-label="Yaz" onclick={onwrite}
      ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16z" /><path d="M13.5 6.5l4 4" /></svg></button
    >
  {/if}
  <button class="ico" title="Ayarlar" aria-label="Ayarlar" onclick={openSettings}>
    <svg viewBox="0 0 24 24" aria-hidden="true"
      ><circle cx="12" cy="12" r="3" /><path
        d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"
      /></svg
    >
  </button>
</nav>

<style>
  .bar {
    height: 36px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 12px;
  }
  /* Sekmeler cam hapta; seçili sekme hapın içinde ikinci, daha parlak hap. */
  .tabs {
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 2px;
    border-radius: 12px;
    background: var(--glass);
    box-shadow:
      inset 0 0 0 1px var(--line),
      inset 0 1px 0 rgb(255 255 255 / 0.06);
  }
  .tabs .ico {
    width: calc(var(--ico-w) - 4px);
    height: calc(var(--ico-h) - 4px);
  }
  .openable {
    cursor: pointer;
  }
  .ico {
    position: relative;
    width: var(--ico-w);
    height: var(--ico-h);
    border-radius: 9px;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .ico:hover {
    color: var(--text-2);
    background: rgb(255 255 255 / 0.04);
  }
  .ico.on {
    color: var(--ico-on-fg);
    background: var(--ico-on-bg);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.12),
      0 0 12px color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .vol.off {
    color: var(--red);
  }
  .apps {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-right: 4px;
  }
  .slot {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-3);
    border: 1px dashed var(--line-strong);
  }
  .slot:hover {
    color: var(--text-2);
    border-color: var(--text-3);
  }
  /* Kısayol: yuvarlak kare içinde uygulamanın kendi simgesi; simge yoksa addan renk alan harf/küre. */
  .app {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    overflow: hidden;
    color: #fff;
    background: hsl(var(--hue) 55% 42%);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.18),
      inset 0 0 0 1px rgb(255 255 255 / 0.08);
    transition: transform var(--t-fast) var(--spring);
  }
  .app:has(img) {
    background: var(--glass);
  }
  .app:hover {
    transform: translateY(-1px) scale(1.06);
  }
  .app:active {
    transform: scale(0.94);
  }
  .app img {
    width: 20px;
    height: 20px;
  }
  .app b {
    font-size: var(--fs-12);
    font-weight: 650;
  }
  .app svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
  }
  .app-error {
    max-width: 180px;
    margin-right: 4px;
    font-size: var(--fs-11);
    color: var(--red);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-11);
    color: var(--text-2);
    white-space: nowrap;
  }
  .pick b {
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-2);
  }
  .chip {
    padding: 3px 9px;
    border-radius: 999px;
    font-size: var(--fs-11);
    color: var(--text-2);
    background: var(--glass);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .chip:hover {
    color: var(--text);
    background: rgb(255 255 255 / 0.08);
  }
  .chip.danger {
    color: var(--red);
  }
  .slot svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
  }
  .ico svg {
    width: var(--ico-svg);
    height: var(--ico-svg);
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .ico .eye {
    stroke-width: 2.4;
  }
  .dot {
    position: absolute;
    top: 3px;
    right: 5px;
    width: var(--ico-dot);
    height: var(--ico-dot);
    border-radius: 50%;
    box-shadow:
      0 0 0 2px var(--island),
      0 0 6px var(--glow);
  }
  .sp {
    flex: 1;
  }
  .hint {
    color: var(--text-3);
    font-size: var(--fs-11);
    margin-right: 6px;
  }
  .fold {
    position: relative;
    margin-right: 6px;
    padding: 3px 10px 3px 20px;
    border-radius: 999px;
    font-size: var(--fs-11);
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .fold::before {
    content: '';
    position: absolute;
    left: 9px;
    top: 50%;
    width: 6px;
    height: 6px;
    margin-top: -3px;
    border-radius: 50%;
    background: var(--amber);
    box-shadow: 0 0 6px var(--amber);
  }
  .fold:hover {
    background: color-mix(in srgb, var(--amber) 24%, transparent);
  }
  .pen {
    color: var(--accent-soft);
  }
</style>
