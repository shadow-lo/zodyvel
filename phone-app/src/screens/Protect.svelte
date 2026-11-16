<script lang="ts">
  let { onfinish }: { onfinish: () => void } = $props();
</script>

<div class="screen">
  <div class="appbar"><h1>Telefonu koru</h1></div>
  <div class="body">
    <div class="t2" style="line-height:1.5">Eşleşme tamam. Koruma şu anda şöyle çalışıyor:</div>
    <div class="card" style="display:flex;flex-direction:column;gap:14px">
      <div class="row"><div class="tx"><b>Cihaz kilidiyle doğrula</b><small>Onay, cevap, yeni görev, durdurma ve ayrıntı okuma her seferinde parmak izi / yüz / PIN ister</small></div><span class="pill" style="color:var(--green)">zorunlu</span></div>
      <div class="row"><div class="tx"><b>Ekran görüntüsü kapalı</b><small>Son uygulamalar önizlemesinde de içerik görünmez</small></div><span class="pill" style="color:var(--green)">açık</span></div>
      <div class="row"><div class="tx"><b>Anahtarlar telefonun kasasında</b><small>Android Keystore; bulut yedeğine girmez</small></div><span class="pill" style="color:var(--green)">açık</span></div>
    </div>
    <div class="lockline">Yeni parmak izi eklenirse güvenlik için yeniden eşleşmen gerekir</div>
  </div>
  <div class="foot"><button class="btn pri" onclick={onfinish}>Bitir</button></div>
</div>
