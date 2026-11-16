<script lang="ts" module>
  export type MiniAction =
    | { op: 'stop'; session: string }
    | { op: 'front'; session: string }
    | { op: 'detail'; session: string }
    | { op: 'write'; session: string }
    | { op: 'close' }
    | { op: 'music'; action: 'toggle' | 'next' | 'prev' }
    | { op: 'timer'; action: 'start' | 'pause' | 'add' | 'reset'; id: number };
</script>

<script lang="ts">
  // Mini pencereler (Faz 9; eskiz design/eskiz-paket3.html ekran 1–3): fare bir hapa gelince ('Mini pencere'
  // kipinde) hap bu pencereye büyür. Mini AI 540×140, Mini Oynatıcı 420×112, Mini Kronometre 420×128.
  // Boşta hiçbir şey kıpırdamaz: saniyelik sayaç yalnız pencere açıkken ve bir şey sürerken çalışır,
  // ilerleme çizgisi yalnız transform (scaleX) ile güncellenir.
  import Zody from '../../mascot/Zody.svelte';
  import { categoryOf } from '../../lib/agent-view';
  import { clockText } from '../../lib/clock-cards';
  import { agentColor, toolName } from '../../lib/format';
  import { meterItems, MINI_SIZE, type LiveItem, type MiniKind } from '../../lib/live';
  import { stepVerb } from '../../lib/steps';
  import type { AgentSession, AgentStep, MeterState, NowPlaying, ZodyMood } from '../../lib/types';
  import Ring from './Ring.svelte';

  interface Props {
    kind: MiniKind;
    /** Mini AI: oturum (çalışan hap ya da bitti/hata bildirimi). */
    session?: AgentSession | null;
    /** Mini Oynatıcı: çalan (ya da pencere açıkken duraklatılan) şarkı ve kapağı. */
    music?: NowPlaying | null;
    art?: string | null;
    /** Mini Kronometre: bütün Zodymetreler ve fareyle gelinen hap. */
    meters?: MeterState[];
    item?: LiveItem | null;
    /** Saniyelik saat (Zodymetre çalışırken ada saniyede bir ilerletir). */
    now: number;
    reduce?: boolean;
    onaction: (action: MiniAction) => void;
  }
  let { kind, session = null, music = null, art = null, meters = [], item = null, now, reduce = false, onaction }: Props = $props();

  const size = $derived(MINI_SIZE[kind]);

  // Kendi saniyelik saati: yalnız AI turu ya da şarkı sürerken ve pencere açıkken (bileşen kapanınca durur).
  let tick = $state(Date.now());
  const ticking = $derived((kind === 'ai' && session?.status === 'running') || (kind === 'music' && Boolean(music?.playing)));
  $effect(() => {
    if (!ticking) return;
    tick = Date.now();
    const id = setInterval(() => (tick = Date.now()), 1000);
    return () => clearInterval(id);
  });
  const at = $derived(Math.max(now, tick));

  // ---------- Mini AI ----------
  const MOOD: Record<string, ZodyMood> = { running: 'working', done: 'done', error: 'error', waiting_input: 'attention' };
  const mood = $derived<ZodyMood>(session ? (MOOD[session.status] ?? 'idle') : 'idle');
  const tone = $derived(mood === 'done' ? 'var(--green)' : mood === 'error' ? 'var(--red)' : mood === 'attention' ? 'var(--amber)' : 'var(--accent-soft)');
  const turn = $derived.by(() => {
    if (!session) return [] as AgentStep[];
    const from = session.turnStartedAt ?? session.statusSince;
    return session.steps.filter(s => !s.sub && s.at >= from && s.kind !== 'prompt');
  });
  const current = $derived([...turn].reverse().find(s => s.state === 'now') ?? turn.at(-1) ?? null);
  const elapsed = $derived.by(() => {
    if (!session) return '';
    const from = session.turnStartedAt ?? session.statusSince;
    const to = session.status === 'running' ? at : (session.turnEndedAt ?? session.statusSince);
    return clockText(Math.max(0, to - from));
  });
  const statusText = $derived(
    !session ? '' : session.status === 'running' ? (current ? stepVerb(current) : 'Çalışıyor') : session.status === 'error' ? 'Hata' : session.step === 'Durduruldu' ? 'Durduruldu' : 'Bitti',
  );
  /** Son cevaptan iki satır (Markdown işaretleri atılır); tur sürerken anlık adım. */
  const answer = $derived.by(() => {
    if (!session) return '';
    const reply = session.status !== 'running' ? session.lastReply : null;
    const text = reply ?? session.step;
    return text
      .replace(/```[\s\S]*?```/g, ' ')
      .replace(/[`*_#>]+/g, '')
      .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
      .replace(/\s+/g, ' ')
      .trim();
  });
  const category = $derived(session ? categoryOf(session) : null);
  const stages = $derived(turn.slice(-5));
  const stageText = $derived.by(() => {
    if (!turn.length) return '';
    if (session?.status === 'running') return `${turn.length}. aşama · ${current ? stepVerb(current).toLocaleLowerCase('tr') : ''}`;
    const files = session?.turnFiles ?? 0;
    return `${turn.length} aşama${files ? ` · ${files} dosya` : ''}`;
  });
  const KIND_TONE: Partial<Record<AgentStep['kind'], string>> = {
    read: 'var(--blue)',
    search: 'var(--blue)',
    web: 'var(--blue)',
    edit: 'var(--accent-soft)',
    write: 'var(--accent-soft)',
    bash: 'var(--teal)',
  };
  const stageTone = (s: AgentStep) => (s.state === 'failed' ? 'var(--red)' : (KIND_TONE[s.kind] ?? 'var(--text-2)'));

  // ---------- Mini Oynatıcı ----------
  const position = $derived.by(() => {
    if (!music) return 0;
    const moved = music.playing ? Math.max(0, at - music.updatedAt) : 0;
    return Math.min(music.durationMs || Infinity, music.positionMs + moved);
  });
  const progress = $derived(music?.durationMs ? Math.min(1, position / music.durationMs) : 0);

  // ---------- Mini Kronometre ----------
  const timers = $derived(meterItems(meters, at).slice(0, 3));
  let picked = $state<number | null>(null);
  const timer = $derived(timers.find(t => t.meterId === (picked ?? item?.meterId)) ?? timers[0] ?? null);
</script>

{#snippet stageIcon(s: AgentStep)}
  <svg viewBox="0 0 24 24" aria-hidden="true">
    {#if s.kind === 'read'}
      <path d="M5 4h10l4 4v12H5z" /><path d="M9 12h6M9 16h4" />
    {:else if s.kind === 'search' || s.kind === 'web'}
      <circle cx="11" cy="11" r="6" /><path d="M20 20l-4.5-4.5" />
    {:else if s.kind === 'edit' || s.kind === 'write'}
      <path d="M4 20h4L19 9l-4-4L4 16z" />
    {:else if s.kind === 'bash'}
      <rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 9l3 3-3 3M12 15h5" />
    {:else}
      <circle cx="12" cy="12" r="3" />
    {/if}
  </svg>
{/snippet}

<div class="mini {kind}" class:still={reduce} style:width="{size.w}px">
  {#if kind === 'ai' && session}
    <div class="lf" style:--k={tone}>
      <Zody {mood} size={74} body={agentColor(session.tool)} />
      <span class="t">{elapsed}</span>
      <span class="s">{statusText}</span>
    </div>
    <div class="rt">
      <div class="hd">
        <b>{session.project}</b>
        {#if category}<span class="cat" style:--k={agentColor(session.tool)}>{category}</span>{/if}
        <small>{toolName(session.tool)}</small>
      </div>
      <div class="ans">{answer}</div>
      {#if stages.length}
        <div class="stg">
          {#each stages as s, i (i)}
            {#if i}<span class="ln" class:d={s.state !== 'now'}></span>{/if}
            <span class="sg" class:now={s.state === 'now' && !reduce} class:failed={s.state === 'failed'} style:--k={stageTone(s)}>{@render stageIcon(s)}</span>
          {/each}
          <span class="lb">{stageText}</span>
        </div>
      {/if}
      <div class="bt">
        {#if session.status === 'running'}
          {#if session.attach}
            <button class="btn stop" onclick={() => onaction({ op: 'stop', session: session.id })}>Durdur</button>
          {:else if session.app}
            <button class="btn" onclick={() => onaction({ op: 'front', session: session.id })}>Pencereyi öne getir</button>
          {/if}
        {:else}
          <button class="btn" onclick={() => onaction({ op: 'write', session: session.id })}>Yaz</button>
        {/if}
        <button class="btn acc" onclick={() => onaction({ op: 'detail', session: session.id })}>Ayrıntı</button>
        <span class="grow"></span>
        <button class="btn ghost ico" title="Kapat" aria-label="Kapat" onclick={() => onaction({ op: 'close' })}>
          <svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18" /></svg>
        </button>
      </div>
    </div>
  {:else if kind === 'music' && music}
    <span class="cover">{#if art}<img src={art} alt="" />{/if}</span>
    <div class="tx">
      <b>{music.title}</b>
      <span class="ar">{music.artist}{#if music.app}<small>{music.artist ? ' · ' : ''}{music.app}</small>{/if}</span>
      <div class="ctl">
        <button class="c" title="Önceki" aria-label="Önceki" disabled={!music.canPrev} onclick={() => onaction({ op: 'music', action: 'prev' })}>
          <svg viewBox="0 0 24 24"><path d="M7 5v14M19 5L9 12l10 7z" /></svg>
        </button>
        <button class="c pp" title={music.playing ? 'Duraklat' : 'Oynat'} aria-label={music.playing ? 'Duraklat' : 'Oynat'} disabled={!music.canToggle} onclick={() => onaction({ op: 'music', action: 'toggle' })}>
          {#if music.playing}
            <svg viewBox="0 0 24 24"><rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" /></svg>
          {:else}
            <svg viewBox="0 0 24 24"><path d="M7 4l13 8-13 8z" /></svg>
          {/if}
        </button>
        <button class="c" title="Sonraki" aria-label="Sonraki" disabled={!music.canNext} onclick={() => onaction({ op: 'music', action: 'next' })}>
          <svg viewBox="0 0 24 24"><path d="M17 5v14M5 5l10 7-10 7z" /></svg>
        </button>
      </div>
      {#if music.durationMs > 0}
        <div class="prog">
          <span>{clockText(position)}</span>
          <span class="tr"><i style:transform="scaleX({progress})"></i></span>
          <span>{clockText(music.durationMs)}</span>
        </div>
      {/if}
    </div>
  {:else if kind === 'timer' && timer}
    {#if timers.length > 1}
      <div class="ktabs">
        {#each timers as t (t.key)}
          <button class="ktab" class:on={t.key === timer.key} onclick={() => (picked = t.meterId ?? null)}>
            <Ring ratio={t.ratio ?? 0} size={14} stroke={3} color={t.color || 'var(--accent)'} />{t.title} <span class="tnum">{t.time}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="bd">
      <div class="rg" class:paused={!timer.running} style:--k={timer.color || 'var(--accent)'}>
        <Ring ratio={timer.ratio ?? 0} size={92} stroke={4.5} color={timer.color || 'var(--accent)'} />
        <b>{timer.time}</b>
        <small>{timer.running ? 'kalan' : 'duraklatıldı'}</small>
      </div>
      <div class="tx">
        <div class="t1">Zodymetre{timers.length > 1 ? ` · ${timers.length} sayaç` : ''}</div>
        <div class="t2">{timer.title}</div>
        <div class="bt">
          <button class="btn" onclick={() => onaction({ op: 'timer', action: timer.running ? 'pause' : 'start', id: timer.meterId ?? 0 })}>{timer.running ? 'Duraklat' : 'Devam'}</button>
          <button class="btn" onclick={() => onaction({ op: 'timer', action: 'add', id: timer.meterId ?? 0 })}>+5 dk</button>
          <button class="btn ghost" onclick={() => onaction({ op: 'timer', action: 'reset', id: timer.meterId ?? 0 })}>Bitir</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  /* Boş yere tıklama alttaki hapın düğmesine geçer (ilgili sekme açılır); yalnız düğmeler tıklanır. */
  .mini {
    position: relative;
    z-index: 1;
    height: 100%;
    pointer-events: none;
    white-space: normal;
    animation: show var(--t-fast) var(--ease) both;
  }
  .mini button {
    pointer-events: auto;
  }
  @keyframes show {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.98);
    }
  }
  .mini.still {
    animation: none;
  }
  .btn {
    font: inherit;
    font-size: var(--fs-11-5);
    white-space: nowrap;
    padding: 4px 11px;
    border-radius: 9px;
    color: var(--text);
    background: var(--surface-2);
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 0.08);
  }
  .btn:hover {
    background: var(--surface-3);
  }
  .btn.acc {
    background: var(--accent);
    color: #fff;
    font-weight: 650;
  }
  .btn.stop {
    background: color-mix(in srgb, var(--red) 14%, transparent);
    color: color-mix(in srgb, var(--red) 45%, #fff);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--red) 30%, transparent);
  }
  .btn.ghost {
    background: none;
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-2);
  }
  .btn.ico {
    padding: 4px 6px;
    display: grid;
    place-items: center;
  }
  .btn svg {
    width: 13px;
    height: 13px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.2;
    stroke-linecap: round;
  }
  .grow {
    flex: 1;
  }

  /* Mini AI */
  .ai {
    display: grid;
    grid-template-columns: 92px minmax(0, 1fr);
    gap: 4px;
    padding: 8px 14px 12px 6px;
  }
  .lf {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }
  .lf .t {
    font-size: var(--fs-15);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--k);
    line-height: 1;
  }
  .lf .s {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .rt {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .hd b {
    font-size: var(--fs-15);
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hd small {
    margin-left: auto;
    font-size: var(--fs-11-5);
    color: var(--text-3);
    white-space: nowrap;
  }
  .cat {
    font-size: var(--fs-11);
    padding: 1px 8px;
    border-radius: 99px;
    border: 1px solid color-mix(in srgb, var(--k) 35%, transparent);
    color: var(--text-2);
    white-space: nowrap;
  }
  .ans {
    font-size: var(--fs-12-5, 12.5px);
    color: var(--text);
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: 1.4em;
  }
  .stg {
    display: flex;
    align-items: center;
  }
  .sg {
    width: 20px;
    height: 20px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    flex: none;
    background: var(--island);
    border: 1px solid color-mix(in srgb, var(--k) 45%, transparent);
    color: var(--k);
  }
  .sg svg {
    width: 11px;
    height: 11px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .sg.now {
    box-shadow: 0 0 10px color-mix(in srgb, var(--k) 65%, transparent);
    background: color-mix(in srgb, var(--k) 18%, var(--island));
  }
  .ln {
    height: 2px;
    width: 12px;
    flex: none;
    background: rgb(255 255 255 / 0.14);
  }
  .ln.d {
    background: color-mix(in srgb, var(--teal) 55%, transparent);
  }
  .lb {
    margin-left: 9px;
    font-size: var(--fs-11-5);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .bt {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  /* Mini Oynatıcı */
  .music {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 4px 16px 10px 12px;
  }
  .cover {
    width: 84px;
    height: 84px;
    border-radius: 14px;
    flex: none;
    overflow: hidden;
    background: var(--cover-fallback);
    box-shadow: 0 6px 22px color-mix(in srgb, var(--cover, var(--coral)) 38%, transparent);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .music .tx {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .music .tx b {
    font-size: var(--fs-15);
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ar {
    font-size: var(--fs-12-5, 12.5px);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .music small {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .c {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .c:hover:not(:disabled) {
    color: var(--text);
  }
  .c:disabled {
    opacity: 0.35;
  }
  .c svg {
    width: 14px;
    height: 14px;
    fill: currentColor;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .c.pp {
    width: 34px;
    height: 34px;
    background: #fff;
    color: #000;
  }
  .c.pp svg {
    width: 12px;
    height: 12px;
  }
  .prog {
    margin-top: 4px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-10-5, 10.5px);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .tr {
    flex: 1;
    height: 3px;
    border-radius: 3px;
    background: rgb(255 255 255 / 0.1);
    overflow: hidden;
  }
  .tr i {
    display: block;
    height: 100%;
    width: 100%;
    border-radius: 3px;
    background: linear-gradient(90deg, #ffb08a, #c3a2ff);
    transform-origin: left;
  }

  /* Mini Kronometre */
  .timer {
    padding: 4px 14px 12px;
  }
  .ktabs {
    display: flex;
    gap: 4px;
    margin-bottom: 4px;
  }
  .ktab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-11-5);
    padding: 3px 9px 3px 6px;
    border-radius: 99px;
    color: var(--text-3);
    border: 1px solid transparent;
  }
  .ktab.on {
    background: var(--surface-2);
    color: var(--text);
    border-color: var(--line);
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .bd {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .rg {
    position: relative;
    width: 92px;
    height: 92px;
    flex: none;
    display: grid;
    place-items: center;
  }
  .rg :global(svg) {
    position: absolute;
    inset: 0;
  }
  .rg.paused {
    opacity: 0.55;
  }
  .rg b {
    position: relative;
    font-size: 19px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    background: linear-gradient(180deg, #fff 35%, var(--k));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .rg small {
    position: absolute;
    bottom: 20px;
    font-size: var(--fs-10, 10px);
    color: var(--text-3);
  }
  .timer .tx {
    flex: 1;
    min-width: 0;
  }
  .t1 {
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .t2 {
    font-size: var(--fs-15);
    font-weight: 650;
    margin: 1px 0 8px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .timer .bt {
    flex-wrap: wrap;
  }
</style>
