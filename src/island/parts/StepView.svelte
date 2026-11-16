<script lang="ts">
  // Ayrıntıda seçili adımın içeriği: okunan satırlar, düzenleme farkı (kırmızı/yeşil) ya da
  // komut ve çıktısı. İçerik yoksa adımın kısa özeti.
  import { stepVerb } from '../../lib/steps';
  import type { AgentStep, StepContent } from '../../lib/types';
  import StepIcon from './StepIcon.svelte';

  let { step, content }: { step: AgentStep; content: StepContent | null } = $props();

  const name = $derived(content?.path ? content.path.split(/[\\/]/).pop() : step.target);
  const ext = $derived(name?.includes('.') ? name.split('.').pop()?.slice(0, 4).toUpperCase() : '');
  const time = $derived(new Date(step.at).toLocaleTimeString('tr-TR', { hour: '2-digit', minute: '2-digit', second: '2-digit' }));
</script>

<section class="view">
  <header>
    {#if content && content.kind !== 'none'}
      <span class="tab">
        {#if content.kind === 'output'}
          <StepIcon {step} />Komut
        {:else}
          {#if ext}<i>{ext}</i>{/if}{name}
        {/if}
      </span>
      {#if content.path}<span class="path mono" title={content.path}>{content.path}</span>{/if}
    {:else}
      <span class="tab"><StepIcon {step} />{stepVerb(step)}</span>
    {/if}
    <span class="time">{time}</span>
  </header>

  {#if !content}
    <p class="muted">Yükleniyor…</p>
  {:else if content.kind === 'none'}
    <div class="summary">
      <b>{stepVerb(step)}</b>
      {#if step.target}<p>{step.target}</p>{/if}
    </div>
  {:else}
    <div class="code mono" class:term={content.kind === 'output'} data-scroll>
      {#each content.lines as line, i (i)}
        <div class={line.mark ?? ''}>
          {#if content.kind !== 'output'}<span class="ln">{line.n ?? ''}</span><span class="sign">{line.mark === 'add' ? '+' : line.mark === 'del' ? '−' : ''}</span>{/if}{line.text || ' '}
        </div>
      {/each}
      {#if content.lines.length === 0 && !content.note}<p class="muted">Gösterilecek satır yok.</p>{/if}
    </div>
  {/if}
  {#if content?.note}<p class="note">{content.note}</p>{/if}
</section>

<style>
  .view {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: var(--glass);
    box-shadow:
      inset 0 0 0 1px var(--line),
      inset 0 1px 0 rgb(255 255 255 / 0.07);
    padding: 4px 10px;
    border-radius: 9px;
    font-size: var(--fs-12);
    max-width: 50%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tab i {
    font-style: normal;
    font-size: var(--fs-9);
    font-weight: 800;
    color: #04182e;
    background: linear-gradient(180deg, #9cc4ff, var(--blue));
    border-radius: 3px;
    padding: 1px 3px;
  }
  .path {
    flex: 1;
    min-width: 0;
    color: var(--text-3);
    font-size: var(--fs-11);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }
  .time {
    margin-left: auto;
    color: var(--text-3);
    font-size: var(--fs-11);
    font-variant-numeric: tabular-nums;
  }
  .code {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 8px 0;
    font-size: var(--fs-12);
    line-height: 1.6;
    scrollbar-width: thin;
  }
  .code div {
    padding: 0 14px 0 8px;
    white-space: pre;
    color: #c9cbe0;
  }
  .ln {
    display: inline-block;
    width: 34px;
    text-align: right;
    margin-right: 6px;
    color: #5a5f70;
    user-select: none;
  }
  .sign {
    display: inline-block;
    width: 14px;
    user-select: none;
  }
  .code .del {
    background: color-mix(in srgb, var(--red) 12%, transparent);
    color: #ff9c9c;
    box-shadow: inset 3px 0 var(--red);
  }
  .code .add {
    background: color-mix(in srgb, var(--green) 11%, transparent);
    color: #95e8b4;
    box-shadow: inset 3px 0 var(--green);
  }
  /* Komut çıktısı terminal görünümünde: koyu kutu, ilk satır "$" istemli komut. */
  .term {
    margin: 8px 10px 10px;
    border-radius: 10px;
    background: rgb(0 0 0 / 0.5);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .term div {
    padding-left: 14px;
    color: #b9f3c9;
  }
  .term div:first-child {
    color: var(--text);
  }
  .term div:first-child::before {
    content: '$ ';
    color: var(--teal);
  }
  .summary {
    padding: 18px;
    display: grid;
    gap: 6px;
  }
  .summary b {
    font-size: var(--fs-15);
  }
  .summary p {
    margin: 0;
    color: var(--text-2);
  }
  .muted,
  .note {
    margin: 12px 14px;
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .note {
    margin-top: 0;
  }
  /* Uzun satır kesilmez; satır numarasının sağından, girintisini koruyarak alta kayar. */
  :global(:root) .code div {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    padding-left: 62px;
    text-indent: -54px;
  }
  :global(:root) .code .ln,
  :global(:root) .code .sign {
    text-indent: 0;
  }
  :global(:root) .term div {
    padding-left: 14px;
    text-indent: 0;
  }
</style>
