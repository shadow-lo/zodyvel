<script lang="ts">
  // Ayarlar › Kaynaklar › Mail: bağlı Gmail hesapları (en çok MAIL_SLOTS) ve hesap ekleme formu (adres +
  // uygulama şifresi). Hepsi adada tek Mail kutucuğunda birleşir. Ekle: sourceConnect('mail', …), değiştir:
  // aynı çağrı hesabın yuvasıyla, kaldır: sourceDisconnect('mail', yuva). Şifre alanı type="password"; değer
  // gönderilir gönderilmez bileşenden silinir, Rust'ta yalnız Windows Kimlik Bilgisi Yöneticisi'ne yazılır.
  // Sonuç `sources://update` ile gelir ("connecting" → "ok" / "error"); liste durumu oradan okur.
  import { call } from '../../../lib/ipc';
  import { MAIL_SLOTS, type AccountLine, type SourceState } from '../../../lib/sources';
  import { sourceConnect, sourceDisconnect } from '../../../lib/sources.svelte';

  interface Props {
    state: SourceState;
  }
  let { state: src }: Props = $props();

  const APP_PASSWORDS = 'https://myaccount.google.com/apppasswords';

  let address = $state('');
  let appPassword = $state('');
  let busy = $state(false);
  let message = $state<string | null>(null);
  /** Açık form: 'new' yeni hesap, sayı o yuvadaki hesabı değiştirir, null kapalı. */
  let editing = $state<'new' | number | null>(null);

  // Bağlı hesaplar (bağlı, bağlanıyor ya da bağlıyken hata). Mail kapalıyken liste gelmez; form görünür.
  const accounts = $derived(src.status === 'ok' || src.status === 'connecting' || src.status === 'error' ? (src.accounts ?? []) : []);
  const showForm = $derived(!accounts.length || editing !== null);
  const full = $derived(accounts.length >= MAIL_SLOTS);
  const validAddress = $derived(/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(address.trim()));
  // Google 16 harfi 4'lü gruplar hâlinde gösterir; boşluklar burada ve Rust'ta silinir.
  const cleanPassword = $derived(appPassword.replace(/\s+/g, ''));
  const canSubmit = $derived(validAddress && cleanPassword.length >= 16 && !busy);
  // Düğme kapalıyken nedeni yanında yazar (yoksa kullanıcı neden basamadığını bilmez).
  const why = $derived.by(() => {
    if (busy) return null;
    if (!address.trim()) return 'Gmail adresini yaz.';
    if (!validAddress) return 'Adres geçerli görünmüyor.';
    if (cleanPassword.length < 16) return `Uygulama şifresi 16 harf, şu an ${cleanPassword.length}.`;
    return null;
  });

  function line(a: AccountLine): string {
    if (a.status === 'ok') return `Bağlı · ${a.badge} okunmamış`;
    if (a.status === 'connecting') return 'Bağlanıyor…';
    return a.error ?? 'Bağlantıda sorun var.';
  }

  function openForm(target: 'new' | number) {
    editing = target;
    message = null;
    appPassword = '';
    address = typeof target === 'number' ? (accounts.find(a => a.slot === target)?.address ?? '') : '';
  }

  async function connect(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    busy = true;
    message = null;
    const fields = { address: address.trim(), appPassword: cleanPassword };
    appPassword = '';
    message = await sourceConnect('mail', fields, typeof editing === 'number' ? editing : undefined);
    busy = false;
    if (!message) {
      editing = null;
      address = '';
    }
  }

  async function disconnect(slot: number) {
    busy = true;
    message = await sourceDisconnect('mail', slot);
    busy = false;
    if (editing === slot) editing = null;
  }
</script>

<div class="mail-setup">
  {#if accounts.length}
    <ul class="accounts">
      {#each accounts as a (a.slot)}
        <li class="who">
          <span class="tx">
            <b>{a.address}</b>
            <small class:err={a.status === 'error'}>{line(a)}</small>
          </span>
          <button class="btn" onclick={() => openForm(a.slot)} disabled={busy}>Değiştir</button>
          <button class="btn" onclick={() => disconnect(a.slot)} disabled={busy}>Kaldır</button>
        </li>
      {/each}
    </ul>
    {#if editing === null}
      {#if full}
        <small class="hint">En çok {MAIL_SLOTS} Gmail hesabı bağlanabilir.</small>
      {:else}
        <button class="btn add" onclick={() => openForm('new')} disabled={busy}>+ Başka Gmail hesabı ekle</button>
      {/if}
    {/if}
  {/if}

  {#if showForm}
    {#if accounts.length}
      <h4>{typeof editing === 'number' ? `${address || 'Hesap'} için yeni şifre` : 'Yeni Gmail hesabı'}</h4>
    {/if}
    <ol class="steps">
      <li>Google hesabında iki adımlı doğrulama açık olmalı.</li>
      <li>
        <button class="link" type="button" onclick={() => call('source_open_link', { url: APP_PASSWORDS })}>Uygulama şifresi oluştur</button>
        (ad olarak "Zodyvel" yazabilirsin). Google 16 harfi bir kez gösterir. Her Gmail hesabının kendi şifresi olur.
      </li>
      <li>Adresini ve o 16 harfi aşağıya yaz.</li>
    </ol>
    <form onsubmit={connect}>
      <label>
        <span>Gmail adresi</span>
        <input type="email" bind:value={address} placeholder="ad@gmail.com" autocomplete="off" spellcheck="false" />
      </label>
      <label>
        <span>Uygulama şifresi</span>
        <input type="password" bind:value={appPassword} placeholder="abcd efgh ijkl mnop" autocomplete="off" spellcheck="false" />
      </label>
      <div class="acts">
        <button class="btn primary" type="submit" disabled={!canSubmit}>{busy ? 'Kaydediliyor…' : 'Bağlantıyı dene'}</button>
        {#if editing !== null}<button class="btn" type="button" onclick={() => (editing = null)}>Vazgeç</button>{/if}
        {#if why}<small class="hint" aria-live="polite">{why}</small>
        {:else if !accounts.length && src.status === 'off'}<small class="hint">Mail'i adada açınca bağlanır.</small>{/if}
      </div>
    </form>
    {#if !accounts.length && src.status === 'error' && src.error}<p class="err">{src.error}</p>{/if}
  {/if}
  {#if message}<p class="err">{message}</p>{/if}

  <p class="privacy">
    Şifreler yalnız bu bilgisayarda, Windows Kimlik Bilgisi Yöneticisi'nde (hesap başına ayrı) durur. Zodyvel doğrudan Gmail'e şifreli (TLS)
    bağlanır, iletileri diske yazmaz ve mail gönderemez. "Kaldır" o hesabın şifresini de siler. Bütün hesaplar adada tek Mail kutucuğunda
    toplanır.
  </p>
</div>

<style>
  .mail-setup {
    display: grid;
    gap: 10px;
  }
  .accounts {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 8px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .who .tx {
    flex: 1;
    min-width: 0;
    display: grid;
  }
  .who b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .add {
    justify-self: start;
  }
  h4 {
    margin: 4px 0 0;
    font-size: var(--fs-13-5);
    font-weight: 600;
  }
  .who small:not(.err) {
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .steps {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 3px;
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  form {
    display: grid;
    gap: 8px;
  }
  label {
    display: grid;
    gap: 3px;
  }
  label span {
    color: var(--text-2);
    font-size: var(--fs-12);
  }
  input {
    font: inherit;
    color: var(--text);
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    padding: 6px 10px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hint {
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  p {
    margin: 0;
  }
  .privacy {
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
</style>
