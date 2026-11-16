<script lang="ts">
  // Ayarlar › Kaynaklar: "Ne kullanıyorsun?". Açık kaynaklar adada Kaynaklar ızgarasında en başta görünür;
  // altında her Claude/Codex oturumu ayrı kutucuk (en çok 6). Müzik ve Bugün ızgarada yok, kendi sekmeleri var
  // (açıp kapatmak Ayarlar › Modüller'de).
  // Mail ve GitHub kutucuğu seçilince altında kurulumu açılır (MailSetup / GithubSetup). Bağlantı bilgileri
  // Windows Kimlik Bilgisi Yöneticisi'nde durur; ayar dosyasına yazılmaz.
  import { onMount } from 'svelte';
  import Zody from '../../mascot/Zody.svelte';
  import { data } from '../../lib/store.svelte';
  import { hub, sourceState, watchSources } from '../../lib/sources.svelte';
  import { SOURCE_NAMES, TILE_COLORS, type SourceId, type SourceState, type TileKind } from '../../lib/sources';
  import Icon from '../Icon.svelte';
  import { go, update } from '../state.svelte';
  import GithubSetup from './sources/GithubSetup.svelte';
  import LocalGitSetup from './sources/LocalGitSetup.svelte';
  import MailSetup from './sources/MailSetup.svelte';

  onMount(() => watchSources());

  const f = $derived(data.settings.features);
  const m = $derived(data.settings.modules);
  const classic = $derived(data.settings.appearance.look !== 'new');
  let open = $state<SourceId | null>(null);

  function sourceLine(s: SourceState, on: boolean): { text: string; tone: string } {
    if (!on) return { text: 'Kapalı', tone: '' };
    if (hub.paused) return { text: 'Duraklatıldı (tepsi)', tone: '' };
    switch (s.status) {
      case 'ok':
        return { text: `Bağlı · ${s.account ?? ''}`, tone: 'ok' };
      case 'connecting':
        return { text: 'Bağlanıyor…', tone: '' };
      case 'error':
        return { text: s.error ?? 'Hata', tone: 'err' };
      case 'needs_setup':
        return { text: 'Bağla', tone: 'warn' };
      default:
        return { text: 'Açık', tone: '' };
    }
  }

  interface Tile {
    id: TileKind;
    name: string;
    line: { text: string; tone: string };
    on: boolean;
    pick: () => void;
  }
  const tiles = $derived<Tile[]>([
    { id: 'claude', name: 'Claude Code', on: m.agents, line: { text: m.agents ? 'AI ajanları · bağlantılar' : 'AI ajanları modülü kapalı', tone: '' }, pick: () => go('baglanti') },
    { id: 'codex', name: 'Codex', on: m.agents, line: { text: m.agents ? 'Oturum açınca görünür' : 'AI ajanları modülü kapalı', tone: '' }, pick: () => go('baglanti') },
    { id: 'mail', name: SOURCE_NAMES.mail, on: f.sourceMail, line: sourceLine(sourceState('mail'), f.sourceMail), pick: () => (open = open === 'mail' ? null : 'mail') },
    { id: 'github', name: SOURCE_NAMES.github, on: f.sourceGithub, line: sourceLine(sourceState('github'), f.sourceGithub), pick: () => (open = open === 'github' ? null : 'github') },
  ]);

  function setSource(id: SourceId, on: boolean) {
    void update(s => {
      if (id === 'mail') s.features.sourceMail = on;
      else s.features.sourceGithub = on;
      if (on) s.features.sources = true;
    });
  }
</script>

<header class="page-head">
  <h1>Kaynaklar</h1>
  <p>Adada görmek istediklerini seç. AI kullanmasan da mail ve GitHub adada durur; o an en önemlisi solda büyür.</p>
</header>

<section class="group">
  <label class="row">
    <span class="ic"><Icon name="grid" /></span>
    <span class="tx">
      <b>Kaynaklar</b>
      <small>AI sekmesi “Kaynaklar” olur: solda o an seni en çok ilgilendiren (onay bekleyen ajan, kırık CI, okunmamış mail), sağda ızgara: Mail ve GitHub en başta, altında çalışan her Claude oturumu ayrı. Müzik ve Bugün kendi sekmelerinde kalır. Kapalıyken hiçbir kaynak internete çıkmaz.</small>
      {#if f.sources && classic}
        <span class="warn">Yalnız Yeni görünümde. <button class="link" onclick={() => go('gorunum')}>Görünüm</button></span>
      {/if}
    </span>
    <input class="switch" type="checkbox" role="switch" checked={f.sources} onchange={e => update(s => (s.features.sources = e.currentTarget.checked))} />
  </label>
</section>

<section class="group" class:dim={!f.sources}>
  <h3>Ne kullanıyorsun?</h3>
  <p class="note">Izgarada en çok 6 kutucuk: Mail ve GitHub yerini korur, oturumlar ardından gelir. Bağlantı bilgileri Windows Kimlik Bilgisi Yöneticisi'nde şifreli durur; Zodyvel'in ayar dosyasına yazılmaz, bağlantıyı kaldırınca silinir.</p>
  <div class="tiles">
    {#each tiles as t (t.id)}
      <button class="tile" class:on={t.on} class:open={open === t.id} onclick={t.pick} aria-expanded={t.id === 'mail' || t.id === 'github' ? open === t.id : undefined}>
        <Zody body={TILE_COLORS[t.id]} size={30} />
        <span class="tt"><span class="nm">{t.name}</span><small class={t.line.tone}>{t.line.text}</small></span>
      </button>
    {/each}
  </div>
</section>

{#if open}
  {@const id = open}
  <section class="group">
    <h3>{SOURCE_NAMES[id]}</h3>
    <label class="row">
      <span class="ic"><Icon name={id === 'mail' ? 'bell' : 'link'} /></span>
      <span class="tx">
        <b>Adada göster</b>
        <small>Kapalıyken bu kaynak internete hiç çıkmaz; kayıtlı bağlantı bilgisi durur.</small>
      </span>
      <input class="switch" type="checkbox" role="switch" checked={id === 'mail' ? f.sourceMail : f.sourceGithub} onchange={e => setSource(id, e.currentTarget.checked)} />
    </label>
    <div class="setup">
      {#if id === 'mail'}
        <MailSetup state={sourceState('mail')} />
      {:else}
        <GithubSetup state={sourceState('github')} />
        <LocalGitSetup />
      {/if}
    </div>
  </section>
{/if}

<style>
  .note {
    margin: 0;
    padding: 0 16px 10px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  .dim .tiles {
    opacity: 0.6;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
    padding: 0 16px 16px;
  }
  .tile {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    text-align: left;
    color: var(--text);
    opacity: 0.7;
  }
  .tile.on {
    opacity: 1;
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  }
  .tile.open {
    background: color-mix(in srgb, var(--accent) 12%, var(--surface-2));
  }
  .tt {
    display: grid;
    min-width: 0;
  }
  .nm {
    font-weight: 600;
    font-size: var(--fs-13-5);
  }
  .tt small {
    color: var(--text-2);
    font-size: var(--fs-11-5);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tt small.ok {
    color: var(--green);
  }
  .tt small.warn {
    color: var(--amber);
  }
  .tt small.err {
    color: var(--red);
  }
  .setup {
    padding: 4px 16px 16px;
  }
  .tx .warn {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
