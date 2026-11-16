<script lang="ts">
  import { onMount } from 'svelte';
  import { cancelScan, ipc, scanQr } from '../lib/ipc';
  import { errText } from '../lib/format';
  import type { PairBegin } from '../lib/types';
  let { onback, onbegun, onfail }: { onback: () => void; onbegun: (b: PairBegin) => void; onfail: (m: string) => void } = $props();
  let busy = $state(false);
  let gone = false;

  onMount(() => {
    (async () => {
      try {
        const qr = await scanQr();
        if (gone) return;
        if (qr === null) return onback();
        busy = true;
        onbegun(await ipc.pairBegin(qr));
      } catch (e) {
        if (!gone) onfail(errText(e));
      }
    })();
    return () => { gone = true; };
  });

  function back() {
    gone = true;
    cancelScan().finally(onback);
  }
</script>

<div class="screen scan">
  <div class="appbar"><button class="back" aria-label="Geri" onclick={back}>←</button><h1>QR'ı okut</h1></div>
  <div class="body">
    <div class="viewfinder"><div class="box"></div></div>
    <div class="t2 small" style="text-align:center">{busy ? 'Bilgisayarla el sıkışılıyor…' : 'Bilgisayardaki QR kodunu çerçevenin içine al'}</div>
    <div class="card small t2">QR 2 dakika geçerli ve <b>yalnız bir kez</b> kullanılabilir. İçinde şifre değil, bilgisayarın kimlik izi ve tek seferlik anahtar var.</div>
  </div>
</div>
