<script lang="ts">
  // Mini adada Kaynaklar düzeninin sol kartı, ızgarada GitHub seçiliyken (MiniAI'nin `.card.lead.src` kartı
  // içinde; çerçeve orada). Solda GitHub Zody'si, sağda başlık ve en acil 3 satır: kırık CI (altında Günlüğü aç ·
  // Yeniden çalıştır · Claude'a düzelttir), inceleme isteği, bildirim. Yerel Git açıksa son satır commitlenmemiş
  // ya da gönderilmemiş değişikliği olan depolar (tek satır; dokununca ayrıntıda "Depolarım"). Durağan; yalnız
  // yeni olayda Zody sıçrar.
  import { headline, ordered } from '../../lib/github';
  import { changes, repoNote, sortRepos } from '../../lib/localgit';
  import { git, localGitOn } from '../../lib/localgit.svelte';
  import type { SourceState } from '../../lib/sources';
  import { hub, sourceRefresh } from '../../lib/sources.svelte';
  import GithubRow from '../parts/GithubRow.svelte';
  import SourceZody from '../parts/SourceZody.svelte';

  interface Props {
    state: SourceState;
    /** Ayrıntıyı (DetailGithub) aç. */
    onopen: () => void;
  }
  let { state, onopen }: Props = $props();

  const live = $derived(ordered(state.items).filter(i => i.tag !== null));
  const firstCi = $derived(live.find(i => i.tag === 'ci_failed')?.id);
  const rows = $derived(live.slice(0, 3));
  const pill = $derived(headline(state));
  const tone = $derived(state.status === 'error' ? 'alert' : state.urgency >= 3 ? 'alert' : state.badge ? 'attention' : 'calm');
  const NOTE: Record<string, string> = { off: 'Kapalı', needs_setup: 'Ayarlar › Kaynaklar’dan bağla', connecting: 'Bağlanıyor…' };
  const pending = $derived(localGitOn() ? sortRepos(git.overview?.repos ?? []).filter(r => changes(r) > 0 || r.ahead > 0) : []);

  function repos() {
    hub.githubTab = 'repos';
    onopen();
  }
</script>

{#snippet local()}
  {@const top = pending[0]}
  {#if top}
    <button class="loc" onclick={repos} title="Depolarım: commit ve push">
      <span class="tag">yerel</span>
      {#if pending.length === 1}<b>{top.name}</b><span class="su">{repoNote(top)}</span>{:else}<span class="su">{pending.length} depoda değişiklik · {top.name}…</span>{/if}
    </button>
  {/if}
{/snippet}

<div class="gh">
  <SourceZody tile="github" {tone} mood={tone === 'alert' ? 'error' : 'idle'} size={44} bump={hub.bump?.id === 'github' ? hub.bump.key : null} />
  <div class="ct">
    <button class="hd" onclick={onopen} title="GitHub ayrıntısı">
      <b>GitHub</b>
      {#if state.account}<small class="acc">{state.account}</small>{/if}
      {#if pill}<span class="pill {pill.tone}">{pill.text}</span>{/if}
      <span class="go" aria-hidden="true">↗</span>
    </button>
    {#if state.status === 'ok'}
      {#each rows.slice(0, pending.length ? 2 : 3) as item (item.id)}
        <GithubRow {item} expanded={item.id === firstCi} compact />
      {:else}
        {#if !pending.length}<p class="calm">Her şey yolunda: kırık CI, inceleme isteği ya da bildirim yok.</p>{/if}
      {/each}
      {@render local()}
    {:else if state.status === 'error'}
      <p class="bad">{state.error ?? 'GitHub’a ulaşılamıyor.'}</p>
      <button class="retry" onclick={() => sourceRefresh('github')}>Yeniden dene</button>
    {:else if pending.length}
      {@render local()}
    {:else}
      <p class="calm">{NOTE[state.status]}</p>
    {/if}
  </div>
</div>

<style>
  .gh {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    min-width: 0;
  }
  .ct {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 3px;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 0 8px 2px;
    text-align: left;
  }
  .acc {
    color: var(--text-3);
    font-size: var(--fs-12);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    flex: none;
    padding: 0 7px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    font-weight: 600;
    color: var(--text-2);
    background: var(--surface-3);
  }
  .pill.red {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 16%, transparent);
  }
  .pill.amber {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 16%, transparent);
  }
  .go {
    margin-left: auto;
    color: var(--text-3);
  }
  .hd:hover .go {
    color: var(--text);
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
  .retry {
    justify-self: start;
    padding: 0 8px;
    color: var(--text-2);
    font-size: var(--fs-12);
    font-weight: 600;
  }
  .retry:hover {
    color: var(--text);
  }
  .loc {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 3px 8px;
    border-radius: 8px;
    font-size: var(--fs-12);
    color: var(--text);
    background: var(--surface-2);
    text-align: left;
  }
  .loc:hover {
    background: var(--surface-3);
  }
  .loc .tag {
    flex: none;
    padding: 0 6px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    font-weight: 600;
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 16%, transparent);
  }
  .loc b {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .loc .su {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-2);
  }

</style>
