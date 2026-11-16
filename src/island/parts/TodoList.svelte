<script lang="ts">
  // Yapılacaklar: tıkla → bitti/geri al, × → sil, alttan ekle (Enter). Liste uzarsa kendi içinde kayar.
  import { startTyping, todayApply } from '../../lib/store.svelte';
  import type { Todo } from '../../lib/types';

  let { todos }: { todos: Todo[] } = $props();

  let text = $state('');
  let error = $state<string | null>(null);
  const left = $derived(todos.filter(t => !t.done).length);

  async function add() {
    if (!text.trim()) return;
    error = await todayApply({ op: 'addTodo', text });
    if (!error) text = '';
  }
  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') void add();
  }
</script>

<section class="todo">
  <h4>Yapılacaklar<small>{left ? `${left} kaldı` : todos.length ? 'hepsi bitti' : ''}</small></h4>
  <ul data-scroll>
    {#each todos as t (t.id)}
      <li class:ok={t.done}>
        <button class="item" onclick={() => todayApply({ op: 'toggleTodo', id: t.id })} aria-pressed={t.done}>
          <span class="cb">{t.done ? '✓' : ''}</span><span class="tx" title={t.text}>{t.text}</span>
        </button>
        <button class="x" aria-label="Sil" title="Sil" onclick={() => todayApply({ op: 'removeTodo', id: t.id })}>×</button>
      </li>
    {:else}
      <li class="empty">Liste boş.</li>
    {/each}
  </ul>
  {#if error}<p class="err">{error}</p>{/if}
  <div class="add">
    <input bind:value={text} onfocus={startTyping} {onkeydown} placeholder="Yeni iş ekle…" maxlength="120" />
    <button onclick={add} disabled={!text.trim()} aria-label="Ekle">+</button>
  </div>
</section>

<style>
  .todo {
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: var(--card-depth);
    border-radius: 16px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    min-height: 0;
  }
  h4 {
    margin: 0 0 2px;
    font-size: var(--fs-12-5);
    font-weight: 650;
    display: flex;
  }
  h4 small {
    margin-left: auto;
    color: var(--text-3);
    font-weight: 500;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    align-items: center;
    border-radius: 9px;
    background: var(--surface-2);
  }
  .item {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    font-size: var(--fs-12-5);
    text-align: left;
  }
  .cb {
    width: 15px;
    height: 15px;
    border-radius: 5px;
    border: 2px solid var(--text-3);
    flex: none;
    display: grid;
    place-items: center;
    font-size: var(--fs-10);
  }
  .tx {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ok .tx {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .ok .cb {
    background: var(--green);
    border-color: var(--green);
    color: #052b14;
  }
  .x {
    width: 24px;
    flex: none;
    color: var(--text-3);
    opacity: 0;
    font-size: var(--fs-14);
  }
  li:hover .x {
    opacity: 1;
  }
  .x:hover {
    color: var(--red);
  }
  .empty {
    background: none;
    color: var(--text-3);
    font-size: var(--fs-12);
    padding: 4px 0;
  }
  .add {
    display: flex;
    gap: 6px;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 30px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--text);
    padding: 0 10px;
    font: inherit;
    font-size: var(--fs-12);
  }
  .add button {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 700;
    font-size: var(--fs-16);
  }
  .add button:disabled {
    opacity: 0.45;
  }
  .err {
    margin: 0;
    font-size: var(--fs-11);
    color: var(--amber);
  }
</style>
