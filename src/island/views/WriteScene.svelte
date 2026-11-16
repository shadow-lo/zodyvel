<script lang="ts">
  // Yazma sahnesi (U1): mini adada simge çubuğu ve mini AI'nın yerine açılır (✎, kısayol ya da kartın
  // "✎ Yaz"ı). Üstte "Kime" şeridi (açık oturumlar · Kapanmış ▾ · + Yeni görev), solda hedefin Zody'si,
  // ortada son satırlar, altta yazı kutusu. Esc ya da ‹ mini AI'ya döner. Yalnız Yeni görünümde.
  // Animasyon yok: seçimler yalnız renk değiştirir, imleç durağan.
  import { onMount, tick } from 'svelte';
  import { agentMood } from '../../lib/mood';
  import { statusTone } from '../../lib/agent-view';
  import { stepVerb } from '../../lib/steps';
  import { toolName, windowLabel } from '../../lib/format';
  import {
    data,
    focusSession,
    listClosedSessions,
    listRecentProjects,
    pickFolder,
    resumeSession,
    sendNote,
    sendTask,
    startTask,
    startTyping,
    type IslandContext,
  } from '../../lib/store.svelte';
  import type { ClosedSession, RecentProject, TaskPermission } from '../../lib/types';
  import { firstTarget, folderFor, nextTaskNo, sendLabel, stripAgents, whenLabel, withAttachments, writeMode, type Attachment } from '../../lib/write';
  import StepIcon from '../parts/StepIcon.svelte';
  import ToolZody from '../parts/ToolZody.svelte';

  interface Props {
    ctx: IslandContext;
    /** Mini AI'ya dön (‹, gönderimden sonra ya da bekleyen onaya bakmak için). */
    onback: () => void;
  }
  let { ctx, onback }: Props = $props();

  const newOn = $derived(data.settings.features.newTask);
  // Hedef: oturum kimliği, 'closed' (kapanmış listesi) ya da 'new'. Açılırken istenen oturum seçilir.
  const wanted = data.writeTarget;
  const newAtStart = data.settings.features.newTask;
  let target = $state<string>(
    wanted === 'new' && newAtStart ? 'new' : (firstTarget(data.agents, wanted === 'new' ? null : (wanted ?? data.selected), newAtStart) ?? ''),
  );
  data.writeTarget = null;
  // Raftan "Claude'a sor": sahne açılırken eklenen dosyalar (× ile çıkarılır). Gönderilince metne yol olarak eklenir.
  let attached = $state<Attachment[]>(data.writeAttach);
  data.writeAttach = [];

  const agent = $derived(ctx.agents.find(a => a.id === target) ?? null);
  const mode = $derived(agent ? writeMode(agent) : null);
  // Şeride üç oturum sığar; daha çoksa iki oturum ve "+N" (Kapanmış ve Yeni görev hep görünsün).
  const strip = $derived(stripAgents(ctx.agents, agent?.id ?? null, ctx.agents.length > 3 ? 2 : 3));

  let draft = $state('');
  let error = $state<string | null>(null);
  let info = $state<string | null>(null);
  let busy = $state(false);
  let box = $state<HTMLTextAreaElement | undefined>();

  // Kapanmış oturumlar ve son klasörler yalnız o hedef seçilince bir kez okunur.
  let closed = $state<ClosedSession[] | null>(null);
  let resume = $state<ClosedSession | null>(null);
  let projects = $state<RecentProject[] | null>(null);
  let folder = $state('');
  let permission = $state<TaskPermission>('default');
  const OTHER = '…';

  async function choose(next: string) {
    target = next;
    error = null;
    info = null;
    resume = null;
    if (next === 'closed' && !closed) closed = await listClosedSessions(10);
    if (next === 'new' && !projects) {
      projects = await listRecentProjects();
      // Ek son klasörlerden birinin içindeyse görev o klasörde başlar.
      if (!folder) folder = (attached[0] && folderFor(attached[0].path, projects)) || projects[0]?.path || '';
    }
    if (next !== 'closed') await focusBox();
  }

  async function folderChanged(value: string) {
    if (value !== OTHER) return void (folder = value);
    const picked = await pickFolder();
    if (picked) {
      const name = picked.split(/[\\/]/).filter(Boolean).at(-1) ?? picked;
      if (!projects?.some(p => p.path === picked)) projects = [{ path: picked, name, lastUsed: Date.now() }, ...(projects ?? [])];
      folder = picked;
    } else {
      // Seçici kapandı: liste eski seçimde kalsın.
      folder = folder || projects?.[0]?.path || '';
    }
    await focusBox();
  }

  async function focusBox() {
    await tick();
    box?.focus();
  }

  // Ada normalde odak almaz: sahne açılınca klavyeyi ister (ada kapanınca Rust odağı geri verir).
  onMount(() => {
    startTyping();
    if (target === 'new') void choose('new');
    else void focusBox();
  });

  async function submit(now = false) {
    const typed = draft.trim();
    if (!typed || busy) return;
    const text = withAttachments(typed, attached);
    busy = true;
    error = null;
    info = null;
    try {
      if (target === 'new') {
        data.taskFailed = null;
        const r = await startTask(folder, text, permission);
        if (typeof r === 'string') return void (error = r);
        sent();
        data.selected = r.sessionId;
        onback();
      } else if (target === 'closed' && resume) {
        error = await resumeSession(resume, text);
        if (error) return;
        sent();
        onback();
      } else if (agent && mode === 'queue') {
        error = await sendNote(agent, text, now);
        if (!error) {
          sent();
          info = now ? 'Bir sonraki adımda araya girecek.' : 'Sıraya kondu; tur bitince gider.';
        }
      } else if (agent && mode === 'task') {
        error = await sendTask(agent, text);
        if (!error) {
          sent();
          info = 'İletildi; Claude yeni tura başladı.';
        }
      } else if (agent && mode === 'copy') {
        try {
          await navigator.clipboard.writeText(text);
        } catch {
          return void (error = 'Panoya kopyalanamadı.');
        }
        sent();
        info = 'Panoya kopyalandı; terminalde yapıştırıp gönder.';
        if (agent.app) error = await focusSession(agent.id);
      }
    } finally {
      busy = false;
    }
  }

  function sent() {
    draft = '';
    attached = [];
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void submit();
    }
  }

  const button = $derived(target === 'new' ? 'Başlat' : target === 'closed' ? 'Devam et' : sendLabel(mode ?? 'task'));
  const placeholder = $derived(
    attached.length && target !== 'closed' ? (attached.length === 1 ? `${attached[0]!.name} hakkında ne sorayım?` : 'Bu dosyalar hakkında ne sorayım?')
    : target === 'new' ? 'Görevi yaz…' : target === 'closed' ? 'Ne yapsın? (ör. kaldığın yerden devam et)' : mode === 'queue' ? 'Ajana not yaz…' : 'Yeni istek yaz…',
  );
  // Son cevaptan iki satır (Markdown işaretleri atılır).
  const reply = $derived(agent?.lastReply ? agent.lastReply.replace(/[*`#>_]/g, '').replace(/^\s*-\s+/gm, '').split('\n').filter(Boolean).join(' ') : null);
  const recent = $derived(agent?.steps.slice(-2) ?? []);
  const fail = $derived(data.taskFailed);
  const nextNo = $derived(nextTaskNo(ctx.agents));
  const blockedTitle = (project: string, tool: string) => `${project}: ${toolName(tool)}'e yalnız kendi penceresinden yazılır`;
</script>

<div class="scene">
  <header class="top">
    <button class="back" title="Geri (Esc)" aria-label="Geri" onclick={onback}>‹</button>
    <span class="lbl">Kime</span>
    <div class="targets" role="radiogroup" aria-label="Kime yazılacak">
      {#each strip.shown as a (a.id)}
        {@const blocked = writeMode(a) === 'blocked'}
        <button
          class="pill"
          class:on={target === a.id}
          class:off={blocked}
          role="radio"
          aria-checked={target === a.id}
          aria-disabled={blocked}
          style:--tone={statusTone[a.status]}
          title={blocked ? blockedTitle(a.project, a.tool) : `${a.project} · ${toolName(a.tool)}`}
          onclick={() => !blocked && choose(a.id)}
        >
          <ToolZody tool={a.tool} size={18} mood={agentMood(a.status, a.asleep)} />
          <span class="pn">{a.project}</span><i class="dot"></i>
        </button>
      {/each}
      {#if strip.more}
        {@const hidden = ctx.agents.filter(a => !strip.shown.includes(a))}
        {@const nextOpen = hidden.find(a => writeMode(a) !== 'blocked')}
        <button class="pill ghost" title={hidden.map(a => a.project).join(', ')} disabled={!nextOpen} onclick={() => nextOpen && choose(nextOpen.id)}>+{strip.more}</button>
      {/if}
      {#if newOn}
        <button class="pill ghost" class:on={target === 'closed'} role="radio" aria-checked={target === 'closed'} onclick={() => choose('closed')}>Kapanmış ▾</button>
        <button class="pill add" class:on={target === 'new'} role="radio" aria-checked={target === 'new'} onclick={() => choose('new')}>+ Yeni görev</button>
      {/if}
    </div>
    {#if ctx.approvals.length}
      <button class="wait" onclick={onback} title="Bekleyen isteğe bak">{ctx.approvals.length} onay</button>
    {/if}
  </header>

  <div class="main">
    {#if target === 'new' && newOn}
      <span class="z"><ToolZody tool="claude" size={56} num={nextNo} /></span>
      <div class="col">
        <div class="opts">
          <label class="opt"><small>Klasör</small>
            <select class="sel" value={folder} onchange={e => folderChanged(e.currentTarget.value)} aria-label="Klasör">
              {#if !projects?.length}<option value="" disabled>Klasör yok</option>{/if}
              {#each projects ?? [] as p (p.path)}<option value={p.path} title={p.path}>{p.name}</option>{/each}
              <option value={OTHER}>Başka klasör…</option>
            </select>
          </label>
          <span class="opt"><small>İzin</small>
            <span class="seg" role="radiogroup" aria-label="İzin kipi">
              <button role="radio" aria-checked={permission === 'default'} class:on={permission === 'default'} onclick={() => (permission = 'default')} title="Her düzenleme ve komut için onayın sorulur">Sorarak</button>
              <button role="radio" aria-checked={permission === 'acceptEdits'} class:on={permission === 'acceptEdits'} onclick={() => (permission = 'acceptEdits')} title="Dosya düzenlemeleri sorulmadan yapılır; komutlar yine sorulur">Düzenlemeler serbest</button>
            </span>
          </span>
        </div>
        {@render compose()}
        {#if error}<small class="err">{error}</small>
        {:else if fail}<small class="err">Görev başlamadı: {fail.message}</small>
        {:else}<small class="hint">Claude Code bu klasörde arka planda başlar; adaya {nextNo} numaralı Zody olarak düşer, onaylar buraya gelir. Planından gider.</small>{/if}
      </div>
    {:else if target === 'closed' && !resume}
      <span class="z"><ToolZody tool="claude" size={56} mood="sleep" /></span>
      <div class="col">
        {#if closed === null}
          <small class="hint">Kapanmış oturumlar okunuyor…</small>
        {:else if closed.length === 0}
          <small class="hint">Kapanmış oturum bulunamadı.</small>
        {:else}
          <ul class="list" aria-label="Kapanmış oturumlar" data-scroll>
            {#each closed as c (c.sessionId)}
              <li>
                <button onclick={() => ((resume = c), void focusBox())} title={c.cwd}>
                  <b>{c.project}</b><span class="what">“{c.firstPrompt}”</span><small>{whenLabel(c.endedAt)}</small>
                </button>
              </li>
            {/each}
          </ul>
          <small class="hint">Seçince kaldığı yerden devam eder (<span class="mono">--resume</span>, oturumun klasöründe). Son 10 oturum.</small>
        {/if}
      </div>
    {:else if target === 'closed' && resume}
      <span class="z"><ToolZody tool="claude" size={56} mood="sleep" /></span>
      <div class="col">
        <span class="ln picked"><b>{resume.project}</b><span class="what">“{resume.firstPrompt}”</span><small>{whenLabel(resume.endedAt)}</small><button class="inl" onclick={() => (resume = null)}>listeye dön</button></span>
        {@render compose()}
        {#if error}<small class="err">{error}</small>{:else}<small class="hint">Claude Code bu oturumu klasöründe sürdürür; adaya yeniden düşer.</small>{/if}
      </div>
    {:else if agent}
      <span class="z halo" style:--tone={statusTone[agent.status]}>
        <ToolZody tool={agent.tool} size={56} mood={agentMood(agent.status, agent.asleep)} num={agent.taskNo} />
      </span>
      <div class="col">
        <div class="last">
          {#if mode === 'queue' && recent.length}
            {#each recent as s (s.id)}
              <span class="ln"><StepIcon step={s} /><span>{stepVerb(s)}</span>{#if s.target && s.kind !== 'done'}<span class="mono t">{s.target}</span>{/if}</span>
            {/each}
          {:else if reply}
            <span class="ln reply"><b>{toolName(agent.tool)}:</b> {reply}</span>
          {:else}
            <span class="ln">{agent.step}</span>
          {/if}
        </div>
        {@render compose()}
        {#if error}<small class="err">{error}</small>
        {:else if info}<small class="hint ok">{info}</small>
        {:else if mode === 'queue'}
          <small class="hint">Çalışıyor: tur bitince iletilir · <button class="inl" disabled={!draft.trim() || busy} onclick={() => submit(true)}>Hemen ilet</button>{#if agent.notes.length}{` · sırada ${agent.notes.length} not`}{/if}</small>
        {:else if mode === 'task'}
          <small class="hint">Bitti, seni bekliyor: yeni tur olarak hemen başlar · Shift+Enter yeni satır</small>
        {:else}
          <small class="hint">Bu oturum adadan görev beklemiyor: panoya kopyalanır{agent.app ? `, ${windowLabel(agent.app).replace(' git', '')} öne gelir` : ''}.</small>
        {/if}
      </div>
    {:else}
      <span class="z"><ToolZody tool="claude" size={56} mood="sleep" /></span>
      <div class="col"><small class="hint">Yazılabilecek açık oturum yok.</small></div>
    {/if}
  </div>
</div>

{#snippet compose()}
  <form class="box" onsubmit={e => (e.preventDefault(), void submit())}>
    {#each attached as a (a.path)}
      <!-- Raftan gelen ek: yazı kutusunun başında küçük bir hap; × çıkarır. -->
      <span class="chip" title={a.path}><span class="cn">{a.name}</span><button type="button" class="cx" aria-label="{a.name} ekini çıkar" title="Eki çıkar" onclick={() => (attached = attached.filter(x => x !== a))}>×</button></span>
    {/each}
    <textarea bind:this={box} bind:value={draft} rows="1" maxlength="4000" {placeholder} {onkeydown} onfocus={startTyping} aria-label={placeholder}></textarea>
    <button class="send" disabled={!draft.trim() || busy || (target === 'new' && !folder)}>{button} ↵</button>
  </form>
{/snippet}

<style>
  .scene {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .top {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px 0 8px;
  }
  .back {
    width: 28px;
    height: 26px;
    flex: none;
    border-radius: 8px;
    font-size: 18px;
    line-height: 1;
    color: var(--text-2);
  }
  .back:hover {
    background: var(--surface-2);
  }
  .lbl {
    flex: none;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .targets {
    flex: 1;
    display: flex;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }
  .pill {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    max-width: 130px;
    padding: 0 9px 0 5px;
    border-radius: 999px;
    font-size: var(--fs-12);
    color: var(--text-2);
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .pill:hover {
    border-color: var(--line-strong);
  }
  .pill.on {
    color: var(--text);
    border-color: var(--accent-soft);
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
  }
  .pill.off {
    opacity: 0.45;
    cursor: default;
  }
  .pn {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill .dot {
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--tone);
  }
  .pill.ghost,
  .pill.add {
    padding: 0 10px;
  }
  .pill.add {
    color: var(--accent-soft);
  }
  .wait {
    flex: none;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: var(--fs-11);
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .wait:hover {
    background: color-mix(in srgb, var(--amber) 24%, transparent);
  }
  .main {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 12px;
    margin: 0 10px 10px;
    padding: 10px 14px 10px 10px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    background-image: var(--card-sheen);
  }
  .z {
    flex: none;
    align-self: center;
    border-radius: 50%;
  }
  .halo {
    background: radial-gradient(circle, color-mix(in srgb, var(--tone) 26%, transparent) 0, transparent 68%);
  }
  .col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
  }
  .last {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
  }
  .ln {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-12);
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ln .t {
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .reply {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    white-space: normal;
  }
  .reply b,
  .picked b {
    color: var(--text);
    font-weight: 600;
  }
  .box {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    padding: 4px 4px 4px 12px;
    border-radius: 11px;
    background: var(--island);
    border: 1px solid var(--line-strong);
  }
  .chip {
    flex: none;
    max-width: 150px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 0 2px 0 8px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
    color: var(--accent-soft);
    font-size: var(--fs-11);
    font-weight: 600;
  }
  .cn {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cx {
    width: 18px;
    height: 18px;
    flex: none;
    border-radius: 5px;
    color: var(--text-3);
    line-height: 1;
  }
  .cx:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .box:focus-within {
    border-color: var(--accent-soft);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 3.9em;
    field-sizing: content;
    resize: none;
    border: 0;
    outline: 0;
    padding: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: var(--fs-12-5);
    line-height: 1.3;
    /* İmleç yanıp sönmez: boşta animasyon yok. */
    caret-color: var(--accent-soft);
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .send {
    flex: none;
    align-self: flex-end;
    padding: 5px 12px;
    border-radius: 8px;
    font-size: var(--fs-12);
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
  }
  .send:disabled {
    opacity: 0.45;
  }
  .hint,
  .err {
    font-size: var(--fs-11);
    line-height: 1.35;
    color: var(--text-3);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .hint.ok {
    color: var(--green);
  }
  .err {
    color: var(--red);
  }
  .inl {
    color: var(--accent-soft);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .inl:disabled {
    color: var(--text-3);
  }
  .picked .inl {
    margin-left: auto;
    font-size: var(--fs-11);
  }
  .opts {
    display: flex;
    gap: 14px;
    align-items: center;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .opt small {
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .sel {
    max-width: 150px;
    padding: 3px 8px;
    border: 0;
    border-radius: 8px;
    font: inherit;
    font-size: var(--fs-12);
    color: var(--text);
    background: var(--surface-2);
  }
  .seg {
    display: flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .seg button {
    padding: 2px 9px;
    border-radius: 7px;
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .seg .on {
    color: var(--text);
    background: var(--surface-3);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 88px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .list button {
    width: 100%;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 8px;
    text-align: left;
    font-size: var(--fs-12);
    color: var(--text-2);
    white-space: nowrap;
  }
  .list button:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .list b {
    font-weight: 600;
    color: var(--text);
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .list small,
  .picked small {
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
</style>
