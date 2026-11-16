<script lang="ts">
  // GitHub satırı (MiniGithub, DetailGithub): Row şablonu + satır altı düğmeler. Satıra dokunmak tarayıcıda açar.
  // "Yeniden çalıştır" ve "Claude'a düzelttir" önce satırın içinde onay ister (eylem kaydında Confirm). Durağan.
  import { buttons, type RowButton } from '../../lib/github';
  import type { SourceItem } from '../../lib/sources';
  import { sourceAction } from '../../lib/sources.svelte';
  import Row from './Row.svelte';

  interface Props {
    item: SourceItem;
    now?: number;
    /** Düğmeler görünsün mü (mini adada yalnız ilk kırık CI satırında). */
    expanded?: boolean;
    /** Dar kart (mini ada): kısa düğme adları ve kısa onay sorusu. */
    compact?: boolean;
  }
  let { item, now, expanded = false, compact = false }: Props = $props();

  const SHORT: Record<string, string> = { open: 'Günlük', rerun: 'Yeniden', fix_with_claude: 'Claude', mark_read: 'Okundu' };

  let asking = $state<RowButton | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);

  const list = $derived(buttons(item).filter(b => b.action !== 'open' || item.tag === 'ci_failed'));

  async function run(b: RowButton) {
    if (b.confirm && asking !== b) {
      asking = b;
      return;
    }
    asking = null;
    busy = true;
    error = await sourceAction('github', item.id, b.action);
    busy = false;
    done = error ? null : b.action === 'rerun' ? 'Yeniden çalışıyor' : b.action === 'fix_with_claude' ? 'Claude başladı' : null;
  }
</script>

{#snippet acts()}
  {#if asking}
    <span class="ask">{compact ? 'Emin misin?' : asking.confirm}</span>
    <button class="act strong" onclick={() => asking && run(asking)}>Evet</button>
    <button class="act" onclick={() => (asking = null)}>Vazgeç</button>
  {:else if done}
    <span class="ok">{done} ✓</span>
  {:else}
    {#each list as b, i (b.action)}
      <button class="act" class:strong={i === 0 && item.tag === 'ci_failed'} disabled={busy} title={b.label} onclick={() => run(b)}>{compact ? SHORT[b.action] : b.label}</button>
    {/each}
  {/if}
  {#if error}<span class="err" title={error}>{error}</span>{/if}
{/snippet}

<Row {item} {now} onselect={() => void sourceAction('github', item.id, 'open')} actions={expanded && list.length ? acts : undefined} />

<style>
  .act {
    color: var(--text-2);
    font-weight: 500;
  }
  .act:hover:not(:disabled) {
    color: var(--text);
  }
  .act.strong {
    color: var(--red);
    font-weight: 600;
  }
  .act:disabled {
    opacity: 0.5;
  }
  .ask {
    color: var(--text);
  }
  .ok {
    color: var(--text-2);
  }
  .err {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--red);
  }
</style>
