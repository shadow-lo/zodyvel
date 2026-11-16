<script lang="ts">
  // Tek mail: gönderen, zaman, konu, düz metin ve eylemler (Yanıtla ↗ · Tarayıcıda aç · Okundu yap · Sonra).
  // Mail sekmesinin okuma bölmesinde kullanılır. Önizleme `BODY.PEEK` ile gelir, iletiyi okundu yapmaz; yalnız bu bileşenin belleğinde durur.
  // "Yanıtla" bu sürümde iletinin Gmail sayfasını açar (adadan gönderme yok; o hesabın Gmail'i açılır).
  // Birden çok hesap bağlıysa konunun altında hangi adrese geldiği yazar. Mini adada ve büyük adada ortak.
  import { ago, type SourceItem } from '../../lib/sources';
  import { sourceAction, sourcePreview } from '../../lib/sources.svelte';

  interface Props {
    item: SourceItem;
    now?: number;
    /** Mini ada: önizleme birkaç satırda kesilir. */
    compact?: boolean;
    /** "‹" ve "Sonra": listeye dön. */
    onback: () => void;
    /** Birden çok Gmail bağlıyken iletinin geldiği adres. */
    via?: string | null;
  }
  let { item, now = Date.now(), compact = false, onback, via = null }: Props = $props();

  let text = $state<string | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    const id = item.id;
    text = null;
    error = null;
    let live = true;
    void sourcePreview('mail', id).then(r => {
      if (!live) return;
      if ('text' in r) text = r.text || '(metin yok)';
      else error = r.error;
    });
    return () => {
      live = false;
    };
  });

  async function act(action: 'open' | 'mark_read') {
    if (busy) return;
    busy = true;
    const failed = await sourceAction('mail', item.id, action);
    busy = false;
    if (failed) error = failed;
    else if (action === 'mark_read') onback();
  }
</script>

<div class="msg" class:compact>
  <div class="hd">
    <button class="back" onclick={onback} aria-label="Listeye dön">‹</button>
    <b class="from" title={item.title}>{item.title}</b>
    <small class="tm">{ago(item.timeMs, now)}</small>
  </div>
  <p class="subject" title={item.summary}>{item.summary}</p>
  {#if via}<p class="via">→ {via}</p>{/if}
  {#if error}
    <p class="note err">{error}</p>
  {:else if text === null}
    <p class="note">Önizleme alınıyor…</p>
  {:else}
    <p class="body">{text}</p>
  {/if}
  <div class="acts">
    <button class="p" onclick={() => act('open')} disabled={busy} title="Gmail'de iletinin sayfasını açar; yanıtı orada yazarsın">Yanıtla ↗</button>
    <button onclick={() => act('open')} disabled={busy}>Tarayıcıda aç</button>
    {#if item.unread}<button onclick={() => act('mark_read')} disabled={busy}>Okundu yap</button>{/if}
    {#if !compact}<button onclick={onback}>Sonra</button>{/if}
  </div>
</div>

<style>
  .msg {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    min-height: 0;
    height: 100%;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--fs-14);
  }
  .back {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
  }
  .back:hover {
    color: var(--text);
    background: var(--surface-3);
  }
  .from {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .tm {
    margin-left: auto;
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .subject {
    margin: 0;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-12-5);
    font-weight: 600;
  }
  .via {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .body {
    margin: 0;
    min-height: 0;
    overflow-y: auto;
    white-space: pre-line;
    overflow-wrap: anywhere;
    color: var(--text-2);
    font-size: var(--fs-12-5);
    user-select: text;
  }
  .compact .body {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  .note {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .note.err {
    color: var(--red);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    margin-top: auto;
    padding-top: 4px;
    font-size: var(--fs-12-5);
  }
  .acts button {
    color: var(--text-2);
  }
  .acts button:hover:not(:disabled) {
    color: var(--text);
  }
  .acts button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .acts .p {
    color: var(--amber);
    font-weight: 600;
  }
</style>
