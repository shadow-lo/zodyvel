<script lang="ts">
  import { lineCounts, toolName, windowLabel } from '../../lib/format';
  import { untrack, type Snippet } from 'svelte';
  import { answerApproval, data, focusSession, passToTool } from '../../lib/store.svelte';
  import type { ApprovalRequest } from '../../lib/types';

  interface Props {
    request: ApprovalRequest;
    extra?: number;
    /** Kuyrukta sıradaki istek ("Zodyvel · Düzenleme"); verilirse "+N istek daha" yerine gösterilir. */
    next?: string;
    /** Başlıkta kuyruk düğmeleri (‹ 1/2 ›). */
    nav?: Snippet;
    /** Dosyaya tıklanınca tam fark (mini görünümde ayrıntıyı açar). Yoksa dosyalar yalnız bilgi. */
    onfile?: () => void;
    /** "Sonra ⌄" (U5): kartı cevaplamadan katla; verilmezse düğme yok. */
    onlater?: () => void;
  }
  let { request, extra = 0, next, nav, onfile, onlater }: Props = $props();

  // Geri sayım yalnız adadan karar verilebilen kart ekrandayken saniyede bir güncellenir.
  let now = $state(Date.now());
  $effect(() => {
    if (!request.expiresAt) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const left = $derived(request.expiresAt ? Math.max(0, Math.ceil((request.expiresAt - now) / 1000)) : null);

  let busy = $state(false);
  let error = $state<string | null>(null);
  // "Her zaman izin ver" kalıcı bir kural yazar: önce ne izinli olacağı gösterilir, ikinci tıkla verilir.
  let confirming = $state(false);
  async function decide(decision: 'allow' | 'deny' | 'always') {
    if (busy) return;
    busy = true;
    error = await answerApproval(request.id, decision);
    busy = false;
    confirming = false;
  }

  // Klavye kısayolu yalnız ilk (hedef) karta gelir; karar fareyle verilmiş gibi aynı yoldan geçer.
  // Kısayollar yalnız tek seferlik onay/ret verir; kalıcı izin hiçbir zaman kısayolla verilmez.
  const keyed = $derived(data.keyTarget === request.id && request.actionable);
  const keys = $derived(data.settings.shortcuts);
  $effect(() => {
    const pending = data.keyDecision;
    if (!pending || pending.id !== request.id) return;
    untrack(() => {
      data.keyDecision = null;
      void decide(pending.decision);
    });
  });

  // "Terminale git": oturumun penceresi biliniyorsa.
  const app = $derived(data.agents.find(a => a.id === request.sessionId)?.app ?? null);
  async function focus() {
    error = await focusSession(request.sessionId);
  }

  const MAX_FILES = 2;
  const files = $derived(request.files.slice(0, MAX_FILES));
  const moreFiles = $derived(request.files.length - files.length);
</script>

<section class="card" aria-label="Onay isteği">
  <header>
    <span class="who"><b>{toolName(request.tool)}</b> · {request.project}</span>
    {#if request.demo}<span class="demo">Örnek</span>{/if}
    {#if app && !nav}<button class="go" onclick={focus} title="{app} penceresini öne getir">{windowLabel(app)} ↗</button>{/if}
    {@render nav?.()}
    {#if left !== null}<span class="left" class:low={left <= 15}>{left} sn</span>{/if}
    {#if onlater && request.actionable}<button class="later" onclick={onlater} title="Kartı küçült, sonra cevapla{left !== null ? ' (süre dolarsa karar aracın penceresine geçer)' : ''}">Sonra ⌄</button>{/if}
  </header>
  <p class="action">{request.action}{#if next}<span class="extra">{` · Sıradaki: ${next}`}</span>{:else if extra > 0}<span class="extra"> · +{extra} istek daha</span>{/if}</p>
  {#if confirming && request.always}
    <p class="confirm">Bundan sonra sormadan izin verilecek: <b>{request.always}</b></p>
  {:else if files.length}
    <div class="files">
      {#each files as file (file.path)}
        {#if onfile}
          <button class="file" onclick={onfile} title="{file.path} · tam farkı göster">
            <span class="name mono">{file.name}</span><span class="add">+{file.added}</span><span class="del">−{file.removed}</span>
          </button>
        {:else}
          <span class="file" title={file.path} aria-label="{file.name} {lineCounts(file.added, file.removed)}">
            <span class="name mono">{file.name}</span><span class="add">+{file.added}</span><span class="del">−{file.removed}</span>
          </span>
        {/if}
      {/each}
      {#if moreFiles > 0}<span class="more">+{moreFiles} dosya</span>{/if}
    </div>
  {:else if request.detail}
    <code class="detail mono">{request.detail}</code>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  {#if request.actionable && confirming}
    <div class="actions">
      <button class="deny" disabled={busy} onclick={() => (confirming = false)}>Vazgeç</button>
      <button class="allow" disabled={busy} onclick={() => decide('always')}>Evet, her zaman</button>
    </div>
  {:else if request.actionable}
    <div class="actions">
      {#if keyed && (keys.approve || keys.deny)}
        <span class="keys" title="Klavye kısayolları">
          {#if keys.approve}<kbd>{keys.approve}</kbd>{' onayla'}{/if}{#if keys.approve && keys.deny}{' · '}{/if}{#if keys.deny}<kbd>{keys.deny}</kbd>{' reddet'}{/if}
        </span>
      {/if}
      <button class="deny" disabled={busy} onclick={() => decide('deny')}>Reddet</button>
      {#if request.always}
        <button class="deny always" disabled={busy} onclick={() => (confirming = true)} title={request.always}>Her zaman izin ver</button>
      {/if}
      <button class="allow" disabled={busy} onclick={() => decide('allow')}>İzin ver</button>
    </div>
  {:else}
    <!-- Karar araçta verilecek; kart yalnız bilgi. Araç ilerleyince kendiliğinden kalkar, istenirse kapatılır. -->
    <div class="actions">
      <p class="native">Kararı {toolName(request.tool)} penceresinde ver.</p>
      <button class="deny" disabled={busy} onclick={async () => (error = await passToTool(request.id))}>Kapat</button>
    </div>
  {/if}
</section>

<style>
  .card {
    display: grid;
    /* Uzun komut ya da dosya adı kartı genişletmez; kısalır. */
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .action {
    margin: 0;
    font-weight: 600;
  }
  .extra {
    font-weight: 400;
    color: var(--text-3);
  }
  /* Komut terminal kutusunda (Faz 1 §2.4): koyu zemin, "$" istemi. */
  .detail::before {
    content: '$ ';
    color: var(--teal);
  }
  .detail {
    display: block;
    padding: 6px 10px;
    border-radius: 9px;
    background: rgb(0 0 0 / 0.5);
    box-shadow: inset 0 0 0 1px var(--line);
    color: #e9ecf5;
    font-size: var(--fs-12);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .files {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .file {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    padding: 5px 9px;
    border-radius: 8px;
    /* Hem minideki hem ayrıntıdaki kart zemininde seçilsin. */
    background: color-mix(in srgb, var(--text) 7%, transparent);
    font-size: var(--fs-12);
    font-variant-numeric: tabular-nums;
  }
  button.file:hover {
    background: color-mix(in srgb, var(--text) 13%, transparent);
  }
  .name {
    color: #e7e2ff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .add {
    color: var(--green);
  }
  .del {
    color: var(--red);
  }
  .more {
    flex: none;
    font-size: var(--fs-11-5);
    color: var(--text-3);
  }
  .confirm {
    margin: 0;
    padding: 6px 9px;
    border-radius: 8px;
    background: var(--surface);
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .confirm b {
    color: var(--text);
    font-weight: 600;
  }
  /* Dar adada üç düğme sığmazsa taşmaz: yalnız "Her zaman izin ver" kısalır, yazı "…" ile biter (karar 9 Kasım 23:09). */
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .keys {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-10-5);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  kbd {
    font: inherit;
    color: var(--text-2);
  }
  .actions button {
    flex: 0 1 auto;
    min-width: 0;
    padding: 5px 12px;
    border-radius: 9px;
    font-weight: 600;
    font-size: var(--fs-12);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .actions button:not(.always) {
    flex-shrink: 0;
  }
  .actions button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  /* Cam düğmeler; "İzin ver" parlak amber (T5). Parıltı durağan gölge, animasyon yok. */
  .deny {
    background: var(--glass);
    color: var(--text-2);
    box-shadow:
      inset 0 0 0 1px var(--line),
      inset 0 1px 0 rgb(255 255 255 / 0.07);
  }
  .deny:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .allow {
    background: var(--amber-grad);
    color: var(--on-amber);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.45),
      0 0 16px color-mix(in srgb, var(--amber) 45%, transparent);
  }
  .allow:hover {
    background: linear-gradient(180deg, #ffe8b4, var(--amber-hi));
  }
  .native,
  .error {
    flex: 1;
    margin: 0;
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .error {
    color: var(--red);
  }
  .later {
    flex: none;
    padding: 2px 9px;
    border-radius: 999px;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .later:hover {
    color: var(--text);
    background: var(--surface-2);
  }
</style>
