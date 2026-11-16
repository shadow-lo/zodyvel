<script lang="ts">
  // Genel bakış: ajanlar, bağlantılar, modüller (tıkla → ilgili sayfa) ve sık kullanılan anahtarlar.
  // Yeni görünümde üç kutu yerine adanın canlı önizlemesi (gerçek Zody, seçili tema; hareketsiz) ve yanında
  // "ilgi isteyen bir şey var mı" satırları; altta kullanıcının kendi kısayolları.
  import Zody from '../../mascot/Zody.svelte';
  import { agentColor } from '../../lib/format';
  import { agentMood, pickLead } from '../../lib/mood';
  import { context, data } from '../../lib/store.svelte';
  import { themeById } from '../../lib/theme';
  import Icon from '../Icon.svelte';
  import { connectionIssues, go, setAutostart, ui, update } from '../state.svelte';

  const ctx = $derived(context());
  const agentLine = $derived(
    ctx.agents.length === 0
      ? 'Şu an oturum yok'
      : [ctx.working && `${ctx.working} çalışıyor`, ctx.waiting && `${ctx.waiting} bekliyor`, ctx.approvals.length && `${ctx.approvals.length} onay`]
          .filter(Boolean)
          .join(' · ') || 'Hepsi boşta',
  );
  const linked = $derived(ui.connections.filter(c => c.installed).length);
  const issues = $derived(connectionIssues(ui.connections));
  const moduleNames: Record<string, string> = { agents: 'AI', music: 'Müzik', today: 'Bugün', system: 'Bilgisayar' };
  const enabled = $derived(Object.entries(data.settings.modules).filter(([, on]) => on).map(([id]) => moduleNames[id] ?? id));

  const fresh = $derived(data.settings.appearance.look === 'new');
  const lead = $derived(pickLead(ctx.agents, data.selected));
  const theme = $derived(themeById(data.settings.appearance.theme).name);
  const sessions = $derived(ctx.agents.length === 0 ? 'Oturum yok' : `${ctx.agents.length} oturum`);
  // Üçüncü satır: bakılması gereken şey. Bağlantı yoksa ajanlar adada görünmez.
  const attention = $derived(
    issues
      ? { tone: 'var(--amber)', title: `${issues} bağlantı ilgi bekliyor`, text: 'güncellemek için Bağlantılar', action: 'Bak →' }
      : linked === 0
        ? { tone: 'var(--amber)', title: 'Bağlantı yok', text: 'ajanlar görünmez', action: 'Bağla →' }
        : ctx.approvals.length
          ? { tone: 'var(--amber)', title: `${ctx.approvals.length} onay bekliyor`, text: 'adada kararını bekliyor', action: null }
          : { tone: 'var(--green)', title: 'Her şey yolunda', text: `${linked} araç bağlı`, action: null },
  );
  const KEYS = [
    { id: 'toggle', title: 'Adayı aç / küçült' },
    { id: 'approve', title: 'Onayla' },
    { id: 'deny', title: 'Reddet' },
  ] as const;
  const keys = $derived(KEYS.map(k => ({ ...k, combo: data.settings.shortcuts[k.id] })).filter(k => k.combo));
</script>

<header class="page-head">
  <h1>Genel bakış</h1>
  <p>Ada çalışıyor. Fareyi ekranın üst ortasına götür; üzerine gelince küçük görünüm, tıklayınca ayrıntı açılır.</p>
</header>

{#if fresh}
  <section class="hero">
    <button class="stage" onclick={() => go('gorunum')} title="Görünüm ayarları">
      <span class="mini" aria-hidden="true">
        <span class="bar"><i class="on"></i><i></i><i></i><i></i></span>
        <span class="row2">
          {#if lead}
            <Zody mood={agentMood(lead.status, lead.asleep)} body={agentColor(lead.tool)} size={34} />
            <span class="tx2"><strong>{lead.project}</strong><em>{lead.step || 'Bekliyor'}</em></span>
          {:else}
            <Zody mood={ctx.mood} size={34} />
            <span class="tx2"><strong>{'Ada boşta'}</strong><em>{'Ajan başlayınca burada görünür'}</em></span>
          {/if}
        </span>
      </span>
      <span class="cap">{`Ada şu an böyle görünüyor · ${theme} teması`}</span>
    </button>
    <div class="facts">
      <button class="fact" onclick={() => go('baglanti')}>
        <span class="dot" style:background={ctx.approvals.length ? 'var(--amber)' : ctx.working ? 'var(--blue)' : 'var(--text-3)'}></span>
        <span class="ft"><strong>{sessions}</strong>{` · ${agentLine}`}</span>
      </button>
      <button class="fact" onclick={() => go('moduller')}>
        <span class="dot" style:background="var(--accent)"></span>
        <span class="ft"><strong>{`${enabled.length} modül açık`}</strong>{` · ${enabled.join(', ') || 'hepsi kapalı'}`}</span>
      </button>
      <button class="fact" onclick={() => go('baglanti')}>
        <span class="dot" style:background={attention.tone}></span>
        <span class="ft"><strong>{attention.title}</strong>{` · ${attention.text}`}</span>
        {#if attention.action}<span class="btn go">{attention.action}</span>{/if}
      </button>
    </div>
  </section>
{:else}
<div class="tiles">
  <button class="tile" onclick={() => go('baglanti')}>
    <span class="k"><span class="dot" style:background={ctx.approvals.length ? 'var(--amber)' : ctx.working ? 'var(--blue)' : 'var(--text-3)'}></span>AI ajanları</span>
    <b>{ctx.agents.length}</b>
    <small>{agentLine}</small>
  </button>
  <button class="tile" onclick={() => go('baglanti')}>
    <span class="k"><span class="dot" style:background={issues ? 'var(--amber)' : linked ? 'var(--green)' : 'var(--text-3)'}></span>Bağlantılar</span>
    <b>{linked} / {ui.connections.length || 2}</b>
    <small>{issues ? `${issues} bağlantı ilgi bekliyor` : linked ? 'Claude Code ve Codex' : 'Henüz bağlı araç yok'}</small>
  </button>
  <button class="tile" onclick={() => go('moduller')}>
    <span class="k"><span class="dot" style:background="var(--accent)"></span>Modüller</span>
    <b>{enabled.length} açık</b>
    <small>{enabled.join(' · ') || 'Hepsi kapalı'}</small>
  </button>
</div>
{/if}

<section class="group">
  <h3>Hızlı</h3>
  <div class="row">
    <span class="ic"><Icon name="palette" /></span>
    <span class="tx"><b>Tema</b><small>{themeById(data.settings.appearance.theme).name}</small></span>
    <button class="btn" onclick={() => go('gorunum')}>Değiştir</button>
  </div>
  <label class="row">
    <span class="ic"><Icon name="sound" /></span>
    <span class="tx"><b>Sesler</b><small>Zody sesleri; ayrıntı Bildirimler’de</small></span>
    <input class="switch" type="checkbox" role="switch" checked={data.settings.island.sound} onchange={e => update(s => (s.island.sound = e.currentTarget.checked))} />
  </label>
  <label class="row" class:disabled={!ui.autostart?.available}>
    <span class="ic"><Icon name="power" /></span>
    <span class="tx"><b>Windows ile başlat</b><small>{ui.autostart?.available ? 'Oturum açılınca ada hazır olur' : 'Geliştirme sürümünde kapalı'}</small></span>
    <input class="switch" type="checkbox" role="switch" disabled={!ui.autostart?.available} checked={ui.autostart?.enabled ?? false} onchange={e => setAutostart(e.currentTarget.checked)} />
  </label>
</section>

{#if fresh}
  <section class="group keys">
    <h3>Kısayollar</h3>
    {#each keys as k (k.id)}
      <p><span>{k.title}</span>{#each k.combo.split('+') as part (part)}<kbd>{part}</kbd>{/each}</p>
    {:else}
      <p class="nokeys"><span>{'Kısayol atanmamış'}</span><button class="btn" onclick={() => go('kisayol')}>Kısayol ata</button></p>
    {/each}
  </section>
{/if}

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    margin-bottom: 14px;
  }
  .tile {
    background: linear-gradient(180deg, rgb(255 255 255 / 0.03), transparent), var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    text-align: left;
    min-width: 0;
    transition:
      border-color 140ms,
      transform 140ms var(--spring);
  }
  .tile:hover {
    border-color: var(--line-strong);
    transform: translateY(-1px);
  }
  .tile b {
    font-size: 22px;
    font-weight: 700;
  }
  .tile small {
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .k {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  /* Yeni görünüm: önizleme ve durum satırları. Zody hareketsiz (alive yok); yalnız hover'da renk. */
  .hero {
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 14px;
    align-items: center;
    padding: 16px;
    margin-bottom: 14px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background:
      radial-gradient(120% 140% at 0% 0%, color-mix(in srgb, var(--accent) 14%, transparent), transparent 60%),
      var(--surface);
  }
  .stage {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 0 0 6px;
    border-radius: 12px;
    background: linear-gradient(180deg, #2b3445, #1b2130);
  }
  .mini {
    width: 250px;
    padding: 6px 10px 10px;
    border-radius: 0 0 18px 18px;
    background: var(--island);
    border: 1px solid color-mix(in srgb, var(--island), rgb(255 255 255) 7%);
    border-top: 0;
    display: grid;
    gap: 6px;
  }
  .mini .bar {
    display: flex;
    gap: 6px;
  }
  .mini .bar i {
    width: 16px;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
  }
  .mini .bar i.on {
    background: var(--accent-soft);
  }
  .row2 {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 12px;
    background: var(--surface);
    text-align: left;
  }
  .tx2 {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .tx2 strong {
    font-size: var(--fs-12-5);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tx2 em {
    font-style: normal;
    font-size: var(--fs-10-5);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cap {
    color: var(--text-3);
    font-size: var(--fs-10-5);
  }
  .facts {
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  .fact {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 9px;
    text-align: left;
    color: var(--text-2);
    font-size: var(--fs-12-5);
    min-width: 0;
  }
  .fact:hover {
    background: var(--surface-2);
  }
  .ft {
    min-width: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .ft strong {
    color: var(--text);
    font-weight: 600;
  }
  .fact .dot {
    flex: none;
  }
  .go {
    margin-left: auto;
    flex: none;
    display: inline-flex;
    align-items: center;
  }
  .keys {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
  }
  .keys h3 {
    grid-column: 1 / -1;
  }
  .keys p {
    margin: 0;
    padding: 8px 16px 12px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .keys span {
    width: 100%;
    margin-bottom: 2px;
  }
  .keys .nokeys {
    grid-column: 1 / -1;
    flex-wrap: nowrap;
  }
  .keys .nokeys span {
    width: auto;
    margin: 0;
    flex: 1;
  }
  kbd {
    padding: 1px 6px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-11);
  }
</style>
