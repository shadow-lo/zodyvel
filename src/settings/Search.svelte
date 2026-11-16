<script lang="ts">
  // Ayarlarda arama kutusu: sayfa ve ayar adlarında arar, seçince o sayfaya gidip satırı vurgular.
  // Ctrl+K ya da "/" ile odaklanır. Dizin src/settings/searchIndex.ts'de, sayfa kaynaklarından üretilir.
  import { onMount } from 'svelte';
  import { search, type Hit } from '../lib/search';
  import Icon from './Icon.svelte';
  import { index } from './searchIndex';
  import { go, type Page } from './state.svelte';

  let query = $state('');
  let open = $state(false);
  let active = $state(0);
  let box: HTMLInputElement | undefined = $state();

  const hits = $derived<Hit[]>(query.trim() ? search(index(), query) : []);
  const showList = $derived(open && query.trim().length > 0);

  $effect(() => {
    void hits;
    active = 0;
  });

  function pick(hit: Hit): void {
    go(hit.page as Page, hit.kind === 'row' ? hit.title : '');
    query = '';
    open = false;
    box?.blur();
  }

  function onkeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      if (query) query = '';
      else box?.blur();
      e.preventDefault();
    } else if (e.key === 'ArrowDown' && hits.length) {
      active = (active + 1) % hits.length;
      e.preventDefault();
    } else if (e.key === 'ArrowUp' && hits.length) {
      active = (active - 1 + hits.length) % hits.length;
      e.preventDefault();
    } else if (e.key === 'Enter') {
      const hit = hits[active];
      if (hit) pick(hit);
      e.preventDefault();
    }
  }

  onMount(() => {
    const focus = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && (e.target.closest('input, textarea, select') !== null || e.target.isContentEditable);
      if ((e.ctrlKey && e.key.toLowerCase() === 'k') || (e.key === '/' && !typing && !e.ctrlKey && !e.altKey)) {
        e.preventDefault();
        box?.focus();
        box?.select();
      }
    };
    window.addEventListener('keydown', focus);
    return () => window.removeEventListener('keydown', focus);
  });
</script>

<div class="search">
  <label class="field">
    <Icon name="search" size={16} />
    <input
      bind:this={box}
      bind:value={query}
      type="text"
      role="combobox"
      aria-label="Ayarlarda ara"
      aria-expanded={showList}
      aria-controls="search-list"
      aria-activedescendant={showList && hits.length ? `hit-${active}` : undefined}
      placeholder="Ayarlarda ara  (Ctrl+K)"
      autocomplete="off"
      spellcheck="false"
      onfocus={() => (open = true)}
      onblur={() => setTimeout(() => (open = false), 120)}
      {onkeydown}
    />
  </label>
  {#if showList}
    <ul id="search-list" role="listbox" aria-label="Arama sonuçları">
      {#each hits as hit, i (`${hit.page}/${hit.group}/${hit.title}`)}
        <li id="hit-{i}" role="option" aria-selected={i === active}>
          <button class:on={i === active} tabindex="-1" onmousedown={e => e.preventDefault()} onclick={() => pick(hit)} onmousemove={() => (active = i)}>
            <b>{hit.title}</b>
            <small>{hit.kind === 'page' ? 'Sayfa' : hit.group ? `${hit.pageName} › ${hit.group}` : hit.pageName}</small>
          </button>
        </li>
      {:else}
        <li class="none" role="option" aria-selected="false" aria-disabled="true">“{query.trim()}” için sonuç yok</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .search {
    position: relative;
    width: min(420px, 100%);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 12px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--text-3);
    transition: border-color var(--t-fast);
  }
  .field:focus-within {
    border-color: var(--accent);
    color: var(--text-2);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    background: none;
    border: 0;
    outline: 0;
    color: var(--text);
    font: inherit;
  }
  input::placeholder {
    color: var(--text-3);
  }
  /* Odak halkası kutunun kendisinde (.field:focus-within); içteki giriş ayrıca çizmesin. */
  input:focus-visible {
    outline: 0;
  }
  ul {
    position: absolute;
    z-index: 10;
    top: 40px;
    left: 0;
    right: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    border-radius: 12px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.45);
  }
  li button {
    width: 100%;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 10px;
    border-radius: 8px;
    text-align: left;
  }
  li button.on {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  li b {
    font-weight: 600;
  }
  li small {
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
  }
  .none {
    padding: 9px 10px;
    color: var(--text-3);
  }
</style>
