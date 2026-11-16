<script lang="ts">
  // Kaynak satırı şablonu (Mail, GitHub ortak): nokta · başlık · (hesap) · zaman · özet · etiket. Okunmamış
  // dolu, okunmuş soluk. Satıra dokunmak `onselect`; satır altı düğmeler `actions` ile verilir. Durağan.
  import type { Snippet } from 'svelte';
  import { ago, TAG_LABELS, type SourceItem } from '../../lib/sources';

  interface Props {
    item: SourceItem;
    /** Zamanın göreli yazısı için (çağıran dakikada bir verir; yoksa şimdi). */
    now?: number;
    onselect?: () => void;
    /** Satırın altındaki düğmeler (ör. kırık CI'da "Günlüğü aç · Yeniden çalıştır"). */
    actions?: Snippet;
    /** Birden çok Gmail bağlıyken iletinin geldiği hesap (kısa ad). */
    via?: string | null;
  }
  let { item, now = Date.now(), onselect, actions, via = null }: Props = $props();
</script>

<div class="row" class:unread={item.unread} class:alert={item.tag === 'ci_failed'}>
  <svelte:element this={onselect ? 'button' : 'div'} class="main" role={onselect ? undefined : 'group'} onclick={onselect} title={`${item.title} · ${item.summary}${item.account && via ? ` · ${item.account}` : ''}`}>
    {#if item.tag}<span class="tag {item.tag}">{TAG_LABELS[item.tag]}</span>{:else}<span class="dot" aria-label={item.unread ? 'okunmamış' : 'okunmuş'}></span>{/if}
    <b class="ti">{item.title}</b>
    {#if via}<span class="via">{via}</span>{/if}
    <small class="tm">{ago(item.timeMs, now)}</small>
    <span class="su">{item.summary}</span>
  </svelte:element>
  {#if actions}<span class="acts">{@render actions()}</span>{/if}
</div>

<style>
  .row {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 3px 8px;
    border-radius: 8px;
    font-size: var(--fs-12);
    color: var(--text-2);
    text-align: left;
  }
  .unread .main {
    color: var(--text);
    background: var(--surface-2);
  }
  button.main:hover {
    background: var(--surface-3);
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.2px var(--text-3);
  }
  .unread .dot {
    background: var(--amber);
    box-shadow: none;
  }
  .ti {
    flex: none;
    max-width: 38%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .via {
    flex: none;
    max-width: 22%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 6px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    color: var(--text-2);
    background: var(--surface-3);
  }
  .tm {
    flex: none;
    color: var(--text-3);
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }
  .su {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    flex: none;
    padding: 0 6px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    font-weight: 600;
    color: var(--text-2);
    background: var(--surface-3);
  }
  .tag.ci_failed {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 16%, transparent);
  }
  .tag.review {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 16%, transparent);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 0 8px 2px 22px;
    font-size: var(--fs-11-5);
  }
</style>
