<script lang="ts">
  // Onay (adadaki kartın telefon hâli): İzin ver / Reddet. "Her zaman izin ver" telefonda bilerek yok.
  // Her karar parmak izi + Keystore imzası ister; masaüstünün ack'ı gelince ekran kapanır.
  import Zody from '../Zody.svelte';
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { toolName } from '../lib/view';
  import type { ApprovalInfo, SessionInfo } from '../lib/types';
  let { ap, session, unlocked, unlock, onback, ondone }: {
    ap: ApprovalInfo; session: SessionInfo; unlocked: boolean;
    unlock: () => Promise<boolean>; onback: () => void; ondone: () => void;
  } = $props();
  let busy = $state(false);
  let err = $state('');

  async function decide(allow: boolean) {
    busy = true; err = '';
    try { await ipc.liveDecide(ap.id, allow); ondone(); } catch (e) { err = errText(e); } finally { busy = false; }
  }
</script>

<div class="appbar"><button class="back" onclick={onback} aria-label="Geri"><Icon name="back" size={22} /></button></div>
<div class="body papp">
  <div class="zc"><Zody size="big" tone="amber" tool={session.tool} /></div>
  <div class="q">{ap.kind === 'plan' ? 'Plan onayı bekliyor' : 'Onay bekliyor'}</div>
  <div class="w2">{session.name}{session.folder ? ' · ' + session.folder : ''} · {toolName(session.tool)}</div>
  {#if unlocked && ap.action}
    <div class="tb2"><div class="h">{ap.kind === 'plan' ? 'Plan' : 'İstek'}</div><span class="mono">{ap.action}</span></div>
    {#if session.reply}<div class="w2 left">{session.reply}</div>{/if}
  {:else}
    <div class="lockbox">
      <Icon name="lock" size={22} /><b>Ayrıntı kilitli</b>
      <small>İsteğin ne olduğu parmak izi ya da PIN ile 2 dakika görünür.</small>
      <button class="btn pri sm" onclick={unlock}><Icon name="finger" size={16} />Ayrıntıları aç</button>
    </div>
  {/if}
  {#if err}<div class="banner err">{err}</div>{/if}
</div>
<div class="foot col papp-foot">
  <div class="fp"><Icon name="finger" size={14} />Karar verirken parmak izi istenecek</div>
  <button class="btn amber" onclick={() => decide(true)} disabled={busy}>İzin ver</button>
  <button class="btn gh" onclick={() => decide(false)} disabled={busy}>Reddet</button>
</div>
