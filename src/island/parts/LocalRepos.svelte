<script lang="ts">
  // GitHub ayrıntısında "Depolarım": üstte bu bilgisayardaki depolar (Yerel Git açıksa), altta GitHub'da son
  // push edilenler. Değişikliği olan depoya dokununca dosyalar (seçilebilir), commit mesajı ve Commit /
  // Commit + Push / Push / Pull / Fetch / Geri al düğmeleri açılır. Fetch dışındaki her işlem önce satırın içinde
  // onay ister; hassas görünen dosya seçildiyse ayrıca kutucuk işaretlenmeden "Evet" basılamaz. Geri al dosyaların
  // şimdiki hâlini Geri Dönüşüm Kutusu'na taşır. Durağan: animasyon yok, yoklama yok.
  import { ago, type RepoLine } from '../../lib/sources';
  import {
    askDiscard,
    askFor,
    askPull,
    canCommitPush,
    canFetch,
    canPull,
    canPush,
    changes,
    defaultPicked,
    KIND_LETTER,
    remoteRows,
    repoNote,
    sortRepos,
    visibility,
    type Ask,
    type RepoView,
  } from '../../lib/localgit';
  import { git, gitCommit, gitDiscard, gitFetch, gitOpenFolder, gitPull, gitPush, localGitOn } from '../../lib/localgit.svelte';
  import { sourceAction } from '../../lib/sources.svelte';
  import { openSettings, startTyping } from '../../lib/store.svelte';

  interface Props {
    /** GitHub'da son push edilenler (GitHub bağlıysa). */
    remote: RepoLine[] | undefined;
    connected: boolean;
    now?: number;
  }
  let { remote, connected, now = Date.now() }: Props = $props();

  const on = $derived(localGitOn());
  const repos = $derived(sortRepos(git.overview?.repos ?? []));
  const others = $derived(remoteRows(remote, git.overview?.repos ?? []));

  // Açık depo ve onun formu (tek depo açık kalır).
  let openPath = $state<string | null>(null);
  let picked = $state<string[]>([]);
  let message = $state('');
  let asking = $state<(Ask & { push: boolean }) | null>(null);
  let fetching = $state(false);
  let sure = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);

  const current = $derived(repos.find(r => r.path === openPath) ?? null);
  const pickedSensitive = $derived(current ? current.files.filter(f => f.sensitive && picked.includes(f.path)).map(f => f.path) : []);

  function toggle(r: RepoView) {
    if (openPath === r.path) {
      openPath = null;
      return;
    }
    openPath = r.path;
    picked = defaultPicked(r);
    message = '';
    asking = null;
    error = done = null;
  }

  function flip(path: string) {
    picked = picked.includes(path) ? picked.filter(p => p !== path) : [...picked, path];
    asking = null;
  }

  function ask(r: RepoView, push: boolean, pushOnly = false) {
    error = done = null;
    if (!pushOnly) {
      if (!message.trim()) return void (error = 'Commit mesajını yaz.');
      if (!picked.length) return void (error = 'En az bir dosya seç.');
    }
    sure = false;
    asking = { ...askFor(r, pushOnly ? [] : picked, push, remote), push };
  }

  function askOther(a: Ask) {
    error = done = null;
    sure = false;
    asking = { ...a, push: false };
  }

  /** Seçimden, artık listede olmayan dosyaları çıkarır (işlemden sonra). */
  function keepPicked(r: RepoView) {
    const fresh = git.overview?.repos.find(x => x.path === r.path);
    picked = fresh ? picked.filter(p => fresh.files.some(f => f.path === p)) : [];
  }

  async function fetchNow(r: RepoView) {
    error = done = null;
    fetching = busy = true;
    error = await gitFetch(r.path);
    const fresh = git.overview?.repos.find(x => x.path === r.path);
    done = error ? null : fresh?.behind ? `GitHub’da ${fresh.behind} yeni commit var.` : 'GitHub’la aynı ✓';
    fetching = busy = false;
  }

  async function confirm(r: RepoView) {
    const a = asking;
    if (!a || (pickedSensitive.length && a.kind === 'commit' && !sure)) return;
    asking = null;
    busy = true;
    if (a.kind === 'push') {
      error = await gitPush(r.path);
      done = error ? null : 'Gönderildi ✓';
    } else if (a.kind === 'pull') {
      error = await gitPull(r.path);
      done = error ? null : 'Güncellendi ✓';
    } else if (a.kind === 'discard') {
      const n = picked.length;
      error = await gitDiscard(r.path, picked);
      done = error ? null : `${n} dosya geri alındı ✓ (eski hâlleri Geri Dönüşüm Kutusu’nda)`;
      keepPicked(r);
    } else {
      const result = await gitCommit(r.path, message, picked, pickedSensitive, a.push);
      if ('error' in result) {
        error = result.error;
      } else {
        message = '';
        done = result.pushed ? `Commit ${result.sha} gönderildi ✓` : `Commit ${result.sha} ✓`;
        error = result.pushError ? `Commit yapıldı ama gönderilemedi: ${result.pushError}` : null;
        const fresh = git.overview?.repos.find(x => x.path === r.path);
        picked = fresh ? defaultPicked(fresh) : [];
      }
    }
    busy = false;
  }

  function onkeydown(e: KeyboardEvent, r: RepoView) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      ask(r, false);
    }
  }

  const CI: Record<RepoLine['ci'], string> = { ok: 'CI ✓', failed: 'CI ✕', pending: 'CI …', none: '' };
  const short = (slug: string) => slug.split('/').pop() ?? slug;
</script>

<div class="repos">
  {#if on}
    <h4>Bu bilgisayar</h4>
    {#if git.error}
      <p class="bad">{git.error}</p>
    {:else if !git.overview}
      <p class="calm">{git.loading ? 'Depolara bakılıyor…' : 'Henüz bakılmadı; Yenile’ye bas.'}</p>
    {:else if !git.overview.git}
      <p class="bad">Git bulunamadı. Git for Windows’u kur ya da GitHub Desktop’u güncelle.</p>
    {:else if repos.length === 0}
      <p class="calm">Depo bulunamadı. <button class="link" onclick={openSettings}>Ayarlar › Kaynaklar › GitHub</button>’dan depolarının olduğu klasörü ekle.</p>
    {:else}
      {#each repos as r (r.path)}
        {@const open = openPath === r.path}
        {@const dirty = changes(r) > 0}
        <div class="repo" class:open class:dirty>
          <button class="head" onclick={() => toggle(r)} title={r.path} aria-expanded={open}>
            <span class="dot" class:on={dirty || r.ahead > 0} class:err={!!r.error}></span>
            <b>{r.name}</b>
            {#if r.branch}<small class="br">{r.branch}</small>{/if}
            <span class="note">{repoNote(r)}</span>
            <span class="chev" aria-hidden="true">{open ? '▾' : '▸'}</span>
          </button>
          {#if open}
            <div class="body">
              {#if r.files.length}
                <div class="files">
                  {#each r.files as f (f.path)}
                    <label class="file" class:off={!picked.includes(f.path)} class:sens={f.sensitive} title={f.orig ? `${f.orig} → ${f.path}` : f.path}>
                      <input type="checkbox" checked={picked.includes(f.path)} disabled={f.kind === 'conflict' || busy} onchange={() => flip(f.path)} />
                      <span class="k {f.kind}" title={KIND_LETTER[f.kind].label}>{KIND_LETTER[f.kind].letter}</span>
                      <span class="p">{f.path}</span>
                      {#if f.sensitive}<span class="pill red">hassas</span>{/if}
                      {#if f.big}<span class="pill amber">50 MB+</span>{/if}
                    </label>
                  {/each}
                  {#if r.more}<p class="calm">+{r.more} dosya daha (GitHub Desktop’ta gör)</p>{/if}
                </div>
                <div class="pick">
                  <button class="link" onclick={() => (picked = r.files.filter(f => f.kind !== 'conflict').map(f => f.path))}>Tümü</button>
                  <button class="link" onclick={() => (picked = [])}>Hiçbiri</button>
                  <small>{picked.length} / {r.files.length} seçili</small>
                  {#if picked.length}
                    <button class="link danger" disabled={busy} onclick={() => askOther(askDiscard(r, picked))} title="Seçilen dosyalardaki değişiklikleri geri al (discard)">Seçilenleri geri al</button>
                  {/if}
                </div>
                <input
                  class="msg"
                  bind:value={message}
                  onfocus={startTyping}
                  onkeydown={e => onkeydown(e, r)}
                  placeholder="Commit mesajı (ne değişti?)"
                  maxlength="5000"
                  spellcheck="false"
                  disabled={busy}
                  aria-label="Commit mesajı"
                />
              {/if}

              {#if asking}
                <div class="ask" role="alertdialog" aria-label="Onay">
                  <p class="q">{asking.text}</p>
                  {#each asking.warnings as w (w)}<p class="warn">⚠ {w}</p>{/each}
                  {#if asking.kind === 'commit' && pickedSensitive.length}
                    <label class="sure"><input type="checkbox" bind:checked={sure} /> Hassas dosyaları da commitle, eminim</label>
                  {/if}
                  <div class="acts">
                    <button
                      class="act strong"
                      class:danger={asking.kind === 'discard'}
                      disabled={asking.kind === 'commit' && pickedSensitive.length > 0 && !sure}
                      onclick={() => confirm(r)}>{asking.kind === 'discard' ? 'Evet, geri al' : 'Evet'}</button
                    >
                    <button class="act" onclick={() => (asking = null)}>Vazgeç</button>
                  </div>
                </div>
              {:else}
                <div class="acts">
                  {#if r.files.length}
                    <button class="act strong" disabled={busy} onclick={() => ask(r, false)}>Commit</button>
                    {#if canCommitPush(r)}<button class="act" disabled={busy} onclick={() => ask(r, true)}>Commit + Push</button>{/if}
                  {/if}
                  {#if canPush(r)}<button class="act" disabled={busy} onclick={() => ask(r, true, true)}>Push (↑{r.ahead})</button>{/if}
                  {#if canPull(r)}<button class="act strong" disabled={busy} onclick={() => askOther(askPull(r))}>Pull (↓{r.behind})</button>{/if}
                  {#if canFetch(r)}<button class="act" disabled={busy} onclick={() => fetchNow(r)} title="GitHub’da yeni commit var mı bak (dosyalar değişmez)">Fetch</button>{/if}
                  <button class="act" onclick={async () => (error = await gitOpenFolder(r.path))}>Klasörü aç</button>
                  {#if busy}<small class="calm">{fetching ? 'GitHub’a bakılıyor…' : 'Git çalışıyor…'}</small>{/if}
                </div>
              {/if}
              {#if !r.upstream && r.branch && (r.files.length || r.ahead)}
                <p class="calm">Bu dal GitHub’da yok; göndermek için GitHub Desktop’ta bir kez “Publish branch” de.</p>
              {/if}
              {#if visibility(r, remote) === 'public'}<p class="calm">Bu depo GitHub’da herkese açık.</p>{/if}
              {#if done}<p class="ok">{done}</p>{/if}
              {#if error}<p class="bad">{error}</p>{/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  {:else}
    <p class="calm">
      Bilgisayarındaki depolarda commit ve push için <button class="link" onclick={openSettings}>Ayarlar › Kaynaklar › GitHub</button> › “Bilgisayarımdaki
      depolar”ı aç.
    </p>
  {/if}

  {#if connected}
    <h4>GitHub’da son push edilenler</h4>
    {#each others as r (r.slug)}
      <button class="remote" onclick={() => void sourceAction('github', r.slug, 'open_repo')} title={`${r.slug} · GitHub’da aç`}>
        <b>{short(r.slug)}</b>
        {#if r.private}<span class="pill">gizli</span>{/if}
        {#if CI[r.ci]}<span class="ci {r.ci}">{CI[r.ci]}</span>{/if}
        {#if r.cloned}<small class="here">bu bilgisayarda</small>{/if}
        <small class="tm">{ago(r.timeMs, now)}</small>
      </button>
    {:else}
      <p class="calm">GitHub’da henüz depo yok.</p>
    {/each}
  {/if}
</div>

<style>
  .repos {
    display: grid;
    align-content: start;
    gap: 4px;
    min-width: 0;
  }
  h4 {
    margin: 4px 8px 0;
    color: var(--text-3);
    font-size: var(--fs-11);
    font-weight: 600;
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
  .ok {
    color: var(--green);
  }
  .warn {
    color: var(--red);
  }
  .link {
    color: var(--accent);
    font-weight: 600;
  }
  .pick .danger {
    margin-left: auto;
  }
  .link.danger,
  .act.strong.danger {
    color: var(--red);
  }
  .repo {
    display: grid;
    gap: 4px;
    min-width: 0;
    border-radius: 9px;
  }
  .repo.open {
    background: var(--surface-2);
    padding-bottom: 6px;
  }
  .head,
  .remote {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 4px 8px;
    border-radius: 8px;
    font-size: var(--fs-12);
    color: var(--text-2);
    text-align: left;
  }
  .head:hover,
  .remote:hover {
    background: var(--surface-3);
  }
  .dirty .head {
    color: var(--text);
  }
  b {
    flex: none;
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.2px var(--text-3);
  }
  .dot.on {
    background: var(--amber);
    box-shadow: none;
  }
  .dot.err {
    background: var(--red);
    box-shadow: none;
  }
  .br {
    flex: none;
    max-width: 22%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .note {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    margin-left: auto;
    color: var(--text-3);
  }
  .body {
    display: grid;
    gap: 6px;
    padding: 0 8px;
  }
  .body p {
    padding: 0;
  }
  .files {
    display: grid;
    gap: 1px;
    max-height: 132px;
    overflow-y: auto;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--fs-11-5);
    color: var(--text);
  }
  .file.off {
    color: var(--text-3);
  }
  .file input {
    flex: none;
    margin: 0;
    accent-color: var(--accent);
  }
  .k {
    flex: none;
    width: 14px;
    text-align: center;
    font-weight: 700;
    font-size: var(--fs-10-5);
    color: var(--text-2);
  }
  .k.added,
  .k.untracked {
    color: var(--green);
  }
  .k.deleted,
  .k.conflict {
    color: var(--red);
  }
  .k.modified,
  .k.renamed {
    color: var(--amber);
  }
  .p {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill {
    flex: none;
    padding: 0 6px;
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
  .pick {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--fs-11-5);
  }
  .pick small {
    color: var(--text-3);
  }
  .msg {
    height: 28px;
    padding: 0 9px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-12);
  }
  .msg:focus {
    outline: none;
    border-color: var(--accent);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    font-size: var(--fs-11-5);
  }
  .act {
    color: var(--text-2);
    font-weight: 500;
  }
  .act:hover:not(:disabled) {
    color: var(--text);
  }
  .act.strong {
    color: var(--accent);
    font-weight: 700;
  }
  .act:disabled {
    opacity: 0.45;
  }
  .ask {
    display: grid;
    gap: 5px;
    padding: 7px 0 2px;
    border-top: 1px solid var(--line);
  }
  .ask p {
    padding: 0;
  }
  .q {
    color: var(--text);
    font-weight: 600;
  }
  .sure {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--red);
    font-size: var(--fs-11-5);
  }
  .ci {
    flex: none;
    font-size: var(--fs-10-5);
    font-weight: 600;
    color: var(--text-3);
  }
  .ci.ok {
    color: var(--green);
  }
  .ci.failed {
    color: var(--red);
  }
  .here {
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11);
  }
  .tm {
    margin-left: auto;
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }
</style>
