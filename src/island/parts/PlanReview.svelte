<script lang="ts">
  // Plan incelemesi (Claude plan modu): plan okunur; Onayla, Reddet ya da "Değiştir…" ile ne değişmesi
  // gerektiği yazılıp plan geri çevrilir (Claude plan modunda kalıp planı düzeltir). "Onayla ·
  // düzenlemeleri sorma" Claude'un önerdiği izin kipine geçer; önce ne olacağı gösterilir.
  import { toolName, windowLabel } from '../../lib/format';
  import { answerApproval, data, focusSession, passToTool, startTyping } from '../../lib/store.svelte';
  import type { ApprovalRequest } from '../../lib/types';
  import MarkdownView from './MarkdownView.svelte';

  let { request }: { request: ApprovalRequest } = $props();

  let now = $state(Date.now());
  $effect(() => {
    if (!request.expiresAt || !request.actionable) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const left = $derived(request.expiresAt ? Math.max(0, Math.ceil((request.expiresAt - now) / 1000)) : null);
  const clock = $derived(left === null ? '' : `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`);

  let busy = $state(false);
  let error = $state<string | null>(null);
  let mode = $state<'read' | 'revise' | 'always'>('read');
  let note = $state('');

  async function decide(decision: 'allow' | 'deny' | 'always' | 'revise') {
    if (busy) return;
    busy = true;
    error = await answerApproval(request.id, decision, decision === 'revise' ? note.trim() : undefined);
    busy = false;
    if (!error) mode = 'read';
  }

  const app = $derived(data.agents.find(a => a.id === request.sessionId)?.app ?? null);
  async function focus() {
    error = await focusSession(request.sessionId);
  }
</script>

<section class="plan" aria-label="Plan incelemesi">
  <header>
    <span class="who"><b>{toolName(request.tool)}</b> · {request.project} · planını sundu</span>
    {#if request.demo}<span class="demo">Örnek</span>{/if}
    {#if app}<button class="go" onclick={focus} title="{app} penceresini öne getir">{windowLabel(app)} ↗</button>{/if}
    {#if request.actionable && left !== null}<span class="left" class:low={left <= 30}>{clock}</span>{/if}
  </header>
  <div class="doc" data-scroll>
    <MarkdownView source={request.plan ?? request.detail} />
  </div>
  <footer>
    {#if error}<p class="msg error">{error}</p>{/if}
    {#if !request.actionable}
      <p class="msg">
        {#if request.closedEarly}{toolName(request.tool)} adayı beklemedi (bağlantı kaydı eski). Ayarlar → Bağlantılar → Güncelle; şimdilik kararı {toolName(request.tool)} penceresinde ver.
        {:else}Kararı {toolName(request.tool)} penceresinde ver.{/if}
      </p>
      <div class="row">
        <button class="btn" disabled={busy} onclick={async () => (error = await passToTool(request.id))}>Kapat</button>
      </div>
    {:else if mode === 'revise'}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        bind:value={note}
        rows="2"
        maxlength="2000"
        placeholder="Planda ne değişsin? Claude planı buna göre düzeltip yeniden sunar."
        onfocus={startTyping}
        autofocus
      ></textarea>
      <div class="row">
        <button class="btn" disabled={busy} onclick={() => (mode = 'read')}>Vazgeç</button>
        <button class="btn ok" disabled={busy || !note.trim()} onclick={() => decide('revise')}>Geri gönder</button>
      </div>
    {:else if mode === 'always' && request.always}
      <p class="msg">Plan onaylanır ve bundan sonra sormadan: <b>{request.always}</b></p>
      <div class="row">
        <button class="btn" disabled={busy} onclick={() => (mode = 'read')}>Vazgeç</button>
        <button class="btn ok" disabled={busy} onclick={() => decide('always')}>Evet, onayla</button>
      </div>
    {:else}
      <div class="row">
        <button class="btn" disabled={busy} onclick={() => decide('deny')} title="Plan onaylanmaz; Claude plan modunda kalır">Reddet</button>
        <button class="btn" disabled={busy} onclick={() => (mode = 'revise')}>Değiştir…</button>
        <span class="gap"></span>
        {#if request.always}
          <button class="btn" disabled={busy} onclick={() => (mode = 'always')} title={request.always}>Onayla · düzenlemeleri sorma</button>
        {/if}
        <button class="btn ok" disabled={busy} onclick={() => decide('allow')}>Onayla</button>
      </div>
    {/if}
  </footer>
</section>

<style>
  .plan {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px 0;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .who {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .who b {
    color: var(--amber);
    font-weight: 600;
  }
  .demo {
    font-size: var(--fs-10-5);
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--surface-3);
    color: var(--text-3);
  }
  .go {
    flex: none;
    font-size: var(--fs-11);
    color: var(--text-3);
    padding: 1px 6px;
    border-radius: 6px;
  }
  .go:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .left {
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }
  .left.low {
    color: var(--amber);
  }
  .doc {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 8px 10px 0;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface-2);
    scrollbar-width: thin;
  }
  footer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .gap {
    flex: 1;
  }
  .row:not(:has(.gap)) {
    justify-content: flex-end;
  }
  .msg {
    margin: 0;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .msg b {
    color: var(--text);
    font-weight: 600;
  }
  .error {
    color: var(--red);
  }
  textarea {
    resize: none;
    padding: 7px 10px;
    border-radius: 9px;
    border: 1px solid var(--line-strong);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-12-5);
  }
  textarea:focus {
    outline: none;
    border-color: var(--amber);
  }
  .btn {
    height: 28px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text-2);
    font-weight: 600;
    font-size: var(--fs-12);
    white-space: nowrap;
  }
  .btn:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .btn.ok {
    background: var(--amber);
    color: var(--on-amber);
  }
  .btn.ok:hover {
    background: var(--amber-hi);
  }
</style>
