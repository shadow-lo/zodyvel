<script lang="ts">
  import Zody from '../Zody.svelte';
  import { ipc } from '../lib/ipc';
  import { codeDigits, errText } from '../lib/format';
  let { code, fingerprint, oncancel, ondone, onfail }: { code: string; fingerprint: string; oncancel: () => void; ondone: () => void; onfail: (m: string) => void } = $props();
  let waiting = $state(false);

  async function typed() {
    waiting = true;
    try {
      await ipc.pairFinish(true);
      ondone();
    } catch (e) {
      onfail(errText(e));
    }
  }
</script>

<div class="screen">
  <div class="appbar"><h1>Kodu bilgisayara yaz</h1></div>
  <div class="body" style="gap:16px">
    <div style="display:flex;justify-content:center;margin-top:8px"><Zody size="big" tone="amber" /></div>
    <div style="text-align:center;font-weight:650;font-size:17px">Bilgisayarda "Telefon ekle" penceresine bu kodu yaz</div>
    <div class="code6">{#each codeDigits(code) as d}<span>{d}</span>{/each}</div>
    <div class="card small t2">Kodu bilgisayara <b>sen yazarsın</b>; ekranda hazır görünmez. Kod tutmazsa eşleşme iptal olur, biri araya girmeye çalışıyor olabilir.</div>
    <div class="card row">
      <Zody size="sm" />
      <div class="small"><b>Bilgisayar</b><div class="muted">Kimlik izi: {fingerprint}</div></div>
    </div>
    {#if waiting}<div class="banner warn"><span class="dot"></span>Bilgisayarda onay bekleniyor…</div>{/if}
  </div>
  <div class="foot">
    <button class="btn gh" onclick={oncancel} disabled={waiting}>İptal</button>
    <button class="btn pri" onclick={typed} disabled={waiting}>Kodu yazdım</button>
  </div>
</div>
