<script lang="ts">
  // Planın Markdown'u: parseMarkdown bloklarını düz metin olarak çizer ({@html} yok).
  import { parseMarkdown, type Inline } from '../../lib/markdown';

  let { source }: { source: string } = $props();
  const blocks = $derived(parseMarkdown(source));
</script>

<!-- Tek satırda: parçaların arasına boşluk girmesin ("`kod`:" bitişik kalır). -->
{#snippet text(parts: Inline[])}{#each parts as part, i (i)}{#if part.t === 'code'}<code class="mono">{part.v}</code>{:else if part.t === 'strong'}<b>{part.v}</b>{:else if part.t === 'em'}<i>{part.v}</i>{:else}{part.v}{/if}{/each}{/snippet}

<div class="md">
  {#each blocks as block, i (i)}
    {#if block.t === 'h'}
      <p class="h h{block.level}">{@render text(block.text)}</p>
    {:else if block.t === 'p'}
      <p>{@render text(block.text)}</p>
    {:else if block.t === 'list'}
      <ul class:ol={block.ordered}>
        {#each block.items as item, n (n)}
          <li style:--depth={item.depth} class:done={item.check === true}>
            <span class="mark" class:box={item.check !== null}>
              {#if item.check !== null}{item.check ? '✓' : ''}{:else if block.ordered && item.depth === 0}{block.start + block.items.slice(0, n).filter(x => x.depth === 0).length}.{:else}•{/if}
            </span>
            <span>{@render text(item.text)}</span>
          </li>
        {/each}
      </ul>
    {:else if block.t === 'code'}
      <pre class="mono">{block.text}</pre>
    {:else if block.t === 'quote'}
      <blockquote>{@render text(block.text)}</blockquote>
    {:else if block.t === 'table'}
      <div class="tw">
        <table>
          <thead><tr>{#each block.head as cell, c (c)}<th>{@render text(cell)}</th>{/each}</tr></thead>
          <tbody>
            {#each block.rows as row, r (r)}
              <tr>{#each row as cell, c (c)}<td>{@render text(cell)}</td>{/each}</tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <hr />
    {/if}
  {/each}
</div>

<style>
  .md {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: var(--fs-12-5);
    line-height: 1.5;
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  p {
    margin: 0;
  }
  .h {
    color: var(--text);
    font-weight: 700;
    line-height: 1.3;
  }
  .h1 {
    font-size: var(--fs-14);
  }
  .h2 {
    font-size: var(--fs-13-5);
    margin-top: 4px;
  }
  .h3 {
    font-size: var(--fs-12-5);
  }
  b {
    color: var(--text);
    font-weight: 650;
  }
  code {
    font-size: 0.92em;
    padding: 0 4px;
    border-radius: 5px;
    background: var(--surface-3);
    color: var(--text);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  li {
    display: flex;
    gap: 7px;
    padding-left: calc(var(--depth) * 16px);
  }
  li.done > span:last-child {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .mark {
    flex: none;
    min-width: 12px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .ol > li > .mark {
    min-width: 16px;
  }
  .mark.box {
    width: 12px;
    height: 12px;
    min-width: 0;
    margin-top: 4px;
    border-radius: 4px;
    border: 1px solid var(--line-strong);
    display: grid;
    place-items: center;
    font-size: var(--fs-10-5);
    line-height: 1;
    color: var(--green);
  }
  pre {
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    font-size: var(--fs-11-5);
    line-height: 1.45;
    white-space: pre-wrap;
  }
  blockquote {
    margin: 0;
    padding-left: 10px;
    border-left: 2px solid var(--line-strong);
    color: var(--text-3);
  }
  .tw {
    overflow-x: auto;
    scrollbar-width: thin;
  }
  table {
    border-collapse: collapse;
    font-size: var(--fs-12);
  }
  th,
  td {
    padding: 3px 10px 3px 0;
    text-align: left;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }
  th {
    color: var(--text);
    font-weight: 650;
  }
  hr {
    width: 100%;
    margin: 2px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
</style>
