<script lang="ts">
  // Seçenekli sorular (Claude Code'un soru aracı). Her soruya bir seçenek (çoklu seçimde birkaç)
  // ya da "Diğer" ile yazılan cevap; hepsi cevaplanınca Gönder açılır. "Araçta cevapla" soruyu
  // aracın kendi penceresine bırakır.
  import { toolName, windowLabel } from '../../lib/format';
  import { answerQuestions, data, focusSession, passToTool, startTyping } from '../../lib/store.svelte';
  import type { ApprovalRequest } from '../../lib/types';

  let { request }: { request: ApprovalRequest } = $props();

  const OTHER = '\u0000diğer';
  let picked = $state<Record<string, string[]>>({});
  let other = $state<Record<string, string>>({});
  let busy = $state(false);
  let error = $state<string | null>(null);

  let now = $state(Date.now());
  $effect(() => {
    if (!request.expiresAt) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const left = $derived(request.expiresAt ? Math.max(0, Math.ceil((request.expiresAt - now) / 1000)) : null);

  function toggle(question: string, label: string, multi: boolean) {
    const current = picked[question] ?? [];
    picked[question] = multi ? (current.includes(label) ? current.filter(l => l !== label) : [...current, label]) : [label];
  }

  function answer(question: string): string {
    const labels = (picked[question] ?? []).map(l => (l === OTHER ? (other[question] ?? '').trim() : l)).filter(Boolean);
    return labels.join(', ');
  }
  const ready = $derived(request.questions.every(q => answer(q.question) !== ''));

  async function send() {
    if (!ready || busy) return;
    busy = true;
    error = await answerQuestions(request.id, Object.fromEntries(request.questions.map(q => [q.question, answer(q.question)])));
    busy = false;
  }
  // Adadan cevaplanamayan soru (Codex, süresi dolmuş): aracın penceresine git ya da kartı kapat.
  const app = $derived(data.agents.find(a => a.id === request.sessionId)?.app ?? null);
  async function focus() {
    error = await focusSession(request.sessionId);
  }

  async function pass() {
    if (busy) return;
    busy = true;
    error = await passToTool(request.id);
    busy = false;
  }
</script>

<section class="form" aria-label="Sorular">
  <div class="qa" data-scroll>
    {#each request.questions as q (q.question)}
      <div class="q">
        <p class="title">
          {#if q.header}<small>{q.header}</small>{/if}{q.question}{#if q.multiSelect}<em>birden fazla seçebilirsin</em>{/if}
        </p>
        <div class="opts">
          {#each [...q.options, { label: OTHER, description: '' }] as option (option.label)}
            {@const on = (picked[q.question] ?? []).includes(option.label)}
            <button class="opt" class:on class:multi={q.multiSelect} onclick={() => toggle(q.question, option.label, q.multiSelect)}>
              <span class="mark"></span>
              {#if option.label === OTHER}
                <span><b>Diğer</b><small>Kendi cevabını yaz</small></span>
              {:else}
                <span><b>{option.label}</b>{#if option.description}<small>{option.description}</small>{/if}</span>
              {/if}
            </button>
            {#if option.label === OTHER && on}
              <!-- svelte-ignore a11y_autofocus -->
              <input class="other" bind:value={other[q.question]} placeholder="Cevabını yaz…" onfocus={startTyping} autofocus />
            {/if}
          {/each}
        </div>
      </div>
    {/each}
  </div>
  <footer>
    <span class="note">
      {#if error}<span class="error">{error}</span>
      {:else if !request.actionable}Bu soruları {toolName(request.tool)} penceresinde cevapla.
      {:else}Cevaplar {toolName(request.tool)}'a gider{#if left !== null} · {left} sn{/if}{/if}
    </span>
    {#if request.actionable}
      <button class="btn" disabled={busy} onclick={pass}>Araçta cevapla</button>
      <button class="btn ok" disabled={!ready || busy} onclick={send}>Gönder</button>
    {:else}
      {#if app}<button class="btn" onclick={focus} title="{app} penceresini öne getir">{windowLabel(app)} ↗</button>{/if}
      <button class="btn" disabled={busy} onclick={pass}>Kapat</button>
    {/if}
  </footer>
</section>

<style>
  .form {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .qa {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    scrollbar-width: thin;
  }
  .title {
    margin: 0;
    font-weight: 650;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 8px;
    align-items: baseline;
  }
  .title small {
    font-size: var(--fs-10);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 12%, transparent);
    padding: 1px 6px;
    border-radius: 6px;
  }
  .title em {
    font-style: normal;
    font-weight: 400;
    font-size: var(--fs-11);
    color: var(--text-3);
  }
  .opts {
    display: grid;
    gap: 4px;
    margin-top: 6px;
  }
  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 7px 10px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid transparent;
    text-align: left;
  }
  .opt:hover {
    border-color: var(--line-strong);
  }
  .opt.on {
    border-color: var(--amber);
    background: color-mix(in srgb, var(--amber) 8%, var(--surface-2));
  }
  .mark {
    width: 14px;
    height: 14px;
    margin-top: 2px;
    flex: none;
    border-radius: 50%;
    border: 2px solid var(--text-3);
  }
  .multi .mark {
    border-radius: 4px;
  }
  .on .mark {
    border-color: var(--amber);
    background: var(--amber);
    box-shadow: inset 0 0 0 2px var(--surface-2);
  }
  .opt b {
    display: block;
    font-weight: 600;
  }
  .opt small {
    color: var(--text-3);
    font-size: var(--fs-12);
  }
  .other {
    margin: 0 0 2px 34px;
    height: 30px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--text);
    padding: 0 9px;
    font: inherit;
    font-size: var(--fs-12);
  }
  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 10px 14px;
    border-top: 1px solid var(--line);
  }
  .note {
    margin-right: auto;
    color: var(--text-3);
    font-size: var(--fs-11-5);
  }
  .error {
    color: var(--red);
  }
  .btn {
    height: 28px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--surface-3);
    font-weight: 600;
    font-size: var(--fs-12);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .btn.ok {
    background: var(--amber);
    color: var(--on-amber);
  }
</style>
