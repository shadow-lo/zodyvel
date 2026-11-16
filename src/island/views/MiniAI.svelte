<script lang="ts">
  // Mini AI Yöneticisi: solda seçili oturum (büyük kart), sağda diğerleri (küçük kartlar),
  // en sağda kırmızı müzik alanı. Bekleyen onay soldaki kartın yerini alır.
  // Sağdaki oturumlar tam boy bir liste, müzik alanı kapaklı ve yazısı iki satır.
  // Kaynaklar düzeni (Özellikler → Kaynaklar): sağda ızgara (Mail/GitHub ve her oturum ayrı),
  // solda o an en çok ilgilendiren (sources.ts featured()); onay/soru yine her şeyin önünde. Müzik ve Bugün
  // ızgarada yok, kendi sekmeleri var.
  import Zody from '../../mascot/Zody.svelte';
  import { since, toolName, windowLabel } from '../../lib/format';
  import { agentMood, pickLead } from '../../lib/mood';
  import { stepVerb } from '../../lib/steps';
  import { answerApproval, data, focusSession, foldApproval, openSettings, passToTool, type IslandContext } from '../../lib/store.svelte';
  import { parseMarkdown } from '../../lib/markdown';
  import { categoryOf, modeBadge, rowStatus, statusTone, todoProgress } from '../../lib/agent-view';
  import ApprovalCard from '../parts/ApprovalCard.svelte';
  import StatusBadge from '../parts/StatusBadge.svelte';
  import StepIcon from '../parts/StepIcon.svelte';
  import Vinyl from '../parts/Vinyl.svelte';
  import ToolZody from '../parts/ToolZody.svelte';
  import { writeMode } from '../../lib/write';
  import { hubOn, hubView, pickTile, sourceState } from '../../lib/sources.svelte';
  import { tileSession, type SourceId, type TileId } from '../../lib/sources';
  import SourceGrid from '../parts/SourceGrid.svelte';
  import MiniMail from './MiniMail.svelte';
  import MiniGithub from './MiniGithub.svelte';

  interface Props {
    ctx: IslandContext;
    onopen: () => void;
    onmusic: () => void;
    /** Yazma sahnesi (Yeni görünüm + özellik açıksa verilir): "✎ Yaz" o oturumla açar. */
    onwrite?: (target: string) => void;
    /** Kaynaklar düzeni: Mail/GitHub ayrıntısı. */
    onsource?: (id: SourceId) => void;
  }
  let { ctx, onopen, onmusic, onwrite, onsource }: Props = $props();

  const hubbed = $derived(hubOn());
  const view = $derived(hubbed ? hubView(ctx, data.minute) : null);
  // Sol kartta ajan değil bir kaynak duruyorsa (onay yoksa) o kaynağın kartı çizilir; oturum duruyorsa o oturum.
  const shown = $derived(view?.shown ?? null);
  const shownSession = $derived(shown ? tileSession(shown) : null);
  const toolAgents = $derived(shownSession ? ctx.agents.filter(a => a.id === shownSession) : ctx.agents);

  const approval = $derived(ctx.approvals[0]);
  const lead = $derived(approval ? (ctx.agents.find(a => a.id === approval.sessionId) ?? null) : pickLead(toolAgents, data.selected));
  const others = $derived(ctx.agents.filter(a => a.id !== lead?.id));
  const index = $derived(lead ? ctx.agents.indexOf(lead) + 1 : 0);
  // Listede iki satırlı oturum satırları; iki satır ve altta "ayrıntıda" bağlantısı sığar.
  const LIST_ROWS = 2;
  const rows = $derived(others.slice(0, LIST_ROWS));

  // Büyük kart: ajanın yapılacaklar ilerlemesi, izin kipi ve son adımlar (ilerleme varsa iki, yoksa üç).
  const progress = $derived(lead ? todoProgress(lead.todos) : null);
  const badge = $derived(modeBadge(lead?.mode ?? null));
  const recent = $derived(lead?.steps.slice(progress ? -2 : -3) ?? []);
  const canNote = $derived(Boolean(lead?.canNote) && data.settings.features.agentNotes);
  function note() {
    if (!lead) return;
    data.selected = lead.id;
    data.noteFocus = true;
    onopen();
  }

  // Birden çok istek bekliyorsa ‹ › ile aralarında geçilir; öne alınan istek kısayolların da hedefidir.
  const queue = $derived(data.settings.modules.agents ? data.approvals.filter(r => !ctx.folded.includes(r)) : []);
  // "Sonra ⌄" (Özellikler → Kartı sonraya katla): kart katlanır, istek beklemeye devam eder.
  const foldable = $derived(data.settings.features.foldCard);
  const writable = $derived(Boolean(onwrite && lead && writeMode(lead) !== 'blocked'));
  const position = $derived(approval ? queue.findIndex(r => r.id === approval.id) : -1);
  function shift(by: number) {
    const target = queue[(position + by + queue.length) % queue.length];
    if (target) data.approvalFirst = target.id;
  }
  // Sıradaki isteğin kısa özeti, ikinci istek gözden kaçmasın.
  const upcoming = $derived.by(() => {
    if (queue.length < 2 || position < 0) return undefined;
    const r = queue[(position + 1) % queue.length]!;
    return r.project;
  });
  let passError = $state<string | null>(null);
  // Soru adadan cevaplanamıyorsa (Codex, süre doldu) aracın penceresine gitmek için.
  const askApp = $derived(approval ? (ctx.agents.find(a => a.id === approval.sessionId)?.app ?? null) : null);

  // Plan kartında adım sayısı: planın ilk numaralı listesi.
  const planSteps = $derived.by(() => {
    if (approval?.kind !== 'plan' || !approval.plan) return 0;
    const list = parseMarkdown(approval.plan).find(b => b.t === 'list' && b.ordered);
    return list?.t === 'list' ? list.items.filter(i => i.depth === 0).length : 0;
  });

  function select(id: string) {
    data.selected = id;
  }

  /** Izgarada dokunma: birinci sol kartı çevirir, aynı kutucuğa ikinci dokunuş ayrıntıyı açar. */
  function tile(id: TileId) {
    // "+N oturum": sığmayan oturumlar ayrıntıda.
    if (id === 'more') return onopen();
    const again = pickTile(id, shown ?? '');
    const session = tileSession(id);
    if (session) data.selected = session;
    if (!again) return;
    if (id === 'mail' || id === 'github') onsource?.(id);
    else onopen();
  }
  function addSource() {
    // Ayarlar penceresi son açık sayfayla açılır (aynı köken, ortak localStorage).
    try {
      localStorage.setItem('zodyvel.settings.page', 'kaynaklar');
    } catch {
      // depolama kapalı: Ayarlar son sayfasıyla açılır
    }
    openSettings();
  }
</script>

{#snippet later()}
  {#if foldable && approval?.actionable}
    <button class="fold" onclick={() => approval && foldApproval(approval.id)} title="Kartı küçült, sonra cevapla">Sonra ⌄</button>
  {/if}
{/snippet}

{#snippet nav()}
  {#if queue.length > 1}
    <span class="queue">
      <button class="qb" onclick={() => shift(-1)} aria-label="Önceki istek">‹</button>
      <span>{position + 1}/{queue.length}</span>
      <button class="qb" onclick={() => shift(1)} aria-label="Sonraki istek">›</button>
    </span>
  {/if}
{/snippet}

<div class="body">
  {#if approval?.kind === 'question'}
    <section class="card lead ask question">
      <span class="who"><b class="amber">{toolName(approval.tool)} · {approval.project}</b>{@render nav()}{@render later()}</span>
      <small class="act">{approval.action}{#if upcoming}{` · Sıradaki: ${upcoming}`}{/if}</small>
      <p class="q">“{approval.questions[0]?.question ?? approval.detail}”{#if approval.questions.length > 1}<small> ve {approval.questions.length - 1} soru daha</small>{/if}</p>
      {#if passError}<p class="err">{passError}</p>{/if}
      <div class="btns">
        {#if approval.actionable}
          <button class="btn" onclick={async () => (passError = await passToTool(approval.id))}>Araçta cevapla</button>
          <button class="btn ok" onclick={onopen}>Cevapla →</button>
        {:else}
          <!-- Soru artık aracın kendi penceresinde; kart yalnız bilgi, kapatılabilir. -->
          <span class="native">
            {#if approval.closedEarly}
              {toolName(approval.tool)} adayı beklemedi (bağlantı kaydı eski). Ayarlar → Bağlantılar → Güncelle; şimdilik {toolName(approval.tool)} penceresinde cevapla.
            {:else if approval.nativeOnly}
              {toolName(approval.tool)} bu soruyu kendi penceresinde soruyor; orada cevapla.
            {:else}
              Adadaki süre doldu; {toolName(approval.tool)} penceresinde cevapla.
            {/if}
          </span>
          {#if askApp}<button class="btn" onclick={async () => (passError = await focusSession(approval.sessionId))}>{windowLabel(askApp)} ↗</button>{/if}
          <button class="btn" onclick={async () => (passError = await passToTool(approval.id))}>Kapat</button>
        {/if}
      </div>
    </section>
  {:else if approval?.kind === 'plan'}
    <!-- Plan burada yalnız başlığıyla görünür; okumak ve değişiklik istemek ayrıntıda. -->
    <section class="card lead ask question plan">
      <span class="who"><b class="amber">{toolName(approval.tool)} · {approval.project}</b>{#if approval.demo}<small>Örnek</small>{/if}{@render nav()}{@render later()}</span>
      <small class="act">Planını sundu{#if planSteps}{` · ${planSteps} adım`}{/if}{#if upcoming}{` · Sıradaki: ${upcoming}`}{/if}</small>
      <p class="q">{approval.detail}</p>
      {#if passError}<p class="err">{passError}</p>{/if}
      <div class="btns">
        {#if approval.actionable}
          <button class="btn" onclick={async () => (passError = await answerApproval(approval.id, 'deny'))}>Reddet</button>
          <button class="btn" onclick={onopen}>Oku →</button>
          <button class="btn ok" onclick={async () => (passError = await answerApproval(approval.id, 'allow'))}>Onayla</button>
        {:else}
          <span class="native">Kararı {toolName(approval.tool)} penceresinde ver.</span>
          {#if askApp}<button class="btn" onclick={async () => (passError = await focusSession(approval.sessionId))}>{windowLabel(askApp)} ↗</button>{/if}
          <button class="btn" onclick={async () => (passError = await passToTool(approval.id))}>Kapat</button>
        {/if}
      </div>
    </section>
  {:else if approval}
    <section class="card lead ask">
      <ApprovalCard request={approval} extra={ctx.approvals.length - 1} next={upcoming} {nav} onfile={onopen} onlater={foldable ? () => foldApproval(approval.id) : undefined} />
    </section>
  {:else if hubbed && (shown === 'mail' || shown === 'github')}
    <section class="card lead src">
      {#if shown === 'mail'}
        <MiniMail state={sourceState('mail')} onopen={() => onsource?.('mail')} />
      {:else}
        <MiniGithub state={sourceState('github')} onopen={() => onsource?.('github')} />
      {/if}
    </section>
  {:else if lead}
    <section class="card lead click">
      <!-- Kartın tamamı ayrıntıyı açar; "Sonra…" kendi düğmesi. -->
      <button class="cover" onclick={onopen} title="Ayrıntıyı aç" aria-label="{lead.project} ayrıntısını aç"></button>
      <!-- Durum renginde durağan hale: yalnız boyanır, hareket etmez. -->
      <span class="halo" style:--tone={statusTone[lead.status]}><ToolZody tool={lead.tool} mood={agentMood(lead.status, lead.asleep)} size={64} num={lead.taskNo} alive /></span>
      <span class="info">
        <span class="who">
          <b>{lead.project}</b>
          {#if badge}<span class="mode {badge.tone}" title={badge.text}>{badge.short}</span>{:else}<small>{toolName(lead.tool)}</small>{/if}
          <span class="idx">{index}/{ctx.agents.length}</span>
        </span>
        {#if progress}
          <span class="prog" title="Ajanın yapılacaklar listesi">
            <span class="bar"><span class="fill" style:transform="scaleX({progress.done / progress.total})"></span></span>
            <b>{progress.done}/{progress.total}</b>
            {#if progress.current}<span class="cur">{progress.current.active || progress.current.text}</span>{/if}
          </span>
        {/if}
        {#if recent.length}
          <span class="recent">
            {#each recent as s (s.id)}
              <span class="rs {s.state}"><StepIcon step={s} spinning={lead.status === 'running'} /><span class="verb">{stepVerb(s)}</span>{#if s.target && s.kind !== 'done'}<span class="tgt mono">{s.target}</span>{/if}</span>
            {/each}
          </span>
        {:else}
          <span class="step">{lead.step}</span>
        {/if}
        <span class="meta">
          <span title="Oturum {since(lead.startedAt)} önce başladı"><StatusBadge status={lead.status} suffix={since(lead.statusSince)} /></span>
          {#if lead.turnFiles && !canNote}<small title="Bu turda {lead.turnFiles} dosya değişti">{lead.turnFiles} dosya</small>{/if}
          {#if writable}<button class="later" onclick={() => lead && onwrite?.(lead.id)} title="Yazma sahnesini bu oturumla aç">✎ Yaz</button>
          {:else if lead.canTask && lead.status !== 'running' && lead.status !== 'waiting_approval'}<button class="later" onclick={note} title="Claude'a yeni görev ver">✎ Görev…</button>
          {:else if canNote}<button class="later" onclick={note} title="Ajana not bırak">✎ Not bırak</button>{/if}
        </span>
      </span>
    </section>
  {:else}
    <section class="card lead empty">
      <Zody mood={ctx.mood} size={64} alive interactive />
      <span class="info">
        {#if hubbed}
          <b>Her şey yolunda</b>
          <span class="step">Bekleyen onay, okunmamış mail ya da kırık CI yok.</span>
        {:else}
          <b>Şu an etkin bir yapay zekâ yok</b>
          <span class="step">Claude Code veya Codex'te bir oturum başlayınca burada görünür.</span>
          <button class="link" onclick={openSettings}>Bağlantıları aç</button>
        {/if}
      </span>
    </section>
  {/if}

  {#if view}
    <SourceGrid {view} onpick={tile} onadd={addSource} />
  {:else}
    <section class="card list" aria-label="Diğer oturumlar">
      {#each rows as agent (agent.id)}
        <button class="ses" style:--tone={statusTone[agent.status]} onclick={() => select(agent.id)} title="{agent.project} · {toolName(agent.tool)}">
          <ToolZody tool={agent.tool} mood={agentMood(agent.status, agent.asleep)} size={24} num={agent.taskNo} />
          <span class="sn"><b>{agent.project}</b><small>{#if categoryOf(agent)}{categoryOf(agent)} · {/if}{rowStatus(agent)}</small></span>
        </button>
      {/each}
      {#if others.length === 0}<span class="none">Başka oturum yok</span>{/if}
      {#if lead}
        <button class="all" onclick={onopen}>{others.length > rows.length ? `+${others.length - rows.length} oturum daha · ayrıntıda →` : 'Hepsi ayrıntıda →'}</button>
      {/if}
    </section>
  {/if}

  {#if ctx.tabs.includes('music') && !view}
    <button class="side red wide" onclick={onmusic} title={ctx.music ? `Mini oynatıcı · ${ctx.music.title}` : 'Mini oynatıcı'}>
      {#if ctx.music}
        <span class="art">{#if data.artUrl}<img src={data.artUrl} alt="" />{:else}<Vinyl playing={ctx.music.playing} size={30} />{/if}</span>
        <span class="t two">{ctx.music.title}</span>
        {#if ctx.music.playing}<span class="pp" aria-hidden="true"><i></i><i></i></span>{/if}
      {:else}
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 18V5l11-2v13" /><circle cx="6" cy="18" r="3" /><circle cx="17" cy="16" r="3" /></svg>
        <span class="t">Müzik yok</span>
      {/if}
    </button>
  {/if}
</div>

<style>
  .body {
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
    gap: 8px;
  }
  /* Cam kart (Faz 1 §2.3): yarı saydam, alttan yükselen dalgalı ışık altından görünür. */
  .card {
    background: var(--glass);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: var(--card-r);
  }
  .lead {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 14px;
    text-align: left;
  }
  /* Mail/GitHub kartı: içerik (MiniMail/MiniGithub) kartı doldurur, çerçeve burada. */
  .src {
    align-items: stretch;
    padding: 8px 10px;
  }
  .src > :global(*) {
    flex: 1;
    min-width: 0;
  }
  .click {
    position: relative;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }
  .click:hover {
    background: linear-gradient(180deg, rgb(255 255 255 / 0.08), rgb(255 255 255 / 0.035));
    border-color: var(--line-strong);
  }
  .cover {
    position: absolute;
    inset: 0;
    border-radius: inherit;
  }
  .click .info {
    pointer-events: none;
  }
  .later {
    flex: none;
    white-space: nowrap;
    pointer-events: auto;
    position: relative;
    margin-left: auto;
    padding: 2px 8px;
    border-radius: 999px;
    color: var(--accent-soft);
    font-size: var(--fs-11);
    background: color-mix(in srgb, var(--accent-soft) 12%, transparent);
  }
  .later:hover {
    background: color-mix(in srgb, var(--accent-soft) 22%, transparent);
  }
  .mode {
    flex: none;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent-soft) 14%, transparent);
  }
  .mode.auto {
    color: var(--blue);
    background: color-mix(in srgb, var(--blue) 14%, transparent);
  }
  .mode.danger {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 16%, transparent);
  }
  .prog {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .prog b {
    font-weight: 650;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }
  .bar {
    width: 64px;
    height: 4px;
    flex: none;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  /* Yalnız değer değişince transform geçişi; boştayken hiçbir şey oynamaz. */
  .fill {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, var(--teal), color-mix(in srgb, var(--teal) 60%, #fff));
    box-shadow: 0 0 6px var(--teal);
    transform-origin: left;
    transition: transform var(--t-base) var(--ease);
  }
  .cur {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fold {
    flex: none;
    margin-left: auto;
    padding: 1px 9px;
    border-radius: 999px;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .queue + .fold {
    margin-left: 0;
  }
  .fold:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .queue {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    flex: none;
    color: var(--text-2);
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }
  .qb {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    background: var(--surface-3);
    color: var(--text);
  }
  .qb:hover {
    background: var(--line-strong);
  }
  .ask {
    display: block;
    border-color: color-mix(in srgb, var(--amber) 45%, transparent);
    background: linear-gradient(180deg, color-mix(in srgb, var(--amber) 10%, transparent), rgb(255 255 255 / 0.025) 65%);
    box-shadow:
      var(--card-depth),
      0 0 18px color-mix(in srgb, var(--amber) 14%, transparent);
    overflow: hidden;
  }
  .info {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .who {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .who b {
    flex: 0 1 auto;
    min-width: 2.5em;
    font-weight: 650;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who small,
  .meta small {
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11-5);
    white-space: nowrap;
  }
  .idx {
    flex: none;
    margin-left: auto;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .step {
    color: var(--text-2);
    font-size: var(--fs-12-5);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .recent {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .rs {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    font-size: var(--fs-12);
    color: var(--text-3);
  }
  .rs.now {
    color: var(--text);
  }
  .rs .verb {
    flex: none;
  }
  .rs .tgt {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
    font-size: var(--fs-11-5);
  }
  .question {
    flex-direction: column;
    align-items: stretch;
    justify-content: center;
    gap: 4px;
  }
  .plan {
    display: flex;
  }
  .question .who b {
    min-width: 0;
  }
  .act {
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .question .btns {
    margin-top: 6px;
    align-items: center;
  }
  .amber {
    color: var(--amber);
  }
  .q {
    margin: 0;
    font-weight: 600;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .q small {
    font-weight: 400;
    color: var(--text-3);
  }
  .err {
    margin: 0;
    color: var(--red);
    font-size: var(--fs-12);
  }
  .btns {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .btn {
    height: 28px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--glass);
    box-shadow:
      inset 0 0 0 1px var(--line),
      inset 0 1px 0 rgb(255 255 255 / 0.07);
    font-weight: 600;
    font-size: var(--fs-12);
  }
  .btn:hover {
    background: var(--surface-3);
  }
  .btn.ok {
    background: var(--amber-grad);
    color: var(--on-amber);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.45),
      0 0 16px color-mix(in srgb, var(--amber) 45%, transparent);
  }
  .native {
    flex: 1;
    min-width: 0;
    color: var(--text-2);
    font-size: var(--fs-12);
    text-align: left;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .empty b {
    font-weight: 600;
  }
  .link {
    align-self: flex-start;
    color: var(--accent-soft);
    font-size: var(--fs-12);
  }
  .link:hover {
    text-decoration: underline;
  }
  .none {
    align-self: center;
    text-align: center;
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .side {
    width: 62px;
    flex: none;
    border-radius: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 6px;
    font-size: var(--fs-10-5);
    color: var(--text-2);
  }
  .red {
    background: linear-gradient(180deg, color-mix(in srgb, var(--coral) 22%, transparent), color-mix(in srgb, var(--coral) 8%, transparent));
    border: 1px solid color-mix(in srgb, var(--coral) 38%, transparent);
  }
  .red:hover {
    background: linear-gradient(180deg, color-mix(in srgb, var(--coral) 32%, transparent), color-mix(in srgb, var(--coral) 12%, transparent));
  }
  .side svg {
    width: 22px;
    height: 22px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
  }
  .t {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Hale, oturum listesi ve geniş müzik alanı. Hepsi durağan; yalnız hover'da renk. */
  .halo {
    flex: none;
    width: 76px;
    height: 76px;
    margin: -6px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: radial-gradient(closest-side, color-mix(in srgb, var(--tone) 24%, transparent), transparent);
  }
  .list {
    width: 172px;
    flex: none;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
  }
  .ses {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 8px 5px 12px;
    border-radius: 11px;
    text-align: left;
    min-width: 0;
  }
  .ses::before {
    content: '';
    position: absolute;
    left: 3px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--tone);
  }
  .ses:hover {
    background: var(--surface-2);
  }
  .sn {
    display: grid;
    min-width: 0;
  }
  .sn b {
    font-weight: 600;
    font-size: var(--fs-12-5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sn small {
    font-size: var(--fs-11);
    color: var(--tone);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list .none {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .all {
    margin-top: auto;
    padding: 4px 10px;
    text-align: left;
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .all:hover {
    color: var(--text-2);
  }
  .side.wide {
    width: 78px;
    gap: 7px;
    padding: 8px 6px;
  }
  .art {
    width: 48px;
    height: 48px;
    flex: none;
    border-radius: 12px;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--coral) 30%, var(--surface-2));
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .t.two {
    font-size: var(--fs-11);
    line-height: 1.2;
    color: var(--text);
    text-align: center;
    white-space: normal;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .pp {
    display: flex;
    gap: 3px;
  }
  .pp i {
    width: 3px;
    height: 9px;
    border-radius: 1px;
    background: var(--coral);
  }
</style>
