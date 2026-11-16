<script lang="ts">
  // Ajanda: bilgisayarın gönderdiği 14 günlük plan, salt okunur. Zil = telefon hatırlatma kuracak (G3).
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { clock, groupAgenda, isPast } from '../lib/view';
  import type { AgendaView, LiveState } from '../lib/types';
  let { live, now }: { live: LiveState | null; now: number } = $props();

  let view = $state<AgendaView | null>(null);
  let at = $state<number | null>(null);
  let err = $state('');
  const connected = $derived(live?.link === 'connected');
  let tried = false;
  // Sekme açılınca bir kez istenir (bağlı değilse çekirdek son kopyayı verir); başarısızsa bağlanınca yeniden.
  $effect(() => {
    if (view || (tried && !connected)) return;
    tried = true;
    ipc.liveAgenda().then((v) => { view = v; at = Date.now(); err = ''; }).catch((e) => { err = errText(e); });
  });
  const days = $derived(view ? groupAgenda(view.items, now) : []);
</script>

<div class="body list">
  {#if err && !view}<div class="banner warn">{err}</div>{/if}
  {#if view && !days.length}<div class="muted small c">Önümüzdeki {view.days} günde plan yok.</div>{/if}
  {#each days as d (d.date)}
    <div class="dh"><b>{d.title}</b><small>{d.sub}</small></div>
    {#each d.items as it (it.id + it.date + (it.time ?? ''))}
      <div class="ai" class:past={isPast(it, now)}>
        <span class="t mono">{it.time ?? 'gün boyu'}</span><span class="n">{it.title}</span>
        {#if it.notify && it.time}<span class="bl"><Icon name="bell" size={14} /></span>{/if}
      </div>
    {/each}
  {/each}
  {#if view}<div class="hint c">Planlar bilgisayarda eklenir{at ? ' · son eşitleme ' + clock(at) : ''}</div>{/if}
</div>
