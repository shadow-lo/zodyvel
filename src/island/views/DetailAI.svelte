<script lang="ts">
  // AI ayrıntısı: solda oturum listesi (önce beklemede olan), sağda seçili oturum. Başlıkta model,
  // bağlam doluluğu ve klasör kısayolları; altında Adımlar · Yapılacaklar · Değişenler · Son cevap
  // sekmeleri ve ajana not kutusu. Bekleyen onay içeriğin üstünde, soru ve plan içeriğin yerinde görünür.
  // Başlık iki satır (sağda durum ve simge düğmeleri), bağlam küçük bir çubuk.
  import { tick, untrack } from 'svelte';
  import { byUrgency, categoryOf, contextHot, contextPercent, loopOf, modeBadge, rowStatus, statusTone, todoProgress } from '../../lib/agent-view';
  import { duration, lineCounts, since, toolName, windowLabel } from '../../lib/format';
  import { limitPill } from '../../lib/limits';
  import { agentMood, pickLead } from '../../lib/mood';
  import { defaultStep, stepVerb } from '../../lib/steps';
  import {
    data,
    editorPresent,
    focusSession,
    openFolder,
    planLimits,
    removeNote,
    requestDiff,
    sendNote,
    sendTask,
    sessionContext,
    startTyping,
    stepContent,
    type IslandContext,
  } from '../../lib/store.svelte';
  import type { AgentStep, PlanLimits, SessionContext, StepContent } from '../../lib/types';
  import ApprovalCard from '../parts/ApprovalCard.svelte';
  import MarkdownView from '../parts/MarkdownView.svelte';
  import PlanReview from '../parts/PlanReview.svelte';
  import QuestionForm from '../parts/QuestionForm.svelte';
  import StatusBadge from '../parts/StatusBadge.svelte';
  import StepIcon from '../parts/StepIcon.svelte';
  import StepView from '../parts/StepView.svelte';
  import UsagePanel from '../parts/UsagePanel.svelte';
  import ToolZody from '../parts/ToolZody.svelte';
  import { writeMode } from '../../lib/write';

  interface Props {
    ctx: IslandContext;
    /** Yazma sahnesi (Yeni görünüm + özellik): not kutusunun yerine "✎ Yaz" o oturumla sahneyi açar. */
    onwrite?: (target: string) => void;
  }
  let { ctx, onwrite }: Props = $props();

  const approval = $derived(ctx.approvals[0]);
  const lead = $derived(approval ? (ctx.agents.find(a => a.id === approval.sessionId) ?? null) : pickLead(ctx.agents, data.selected));
  const sessions = $derived(byUrgency(ctx.agents));
  const question = $derived(ctx.approvals.find(r => r.sessionId === lead?.id && r.kind === 'question'));
  const permissions = $derived(ctx.approvals.filter(r => r.sessionId === lead?.id && r.kind === 'permission'));
  const plan = $derived(ctx.approvals.find(r => r.sessionId === lead?.id && r.kind === 'plan'));
  const features = $derived(data.settings.features);

  type TabId = 'steps' | 'todos' | 'changed' | 'reply';
  let tab = $state<TabId>('steps');
  const progress = $derived(lead ? todoProgress(lead.todos) : null);
  const tabs = $derived.by(() => {
    if (!lead) return [];
    const list: { id: TabId; label: string }[] = [{ id: 'steps', label: 'Adımlar' }];
    if (progress) list.push({ id: 'todos', label: `Yapılacaklar ${progress.done}/${progress.total}` });
    if (lead.changed.length) list.push({ id: 'changed', label: `Değişenler ${lead.changed.length}` });
    if (lead.lastReply) list.push({ id: 'reply', label: 'Son cevap' });
    return list;
  });
  // Sekme kaybolursa (yeni tur, liste boşaldı) Adımlar'a dönülür.
  const shown = $derived(tabs.some(t => t.id === tab) ? tab : 'steps');

  // Seçili adım: kullanıcı bir adıma ya da dosyaya tıklayana kadar sürmekte olan (ya da son) adım izlenir.
  let pickedId = $state<number | null>(null);
  $effect(() => {
    void lead?.id;
    untrack(() => {
      pickedId = null;
      tab = 'steps';
      draft = '';
      noteError = null;
    });
  });
  // "Bitti · 4 dosya" bildiriminden gelindi: Değişenler sekmesi ve ilk dosyanın farkı açılır. Oturum
  // değişince sekmeyi sıfırlayan etkiden sonra çalışır.
  $effect(() => {
    if (data.detailTab !== 'changed' || !lead) return;
    untrack(() => {
      data.detailTab = null;
      if (!lead.changed.length) return;
      tab = 'changed';
      pickedId = lead.changed[0]!.step;
    });
  });
  // Son "Tur tamamlandı" adımı seçiliyken ajanın son cevabı tam hâliyle gösterilir; tur bitmişse bu adım
  // kendiliğinden seçilir (cevabı okuyup yeni isteği yazmak için).
  const replyStepId = $derived(lead?.lastReply ? (lead.steps.filter(s => s.kind === 'done').at(-1)?.id ?? null) : null);
  const finished = $derived(lead?.status === 'done' && replyStepId !== null && lead.steps.at(-1)?.id === replyStepId);
  const step = $derived(
    (pickedId !== null ? lead?.steps.find(s => s.id === pickedId) : undefined) ??
      (lead ? (finished ? lead.steps.at(-1)! : defaultStep(lead.steps)) : null),
  );

  // İçerik yalnız seçili adım (ya da durumu, ör. komut bitti) değişince istenir.
  let content = $state<StepContent | null>(null);
  const contentKey = $derived(lead && step ? `${lead.id}:${step.id}:${step.state}` : '');
  $effect(() => {
    const key = contentKey;
    if (!key || !lead || !step) return;
    const [session, current] = [lead, step];
    let live = true;
    untrack(() => (content = null));
    void stepContent(session, current).then(c => {
      if (live) content = c ?? { kind: 'none', path: null, lines: [], note: null };
    });
    return () => (live = false);
  });

  // Bekleyen dosya değişikliğinin farkı: istek değişince bir kez istenir.
  const diffRequest = $derived(permissions.find(r => r.files.length > 0));
  let diff = $state<StepContent | null>(null);
  $effect(() => {
    const id = diffRequest?.id;
    untrack(() => (diff = null));
    if (!id) return;
    let live = true;
    void requestDiff(id).then(c => {
      if (live) diff = c ?? { kind: 'none', path: null, lines: [], note: null };
    });
    return () => (live = false);
  });
  const diffStep = $derived<AgentStep | null>(
    diffRequest
      ? { id: -1, kind: 'edit', target: diffRequest.files[0]?.name ?? '', label: '', at: diffRequest.createdAt, state: 'now', sub: false }
      : null,
  );

  // Model ve bağlam doluluğu: oturum ya da durumu değişince bir kez okunur (yalnız ayrıntı açıkken).
  let contextInfo = $state<SessionContext | null>(null);
  const contextKey = $derived(lead && features.agentContext ? `${lead.id}:${lead.status}:${lead.turnEndedAt}` : '');
  $effect(() => {
    const key = contextKey;
    const session = untrack(() => lead);
    untrack(() => (contextInfo = null));
    if (!key || !session) return;
    let live = true;
    void sessionContext(session).then(c => {
      if (live) contextInfo = c;
    });
    return () => (live = false);
  });
  const model = $derived(contextInfo?.model ? modelName(contextInfo.model) : null);
  // Tahmini doluluk; %80'e varınca amber ve "yakında sıkıştırılacak" ipucu.
  const contextPct = $derived(contextInfo?.window ? contextPercent(contextInfo.tokens, contextInfo.window) : null);
  const contextFull = $derived(contextPct !== null && contextHot(contextPct));
  function modelName(id: string): string {
    const family = ['opus', 'sonnet', 'haiku', 'fable'].find(f => id.toLowerCase().includes(f));
    return family ? family[0]!.toUpperCase() + family.slice(1) : id;
  }

  let editor = $state(false);
  void editorPresent().then(present => (editor = present));

  let actionError = $state<string | null>(null);
  async function focus() {
    if (lead) actionError = await focusSession(lead.id);
  }
  async function open(target: 'folder' | 'code') {
    if (lead) actionError = await openFolder(lead.id, target);
  }

  const running = $derived(lead?.status === 'running');
  const badge = $derived(modeBadge(lead?.mode ?? null));
  const loop = $derived(features.loopAlert && lead ? loopOf(lead.steps) : null);

  // Ajana not: çalışan oturumda "Sonra" tur biterken, "Şimdi" bir sonraki araç çağrısında iletilir.
  // Boştaki oturum: tur sonunda bekleyen köprü varsa görev gönderilir (Claude uyanır); yoksa metin
  // panoya kopyalanır ve oturumun penceresi öne gelir (pencereye yazı enjekte edilmez).
  const notesOn = $derived(features.agentNotes);
  const idle = $derived(lead ? lead.status !== 'running' && lead.status !== 'waiting_approval' : false);
  const queueable = $derived(Boolean(lead?.canNote) && !idle);
  const taskable = $derived(Boolean(lead?.canTask) && idle);
  let draft = $state('');
  let now = $state(false);
  let noteError = $state<string | null>(null);
  let copied = $state(false);
  let input = $state<HTMLInputElement | undefined>();
  const QUICK = ['Devam et', 'Testleri çalıştır'];

  async function submit(text = draft) {
    if (!lead || !text.trim()) return;
    noteError = null;
    if (queueable) {
      noteError = await sendNote(lead, text, now);
      if (!noteError && text === draft) draft = '';
      return;
    }
    if (taskable) {
      noteError = await sendTask(lead, text);
      if (!noteError && text === draft) draft = '';
      return;
    }
    try {
      await navigator.clipboard.writeText(text.trim());
    } catch {
      noteError = 'Panoya kopyalanamadı.';
      return;
    }
    copied = true;
    setTimeout(() => (copied = false), 2500);
    if (text === draft) draft = '';
    if (lead.app) noteError = await focusSession(lead.id);
  }

  // Mini görünümdeki "Sonra…" ayrıntıyı not kutusu odaklı açar.
  $effect(() => {
    if (!data.noteFocus || !input) return;
    untrack(() => {
      data.noteFocus = false;
      now = false;
      void tick().then(() => input?.focus());
    });
  });

  // Kullanım paneli: soldaki bağlantıdan açılır; bekleyen istek her zaman önce gelir.
  const usageOn = $derived(features.usage);
  const showUsage = $derived(usageOn && data.usageOpen && !question && !plan && !permissions.length);
  $effect(() => () => (data.usageOpen = false));

  // Plan limiti hapı: ayrıntı açıkken dakikada bir köprünün küçük dosyasından okunur.
  let limits = $state<PlanLimits | null>(null);
  let limitsAt = $state(Date.now());
  const limitOn = $derived(features.planLimit);
  $effect(() => {
    if (!limitOn) {
      untrack(() => (limits = null));
      return;
    }
    let live = true;
    const load = () =>
      void planLimits().then(l => {
        if (live) [limits, limitsAt] = [l, Date.now()];
      });
    load();
    const timer = setInterval(load, 60_000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  });
  const pill = $derived(limitPill(limits, limitsAt));

  function pickFile(stepId: number) {
    pickedId = stepId;
  }
</script>

<div class="body">
  {#if !lead && usageOn}
    <section class="card main"><UsagePanel /></section>
  {:else if !lead}
    <p class="none">Şu an etkin bir yapay zekâ oturumu yok.</p>
  {:else}
    <aside class="card side">
      <small class="label">Oturumlar</small>
      <ul class="list" data-scroll>
        {#each sessions as agent (agent.id)}
          <li>
            <button
              class="ses"
              class:sel={agent.id === lead.id}
              class:tone={true}
              style:--tone={statusTone[agent.status]}
              onclick={() => ((data.selected = agent.id), (data.usageOpen = false))}
            >
              <ToolZody tool={agent.tool} mood={agentMood(agent.status, agent.asleep)} size={26} num={agent.taskNo} />
              <span class="sn">
                <b>{agent.project}</b>
                <small class="st">{#if categoryOf(agent)}{categoryOf(agent)} · {/if}{rowStatus(agent)}{agent.status === 'waiting_approval' ? ` · ${since(agent.statusSince)}` : ''}</small>
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if pill}<span class="limit {pill.tone}" title={pill.title}>{pill.text}</span>{/if}
      {#if usageOn}
        <button class="foot icon" class:on={showUsage} onclick={() => (data.usageOpen = !data.usageOpen)}
          ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20V10M10 20V4M16 20v-7M22 20H2" /></svg>Kullanım</button
        >
      {/if}
    </aside>
    <section class="card main">
        <header class="top two">
          <ToolZody tool={lead.tool} mood={agentMood(lead.status, lead.asleep)} size={42} num={lead.taskNo} alive />
          <span class="who">
            <span class="line">
              <b class="nm big">{lead.project}</b>
              {#if badge}<span class="mode {badge.tone}">{badge.text}</span>{/if}
              {#if lead.subagents}<span class="mode">{lead.subagents} alt ajan</span>{/if}
            </span>
            <span class="tl row">
              {#if categoryOf(lead)}<span class="cat" title="Klasör / proje">{categoryOf(lead)} kategorisi</span>{/if}
              <span class="tx"
                >{toolName(lead.tool)}{#if model}{` · ${model}`}{/if}{` · `}{lead.turnEndedAt && lead.turnStartedAt
                  ? `${duration(lead.turnEndedAt - lead.turnStartedAt)} sürdü`
                  : since(lead.startedAt)}</span
              >
              {#if contextPct !== null}
                <span
                  class="ctx gauge"
                  class:hot={contextFull}
                  title={contextFull ? 'Bağlam dolmak üzere; ajan yakında konuşmayı sıkıştıracak (tahmini)' : 'Bağlam doluluğu (tahmini)'}
                  ><span class="meter"><i style:transform="scaleX({contextPct / 100})"></i></span>bağlam %{contextPct}{#if contextFull}, yakında sıkıştırılacak{/if}</span
                >
              {/if}
            </span>
          </span>
          <span class="right">
            <StatusBadge status={lead.status} />
            <span class="icons">
              {#if lead.folder}
                <button onclick={() => open('folder')} title="Klasörü aç · {lead.folder}" aria-label="Klasörü aç"
                  ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg></button
                >
                {#if editor && !/^code/i.test(lead.app ?? '')}<button onclick={() => open('code')} title="Klasörü VS Code'da aç" aria-label="VS Code'da aç"
                    ><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16" /></svg></button
                  >{/if}
              {/if}
              {#if lead.app}<button onclick={focus} title="{windowLabel(lead.app)} · {lead.app} penceresini öne getir" aria-label={windowLabel(lead.app)}
                  ><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 9l3 3-3 3M12 15h5" /></svg></button
                >{/if}
            </span>
          </span>
        </header>
      {#if actionError}<p class="err">{actionError}</p>{/if}

      {#each permissions as request (request.id)}
        <div class="ask"><ApprovalCard {request} /></div>
      {/each}

      {#if showUsage}
        <UsagePanel onclose={() => (data.usageOpen = false)} />
      {:else if question}
        <QuestionForm request={question} />
      {:else if plan}
        <PlanReview request={plan} />
      {:else}
        <nav class="tabs" aria-label="Oturum ayrıntısı">
          {#each tabs as t (t.id)}
            <button class:on={shown === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
          {/each}
        </nav>
        {#if loop}
          <p class="loop">⚠ {loop.command ? 'Aynı komut' : 'Aynı düzenleme'} {loop.count} kez hata verdi: <code class="mono">{loop.target}</code><span>Döngüde olabilir</span></p>
        {/if}
        {#if shown === 'reply' && lead.lastReply}
          <div class="reply" data-scroll><MarkdownView source={lead.lastReply} /></div>
        {:else}
          <div class="split">
            {#if shown === 'todos'}
              <ul class="col todos" data-scroll>
                {#each lead.todos as todo, i (i)}
                  <li class={todo.status}>
                    <span class="box" aria-hidden="true">{todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '•' : ''}</span>
                    <span>{todo.status === 'in_progress' && todo.active ? todo.active : todo.text}</span>
                  </li>
                {/each}
              </ul>
            {:else if shown === 'changed'}
              <ul class="col" data-scroll>
                {#each lead.changed as file (file.path)}
                  <li>
                    <button class="stp" class:sel={file.step === step?.id} onclick={() => pickFile(file.step)} title="{file.path} · farkı göster">
                      <span class="fn mono">{file.name}</span>
                      <span class="add">+{file.added}</span><span class="del">−{file.removed}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {:else}
              <ol class="col" data-scroll>
                {#each lead.steps as s (s.id)}
                  <li>
                    <button class="stp {s.state}" class:sel={s.id === step?.id} class:sub={s.sub} onclick={() => (pickedId = s.id)}>
                      <StepIcon step={s} spinning={running} boxed />
                      <span class="verb">{stepVerb(s)}</span>
                      {#if s.target && s.kind !== 'done'}<span class="tgt mono">{s.target}</span>{/if}
                    </button>
                  </li>
                {:else}
                  <li class="empty">Henüz adım yok.</li>
                {/each}
              </ol>
            {/if}
            <div class="view">
              {#if diffStep}
                <StepView step={diffStep} content={diff} />
              {:else if step && step.id === replyStepId && lead.lastReply}
                <!-- Son cevabın tamamı (en çok 4000 karakter, Rust MAX_REPLY). -->

                <div class="reply" data-scroll><MarkdownView source={lead.lastReply} /></div>
              {:else if step}
                <StepView {step} {content} />
              {:else}
                <p class="none">{lead.step || 'Bekliyor'}</p>
              {/if}
            </div>
          </div>
        {/if}
      {/if}

      {#if notesOn}
        {#each lead.notes as n (n.id)}
          <p class="queued">
            <span>✎ Sırada: “{n.text}”</span>
            <small>{n.now ? 'Bir sonraki adımda araya girer' : 'Tur bitince gider'}</small>
            <button onclick={async () => (noteError = await removeNote(lead, n.id))}>İptal</button>
          </p>
        {/each}
        {#if onwrite && writeMode(lead) !== 'blocked'}
          <!-- Ajana not yazma sahnesine taşındı; sıradaki notlar burada iptal edilebilir kalır. -->
          <div class="compose moved">
            <span>{queueable ? 'Not, tur bitince ya da hemen iletilir.' : taskable ? 'Bitti; yeni görevle uyanır.' : 'Yeni istek panoya kopyalanır.'}</span>
            <button class="send" onclick={() => onwrite?.(lead.id)}>✎ Yaz</button>
          </div>
        {:else}
        <form class="compose" onsubmit={e => (e.preventDefault(), void submit())}>
          {#if queueable}
            <span class="seg" role="radiogroup" aria-label="Ne zaman iletilsin">
              <button type="button" role="radio" aria-checked={!now} class:on={!now} onclick={() => (now = false)} title="Tur bitince yeni istek olarak gider">Sonra</button>
              <button type="button" role="radio" aria-checked={now} class:on={now} onclick={() => (now = true)} title="Bir sonraki araç çağrısını durdurup notu iletir">Şimdi yönlendir</button>
            </span>
          {:else if idle}
            {#each QUICK as text (text)}
              <button type="button" class="quick" onclick={() => submit(text)} title={taskable ? 'Claude bu görevle uyanır' : `Panoya kopyala${lead.app ? ' ve pencereye git' : ''}`}>{text}</button>
            {/each}
          {/if}
          <input
            bind:this={input}
            bind:value={draft}
            onfocus={startTyping}
            maxlength="2000"
            placeholder={queueable ? 'Ajana not yaz…' : taskable ? 'Yeni görev yaz…' : idle ? 'Yeni istek yaz (panoya kopyalanır)…' : 'Bu oturuma not gönderilemiyor'}
            disabled={!queueable && !idle}
          />
          <button class="send" disabled={!draft.trim() || (!queueable && !idle)}>{queueable || taskable ? 'Gönder' : 'Kopyala'}</button>
        </form>
        {#if noteError}<p class="err">{noteError}</p>{:else if copied}<p class="hint">Panoya kopyalandı; terminalde yapıştırıp gönder.</p>{:else if idle && !taskable && lead.canNote}<p class="hint">Bu oturum adadan görev beklemiyor (gerekirse Ayarlar → Bağlantılar → Claude → Güncelle).</p>{:else if !queueable && !idle && !lead.canNote}<p class="hint">Not yalnız Claude Code'da, adadan onay modunda gönderilir (Ayarlar → Bağlantılar).</p>{/if}
        {/if}
      {/if}
    </section>
  {/if}
</div>

<style>
  .body {
    height: calc(100% - 36px);
    padding: 0 10px 10px;
    display: flex;
    gap: 8px;
  }
  /* Cam kart (Faz 1 §2.3): yarı saydam, dalgalı ışık altından görünür. */
  .card {
    background: var(--glass);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: var(--card-r);
  }
  .none {
    margin: auto;
    color: var(--text-3);
  }
  .side {
    width: 168px;
    flex: none;
    padding: 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }
  .label {
    padding: 0 6px;
    color: var(--text-3);
    font-size: var(--fs-11);
    font-weight: 600;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    scrollbar-width: thin;
  }
  .ses {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px;
    border-radius: 10px;
    text-align: left;
  }
  .ses:hover,
  .ses.sel {
    background: var(--surface-2);
  }
  .sn {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .sn b {
    font-size: var(--fs-12-5);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .foot {
    padding: 6px;
    border-top: 1px solid var(--line);
    text-align: left;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .foot:hover,
  .foot.on {
    color: var(--text);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px 4px;
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .nm {
    font-size: var(--fs-14);
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tl {
    color: var(--text-3);
    font-size: var(--fs-11-5);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  :global(:root) .ctx.hot {
    color: var(--amber);
  }
  /* Sohbetin klasörü/projesi (Faz 7.1): ad satırının altında küçük çip. */
  .cat {
    flex: none;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    color: var(--text-2);
    background: rgb(255 255 255 / 0.07);
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
  .err {
    margin: 2px 12px;
    color: var(--red);
    font-size: var(--fs-11-5);
  }
  .hint {
    margin: 0 12px 6px;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .ask {
    margin: 6px 10px 0;
    padding: 10px 12px;
    border-radius: 12px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--amber) 10%, transparent), rgb(255 255 255 / 0.03));
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--amber) 45%, transparent),
      0 0 18px color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    padding: 6px 8px;
    color: var(--text-3);
    font-size: var(--fs-12);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tabs button:hover {
    color: var(--text-2);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent-soft);
    box-shadow: 0 6px 10px -8px var(--accent-soft);
  }
  .tabs :global(.badge) {
    margin-left: auto;
  }
  .loop {
    margin: 6px 10px 0;
    padding: 5px 10px;
    display: flex;
    align-items: center;
    gap: 6px;
    border-radius: 9px;
    font-size: var(--fs-11-5);
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 12%, transparent);
  }
  .loop code {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .loop span {
    margin-left: auto;
    flex: none;
    color: var(--text-3);
  }
  .split {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 8px;
    padding: 6px 10px;
  }
  .col {
    width: 200px;
    flex: none;
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
    scrollbar-width: thin;
  }
  .view {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .reply {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 14px;
    font-size: var(--fs-12-5);
    scrollbar-width: thin;
  }
  /* Zaman çizgisi: adım simgeleri ince bir dikey çizgiyle bağlı (durağan). */
  ol.col li {
    position: relative;
  }
  ol.col li + li::before {
    content: '';
    position: absolute;
    left: 18.5px;
    top: -6px;
    height: 10px;
    width: 1px;
    background: var(--line-strong);
  }
  .stp {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    border-radius: 9px;
    color: var(--text-2);
    text-align: left;
    font-size: var(--fs-12);
  }
  .stp.sub {
    padding-left: 22px;
  }
  .stp:hover,
  .stp.sel {
    background: var(--surface-2);
  }
  .stp.sel {
    color: var(--text);
  }
  .stp.now .verb {
    color: var(--text);
    font-weight: 600;
  }
  .verb {
    flex: none;
    max-width: 60%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tgt,
  .fn {
    min-width: 0;
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tgt {
    margin-left: auto;
  }
  .fn {
    flex: 1;
    color: var(--text-2);
  }
  .add,
  .del {
    flex: none;
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }
  .add {
    color: var(--green);
  }
  .del {
    color: var(--red);
  }
  .todos li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 4px 6px;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .todos li.completed {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .todos li.in_progress {
    color: var(--text);
    font-weight: 600;
  }
  .box {
    width: 14px;
    height: 14px;
    flex: none;
    margin-top: 1px;
    border-radius: 4px;
    border: 1px solid var(--line-strong);
    display: grid;
    place-items: center;
    font-size: var(--fs-xs);
    text-decoration: none;
  }
  .completed .box {
    border-color: var(--green);
    color: var(--green);
  }
  .in_progress .box {
    border-color: var(--blue);
    color: var(--blue);
  }
  .empty {
    color: var(--text-3);
    font-size: var(--fs-12);
    padding: 6px 8px;
  }
  .queued {
    margin: 0 10px 4px;
    padding: 4px 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    border-radius: 9px;
    font-size: var(--fs-11-5);
    color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent-soft) 10%, transparent);
  }
  .queued span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .queued small {
    margin-left: auto;
    flex: none;
    color: var(--text-3);
  }
  .queued button {
    flex: none;
    color: var(--text-2);
  }
  .queued button:hover {
    color: var(--text);
  }
  .compose {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px 8px;
    border-top: 1px solid var(--line);
  }
  .moved span {
    flex: 1;
    min-width: 0;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .seg {
    display: flex;
    flex: none;
    padding: 2px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    padding: 4px 9px;
    border-radius: 7px;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .seg button.on {
    color: var(--text);
    background: var(--surface-3);
  }
  .quick {
    flex: none;
    padding: 5px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-size: var(--fs-11-5);
    font-weight: 600;
  }
  .quick:hover {
    background: var(--surface-3);
  }
  .compose input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 10px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-12);
  }
  .compose input:disabled {
    opacity: 0.6;
  }
  .send {
    flex: none;
    height: 30px;
    padding: 0 14px;
    border-radius: 9px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    font-size: var(--fs-12);
  }
  .send:disabled {
    opacity: 0.45;
  }

  /* Durum çizgili oturum listesi, iki satırlı başlık, bağlam çubuğu ve simge düğmeleri. */
  .ses.tone {
    position: relative;
    padding-left: 10px;
  }
  .ses.tone::before {
    content: '';
    position: absolute;
    left: 2px;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--tone);
  }
  .ses.tone.sel {
    background: linear-gradient(90deg, color-mix(in srgb, var(--tone) 14%, transparent), rgb(255 255 255 / 0.04));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--tone) 26%, transparent);
  }
  .ses.tone::before {
    box-shadow: 0 0 6px var(--tone);
  }
  .ses.tone .st {
    color: var(--tone);
  }
  .limit {
    align-self: flex-start;
    margin: 0 6px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    color: var(--text-2);
    background: var(--surface-2);
  }
  .limit.warn {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 12%, transparent);
  }
  .limit.bad {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 14%, transparent);
  }
  .foot.icon {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .foot.icon svg,
  .icons svg {
    width: 15px;
    height: 15px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .top.two {
    gap: 11px;
    padding: 10px 12px 8px;
  }
  .nm.big {
    font-size: var(--fs-16);
    font-weight: 650;
  }
  .tl.row {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .tl.row .tx {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .gauge {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-3);
  }
  .meter {
    width: 44px;
    height: 4px;
    flex: none;
    border-radius: 2px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .meter i {
    display: block;
    height: 100%;
    background: var(--accent-2);
    transform-origin: left;
  }
  .gauge.hot .meter i {
    background: var(--amber);
  }
  .right {
    flex: none;
    display: grid;
    justify-items: end;
    gap: 6px;
  }
  .icons {
    display: flex;
    gap: 4px;
  }
  .icons button {
    width: 28px;
    height: 24px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    background: var(--surface-2);
  }
  .icons button:hover {
    color: var(--text);
    background: var(--surface-3);
  }
</style>
