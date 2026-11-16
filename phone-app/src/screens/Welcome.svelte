<script lang="ts">
  import Zody from '../Zody.svelte';
  let { secure, notice, onscan }: { secure: boolean; notice: string; onscan: () => void } = $props();
</script>

<div class="screen">
  <div class="body center">
    <Zody size="big" tone="idle" />
    <h2>Zodyvel'e hoş geldin</h2>
    <div class="t2">Bilgisayarındaki Zodyvel ile aynı Wi-Fi'de eşleşir. Veri evden çıkmaz, hiçbir sunucu kullanılmaz.</div>
    <div class="card small t2" style="text-align:left;width:100%">
      1. Bilgisayarda <b>Ayarlar › Telefon › Telefon ekle</b><br />
      2. Çıkan QR'ı bu telefonla okut<br />
      3. Telefondaki 6 haneli kodu bilgisayara yaz
    </div>
    {#if !secure}
      <div class="banner warn" style="width:100%;text-align:left">Önce telefonuna bir ekran kilidi (PIN, desen ya da parmak izi) kur. Zodyvel onsuz eşleşmez.</div>
    {/if}
    {#if notice}<div class="banner err" style="width:100%;text-align:left">{notice}</div>{/if}
  </div>
  <div class="foot"><button class="btn pri" disabled={!secure} onclick={onscan}>QR'ı okut</button></div>
</div>
