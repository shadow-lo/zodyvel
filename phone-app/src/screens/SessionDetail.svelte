<script lang="ts">
  // Oturum ayrıntısı: son 3 adım, son cevap, yaz kutusu, durdur. Ayrıntı yalnız 2 dk pencere açıkken görünür;
  // her gönderim ve durdurma ayrıca parmak izi + Keystore imzası ister (çekirdekte).
  import Zody from '../Zody.svelte';
  import Icon from '../ui/Icon.svelte';
  import { ipc } from '../lib/ipc';
  import { errText } from '../lib/format';
  import { MAX_TEXT, statusOf, statusText, statusTone, timeLabel, toolName } from '../lib/view';
  import type { LiveState, SessionInfo } from '../lib/types';
  let { session, live, now, unlocked, unlock, onback, ondone }: {
    session: SessionInfo; live: LiveState; now: number; unlocked: boolean;
    unlock: () => Promise<boolean>; onback: () => void; ondone: () => void;
  } = $props();

  const KIND: Record<string, string> = { read: 'read', edit: 'edit', bash: 'bash', search: 'search' };
  let text = $state('');
  let busy = $state(false);
  let err = $state('');
  let info = $state('');
  let stopArm = $state(false);

  const st = $derived(statusOf(session.status));
  const active = $derived(st === 'running' || st === 'waiting_approval' || st === 'waiting_input');
  const restricted = $derived(live.hw === false);
  const sendHint = $derived(active ? 'Mesaj tur bitince iletilir · "Zodyvel\'den" notuyla' : 'Bu oturuma yeni iş olarak gider');

  async function run(f: () => Promise<void>, ok: string) {
    busy = true; err = ''; info = '';
    try { await f(); info = ok; ondone(); } catch (e) { err = errText(e); } finally { busy = false; }
  }
  const send = () => {
    const t = text.trim();
    if (!t) return;
    run(async () => {
      if (active) await ipc.liveNote(session.id, t, false);
      else await ipc.liveTask(session.id, t);
      text = '';
    }, active ? 'Mesaj iletildi; tur bitince ajana ulaşır.' : 'Görev iletildi.');
  };
  const stop = () => {
    if (!stopArm) { stopArm = true; return; }
    stopArm = false;
    run(() => ipc.liveStop(session.id), 'Durdurma isteği gönderildi.');
  };
</script>

<div class="appbar">
  <button class="back" onclick={onback} aria-label="Geri"><Icon name="back" size={22} /></button>
</div>
<div class="body">
  <div class="top">
    <Zody size="lg" tone={statusTone(session.status)} tool={session.tool} />
    <div class="tx">
      <b>{session.name}</b>
      <small>{#if session.folder}<span class="cat"><Icon name="folder" size={12} />{session.folder}</span> · {/if}{toolName(session.tool)} · {statusText(session.status)}{timeLabel(session, now) ? ' · ' + timeLabel(session, now) : ''}</small>
    </div>
  </div>

  {#if !unlocked || session.steps === null}
    <div class="lockbox">
      <Icon name="lock" size={22} />
      <b>Ayrıntı kilitli</b>
      <small>Adımlar, son cevap ve yaz kutusu parmak izi ya da PIN ile 2 dakika açılır.</small>
      <button class="btn pri sm" onclick={unlock}><Icon name="finger" size={16} />Ayrıntıları aç</button>
    </div>
  {:else}
    {#if session.steps.length}
      <div class="steps">
        {#each session.steps as x, i (i)}
          <div class:now={i === session.steps.length - 1 && st === 'running'}>
            <span class="s k-{KIND[x.kind] ?? 'other'}"><Icon name={KIND[x.kind] ?? 'other'} size={12} /></span>{x.label}<span class="mono">{x.target}</span>
          </div>
        {/each}
      </div>
    {:else if session.step}
      <div class="steps"><div>{session.step}</div></div>
    {/if}
    <div class="lastans">
      <small>Son cevap</small>
      {#if session.reply}{session.reply}{:else}<span class="muted">Bilgisayar son cevabı paylaşmıyor (Ayarlar › Telefon › ayrıntı paylaşımı).</span>{/if}
    </div>
  {/if}

  {#if info}<div class="banner"><span class="dot"></span>{info}</div>{/if}
  {#if err}<div class="banner err">{err}</div>{/if}
</div>

{#if unlocked && session.steps !== null}
  <div class="foot col">
    {#if restricted}
      <div class="warnbox"><Icon name="shield" size={16} /><span><b>Bu telefon kısıtlı</b><small>Mesaj yazma kapalı; durdurma açık.</small></span></div>
    {:else}
      <div class="compose">
        <textarea rows="1" maxlength={MAX_TEXT} placeholder="{toolName(session.tool)}'a yaz…" bind:value={text} disabled={busy}></textarea>
        <button class="snd" onclick={send} disabled={busy || !text.trim()} aria-label="Gönder"><Icon name="send" size={16} /></button>
      </div>
      <div class="hint">{sendHint} · parmak izi istenir</div>
    {/if}
    {#if active}
      <button class="btn no" onclick={stop} disabled={busy}><Icon name="stop" size={16} />{stopArm ? 'Emin misin? Durdur' : 'Durdur'}</button>
    {/if}
  </div>
{/if}
