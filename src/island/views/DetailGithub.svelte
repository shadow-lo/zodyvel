<script lang="ts">
  // Büyük ada, Kaynaklar'da GitHub ayrıntısı: sekmeler (Depolarım · PR ve CI · İncelemem gerekenler ·
  // Bildirimler), her satırın altında düğmeleri. Satıra dokunmak tarayıcıda açar. Satırlar yalnız bellekte (en
  // çok 20). "Depolarım" bilgisayardaki depoları (Yerel Git) ve GitHub'da son push edilenleri gösterir; Yerel
  // Git token olmadan da çalışır. Depolar ayrıntı açılınca bir kez ve "Yenile"de taranır. Durağan.
  import { untrack } from 'svelte';
  import { firstTab, inTab, TABS, type GithubTab } from '../../lib/github';
  import { dirtyCount } from '../../lib/localgit';
  import { git, loadGit, localGitOn } from '../../lib/localgit.svelte';
  import type { SourceState } from '../../lib/sources';
  import { hub, sourceRefresh } from '../../lib/sources.svelte';
  import GithubRow from '../parts/GithubRow.svelte';
  import LocalRepos from '../parts/LocalRepos.svelte';
  import SourceZody from '../parts/SourceZody.svelte';

  interface Props {
    state: SourceState;
    /** Kaynaklar düzenine dön (AI ayrıntısı). */
    onback: () => void;
  }
  // `state` adı `$state` rünüyle çakışmasın diye içeride `source`.
  let { state: source, onback }: Props = $props();

  // Mini karttaki "Depolarım ›" bu sekmeyle açar; bir kez okunur.
  let chosen = $state<GithubTab | null>(untrack(() => hub.githubTab));
  hub.githubTab = null;
  const local = $derived(localGitOn());
  const dirty = $derived(local ? dirtyCount(git.overview) : 0);
  const connected = $derived(source.status === 'ok');
  // Bağlı değilken yalnız "Depolarım" (Yerel Git açıksa).
  const tabs = $derived(connected ? TABS : TABS.filter(t => t.id === 'repos'));
  const tab = $derived(connected ? (chosen ?? firstTab(source.items, dirty)) : 'repos');
  const rows = $derived(inTab(source.items, tab));
  const counts = $derived(
    Object.fromEntries(TABS.map(t => [t.id, t.id === 'repos' ? dirty : inTab(source.items, t.id).filter(i => i.unread).length])) as Record<GithubTab, number>,
  );
  // Ayrıntı açılınca depolar bir kez taranır (yoklama yok).
  $effect(() => {
    if (local) untrack(() => void loadGit());
  });
  const EMPTY: Record<GithubTab, string> = {
    repos: '',
    ci: 'Açık PR’larında ve son depolarının ana dalında kırık CI yok.',
    review: 'Senden inceleme bekleyen PR yok.',
    notif: 'Okunmamış bildirim yok.',
  };
  const NOTE: Record<string, string> = { off: 'GitHub kapalı.', needs_setup: 'Ayarlar › Kaynaklar’dan token ile bağla.', connecting: 'Bağlanıyor…' };
  let refreshing = $state(false);

  async function refresh() {
    refreshing = true;
    await Promise.all([connected ? sourceRefresh('github') : null, tab === 'repos' && local ? loadGit() : null]);
    refreshing = false;
  }
</script>

<div class="detail">
  <header>
    <button class="back" onclick={onback} title="Kaynaklara dön">‹</button>
    <SourceZody tile="github" tone={source.urgency >= 3 ? 'alert' : source.badge ? 'attention' : 'calm'} size={26} />
    <b>GitHub</b>
    {#if source.account}<small>{source.account}</small>{/if}
    <button class="refresh" onclick={refresh} disabled={refreshing || git.loading || !(connected || local)} title="Şimdi yenile">{git.loading ? 'Bakılıyor…' : 'Yenile'}</button>
  </header>

  {#if connected || local}
    <nav>
      {#each tabs as t (t.id)}
        <button class:on={tab === t.id} onclick={() => (chosen = t.id)}>
          {t.label}{#if counts[t.id]}<span class="n" class:red={t.id === 'ci'}>{counts[t.id]}</span>{/if}
        </button>
      {/each}
    </nav>
    <div class="list">
      {#if tab === 'repos'}
        <LocalRepos remote={source.repos} {connected} />
        {#if !connected && source.status !== 'off'}
          <p class="calm">{source.status === 'error' ? (source.error ?? 'GitHub’a ulaşılamıyor.') : NOTE[source.status]}</p>
        {/if}
      {:else}
        {#each rows as item (item.id)}
          <GithubRow {item} expanded />
        {:else}
          <p class="calm">{EMPTY[tab]}</p>
        {/each}
      {/if}
    </div>
  {:else if source.status === 'error'}
    <p class="bad">{source.error ?? 'GitHub’a ulaşılamıyor.'}</p>
    <button class="refresh solo" onclick={refresh} disabled={refreshing}>Yeniden dene</button>
  {:else}
    <p class="calm">{NOTE[source.status]}</p>
  {/if}
</div>

<style>
  .detail {
    display: grid;
    grid-template-rows: auto auto 1fr;
    gap: 8px;
    min-height: 0;
    height: 100%;
    padding: 4px 6px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  header small {
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .back {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    font-size: 18px;
    color: var(--text-2);
  }
  .back:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .refresh {
    margin-left: auto;
    height: 26px;
    padding: 0 10px;
    border-radius: 8px;
    background: var(--surface-3);
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .refresh.solo {
    justify-self: start;
    margin-left: 8px;
  }
  .refresh:disabled {
    opacity: 0.5;
  }
  nav {
    display: flex;
    gap: 4px;
  }
  nav button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border-radius: 8px;
    color: var(--text-2);
    font-size: var(--fs-12);
    font-weight: 600;
  }
  nav button:hover {
    color: var(--text);
  }
  nav button.on {
    color: var(--text);
    background: var(--surface-3);
  }
  .n {
    min-width: 16px;
    padding: 0 5px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    color: var(--on-amber);
    background: var(--amber);
  }
  .n.red {
    color: #fff;
    background: var(--red);
  }
  .list {
    display: grid;
    align-content: start;
    gap: 4px;
    min-height: 0;
    overflow-y: auto;
  }
  p {
    margin: 0;
    padding: 0 8px;
    font-size: var(--fs-12);
  }
  .calm {
    color: var(--text-2);
  }
  .bad {
    color: var(--red);
  }
</style>
