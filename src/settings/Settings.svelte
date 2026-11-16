<script lang="ts">
  // Ayarlar penceresi: solda menü, sağda seçili sayfa. Değişiklikler hemen kaydedilir ve adaya ulaşır.
  // Yeni görünümde menü üç başlık altında (pages.ts GROUPS).
  import { onMount, tick } from 'svelte';
  import Zody from '../mascot/Zody.svelte';
  import { on } from '../lib/ipc';
  import { data, loadSettings, watchAgents, watchSettings } from '../lib/store.svelte';
  import { applyTheme } from '../lib/theme';
  import type { Autostart } from '../lib/types';
  import Icon from './Icon.svelte';
  import About from './pages/About.svelte';
  import Appearance from './pages/Appearance.svelte';
  import Connections from './pages/Connections.svelte';
  import Features from './pages/Features.svelte';
  import IslandPage from './pages/IslandPage.svelte';
  import ZodyAssistant from './pages/ZodyAssistant.svelte';
  import Modules from './pages/Modules.svelte';
  import Notifications from './pages/Notifications.svelte';
  import Overview from './pages/Overview.svelte';
  import Shortcuts from './pages/Shortcuts.svelte';
  import Sources from './pages/Sources.svelte';
  import Startup from './pages/Startup.svelte';
  import Telefon from './pages/Telefon.svelte';
  import { GROUPS, PAGES } from './pages';
  import Search from './Search.svelte';
  import { connectionIssues, go, loadAutostart, refreshConnections, reveal, ui, type Page } from './state.svelte';
  import { fold } from '../lib/search';


  const badges = $derived<Partial<Record<Page, number>>>({ baglanti: connectionIssues(ui.connections) });
  const sections = GROUPS.map(g => ({ title: g.title, pages: g.pages.map(id => PAGES.find(p => p.id === id)!) }));

  $effect(() => applyTheme(data.settings.appearance.theme, data.settings.appearance.surface));

  let main: HTMLElement | undefined = $state();
  $effect(() => {
    void ui.page;
    main?.scrollTo({ top: 0 });
  });

  // Aramadan gelindi: sayfa çizildikten sonra ayarı bulup ortaya kaydırır ve kısa süre vurgular (yalnız opacity).
  let lastReveal = 0;
  $effect(() => {
    const { title, tick: n } = reveal;
    void ui.page;
    if (!title || n === lastReveal) return;
    lastReveal = n;
    void tick().then(() => requestAnimationFrame(() => flash(title)));
  });

  function flash(title: string): void {
    const want = fold(title);
    const label = [...(main?.querySelectorAll<HTMLElement>('.tx b, .name b, .group > h3') ?? [])].find(el => fold(el.textContent ?? '') === want);
    const target = label?.closest<HTMLElement>('.row, .theme, .group') ?? null;
    if (!target) return;
    const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView({ block: 'center', behavior: calm ? 'auto' : 'smooth' });
    target.classList.remove('hit');
    void target.offsetWidth;
    target.classList.add('hit');
    target.addEventListener('animationend', () => target.classList.remove('hit'), { once: true });
  }

  onMount(() => {
    void loadSettings();
    void loadAutostart();
    void refreshConnections();
    const stops = [watchSettings(), watchAgents(), on<Autostart>('autostart://changed', a => (ui.autostart = a))];
    return () => stops.forEach(stop => stop());
  });
</script>

<div class="shell">
  <nav class="side" aria-label="Ayarlar bölümleri">
    <div class="brand">
      <Zody mood="idle" size={34} alive interactive />
      <span><b>Zodyvel</b><small>Sürüm 0.8.0</small></span>
    </div>
    {#each sections as g (g.title)}
      {#if g.title}<small class="gh">{g.title}</small>{/if}
      {#each g.pages as p (p.id)}
        <button class="nav" class:on={ui.page === p.id} aria-current={ui.page === p.id ? 'page' : undefined} onclick={() => go(p.id)}>
          <Icon name={p.icon} />
          {p.name}
          {#if badges[p.id]}<span class="badge" title="Bakman gereken {badges[p.id]} şey var">{badges[p.id]}</span>{/if}
        </button>
      {/each}
    {/each}
    <span class="grow"></span>
    <p class="foot" class:bad={ui.failed}>
      {ui.failed ? 'Ayarlar kaydedilemedi.' : ui.saving ? 'Kaydediliyor…' : 'Değişiklikler hemen uygulanır.'}
    </p>
  </nav>

  <div class="col">
  <header class="top"><Search /></header>
  <main bind:this={main}>
    {#if ui.page === 'genel'}
      <Overview />
    {:else if ui.page === 'gorunum'}
      <Appearance />
    {:else if ui.page === 'ada'}
      <IslandPage />
    {:else if ui.page === 'moduller'}
      <Modules />
    {:else if ui.page === 'ozellikler'}
      <Features />
    {:else if ui.page === 'kaynaklar'}
      <Sources />
    {:else if ui.page === 'zody'}
      <ZodyAssistant />
    {:else if ui.page === 'bildirim'}
      <Notifications />
    {:else if ui.page === 'baglanti'}
      <Connections />
    {:else if ui.page === 'telefon'}
      <Telefon />
    {:else if ui.page === 'kisayol'}
      <Shortcuts />
    {:else if ui.page === 'baslangic'}
      <Startup />
    {:else}
      <About />
    {/if}
  </main>
  </div>
</div>

<style>
  .shell {
    height: 100vh;
    display: flex;
  }
  .side {
    width: 214px;
    flex: none;
    padding: 16px 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent) 8%, var(--bg)), var(--bg) 45%);
    border-right: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 8px 16px;
  }
  .brand b {
    display: block;
    font-size: var(--fs-15);
  }
  .brand small {
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .nav {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: 10px;
    color: var(--text-2);
    text-align: left;
    transition:
      background var(--t-fast),
      color var(--t-fast);
  }
  .nav:hover {
    background: var(--surface);
    color: var(--text);
  }
  .nav.on {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--text);
  }
  .nav.on::before {
    content: '';
    position: absolute;
    left: -10px;
    top: 8px;
    bottom: 8px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .nav.on :global(svg) {
    color: var(--accent);
  }
  .badge {
    margin-left: auto;
    min-width: 18px;
    text-align: center;
    font-size: var(--fs-10);
    font-weight: 700;
    color: var(--on-amber);
    background: var(--amber);
    border-radius: 9px;
    padding: 1px 5px;
  }
  .gh {
    padding: 10px 10px 3px;
    color: var(--text-3);
    font-size: var(--fs-10-5);
    letter-spacing: 0.6px;
    text-transform: uppercase;
  }
  .gh:first-of-type {
    padding-top: 0;
  }
  .grow {
    flex: 1;
  }
  .foot {
    margin: 0;
    padding: 10px 10px 0;
    border-top: 1px solid var(--line);
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .foot.bad {
    color: var(--red);
  }
  .col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .top {
    flex: none;
    padding: 14px 28px 10px;
  }
  main {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px 28px 28px;
  }
</style>
