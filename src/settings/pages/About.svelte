<script lang="ts">
  // Hakkında: sürüm, verilerin yeri, gizlilik, sorun bildirme.
  import { onMount } from 'svelte';
  import Zody from '../../mascot/Zody.svelte';
  import { call, callOrThrow, inTauri } from '../../lib/ipc';
  import { reportText, webviewVersion, type ReportInfo } from '../../lib/report';
  import { data, requestDemoApproval } from '../../lib/store.svelte';
  import Icon from '../Icon.svelte';

  type Found = { version: string } | null;
  let checking = $state(false);
  let updateMsg = $state('');

  async function checkUpdate(): Promise<void> {
    checking = true;
    updateMsg = '';
    try {
      const found = await callOrThrow<Found>('update_check');
      updateMsg = found ? `Yeni sürüm ${found.version} bulundu; güncelleme penceresi açıldı.` : 'Zodyvel güncel.';
    } catch (e) {
      updateMsg = `Denetlenemedi: ${e}`;
    } finally {
      checking = false;
    }
  }

  let busy = $state(false);
  let text = $state('');
  let copied = $state<'ok' | 'fail' | null>(null);
  let info = $state<ReportInfo | null>(null);
  let showText = $state(false);

  // Tarayıcı önizlemesinde Rust yok: örnek veri.
  const PREVIEW: ReportInfo = { version: '0.6.0', windows: 'Windows 11 Home 24H2 (derleme 26100.2894)', screens: ['1920×1080 · %125 (birincil)'], debugOn: false, log: null };

  async function collect(): Promise<void> {
    info = (await call<ReportInfo>('report_info')) ?? (inTauri ? null : PREVIEW);
    if (!info) throw new Error('rapor alınamadı');
    text = reportText(info, $state.snapshot(data.settings), webviewVersion(navigator.userAgent));
  }

  async function copy(value: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(value);
      return true;
    } catch {
      // Pano izni verilmediyse eski yol: gizli metin alanından kopyala.
      const area = document.createElement('textarea');
      area.value = value;
      area.style.cssText = 'position:fixed;opacity:0';
      document.body.append(area);
      area.select();
      const ok = document.execCommand('copy');
      area.remove();
      return ok;
    }
  }

  async function report(): Promise<void> {
    busy = true;
    copied = null;
    try {
      await collect();
      copied = (await copy(text)) ? 'ok' : 'fail';
    } catch {
      copied = 'fail';
    } finally {
      busy = false;
    }
  }

  // "Ayrıntılı kayıt" düğmesi gerçek durumu göstersin (ZODYVEL_DEBUG=1 ile açılmış olabilir).
  onMount(() => void collect().catch(() => {}));

  async function setDebug(on: boolean): Promise<void> {
    await call('report_debug', { on });
    await collect().catch(() => {});
    copied = null;
  }
</script>

<header class="page-head about">
  <Zody mood="done" size={64} alive interactive />
  <div>
    <h1>Zodyvel 0.8.0</h1>
    <p>AI ajanları, müzik, takvim ve sistem yükü için hafif Windows adası. Zody, Zodyvel'in maskotu.</p>
  </div>
</header>

<section class="group">
  <div class="row">
    <span class="ic"><Icon name="folder" /></span>
    <span class="tx"><b>Veriler</b><small>Ayarlar, takvim, yapılacaklar ve zamanlayıcı bu bilgisayarda bir klasörde durur; hiçbir yere gönderilmez.</small></span>
    <button class="btn" onclick={() => call('open_data_folder')}>Klasörü aç</button>
  </div>
  <div class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx"><b>AI ajanları</b><small>Köprü araç adını, komutu ya da dosya yolunu, oturumu ve komut çıktısının sonunu bu bilgisayardaki Zodyvel'e taşır. Okunan satırlar ve çıktılar yalnız ekranda gösterilir; diske yazılmaz, bilgisayardan çıkmaz.</small></span>
  </div>
  <div class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx"><b>Hafif</b><small>Boştayken işlemcinin yüzde birinden azını kullanır; ölçümler yalnız ilgili ekran açıkken yapılır.</small></span>
  </div>
</section>

<section class="group">
  <h3>Güncelleme</h3>
  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>Güncellemeleri denetle</b>
      <small>Zodyvel her açılışta bir kez, GitHub'daki yayın deposuna bakar; yeni sürüm varsa seçim ekranı açılır. İmzası tutmayan dosya kurulmaz. Arka planda sürekli denetim yapılmaz.</small>
      {#if updateMsg}<span class="ok" role="status">{updateMsg}</span>{/if}
    </span>
    <button class="btn primary" disabled={checking} onclick={checkUpdate}>{checking ? 'Denetleniyor…' : 'Denetle'}</button>
  </div>
</section>

<section class="group">
  <h3>Destek</h3>
  <div class="row">
    <span class="ic"><Icon name="info" /></span>
    <span class="tx">
      <b>Sorun bildir</b>
      <small>Sürüm, Windows ve ekran bilgisi, açık özellikler ve ayrıntılı kaydın son 200 satırı panoya kopyalanır; sonra istediğin yere yapıştırırsın. Kullanıcı adı, dosya adları ve yollar ayıklanır; hiçbir yere kendiliğinden gönderilmez.</small>
      {#if copied === 'ok'}
        <span class="ok" role="status">Kopyalandı. <button class="link" onclick={() => (showText = !showText)}>{showText ? 'Gizle' : 'Neyi kopyaladığını gör'}</button></span>
      {:else if copied === 'fail'}
        <span class="err" role="status">Kopyalanamadı. <button class="link" onclick={() => (showText = !showText)}>Metni göster</button> ve elle seç.</span>
      {/if}
    </span>
    <button class="btn primary" disabled={busy} onclick={report}>{busy ? 'Hazırlanıyor…' : 'Sorun bildir'}</button>
  </div>
  <div class="row">
    <span class="ic"><Icon name="ai" /></span>
    <span class="tx"><b>Örnek istekler</b><small>Adada örnek bir onay, soru ya da plan kartı açar; kartın nasıl göründüğünü denemek için. Hiçbir ajana gitmez.</small></span>
    <span class="demo">
      <button class="btn" onclick={() => requestDemoApproval('permission')}>Onay</button>
      <button class="btn" onclick={() => requestDemoApproval('question')}>Soru</button>
      <button class="btn" onclick={() => requestDemoApproval('plan')}>Plan</button>
    </span>
  </div>
  {#if showText && text}
    <div class="preview"><textarea readonly rows="12" value={text} aria-label="Kopyalanan rapor"></textarea></div>
  {/if}
  <div class="row">
    <span class="ic"><Icon name="motion" /></span>
    <span class="tx">
      <b>Ayrıntılı kayıt</b>
      <small>
        {#if info && !info.debugOn}
          Ayrıntılı kayıt kapalı; aç ve sorunu tekrarla, sonra Sorun bildir'e bas. Kayıt yalnız kısa tanı satırları yazar (komut, dosya adı ya da mesaj yazmaz).
        {:else}
          Kısa tanı satırları bilgisayarda bir dosyaya yazılır. Sorun geçince kapatabilirsin.
        {/if}
      </small>
    </span>
    {#if info?.debugOn}
      <button class="btn" onclick={() => setDebug(false)}>Kapat</button>
    {:else}
      <button class="btn" onclick={() => setDebug(true)}>Aç</button>
    {/if}
  </div>
</section>

<style>
  .demo {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .about {
    display: flex;
    gap: 16px;
    align-items: center;
    margin-bottom: 18px;
  }
  .about p {
    margin: 0;
  }
  .ok {
    color: var(--green);
    font-size: var(--fs-12);
  }
  .preview {
    padding: 0 16px 14px;
  }
  textarea {
    width: 100%;
    resize: vertical;
    font: 11.5px/1.45 Consolas, 'Cascadia Mono', monospace;
    color: var(--text-2);
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 12px;
  }
</style>
