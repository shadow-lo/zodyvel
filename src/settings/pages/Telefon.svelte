<script lang="ts">
  // Telefon: eşleştirme, eşli telefonlar, ağ durumu. Köprü yalnız modül açıkken çalışır (Rust `phone/`).
  // Durum Rust'tan okunur ('phone://status' olayında yeniden); QR geri sayımı yalnız pencere açıkken 1 sn'de bir döner.
  import { onMount } from 'svelte';
  import { call, callOrThrow, on } from '../../lib/ipc';
  import { data } from '../../lib/store.svelte';
  import type { PhoneStatus, PhonePairStart } from '../../lib/types';
  import Icon from '../Icon.svelte';
  import { update } from '../state.svelte';

  let st = $state<PhoneStatus | null>(null);
  let qr = $state<(PhonePairStart & { until: number }) | null>(null);
  let left = $state(0);
  let error = $state<string | null>(null);
  let confirmId = $state<string | null>(null); // "Kaldır" ikinci tık
  let confirmAll = $state(false);

  async function refresh(): Promise<void> {
    const next = await call<PhoneStatus>('phone_status');
    if (!next) return;
    st = next;
    // Pencere dışarıdan kapandıysa (süre doldu, eşleşti, iptal) QR'ı bırak.
    if (qr && !next.pairingOpen && !next.pairCode) qr = null;
  }

  onMount(() => {
    void refresh();
    return on('phone://status', () => void refresh());
  });

  // QR açıkken geri sayım; kod gelince ya da kapanınca durur.
  $effect(() => {
    if (!qr || st?.pairCode) return;
    const tick = () => {
      left = Math.max(0, Math.ceil(((qr?.until ?? 0) - Date.now()) / 1000));
      if (left === 0) {
        qr = null;
        void refresh();
      }
    };
    tick();
    const t = setInterval(tick, 1000);
    return () => clearInterval(t);
  });

  async function setEnabled(on: boolean): Promise<void> {
    error = null;
    if (!on) await cancelPair();
    await update(s => (s.modules.phone = on));
    setTimeout(() => void refresh(), 300);
  }

  async function addPhone(): Promise<void> {
    error = null;
    try {
      const r = await callOrThrow<PhonePairStart>('phone_pair_start');
      qr = { ...r, until: Date.now() + r.expiresInSecs * 1000 };
      void refresh();
    } catch (e) {
      error = String(e);
    }
  }

  async function cancelPair(): Promise<void> {
    qr = null;
    await call('phone_pair_cancel');
    void refresh();
  }

  // Telefonda görünen 6 haneli kodu kullanıcı yazar (tek deneme; yanlışsa eşleştirme iptal olur).
  let typed = $state('');
  async function confirmCode(): Promise<void> {
    error = null;
    const code = typed;
    typed = '';
    try {
      await callOrThrow('phone_pair_confirm', { code });
    } catch (e) {
      error = String(e);
    }
    qr = null;
    setTimeout(() => void refresh(), 300);
  }

  async function revoke(id: string): Promise<void> {
    if (confirmId !== id) {
      confirmId = id;
      return;
    }
    confirmId = null;
    try {
      await callOrThrow('phone_revoke', { id });
    } catch (e) {
      error = String(e);
    }
    void refresh();
  }

  async function revokeAll(): Promise<void> {
    if (!confirmAll) {
      confirmAll = true;
      return;
    }
    confirmAll = false;
    error = null;
    try {
      await callOrThrow('phone_revoke_all');
    } catch (e) {
      error = String(e); // kayıt silinemediyse kırmızı hata; dinleme kapalı kalır
    }
    void refresh();
  }

  async function retry(): Promise<void> {
    await call('phone_retry');
    setTimeout(() => void refresh(), 400);
  }

  // Rust tarihleri Unix SANİYE gönderir (G4: önceden milisaniye sanılıp 1970 görünüyordu).
  const when = (s: number) => (s ? new Date(s * 1000).toLocaleDateString('tr-TR', { day: 'numeric', month: 'short' }) : '–');
  const ago = (d: { connected: boolean; lastSeen: number }) => (d.connected ? 'şimdi' : d.lastSeen ? new Date(d.lastSeen * 1000).toLocaleString('tr-TR', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : 'hiç bağlanmadı');
  const mm = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  const enabled = $derived((data.settings.modules.phone ?? false));
</script>

<header class="page-head">
  <h1>Telefon</h1>
  <p>AI onaylarını telefondan ver, Ajanda hatırlatmalarını telefonda al. Yalnız ev ağında, sunucusuz, uçtan uca şifreli.</p>
</header>

<section class="group">
  <label class="row">
    <span class="ic"><Icon name="phone" /></span>
    <span class="tx"><b>Telefon modülü</b><small>Kapalıyken hiçbir ağ bağlantısı dinlenmez, bellek kullanılmaz.</small></span>
    <input class="switch" type="checkbox" role="switch" checked={enabled} onchange={e => setEnabled(e.currentTarget.checked)} />
  </label>
  {#if enabled}
    <div class="row">
      <span class="ic"><Icon name="link" /></span>
      <span class="tx">
        <b>Ağ durumu</b>
        {#if st?.listening}
          <small class="ok"><span class="dot"></span>Özel ağ · yalnız yerel ağda dinleniyor ({st.addr})</small>
        {:else}
          <small>{st?.problem ?? 'Başlatılıyor…'}</small>
        {/if}
      </span>
      {#if st && !st.listening}<button class="btn" onclick={retry}>Tekrar dene</button>{/if}
    </div>
  {/if}
</section>

{#if enabled && st && !st.listening && st.problem}
  <section class="group warnbox">
    <div class="row">
      <span class="tx">
        <b>Telefon bağlantısı dinlenmiyor</b>
        <small>{st.problem}</small>
        <small>Kafe, okul gibi ortak ağlarda Zodyvel telefon bağlantısını dinlemez. Evdeysen Windows'ta Ayarlar › Ağ ve İnternet › Wi-Fi bölümünde bu ağı <b>Özel</b> yap, sonra "Tekrar dene"ye bas.</small>
      </span>
    </div>
  </section>
{/if}

{#if enabled}
  <section class="group">
    <h3>Eşli telefonlar</h3>
    {#each st?.devices ?? [] as d (d.id)}
      <div class="row">
        <span class="zd" class:on={d.connected}><i></i><i></i></span>
        <span class="tx">
          <b>{d.name}{#if !d.hw}<span class="unprot" title={d.hwWhy}>Korumasız</span>{/if}</b>
          <small>Eşleşti {when(d.pairedAt)} · son bağlantı: {ago(d)}{#if d.expired} · 30 günü geçti, bağlanınca yeniden eşleştirme ister{/if}</small>
          <small>{#if d.hw}Donanım korumalı ({d.hwLevel === 'strongbox' ? 'StrongBox' : 'TEE'}){:else}Donanım kanıtı doğrulanamadı{d.hwWhy ? `: ${d.hwWhy}` : '.'} Yalnız izleme, onay/ret, cevap ve durdurma; yeni görev ve mesaj kapalı.{/if}</small>
        </span>
        <button class="btn danger" onclick={() => revoke(d.id)}>{confirmId === d.id ? 'Emin misin? Kaldır' : 'Kaldır'}</button>
      </div>
    {:else}
      <div class="row"><span class="tx"><small>Henüz eşli telefon yok.</small></span></div>
    {/each}
    <div class="row">
      <span class="tx"><small>Telefon kaybolursa "Kaldır" ile anında kesilir; telefon bir daha bağlanamaz. Bağlanmayan telefon 30 gün sonra yeniden eşleştirme ister.</small></span>
      <button class="btn primary" disabled={!st?.listening || (st?.devices.length ?? 0) >= 5} onclick={addPhone}>+ Telefon ekle</button>
    </div>
    {#if (st?.devices.length ?? 0) > 1}
      <div class="row">
        <span class="tx"><b>Hepsini kaldır</b><small>Anahtar da yenilenir; telefonlar yeniden eşleşmek zorunda.</small></span>
        <button class="btn danger" onclick={revokeAll}>{confirmAll ? 'Emin misin? Hepsini kaldır' : 'Hepsini kaldır'}</button>
      </div>
    {/if}
    {#if error}<div class="row"><span class="err">{error}</span></div>{/if}
  </section>

  <section class="group">
    <h3>Telefona ne gider</h3>
    <div class="row"><span class="tx"><b>Onay ve soru özetleri</b><small>Oturum adı, durum, komutun tek satırı (en çok 140 karakter).</small></span></div>
    <div class="row"><span class="tx"><b>Ajanda (önümüzdeki 14 gün)</b><small>Yalnız başlık, saat, tekrar ve ön bildirim süresi. Hatırlatmayı telefon kendisi kurar; plan bazında Ajanda'dan kapatılır.</small></span></div>
    <div class="row"><span class="tx"><b>Hiçbir zaman gitmeyenler</b><small>Komut çıktısı, dosya içeriği, kod farkı, plan metni, son cevap. "Her zaman izin ver" ve erişim seviyesi telefonda yok; telefondan açılan görev hep varsayılan izinle başlar.</small></span></div>
  </section>

  <section class="group">
    <h3>Son telefon eylemleri</h3>
    {#each st?.audit ?? [] as line, i (i)}
      <div class="row"><span class="tx"><small class="line">{line}</small></span></div>
    {:else}
      <div class="row"><span class="tx"><small>Bu oturumda telefondan eylem yok.</small></span></div>
    {/each}
  </section>
{/if}

{#if st?.pairCode}
  <div class="scrim"></div>
  <div class="modal" role="dialog" aria-label="Bu telefonu eşle?">
    <h4>Bu telefonu eşle?</h4>
    <p class="sm">Telefon "<b>{st.pairCode.name}</b>" adıyla bağlanmak istiyor <i>(ad telefonun kendi beyanı, doğrulanmadı)</i>. Kendi telefonunda değilse iptal et. Telefonun ekranında görünen <b>6 haneli kodu</b> buraya yaz:</p>
    <input class="codein" inputmode="numeric" maxlength="6" autocomplete="off" placeholder="______" bind:value={typed} onkeydown={e => e.key === 'Enter' && typed.length === 6 && confirmCode()} />
    <div class="two">
      <button class="btn" onclick={cancelPair}>İptal</button>
      <button class="btn primary" disabled={typed.length !== 6} onclick={confirmCode}>Eşle</button>
    </div>
  </div>
{:else if qr}
  <div class="scrim"></div>
  <div class="modal" role="dialog" aria-label="Telefon ekle">
    <h4>Telefon ekle</h4>
    <p class="sm">Zodyvel telefon uygulamasında "QR'ı okut"a bas ve bu kodu okut.</p>
    <!-- SVG metni Rust'ta üretilir (qrcode; kullanıcı girdisi yok). -->
    <div class="qr">{@html qr.svg}</div>
    <p class="sm tm">{mm(left)} içinde geçersiz olur · tek kullanımlık</p>
    <button class="btn" onclick={cancelPair}>İptal</button>
  </div>
{/if}

<style>
  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--green);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--green);
  }
  .unprot {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--amber);
    border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
  }
  .warnbox {
    border-color: color-mix(in srgb, var(--amber) 40%, transparent);
  }
  .warnbox b {
    color: var(--amber);
  }
  .btn.danger {
    color: var(--red);
  }
  .line {
    font-variant-numeric: tabular-nums;
  }
  .zd {
    flex: none;
    width: 34px;
    height: 24px;
    border-radius: 8px;
    border: 2px solid var(--text-3);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }
  .zd i {
    width: 4px;
    height: 7px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .zd.on {
    border-color: var(--green);
  }
  .zd.on i {
    background: var(--green);
  }
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--scrim);
  }
  .modal {
    position: fixed;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 340px;
    padding: 20px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    border-radius: 16px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    box-shadow: 0 20px 60px rgb(0 0 0 / 0.5);
  }
  .modal h4 {
    margin: 0;
    font-size: var(--fs-15);
  }
  .sm {
    margin: 0;
    text-align: center;
    color: var(--text-2);
    font-size: var(--fs-12);
    line-height: 1.5;
  }
  .tm {
    color: var(--text-3);
  }
  .qr {
    width: 210px;
    height: 210px;
    padding: 8px;
    border-radius: 12px;
    background: #fff;
  }
  .qr :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .codein {
    width: 180px;
    height: 44px;
    text-align: center;
    letter-spacing: 8px;
    border-radius: 9px;
    background: var(--surface-3);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-size: 22px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .two {
    display: flex;
    gap: 8px;
    align-self: stretch;
  }
  .two .btn {
    flex: 1;
  }
</style>
