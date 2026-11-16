<script lang="ts">
  // Başlangıç: Windows ile başlat (kullanıcının Run kaydında tek değer; ek hizmet yok).
  import Icon from '../Icon.svelte';
  import { setAutostart, ui } from '../state.svelte';
</script>

<header class="page-head">
  <h1>Başlangıç</h1>
  <p>Ek bir arka plan hizmeti, görev zamanlayıcı ya da yönetici izni gerekmez.</p>
</header>

<section class="group">
  <label class="row" class:disabled={!ui.autostart?.available}>
    <span class="ic"><Icon name="power" /></span>
    <span class="tx">
      <b>Windows ile başlat</b>
      <small>
        {#if !ui.autostart?.available}
          Geliştirme sürümünde kapalı; kurulu Zodyvel'de çalışır.
        {:else if ui.autostart.disabledBySystem}
          Görev Yöneticisi → Başlangıç uygulamaları'nda kapatılmış. Buradan açarsan orada da açılır.
        {:else}
          Oturum açınca ada kendiliğinden gelir.
        {/if}
      </small>
      {#if ui.autostart?.otherPath && ui.autostart.enabled}
        <span class="warn">
          Kayıt başka bir konumdaki Zodyvel'i başlatıyor: <span class="mono">{ui.autostart.otherPath}</span>
          <button class="link" onclick={() => setAutostart(true)}>Bu kopyayı kaydet</button>
        </span>
      {/if}
      {#if ui.autostartError}<span class="warn">{ui.autostartError}</span>{/if}
    </span>
    <input
      class="switch"
      type="checkbox"
      role="switch"
      disabled={!ui.autostart?.available}
      checked={ui.autostart?.enabled ?? false}
      onchange={e => setAutostart(e.currentTarget.checked)}
    />
  </label>
</section>
