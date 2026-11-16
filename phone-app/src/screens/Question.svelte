<script lang="ts">
  // Soru formu: her soru için seçenek çipleri (tekli/çoklu) ve serbest metin. Gönder parmak izi ister.
  import Zody from '../Zody.svelte';
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { buildAnswers, toolName } from '../lib/view';
  import type { ApprovalInfo, SessionInfo } from '../lib/types';
  let { ap, session, unlocked, unlock, onback, ondone }: {
    ap: ApprovalInfo; session: SessionInfo; unlocked: boolean;
    unlock: () => Promise<boolean>; onback: () => void; ondone: () => void;
  } = $props();

  let picked = $state<string[][]>([]);
  let free = $state<string[]>([]);
  let busy = $state(false);
  let err = $state('');
  const qs = $derived(unlocked ? (ap.questions ?? []) : []);
  const answers = $derived(buildAnswers(qs, picked, free));
  const ready = $derived(qs.length > 0 && Object.keys(answers).length === qs.length);

  function toggle(i: number, opt: string, multi: boolean) {
    const cur = picked[i] ?? [];
    const next = multi ? (cur.includes(opt) ? cur.filter((x) => x !== opt) : [...cur, opt]) : cur[0] === opt ? [] : [opt];
    const p = [...picked];
    while (p.length <= i) p.push([]);
    p[i] = next;
    picked = p;
  }
  async function submit() {
    busy = true; err = '';
    try { await ipc.liveAnswer(ap.id, answers); ondone(); } catch (e) { err = errText(e); } finally { busy = false; }
  }
</script>

<div class="appbar"><button class="back" onclick={onback} aria-label="Geri"><Icon name="back" size={22} /></button></div>
<div class="body papp">
  <div class="qtop">
    <Zody size="lg" tone="amber" tool={session.tool} />
    <div class="tx"><b>Soru soruyor</b><small>{session.name} · {toolName(session.tool)}</small></div>
  </div>
  {#if !unlocked || ap.questions === null}
    <div class="lockbox">
      <Icon name="lock" size={22} /><b>Soru kilitli</b>
      <small>Soru metni parmak izi ya da PIN ile 2 dakika görünür.</small>
      <button class="btn pri sm" onclick={unlock}><Icon name="finger" size={16} />Ayrıntıları aç</button>
    </div>
  {:else}
    {#each qs as q, i (i)}
      <div class="qq">
        <div class="qh">{q.q}{#if q.multi}<small> · birden çok seçilebilir</small>{/if}</div>
        <div class="chips">
          {#each q.options as o (o)}
            <button class="chip" class:on={(picked[i] ?? []).includes(o)} onclick={() => toggle(i, o, q.multi)} disabled={busy || !!(free[i] ?? '').trim()}>
              {#if (picked[i] ?? []).includes(o)}<Icon name="check" size={12} />{/if}{o}
            </button>
          {/each}
        </div>
        <input class="free" placeholder="Başka bir şey yaz…" maxlength="500" bind:value={free[i]} disabled={busy} />
      </div>
    {/each}
  {/if}
  {#if err}<div class="banner err">{err}</div>{/if}
</div>
<div class="foot col papp-foot">
  <div class="fp"><Icon name="finger" size={14} />Gönderirken parmak izi istenecek</div>
  <button class="btn amber" onclick={submit} disabled={busy || !ready}>Gönder</button>
  <button class="btn gh" onclick={onback} disabled={busy}>Bilgisayarda cevaplarım</button>
</div>
