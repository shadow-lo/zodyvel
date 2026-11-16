<script lang="ts">
  // Ayarlar: eşli bilgisayar, donanım koruması (O9), arka planda bağlı kalma, açılışta bağlanma, pil, Ajanda hatırlatıcıları (G3),
  // ayrıntı penceresi, eşleşmeyi kaldırma. Eskiz: design/telefon-eskiz.html §4.
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { remainText } from '../lib/view';
  import type { AppState, LiveState } from '../lib/types';
  let { st, live, now, unlocked, unlock, onchange, onrefresh }: {
    st: AppState; live: LiveState | null; now: number; unlocked: boolean;
    unlock: () => Promise<boolean>; onchange: () => void; onrefresh: () => void;
  } = $props();

  let busy = $state(false);
  let err = $state('');
  let info = $state('');
  let removeArm = $state(false);

  async function run(f: () => Promise<void>) {
    busy = true; err = ''; info = '';
    try { await f(); } catch (e) { err = errText(e); } finally { busy = false; }
  }
  const linkPill = $derived(
    live?.link === 'connected' ? ['ok', 'Bağlı'] :
    live?.link === 'searching' ? ['warn', 'Aranıyor'] :
    live?.link === 'repair' || live?.link === 'revoked' ? ['bad', 'Geçersiz'] :
    live?.enabled === false ? ['', 'Kapalı'] : ['warn', 'Bağlı değil'],
  );
  const hwRow = $derived(
    live?.hw === true ? { pill: 'ok', tag: 'Korumalı', sub: live.hw_level === 'strongbox' ? 'Anahtar StrongBox çipinde' : 'Anahtar güvenli çipte (TEE)' } :
    live?.hw === false ? { pill: 'warn', tag: 'Korumasız', sub: 'Anahtar doğrulanamadı · kısıtlı kip' } :
    { pill: '', tag: 'Bilinmiyor', sub: 'Bağlanınca bilgisayar söyler' },
  );
  const remain = $derived(remainText(live?.unlocked_until ?? null, now));

  const toggle = () => run(async () => { await ipc.liveSetEnabled(!(live?.enabled ?? true)); onrefresh(); });
  const remindSub = $derived(
    live?.reminders === false ? 'Kapalı · alarm kurulmaz' :
    live?.reminder_count != null ? `${live.reminder_count} hatırlatma kurulu · 14 gün` : 'Bağlanınca kurulur · 14 gün',
  );
  const toggleRemind = () => run(async () => { await ipc.remindersSet(!(live?.reminders ?? true)); onrefresh(); });
  const toggleBoot = () => run(async () => { await ipc.autostartSet(!(live?.autostart ?? false)); onrefresh(); });
  const battery = () => run(async () => { const ok = await ipc.batteryExempt(); if (ok) info = 'Pil optimizasyonundan zaten muafsın.'; });
  const lockNow = () => run(async () => { await ipc.lock(); onrefresh(); });
  const remove = () => {
    if (!removeArm) { removeArm = true; return; }
    removeArm = false;
    run(async () => {
      await ipc.unlock(); // eşleşmeyi kaldırmak da kilit ister
      await ipc.unpair();
      onchange();
    });
  };
</script>

<div class="body">
  <div class="sg">
    <div class="sr"><Icon name="phone" /><span class="tx"><b>{st.desktop_name ?? 'Eşli bilgisayar'}</b><small>Kimlik izi {st.fingerprint ?? '—'}</small></span><span class="pill2 {linkPill[0]}">{linkPill[1]}</span></div>
    <div class="sr"><Icon name="shield" /><span class="tx"><b>Donanım</b><small>{hwRow.sub}</small></span><span class="pill2 {hwRow.pill}">{hwRow.tag}</span></div>
  </div>
  {#if live?.hw === false}
    <div class="warnbox"><Icon name="shield" size={16} /><span><b>Bu telefon korumasız</b><small>Bilgisayar, anahtarın güvenli çipte tutulduğunu doğrulayamadı. Yeni görev ve mesaj yazma kapalı; durumu görme, onay/ret, soruya cevap ve durdurma açık.</small></span></div>
  {/if}
  {#if live?.detail && live.link !== 'connected'}<div class="banner warn">{live.detail}</div>{/if}

  <div class="sg">
    <button class="sr" onclick={toggle} disabled={busy}><Icon name="wifi" /><span class="tx"><b>Arka planda bağlı kal</b><small>Ev ağındayken kalıcı bildirimle</small></span><span class="sw" class:on={live?.enabled !== false}></span></button>
    <button class="sr" onclick={toggleBoot} disabled={busy}><Icon name="restart" /><span class="tx"><b>Telefon açılınca bağlan</b><small>Yeniden başlayınca kendiliğinden</small></span><span class="sw" class:on={live?.autostart === true}></span></button>
    <button class="sr" onclick={battery} disabled={busy}><Icon name="timer" /><span class="tx"><b>Pil ayarı</b><small>Bağlantı kopmasın diye muaf tut</small></span><span class="chv"><Icon name="chev" size={16} /></span></button>
  </div>

  <div class="sg">
    <button class="sr" onclick={toggleRemind} disabled={busy}><Icon name="bell" /><span class="tx"><b>Ajanda hatırlatıcıları</b><small>{remindSub}</small></span><span class="sw" class:on={live?.reminders !== false}></span></button>
  </div>

  <div class="sg">
    {#if unlocked && remain}
      <button class="sr" onclick={lockNow} disabled={busy}><Icon name="eye" /><span class="tx"><b>Ayrıntılar açık</b><small>Kapatmak için dokun</small></span><span class="pill2 acc">{remain}</span></button>
    {:else}
      <button class="sr" onclick={unlock} disabled={busy}><Icon name="lock" /><span class="tx"><b>Ayrıntıları aç</b><small>Komut, soru ve son cevap 2 dk görünür</small></span><span class="chv"><Icon name="finger" size={16} /></span></button>
    {/if}
  </div>

  <div class="hint"><Icon name="bell" size={14} />Bildirimler kilit ekranında içerik göstermez. Her eylem parmak izi ya da PIN ister.</div>
  {#if info}<div class="banner"><span class="dot"></span>{info}</div>{/if}
  {#if err}<div class="banner err">{err}</div>{/if}
</div>
<div class="foot col">
  <button class="btn no" onclick={remove} disabled={busy}>{removeArm ? 'Emin misin? Eşleşmeyi kaldır' : 'Eşleşmeyi kaldır'}</button>
</div>
