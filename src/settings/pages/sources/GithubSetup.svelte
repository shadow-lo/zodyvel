<script lang="ts">
  // Ayarlar › Kaynaklar › GitHub: klasik kişisel token ile bağlanma. Token yalnız gönderilirken bu bileşende
  // durur, `source_connect` ile Windows Kimlik Bilgisi Yöneticisi'ne gider ve alan hemen temizlenir. Sonuç
  // "connecting" → "ok"/"error" olarak gelir; bağlanınca kullanıcı adı görünür. Bildirimler yalnız klasik
  // tokenla çalışır (ince taneli token reddedilir).
  import { NEW_TOKEN_URL, tokenProblem } from '../../../lib/github';
  import type { SourceState } from '../../../lib/sources';
  import { call } from '../../../lib/ipc';
  import { sourceConnect, sourceDisconnect } from '../../../lib/sources.svelte';

  interface Props {
    state: SourceState;
  }
  // `state` adı `$state` rünüyle çakışmasın diye içeride `source`.
  let { state: source }: Props = $props();

  let token = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let editing = $state(false);
  let sent = $state(false);

  const connected = $derived(source.status === 'ok' || source.status === 'connecting' || source.status === 'error');
  const warning = $derived(tokenProblem(token));
  const showForm = $derived(editing || source.status === 'needs_setup' || (source.status === 'off' && !sent));

  async function connect() {
    if (!token.trim() || warning) return;
    busy = true;
    error = await sourceConnect('github', { token: token.trim() });
    token = '';
    busy = false;
    if (!error) {
      editing = false;
      sent = true;
    }
  }

  async function disconnect() {
    busy = true;
    error = await sourceDisconnect('github');
    busy = false;
    sent = false;
  }

</script>

<div class="gs">
  {#if source.status === 'ok'}
    <p class="st"><span class="pill ok">Bağlı</span> {source.account ?? ''}</p>
  {:else if source.status === 'connecting'}
    <p class="st"><span class="pill">Bağlanıyor…</span></p>
  {:else if source.status === 'error'}
    <p class="st"><span class="pill bad">Hata</span> <span class="err">{source.error ?? 'GitHub’a ulaşılamıyor.'}</span></p>
  {:else if source.status === 'off' && sent}
    <p class="st muted">Token kaydedildi. “Adada göster” açılınca bağlanır.</p>
  {/if}

  {#if showForm}
    <ol class="steps">
      <li>
        GitHub’da <button class="link" onclick={() => call('source_open_link', { url: NEW_TOKEN_URL })}>klasik token oluştur ↗</button>; kapsamlar
        (<code>repo</code>, <code>notifications</code>) önceden işaretli gelir. İnce taneli token bildirimlerde çalışmaz.
      </li>
      <li>Oluşan <code>ghp_…</code> tokenı buraya yapıştır.</li>
    </ol>
    <form
      class="form"
      onsubmit={e => {
        e.preventDefault();
        void connect();
      }}
    >
      <input type="password" autocomplete="off" spellcheck="false" placeholder="ghp_…" aria-label="GitHub tokenı" bind:value={token} disabled={busy} />
      <button class="btn primary" type="submit" disabled={busy || !token.trim() || !!warning}>{busy ? 'Bağlanıyor…' : 'Bağlantıyı dene'}</button>
      {#if editing}<button class="btn" type="button" onclick={() => ((editing = false), (token = ''))}>Vazgeç</button>{/if}
    </form>
    {#if warning}<p class="warn">{warning}</p>{/if}
  {:else if connected}
    <div class="form">
      <button class="btn" onclick={() => (editing = true)} disabled={busy}>Tokenı değiştir</button>
      <button class="btn" onclick={disconnect} disabled={busy}>Bağlantıyı kaldır</button>
    </div>
  {/if}
  {#if error}<p class="err">{error}</p>{/if}
  <p class="note">
    Token Windows Kimlik Bilgisi Yöneticisi’nde şifreli durur, yalnız <code>api.github.com</code>’a gider. Zodyvel kırık CI, inceleme
    isteği ve bildirimleri okur; yalnız sen düğmeye basınca bildirimi okundu yapar ya da CI’ı yeniden çalıştırır. “Bağlantıyı kaldır”
    tokenı Windows’tan da siler.
  </p>
</div>

<style>
  .gs {
    display: grid;
    gap: 10px;
    padding: 0 16px 16px;
  }
  p {
    margin: 0;
    font-size: var(--fs-12);
  }
  .st {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .muted,
  .note {
    color: var(--text-3);
  }
  .steps {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 6px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  code {
    font-size: var(--fs-11-5);
  }
  .form {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    font-family: inherit;
  }
</style>
