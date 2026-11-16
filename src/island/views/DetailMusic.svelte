<script lang="ts">
  // Müzik ayrıntısı = Kontroller: üstte hızlı ayarlar ve ses düzeyi, altta oynatıcı şeridi (sarma ile).
  // Wi-Fi, Bluetooth, koyu mod, ses ve ses çıkışı doğrudan değişir; gece ışığı ve rahatsız etme için
  // resmi API olmadığından Windows'un ayar sayfası açılır. Durum yalnız bu ekran açılınca okunur;
  // çıkış aygıtları listesi de yalnız karoya tıklanınca alınır.
  import { onMount } from 'svelte';
  import { openWindowsSettings, readControls, readOutputs, setControl, setOutput, setVolume, type IslandContext } from '../../lib/store.svelte';
  import type { AudioOutput, ControlsState } from '../../lib/types';
  import MusicCard from '../parts/MusicCard.svelte';

  let { ctx }: { ctx: IslandContext } = $props();

  type Toggle = 'wifi' | 'bluetooth' | 'dark';
  type Icon = Toggle | 'speaker' | 'moon' | 'focus';
  type Page = 'wifi' | 'bluetooth' | 'sound' | 'nightlight' | 'notifications';

  let ctl = $state<ControlsState | null>(null);
  let loaded = $state(false);
  let busy = $state<Toggle | null>(null);
  let error = $state<{ kind: Toggle; text: string } | null>(null);
  let errorTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void readControls().then(s => {
      ctl = s;
      loaded = true;
    });
    return () => {
      clearTimeout(errorTimer);
      clearTimeout(outputErrorTimer);
    };
  });

  // Ses çıkışı: karoya tıklanınca küçük bir aygıt listesi açılır; tek tıkla değişir.
  let outputs = $state<AudioOutput[]>([]);
  let picker = $state(false);
  let opening = $state(false);
  let switching = $state<string | null>(null);
  let outputError = $state<string | null>(null);
  let outputErrorTimer: ReturnType<typeof setTimeout> | undefined;
  let tileEl: HTMLElement | undefined = $state();
  let pickerEl: HTMLElement | undefined = $state();

  async function toggleOutputs() {
    if (picker) {
      picker = false;
      return;
    }
    if (opening || switching) return;
    opening = true;
    const list = await readOutputs();
    opening = false;
    // Liste alınamadıysa eskisi gibi: Windows'un ses ayarları açılır.
    if (!list) return openWindowsSettings('sound');
    outputs = list;
    picker = true;
  }

  async function chooseOutput(output: AudioOutput) {
    if (switching) return;
    if (output.default) {
      picker = false;
      return;
    }
    switching = output.id;
    const result = await setOutput(output.id);
    switching = null;
    picker = false;
    if (typeof result === 'string') {
      outputError = result;
      clearTimeout(outputErrorTimer);
      outputErrorTimer = setTimeout(() => (outputError = null), 4000);
    } else {
      ctl = result.controls;
      outputs = result.outputs;
    }
  }

  // Liste açıkken dışarı tıklamak kapatır; Esc yalnız listeyi kapatır (ada açık kalır).
  $effect(() => {
    if (!picker) return;
    const away = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!pickerEl?.contains(target) && !tileEl?.contains(target)) picker = false;
    };
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopImmediatePropagation();
      picker = false;
    };
    window.addEventListener('pointerdown', away, true);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointerdown', away, true);
      window.removeEventListener('keydown', key, true);
    };
  });

  function soundSettings() {
    picker = false;
    openWindowsSettings('sound');
  }

  function outputSub(): string {
    if (outputError) return outputError;
    if (opening || switching) return '…';
    return ctl?.volume?.device ?? 'Ayarlar';
  }

  async function toggle(kind: Toggle) {
    const current = ctl?.[kind];
    if (busy || current == null) return;
    busy = kind;
    error = null;
    const result = await setControl(kind, !current);
    busy = null;
    if (typeof result === 'string') {
      error = { kind, text: result };
      clearTimeout(errorTimer);
      errorTimer = setTimeout(() => (error = null), 4000);
    } else {
      ctl = result;
    }
  }

  function sub(kind: Toggle): string {
    if (error?.kind === kind) return error.text;
    if (busy === kind) return '…';
    const value = ctl?.[kind];
    if (value == null) return loaded ? 'Yok' : '';
    if (!value) return 'Kapalı';
    return kind === 'wifi' ? (ctl?.wifiName ?? 'Açık') : 'Açık';
  }

  // Ses: sürüklerken çağrılar üst üste binmez; yoldayken gelen son değer bir sonrakinde gönderilir.
  let sending = false;
  let pending: number | null = null;
  async function pushLevel(level: number) {
    if (sending) {
      pending = level;
      return;
    }
    sending = true;
    await setVolume({ level });
    sending = false;
    if (pending !== null) {
      const next = pending;
      pending = null;
      void pushLevel(next);
    }
  }
  function changeLevel(level: number) {
    if (!ctl?.volume) return;
    const value = Math.max(0, Math.min(100, Math.round(level)));
    if (value === ctl.volume.level && !ctl.volume.muted) return;
    // Windows'taki gibi: ses düzeyini değiştirmek sessizliği kaldırır.
    if (ctl.volume.muted) {
      ctl.volume.muted = false;
      void setVolume({ muted: false });
    }
    ctl.volume.level = value;
    void pushLevel(value);
  }
  function toggleMute() {
    if (!ctl?.volume) return;
    ctl.volume.muted = !ctl.volume.muted;
    void setVolume({ muted: ctl.volume.muted });
  }
  function onwheel(event: WheelEvent) {
    if (!ctl?.volume || event.deltaY === 0) return;
    event.preventDefault();
    changeLevel(ctl.volume.level + (event.deltaY < 0 ? 2 : -2));
  }
</script>

{#snippet icon(name: Icon)}
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if name === 'wifi'}
      <path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0M8.6 16a5 5 0 0 1 6.8 0" /><circle class="fill" cx="12" cy="19.2" r="1.2" />
    {:else if name === 'bluetooth'}
      <path d="M7 7l10 10-5 4V3l5 4L7 17" />
    {:else if name === 'speaker'}
      <path d="M4 9h4l5-4v14l-5-4H4z" /><path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" />
    {:else if name === 'moon'}
      <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />
    {:else if name === 'dark'}
      <circle cx="12" cy="12" r="8.5" /><path class="fill" d="M12 3.5v17a8.5 8.5 0 0 0 0-17z" />
    {:else}
      <path d="M6 8a6 6 0 0 1 12 0c0 7 3 8 3 8H3s3-1 3-8" /><path d="M10 20a2 2 0 0 0 4 0M3 3l18 18" />
    {/if}
  </svg>
{/snippet}

{#snippet toggleTile(kind: Toggle, label: string, page: Page | null)}
  <div class="tg" class:on={ctl?.[kind] === true} class:off={ctl?.[kind] == null} class:busy={busy === kind}>
    <button class="main" disabled={ctl?.[kind] == null || busy !== null} onclick={() => toggle(kind)} aria-pressed={ctl?.[kind] === true}>
      <span class="ti">{@render icon(kind)}</span>
      <span class="tx"><b>{label}</b><small class:err={error?.kind === kind}>{sub(kind)}</small></span>
    </button>
    {#if page}
      <button class="chev" title="Windows ayarlarında aç" aria-label="{label} ayarları" onclick={() => openWindowsSettings(page)}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 6.5 8 9.5l3-3" /></svg>
      </button>
    {/if}
  </div>
{/snippet}

{#snippet pageTile(name: Icon, label: string, note: string, page: Page)}
  <div class="tg link">
    <button class="main" onclick={() => openWindowsSettings(page)} title="Windows ayarlarında açılır">
      <span class="ti">{@render icon(name)}</span>
      <span class="tx"><b>{label}</b><small>{note}</small></span>
      <svg class="out" viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4h6v6M12 4l-7 7" /></svg>
    </button>
  </div>
{/snippet}

<div class="body">
  <div class="toggles">
    {@render toggleTile('wifi', 'Wi-Fi', 'wifi')}
    {@render toggleTile('bluetooth', 'Bluetooth', 'bluetooth')}
    <div class="tg link" class:open={picker} class:busy={opening || switching !== null} bind:this={tileEl}>
      <button class="main" onclick={toggleOutputs} aria-haspopup="menu" aria-expanded={picker} title="Ses çıkışını seç">
        <span class="ti">{@render icon('speaker')}</span>
        <span class="tx"><b>Ses çıkışı</b><small class:err={outputError !== null}>{outputSub()}</small></span>
        <svg class="out drop" viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 6.5 8 10l3.5-3.5" /></svg>
      </button>
    </div>
    {@render pageTile('moon', 'Gece ışığı', 'Ayarlar', 'nightlight')}
    {@render toggleTile('dark', 'Koyu mod', null)}
    {@render pageTile('focus', 'Rahatsız etme', 'Bildirim ayarları', 'notifications')}
  </div>

  {#if picker}
    <div class="picker" role="menu" aria-label="Ses çıkışı" bind:this={pickerEl}>
      {#each outputs as output (output.id)}
        <button class="opt" class:sel={output.default} role="menuitemradio" aria-checked={output.default} onclick={() => chooseOutput(output)}>
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 5" /></svg>
          <span class="nm">{output.name}{#if output.kind}<small>{output.kind}</small>{/if}</span>
        </button>
      {/each}
      <button class="opt more" role="menuitem" onclick={soundSettings}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 4h6v6M12 4l-7 7" /></svg>
        <span>Ses ayarları</span>
      </button>
    </div>
  {/if}

  <!-- data-scroll: tekerlek burada adanın seviyesini değil ses düzeyini değiştirir. -->
  <div class="vol" class:muted={ctl?.volume?.muted} class:off={!ctl?.volume} data-scroll {onwheel}>
    <button class="mute" disabled={!ctl?.volume} onclick={toggleMute} aria-label={ctl?.volume?.muted ? 'Sesi aç' : 'Sesi kapat'} title={ctl?.volume?.muted ? 'Sesi aç' : 'Sesi kapat'}>
      {#if ctl?.volume?.muted}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9h4l5-4v14l-5-4H4z" /><path d="M17 9.5l5 5M22 9.5l-5 5" /></svg>
      {:else}
        {@render icon('speaker')}
      {/if}
    </button>
    <input
      type="range"
      min="0"
      max="100"
      aria-label="Ses düzeyi"
      disabled={!ctl?.volume}
      value={ctl?.volume?.level ?? 0}
      style:--p="{ctl?.volume?.level ?? 0}%"
      oninput={e => changeLevel(Number(e.currentTarget.value))}
    />
    <b>{ctl?.volume ? `%${ctl.volume.level}` : loaded ? 'Yok' : ''}</b>
  </div>

  <section class="card player">
    {#if ctx.music}
      <MusicCard music={ctx.music} variant="bar" extra={ctl?.volume?.device} />
    {:else}
      <p class="none">Şu an çalan bir şey yok.</p>
    {/if}
  </section>
</div>

<style>
  .body {
    position: relative;
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .toggles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .tg {
    height: 54px;
    display: flex;
    align-items: stretch;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
    overflow: hidden;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }
  .tg:hover {
    border-color: var(--line-strong);
  }
  .tg.on {
    background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 28%, transparent), color-mix(in srgb, var(--accent) 12%, transparent));
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .tg.off,
  .tg.busy {
    opacity: 0.55;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    text-align: left;
  }
  .main:disabled {
    cursor: default;
  }
  .ti {
    width: 30px;
    height: 30px;
    border-radius: 10px;
    background: var(--surface-3);
    color: var(--text-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .on .ti {
    background: var(--accent);
    color: var(--on-accent);
  }
  svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  svg .fill {
    fill: currentColor;
  }
  .ti svg {
    width: 16px;
    height: 16px;
  }
  .tx {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tx b {
    font-size: var(--fs-12-5);
    font-weight: 600;
  }
  .tx small {
    font-size: var(--fs-11);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .on .tx small {
    color: var(--text-2);
  }
  .tx small.err {
    color: var(--amber);
  }
  .chev {
    width: 28px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
    border-left: 1px solid var(--line);
  }
  .chev:hover {
    color: var(--text);
    background: rgb(255 255 255 / 0.04);
  }
  .chev svg {
    width: 14px;
    height: 14px;
  }
  .link .out {
    width: 12px;
    height: 12px;
    margin-left: auto;
    flex: none;
    color: var(--text-3);
  }
  .link:hover .out {
    color: var(--text-2);
  }
  /* Ses çıkışı karosundaki ⌄: liste açıkken ters döner (yalnız transform). */
  .link .drop {
    width: 14px;
    height: 14px;
    transition: transform var(--t-fast) var(--ease);
  }
  .open .drop {
    transform: rotate(180deg);
  }
  .open {
    border-color: var(--line-strong);
  }
  /* Aygıt listesi: karonun altında, sağa hizalı; opak yüzey, gölge yalnız çevresinde. */
  .picker {
    position: absolute;
    top: 58px;
    right: 10px;
    z-index: 20;
    width: min(260px, calc(100% - 20px));
    max-height: 220px;
    overflow-y: auto;
    padding: 4px;
    border-radius: 14px;
    /* --surface-2 yarı saydam: altındaki karolar ve ses çubuğu görünmesin diye adanın siyahı üstüne katlanır. */
    background: linear-gradient(var(--surface-2), var(--surface-2)), var(--island);
    border: 1px solid var(--line-strong);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.45);
    transform-origin: top right;
  }
  .opt {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 10px;
    text-align: left;
    font-size: var(--fs-12-5);
    color: var(--text-2);
  }
  .opt:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .opt span {
    min-width: 0;
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Gerçek ad üstte, aygıt türü altta küçük. */
  .opt .nm {
    display: flex;
    flex-direction: column;
  }
  .opt .nm small {
    font-size: var(--fs-11);
    font-weight: 400;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .opt svg {
    width: 14px;
    height: 14px;
    flex: none;
    opacity: 0;
  }
  .opt.sel {
    color: var(--text);
    font-weight: 600;
  }
  .opt.sel svg {
    opacity: 1;
    color: var(--accent);
  }
  .opt.more {
    margin-top: 2px;
    border-top: 1px solid var(--line);
    border-radius: 0 0 10px 10px;
    color: var(--text-3);
  }
  .opt.more svg {
    opacity: 1;
  }
  @media (prefers-reduced-motion: no-preference) {
    .picker {
      animation: drop var(--t-fast) var(--ease) both;
    }
  }
  @keyframes drop {
    from {
      transform: translateY(-6px) scale(0.97);
      opacity: 0;
    }
  }
  .vol {
    height: 40px;
    flex: none;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px 0 8px;
  }
  .vol.off {
    opacity: 0.55;
  }
  .mute {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .mute:hover:not(:disabled) {
    color: var(--text);
    background: var(--surface-2);
  }
  .mute svg {
    width: 16px;
    height: 16px;
  }
  .muted .mute {
    color: var(--amber);
  }
  /* İnce çizgi, dolu kısmı mor; sessizken gri. */
  input[type='range'] {
    flex: 1;
    height: 16px;
    margin: 0;
    appearance: none;
    background: linear-gradient(90deg, var(--accent) var(--p), var(--surface-3) var(--p)) center / 100% 4px no-repeat;
    cursor: pointer;
  }
  .muted input[type='range'] {
    background: linear-gradient(90deg, var(--text-3) var(--p), var(--surface-3) var(--p)) center / 100% 4px no-repeat;
  }
  input[type='range']::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 0 0 3px rgb(0 0 0 / 0.25);
  }
  input[type='range']:disabled {
    cursor: default;
  }
  .vol b {
    width: 38px;
    text-align: right;
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
    border-radius: 16px;
  }
  /* Yeni görünümde kapaktan gelen hafif renk; Klasik'te etkisiz. */
  .player {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    padding: 0 12px;
    background-image: var(--cover-glow), var(--card-sheen);
    border-color: var(--cover-line);
  }
  .player :global(.music) {
    flex: 1;
  }
  .none {
    margin: auto;
    color: var(--text-3);
  }
</style>
