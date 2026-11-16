<script lang="ts">
  // Yeni görev: masaüstünün son klasörlerinden biri (opak kimlik; yol telefona inmez), görev metni, Başlat.
  // İzin kipi her zaman "varsayılan"; telefondan yükseltilemez. Korumasız telefonda kapalı (O9, tasarım kararı).
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { MAX_TEXT } from '../lib/view';
  import type { FolderItem, LiveState } from '../lib/types';
  let { live, ondone }: { live: LiveState | null; ondone: () => void } = $props();

  let folders = $state<FolderItem[] | null>(null);
  let pick = $state('');
  let text = $state('');
  let busy = $state(false);
  let err = $state('');
  const connected = $derived(live?.link === 'connected');
  const restricted = $derived(live?.hw === false);

  // A-D7: istek hata verirse liste boş kalmaz (folders null kalır); bağlantı koptuktan sonra yeniden kurulunca
  // live.link üzerinden kendiliğinden yeniden istenir.
  let failed = $state(false);
  let loading = false; // tepkisel değil: aynı anda ikinci istek atılmasın diye

  // Bağlanınca (ya da sekme açılınca) klasörler bir kez istenir; zamanlayıcı yok.
  $effect(() => {
    if (!connected) { failed = false; return; }
    if (restricted || folders !== null || failed || loading) return;
    loading = true;
    ipc.liveFolders()
      .then((f) => { folders = f; if (f.length && !f.some((x) => x.id === pick)) pick = f[0]?.id ?? ''; })
      .catch((e) => { err = errText(e); folders = null; failed = true; })
      .finally(() => { loading = false; });
  });

  async function start() {
    busy = true; err = '';
    try { await ipc.liveNewTask(pick, text.trim()); text = ''; ondone(); }
    catch (e) {
      err = errText(e);
      if (err.includes('listede yok')) { folders = null; } // liste değişti: yeniden iste
    } finally { busy = false; }
  }
</script>

<div class="body">
  {#if restricted}
    <div class="warnbox"><Icon name="shield" size={16} /><span><b>Bu telefon kısıtlı</b><small>Bilgisayar, anahtarın güvenli çipte tutulduğunu doğrulayamadı. Durumu görme, onay/ret, soruya cevap ve durdurma açık (parmak iziyle); yeni görev ve mesaj yazma kapalı.</small></span></div>
  {:else if !connected}
    <div class="banner warn">Yeni görev için bilgisayara bağlı olman gerekiyor.</div>
  {/if}

  {#if !restricted}
  <div class="lbl">Klasör</div>
  <div class="flist">
    {#if folders === null && connected && failed}
      <div class="muted small">Klasörler alınamadı. Bağlantı yenilenince yeniden denenir.</div>
    {:else if folders === null && connected}
      <div class="muted small">Klasörler yükleniyor…</div>
    {:else if folders && folders.length}
      {#each folders as f (f.id)}
        <button class="fr" class:on={pick === f.id} onclick={() => (pick = f.id)} disabled={busy}>
          <Icon name="folder" size={16} /><span>{f.name}</span>{#if pick === f.id}<i><Icon name="check" size={14} /></i>{/if}
        </button>
      {/each}
    {:else if !restricted && connected}
      <div class="muted small">Bilgisayarda henüz son klasör yok. Bilgisayarda bir görev başlatınca burada görünür.</div>
    {/if}
  </div>
  {/if}

  {#if !restricted}
    <div class="lbl">Görev</div>
    <div class="ta">
      <textarea rows="4" maxlength={MAX_TEXT} placeholder="Ne yapılsın?" bind:value={text} disabled={busy || !connected}></textarea>
      <span class="cnt">{text.length} / {MAX_TEXT}</span>
    </div>
    <div class="hint"><Icon name="shield" size={14} />İzin kipi: varsayılan. Telefondan yükseltilemez; her izin ayrıca sorulur.</div>
  {/if}
  {#if err}<div class="banner err">{err}</div>{/if}
</div>
{#if !restricted}
<div class="foot col">
  <button class="btn pri" onclick={start} disabled={busy || restricted || !connected || !pick || !text.trim()}><Icon name="play" size={16} />Başlat</button>
  <div class="hint c">Başlatırken parmak izi istenecek</div>
</div>
{/if}
