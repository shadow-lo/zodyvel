<script lang="ts">
  // Oturumlar: durum, ad, klasör ve süre kilitsiz görünür; ayrıntı için oturuma dokunmak parmak izi ister.
  import Zody from '../Zody.svelte';
  import Icon from '../ui/Icon.svelte';
  import { needsMe, sortSessions, statusText, statusTone, timeLabel, clock } from '../lib/view';
  import type { LiveState } from '../lib/types';
  let { live, now, busy, onopen }: { live: LiveState | null; now: number; busy: boolean; onopen: (id: string) => void } = $props();

  const list = $derived(live ? sortSessions(live.sessions) : []);
  const away = $derived(!live || live.link !== 'connected');
  const awayTitle = $derived(
    !live || !live.enabled ? 'Arka planda bağlı kalma kapalı' :
    live.link === 'repair' || live.link === 'revoked' ? 'Eşleşme geçersiz' :
    live.link === 'idle' ? 'Bağlantı denemesi durdu' :
    live.link === 'connecting' ? 'Bağlanıyor…' : 'Bilgisayar ağda değil',
  );
  const awayText = $derived(
    !live || !live.enabled ? 'Ayarlar\'dan "Arka planda bağlı kal"ı açınca oturumlar burada görünür.' :
    live.link === 'repair' || live.link === 'revoked' ? (live.detail || 'Ayarlar\'dan eşleşmeyi kaldırıp yeniden eşleş.') :
    live.link === 'idle' ? (live.detail || 'Uygulamayı açınca yeniden denenir.') :
    'Zodyvel yalnız ev ağında bağlanır. Eve dönünce kendiliğinden yeniden bağlanır.',
  );
</script>

{#if away}
  <div class="body center">
    <Zody size="big" tone="idle" />
    <b style="font-size:17px">{awayTitle}</b>
    <span class="t2 small">{awayText}</span>
    {#if live?.last_connected}<span class="muted small">Son bağlantı: {clock(live.last_connected)}</span>{/if}
  </div>
{:else}
  <div class="body list">
    {#each list as s (s.id)}
      <button class="ss" class:att={needsMe(s)} disabled={busy} onclick={() => onopen(s.id)}>
        <Zody tone={statusTone(s.status)} tool={s.tool} />
        <span class="tx"><b>{s.name}</b><small>{s.folder ? s.folder + ' · ' : ''}{statusText(s.status)}</small></span>
        <span class="tm">{timeLabel(s, now)}</span>
      </button>
    {:else}
      <div class="empty"><Zody size="big" tone="idle" /><b>Şu an oturum yok</b><span class="t2 small">Bilgisayarda bir AI oturumu başlayınca burada görünür.</span></div>
    {/each}
    {#if list.length}<div class="hint c"><Icon name="finger" size={14} />Ayrıntı için oturuma dokun · parmak izi istenir</div>{/if}
  </div>
{/if}
