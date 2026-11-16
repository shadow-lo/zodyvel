<script lang="ts">
  // Modlar (Faz 5a, K4; eskiz design/eskiz-paket2.html §4–5): solda mod listesi, sağda seçili modun süresi ve
  // başlarken yapacakları; mod açıkken sağda büyük halka, açık eylemler ve odak döngüsünün yeri. Eylemler yalnız
  // Windows'ta gerçekten yapabildiklerimiz: bildirim süzgeci, müzik, uygulama kapat/aç, Zody'nin sesi, 25/5 odak.
  // Anahtarlar tanımı hemen kaydeder (bir dahaki sefere de öyle başlar). Dalgalı ışık yok (yalnız onay, raf ve
  // AI ayrıntısında; karar 9 Kasım). Saniyelik sayaç yalnız mod çalışırken ve bu ekran açıkken ilerler.
  import { clockText } from '../../lib/clock-cards';
  import { call, inTauri } from '../../lib/ipc';
  import { data, startTyping, todayApply, type IslandContext } from '../../lib/store.svelte';
  import {
    cycleBar,
    MAX_MODE_MIN,
    MAX_MODES,
    MODE_DURATIONS,
    minutesText,
    modeActions,
    modeLeft,
    modeSummary,
    nextPhaseText,
    phaseLeft,
  } from '../../lib/today';
  import type { AppRef, ModeDef, ModeIcon } from '../../lib/types';
  import ModeGlyph from '../parts/ModeGlyph.svelte';
  import Ring from '../parts/Ring.svelte';

  let { ctx }: { ctx: IslandContext } = $props();

  const modes = $derived(ctx.today?.modes ?? []);
  const run = $derived(ctx.modeRun);
  const focusOn = $derived(data.settings.features.focus);
  let picked = $state<string | null>(null);
  // Açık mod varsa o, yoksa son seçilen, yoksa ilk mod.
  const selId = $derived(picked ?? run?.id ?? modes[0]?.id ?? null);
  const sel = $derived(modes.find(m => m.id === selId) ?? null);
  const showRun = $derived(run !== null && run.id === selId);

  let now = $state(Date.now());
  $effect(() => {
    if (!run || run.paused) return;
    now = Date.now();
    const id = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(id);
  });

  let error = $state<string | null>(null);
  async function apply(op: Parameters<typeof todayApply>[0]) {
    error = await todayApply(op);
  }
  const save = (mode: ModeDef) => apply({ op: 'modeSave', mode });
  const toggle = (key: 'onlyAi' | 'music' | 'quiet' | 'focus') => sel && save({ ...sel, [key]: !sel[key] });

  // Özel süre: "✎ Özel" ile dakika yazılır.
  let custom = $state(false);
  let own = $state('');
  function setMinutes(minutes: number) {
    if (!sel) return;
    custom = false;
    void save({ ...sel, minutes });
  }
  function saveOwn() {
    const n = Number(own.trim());
    if (!Number.isInteger(n) || n < 1 || n > MAX_MODE_MIN) return void (error = `Süre 1–${MAX_MODE_MIN} dk olmalı.`);
    own = '';
    setMinutes(n);
  }

  // Uygulama ekle: Windows dosya seçicisi (kapatılacaklar .exe). Önizlemede örnek uygulama.
  async function addApp(list: 'closeApps' | 'openApps') {
    if (!sel || sel[list].length >= 4) return;
    const app: AppRef | null | undefined = inTauri
      ? await call<AppRef | null>('modes_pick_app')
      : { name: list === 'closeApps' ? 'Discord' : 'VS Code', path: list === 'closeApps' ? 'C:\\Apps\\Discord.exe' : 'C:\\Apps\\Code.exe' };
    if (!app) return;
    if (list === 'closeApps' && !/\.exe$/i.test(app.path)) return void (error = 'Kapatmak için .exe seç (kısayol olmaz).');
    void save({ ...sel, [list]: [...sel[list], app] });
  }
  const removeApp = (list: 'closeApps' | 'openApps', path: string) => sel && save({ ...sel, [list]: sel[list].filter(a => a.path !== path) });

  // Düzenle (✎): ad, simge, renk; yeni mod da buradan.
  const ICONS: ModeIcon[] = ['brief', 'book', 'moon', 'game', 'star', 'leaf'];
  const COLORS = ['#5b9cff', '#2ec4b6', '#9d7bff', '#ff7a59', '#4fcf86', '#f472b6', '#ffb020'];
  let editing = $state<ModeDef | null>(null);
  function newMode() {
    editing = {
      id: '',
      name: '',
      icon: 'star',
      color: COLORS[modes.length % COLORS.length] ?? '#8b7cff',
      minutes: 50,
      onlyAi: false,
      music: true,
      quiet: false,
      focus: false,
      closeApps: [],
      openApps: [],
    };
  }
  async function saveEdit() {
    if (!editing) return;
    if (!editing.name.trim()) return void (error = 'Modun adı boş olamaz.');
    const isNew = !editing.id;
    const before = new Set(modes.map(m => m.id));
    await apply({ op: 'modeSave', mode: { ...editing, name: editing.name.trim() } });
    if (error) return;
    if (isNew) picked = (ctx.today?.modes ?? []).find(m => !before.has(m.id))?.id ?? picked;
    editing = null;
  }
  async function removeMode() {
    if (!editing?.id) return;
    await apply({ op: 'modeRemove', id: editing.id });
    if (!error) {
      editing = null;
      picked = null;
    }
  }
  function editKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void saveEdit();
    } else if (event.key === 'Escape') {
      event.stopPropagation();
      editing = null;
    }
  }

  const hhmm = (ms: number) => new Date(ms).toLocaleTimeString('tr-TR', { hour: '2-digit', minute: '2-digit' });
  const left = $derived(run ? modeLeft(run, now) : null);
  const elapsed = $derived(run ? Math.max(0, now - run.startedAt) : 0);
  const ratio = $derived(run && left !== null ? Math.min(1, left / Math.max(1, run.minutes * 60_000)) : 1);
  const phase = $derived(run ? phaseLeft(run, now) : null);
  const durationLabel = (m: number) => (m === 0 ? 'Süresiz' : minutesText(m));
</script>

<div class="body">
  <div class="md">
    <div class="card list">
      {#each modes as m (m.id)}
        {@const active = run?.id === m.id}
        <button class="row" class:sel={m.id === selId} style:--k={m.color} onclick={() => ((picked = m.id), (editing = null))}>
          <ModeGlyph icon={m.icon} color={m.color} size={32} />
          <span class="nm"><b>{m.name}</b><small>{active ? (run?.paused ? 'Duraklatıldı' : 'Açık') : modeSummary(m, focusOn)}</small></span>
          {#if active && run}
            <span class="live"><i></i>{left !== null ? clockText(left) : minutesText(Math.max(1, Math.floor(elapsed / 60_000)))}</span>
          {/if}
        </button>
      {/each}
      {#if modes.length < MAX_MODES}
        <button class="row add" onclick={newMode}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>Yeni mod
        </button>
      {/if}
    </div>

    {#if editing}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="card side" style:--k={editing.color} onkeydown={editKey}>
        <div class="head">
          <ModeGlyph icon={editing.icon} color={editing.color} size={48} />
          <input class="title" maxlength="24" placeholder="Modun adı" aria-label="Modun adı" bind:value={editing.name} onfocus={startTyping} />
        </div>
        <div>
          <div class="hdr">Simge</div>
          <div class="pills">
            {#each ICONS as ic (ic)}
              <button class="glyph" class:on={editing.icon === ic} aria-label={ic} onclick={() => editing && (editing.icon = ic)}><ModeGlyph icon={ic} color={editing.color} size={26} /></button>
            {/each}
          </div>
        </div>
        <div>
          <div class="hdr">Renk</div>
          <div class="pills">
            {#each COLORS as c (c)}
              <button class="sw" class:on={editing.color === c} style:--c={c} aria-label={c} onclick={() => editing && (editing.color = c)}></button>
            {/each}
          </div>
        </div>
        <div class="go2">
          <button class="start" onclick={saveEdit}>{editing.id ? 'Kaydet' : 'Ekle'}</button>
          <button class="btn" onclick={() => (editing = null)}>Vazgeç</button>
          <span class="grow"></span>
          {#if editing.id && run?.id !== editing.id}<button class="btn danger" onclick={removeMode}>Sil</button>{/if}
        </div>
      </div>
    {:else if showRun && run && sel}
      <div class="card side" style:--k={sel.color}>
        <div class="head small">
          <span class="chip"><i></i>{sel.name} modu {run.paused ? 'duraklatıldı' : 'açık'}</span>
          <span class="grow"></span>
          <span class="hdr tnum">{hhmm(run.startedAt)}'de başladı{run.endsAt !== null ? ` · ${hhmm(run.endsAt)}'de biter` : run.minutes === 0 ? ' · süresiz' : ''}</span>
        </div>
        <div class="runr">
          <div class="big">
            <Ring {ratio} size={124} stroke={3.4} color="var(--k)" />
            <span class="n">
              <b class="tnum">{left !== null ? clockText(left) : clockText(elapsed)}</b>
              <small>{left !== null ? `kaldı · ${minutesText(run.minutes)}` : 'geçti · süresiz'}</small>
            </span>
          </div>
          <div class="col">
            <div class="on-list">
              {#each modeActions(sel, focusOn && run.focus !== null) as a (a)}
                <div><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{a}</div>
              {:else}
                <div class="muted">Bu mod yalnız süre tutuyor.</div>
              {/each}
              {#if run.closed.length}<div class="muted">Kapatıldı: {run.closed.map(a => a.name).join(', ')}</div>{/if}
            </div>
            {#if run.focus}
              <div>
                <div class="hdr">Odak döngüsü · {run.focus.round}. tur{phase !== null ? ` · ${clockText(phase)}` : ''}</div>
                <div class="cycle">
                  {#each cycleBar(run.focus) as p, i (i)}<i class="{p.kind} {p.state}"></i>{/each}
                  <span>{nextPhaseText(run.focus)}</span>
                </div>
              </div>
            {/if}
          </div>
        </div>
        <div class="go2">
          {#if run.minutes > 0}<button class="btn" onclick={() => apply({ op: 'modeAdd', minutes: 5 })}>+5 dk</button>{/if}
          <button class="btn" onclick={() => apply({ op: run.paused ? 'modeResume' : 'modePause' })}>{run.paused ? 'Sürdür' : 'Duraklat'}</button>
          <span class="grow"></span>
          <button class="btn strong" onclick={() => apply({ op: 'modeStop' })}>Bitir</button>
        </div>
        {#if error}<span class="err" role="alert">{error}</span>{/if}
      </div>
    {:else if sel}
      <div class="card side" style:--k={sel.color}>
        <div class="head">
          <ModeGlyph icon={sel.icon} color={sel.color} size={48} />
          <span class="nm"><b>{sel.name}</b><small>{modeActions(sel, focusOn).length ? 'Başlayınca aşağıdakiler bir kez yapılır.' : 'Yalnız süre tutar; eylem seçebilirsin.'}</small></span>
          <span class="grow"></span>
          <button class="ib" title="Adı, simgesi, rengi" aria-label="Modu düzenle" onclick={() => sel && (editing = { ...sel })}>
            <svg viewBox="0 0 24 24"><path d="M4 20h4L19 9l-4-4L4 16z" /></svg>
          </button>
        </div>
        <div>
          <div class="hdr">Süre</div>
          <div class="pills">
            {#each MODE_DURATIONS as m (m)}
              <button class="pl" class:on={!custom && sel.minutes === m} onclick={() => setMinutes(m)}>{durationLabel(m)}</button>
            {/each}
            {#if custom}
              <input class="own" inputmode="numeric" maxlength="3" placeholder="dk" aria-label="Özel süre, dakika" bind:value={own} onfocus={startTyping} onkeydown={e => e.key === 'Enter' && saveOwn()} onblur={() => own.trim() && saveOwn()} />
            {:else}
              <button class="pl" class:on={!MODE_DURATIONS.includes(sel.minutes)} onclick={() => (custom = true)}>
                {MODE_DURATIONS.includes(sel.minutes) ? '✎ Özel' : `✎ ${minutesText(sel.minutes)}`}
              </button>
            {/if}
          </div>
        </div>
        <div class="acts-wrap">
          <div class="hdr">Başlayınca</div>
          <div class="acts">
            <button class="act" onclick={() => toggle('onlyAi')}><span class="tx">Yalnız AI bildirimleri</span><span class="sw2" class:on={sel.onlyAi}></span></button>
            <button class="act" onclick={() => toggle('music')}><span class="tx">Müzik devam etsin</span><span class="sw2" class:on={sel.music}></span></button>
            <button class="act" onclick={() => toggle('quiet')}><span class="tx">Zody sessiz</span><span class="sw2" class:on={sel.quiet}></span></button>
            {#if focusOn}
              <button class="act" onclick={() => toggle('focus')}><span class="tx">25/5 odak döngüsü</span><span class="sw2" class:on={sel.focus}></span></button>
            {/if}
            {#each sel.closeApps as a (a.path)}
              <span class="act app"><span class="tx" title={a.path}>{a.name} kapansın</span><button class="x" aria-label="{a.name} kaldır" onclick={() => removeApp('closeApps', a.path)}>×</button></span>
            {/each}
            {#each sel.openApps as a (a.path)}
              <span class="act app"><span class="tx" title={a.path}>{a.name} açılsın</span><button class="x" aria-label="{a.name} kaldır" onclick={() => removeApp('openApps', a.path)}>×</button></span>
            {/each}
            {#if sel.closeApps.length < 4}<button class="act more" onclick={() => addApp('closeApps')}>+ Kapatılacak uygulama</button>{/if}
            {#if sel.openApps.length < 4}<button class="act more" onclick={() => addApp('openApps')}>+ Açılacak uygulama</button>{/if}
          </div>
        </div>
        <div class="go2">
          <button class="start" disabled={run !== null} onclick={() => sel && apply({ op: 'modeStart', id: sel.id })}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5v14l12-7z" /></svg>Başlat · {durationLabel(sel.minutes)}
          </button>
          <span class="hint">{run ? 'Önce açık modu bitir.' : sel.closeApps.length ? 'Bitince Bitiş bildirimi; kapanan uygulamalar sorulur.' : 'Bitince Bitiş bildirimi ve ses.'}</span>
        </div>
        {#if error}<span class="err" role="alert">{error}</span>{/if}
      </div>
    {:else}
      <div class="card side empty">Henüz mod yok. Soldan "Yeni mod" ile ekle.</div>
    {/if}
  </div>
</div>

<style>
  .body {
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
  }
  .md {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: 214px minmax(0, 1fr);
    gap: 8px;
    min-height: 0;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    min-height: 0;
  }
  button {
    font: inherit;
    cursor: pointer;
    color: inherit;
  }
  .list {
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 7px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
  }
  .row:hover {
    background: var(--glass);
  }
  .row.sel {
    background: linear-gradient(90deg, color-mix(in srgb, var(--k) 20%, transparent), rgb(255 255 255 / 0.03));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 35%, transparent);
  }
  .nm {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .nm b {
    font-size: var(--fs-13-5);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nm small {
    font-size: var(--fs-11);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .live {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: var(--fs-11);
    font-weight: 600;
    color: var(--k);
    font-variant-numeric: tabular-nums;
  }
  .live i,
  .chip i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--k);
    box-shadow: 0 0 8px var(--k);
  }
  .row.add {
    margin-top: auto;
    justify-content: center;
    gap: 6px;
    border: 1px dashed var(--line-strong);
    color: var(--text-3);
    font-size: var(--fs-12-5);
  }
  .row.add svg,
  .ib svg,
  .start svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .side {
    position: relative;
    padding: 11px 13px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .side.empty {
    align-items: center;
    justify-content: center;
    color: var(--text-3);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 11px;
  }
  .head .nm b {
    font-size: var(--fs-16);
    font-weight: 650;
  }
  .head .nm small {
    font-size: var(--fs-12);
  }
  .grow {
    flex: 1;
  }
  .hdr {
    font-size: var(--fs-11);
    color: var(--text-3);
    margin-bottom: 5px;
  }
  .head .hdr {
    margin: 0;
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .pl {
    border: 0;
    border-radius: 99px;
    padding: 4px 11px;
    font-size: var(--fs-12);
    color: var(--text-2);
    background: var(--surface-3);
  }
  .pl.on {
    color: var(--text);
    background: color-mix(in srgb, var(--k) 24%, var(--surface-3));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 60%, transparent);
  }
  input {
    font: inherit;
    font-size: var(--fs-12);
    color: var(--text);
    background: var(--surface-3);
    border: 1px solid var(--k);
    border-radius: 99px;
    padding: 3px 10px;
    outline: none;
  }
  .own {
    width: 56px;
    text-align: center;
  }
  .title {
    flex: 1;
    font-size: var(--fs-15);
    border-radius: 10px;
    padding: 6px 10px;
  }
  .acts-wrap {
    min-height: 0;
    overflow-y: auto;
  }
  .acts {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 5px 8px;
  }
  .act {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 5px 8px;
    border-radius: 10px;
    border: 1px solid rgb(255 255 255 / 0.05);
    background: rgb(255 255 255 / 0.03);
    font-size: var(--fs-12);
    color: var(--text-2);
    text-align: left;
  }
  .act .tx {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .act.more {
    border-style: dashed;
    border-color: var(--line-strong);
    color: var(--text-3);
    justify-content: center;
  }
  .act .x {
    border: 0;
    background: transparent;
    color: var(--text-3);
    font-size: 15px;
    line-height: 1;
    padding: 0 2px;
  }
  .act .x:hover {
    color: var(--red);
  }
  .sw2 {
    position: relative;
    flex: none;
    width: 26px;
    height: 15px;
    border-radius: 99px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .sw2::after {
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
  .sw2.on {
    background: color-mix(in srgb, var(--k) 70%, #000);
    box-shadow: none;
  }
  .sw2.on::after {
    transform: translateX(11px);
    background: #fff;
  }
  .go2 {
    margin-top: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .start {
    border: 0;
    border-radius: 13px;
    padding: 8px 18px;
    font-size: var(--fs-13-5);
    font-weight: 650;
    color: #fff;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--k) 65%, #fff), var(--k));
    box-shadow:
      0 0 20px color-mix(in srgb, var(--k) 45%, transparent),
      inset 0 1px 0 rgb(255 255 255 / 0.35);
  }
  .start svg {
    fill: #fff;
    stroke: none;
  }
  .start:disabled {
    opacity: 0.45;
    cursor: default;
    box-shadow: none;
  }
  .hint {
    font-size: var(--fs-11);
    color: var(--text-3);
    line-height: 1.35;
  }
  .btn {
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
  .btn.strong {
    color: var(--text);
  }
  .btn.danger:hover {
    color: var(--red);
  }
  .ib {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 9px;
    display: grid;
    place-items: center;
    background: transparent;
    color: var(--text-3);
  }
  .ib:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .glyph {
    border: 0;
    padding: 2px;
    border-radius: 10px;
    background: transparent;
    opacity: 0.55;
  }
  .glyph.on {
    opacity: 1;
    box-shadow: inset 0 0 0 1.5px var(--k);
  }
  .sw {
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: var(--c);
    opacity: 0.6;
  }
  .sw.on {
    opacity: 1;
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 3.5px var(--c);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-radius: 99px;
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--k);
    background: color-mix(in srgb, var(--k) 16%, transparent);
  }
  .runr {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 0 4px;
  }
  .big {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
  }
  .big .n {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .big .n b {
    font-size: 30px;
    font-weight: 300;
    letter-spacing: -0.02em;
    line-height: 1;
    background: linear-gradient(180deg, #fff 40%, var(--k));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .big .n small {
    font-size: var(--fs-11);
    color: var(--text-3);
    margin-top: 3px;
  }
  .col {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .on-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: var(--fs-12-5);
    color: var(--text-2);
  }
  .on-list div {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .on-list svg {
    width: 14px;
    height: 14px;
    flex: none;
    fill: none;
    stroke: var(--k);
    stroke-width: 2.4;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .muted {
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .cycle {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .cycle i {
    width: 22px;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
  }
  .cycle i.break {
    width: 8px;
  }
  .cycle i.done {
    background: var(--k);
  }
  .cycle i.now {
    background: linear-gradient(90deg, var(--k) 50%, var(--surface-3) 50%);
    box-shadow: 0 0 6px color-mix(in srgb, var(--k) 60%, transparent);
  }
  .cycle span {
    margin-left: 6px;
  }
  .err {
    position: absolute;
    right: 14px;
    top: 8px;
    font-size: var(--fs-11);
    color: var(--red);
  }
</style>
