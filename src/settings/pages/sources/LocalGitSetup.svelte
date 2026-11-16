<script lang="ts">
  // Ayarlar › Kaynaklar › GitHub › Bilgisayarımdaki depolar (Yerel Git): adada "Depolarım" sekmesinde bu
  // klasörlerdeki depoların commitlenmemiş değişiklikleri görünür, onaylı commit ve push yapılır. Klasör depo
  // ise kendisi, değilse 2 kat altındaki depolar. "GitHub Desktop'tan al" GitHub Desktop'un depo listesinden
  // yolları bir kez okur (yalnız yol; GitHub Desktop'a dokunmaz).
  import { callOrThrow, inTauri } from '../../../lib/ipc';
  import { foldersFrom } from '../../../lib/localgit';
  import { data } from '../../../lib/store.svelte';
  import { update } from '../../state.svelte';

  const on = $derived(data.settings.features.localGit);
  const folders = $derived(data.settings.git.folders);
  let busy = $state(false);
  let note = $state<string | null>(null);
  let error = $state<string | null>(null);

  function setOn(value: boolean) {
    void update(s => {
      s.features.localGit = value;
      if (value) {
        s.features.sources = true;
        s.features.sourceGithub = true;
      }
    });
  }

  function add(paths: string[]) {
    const next = paths.filter(p => !folders.some(f => f.toLowerCase() === p.toLowerCase()));
    if (next.length) void update(s => (s.git.folders = [...s.git.folders, ...next]));
    return next.length;
  }

  async function pick() {
    error = note = null;
    if (!inTauri) {
      add(['C:\\Users\\demo\\work']);
      return;
    }
    busy = true;
    try {
      const path = await callOrThrow<string | null>('git_pick_folder');
      if (path) add([path]);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function fromDesktop() {
    error = note = null;
    busy = true;
    try {
      const found = inTauri ? await callOrThrow<string[]>('git_desktop_repos') : ['C:\\Users\\demo\\work\\Api', 'C:\\Users\\demo\\work\\Psrc', 'D:\\Oyun\\Barinak'];
      const n = add(foldersFrom(found, folders));
      note = found.length === 0 ? 'GitHub Desktop listesinde depo bulunamadı; “Klasör ekle” ile ekle.' : n ? `${found.length} depo bulundu, ${n} klasör eklendi.` : `${found.length} depo bulundu, hepsi zaten listede.`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function remove(path: string) {
    void update(s => (s.git.folders = s.git.folders.filter(f => f !== path)));
  }
</script>

<div class="lg">
  <label class="row">
    <span class="tx">
      <b>Bilgisayarımdaki depolar</b>
      <small>Adada GitHub › “Depolarım”: commitlenmemiş değişiklikler, onaylı Commit, Push, Pull ve Geri al. Push, Fetch ve Pull Zodyvel’e bağlı GitHub tokenını kullanır (yalnız github.com’a gider); yoksa bilgisayarındaki git girişini.</small>
    </span>
    <input class="switch" type="checkbox" role="switch" checked={on} onchange={e => setOn(e.currentTarget.checked)} />
  </label>

  {#if on}
    {#if folders.length}
      <ul class="folders">
        {#each folders as f (f)}
          <li><span title={f}>{f}</span><button class="x" onclick={() => remove(f)} title="Listeden çıkar (klasöre dokunmaz)" aria-label="Çıkar">×</button></li>
        {/each}
      </ul>
    {:else}
      <p class="muted">Henüz klasör yok. Depolarının durduğu klasörü (ör. <code>Masaüstü\work</code>) ekle; içindeki depolar bulunur. “Yeni görev”in son klasörlerindeki depolar kendiliğinden görünür.</p>
    {/if}
    <div class="form">
      <button class="btn" onclick={pick} disabled={busy}>Klasör ekle</button>
      <button class="btn" onclick={fromDesktop} disabled={busy}>GitHub Desktop’tan al</button>
    </div>
    {#if note}<p class="muted">{note}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}
    <p class="note">
      Dosya içerikleri okunmaz ve hiçbir yere gönderilmez; yalnız <code>git status</code>. Commit ve push adada onay ister. <code>.env</code>, anahtar
      ve şifre dosyası gibi görünenler varsayılan olarak commit dışında kalır. Pull, dal değiştirme ve zorla push yok; onlar için GitHub Desktop.
    </p>
  {/if}
</div>

<style>
  .lg {
    display: grid;
    gap: 10px;
    margin-top: 6px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .tx {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .tx small {
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  p {
    margin: 0;
    font-size: var(--fs-12);
  }
  .muted,
  .note {
    color: var(--text-3);
  }
  .err {
    color: var(--red);
  }
  code {
    font-size: var(--fs-11-5);
  }
  .folders {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .folders li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 5px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    font-size: var(--fs-12);
  }
  .folders span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    color: var(--text-2);
  }
  .x:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .form {
    display: flex;
    gap: 8px;
    align-items: center;
  }
</style>
