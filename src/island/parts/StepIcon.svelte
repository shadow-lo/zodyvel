<script lang="ts">
  // Adımın simgesi: bitti ✓, sürüyor (çalışıyorsa dönen halka), olmadı ×; yoksa türün simgesi.
  // `boxed` (ayrıntıdaki zaman çizgisi): simge türün renginde küçük bir kutuda; sürmekte olan durağan parlar.
  import type { AgentStep } from '../../lib/types';

  let { step, spinning = false, boxed = false }: { step: AgentStep; spinning?: boolean; boxed?: boolean } = $props();

  const kindColor = $derived(
    step.kind === 'read'
      ? 'var(--blue)'
      : step.kind === 'edit' || step.kind === 'write'
        ? 'var(--accent-soft)'
        : step.kind === 'bash'
          ? 'var(--teal)'
          : step.kind === 'question' || step.kind === 'approval'
            ? 'var(--amber)'
            : 'var(--text-2)',
  );
</script>

<span class="si {step.state}" class:boxed style:--k={boxed ? kindColor : undefined} aria-hidden="true">
  {#if step.state === 'now' && spinning}
    <span class="spinner"></span>
  {:else if step.state === 'done' && step.kind !== 'prompt'}
    <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M8 12l3 3 5-6" /></svg>
  {:else if step.state === 'failed'}
    <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M9 9l6 6M15 9l-6 6" /></svg>
  {:else if step.kind === 'read'}
    <svg viewBox="0 0 24 24"><path d="M5 4h10l4 4v12H5z" /><path d="M9 12h6M9 16h4" /></svg>
  {:else if step.kind === 'edit' || step.kind === 'write'}
    <svg viewBox="0 0 24 24"><path d="M4 20h4L19 9l-4-4L4 16z" /></svg>
  {:else if step.kind === 'bash'}
    <svg viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 9l3 3-3 3M12 15h5" /></svg>
  {:else if step.kind === 'question' || step.kind === 'approval'}
    <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16.5v.5" /></svg>
  {:else if step.kind === 'prompt'}
    <svg viewBox="0 0 24 24"><path d="M4 5h16v11H9l-5 4z" /></svg>
  {:else}
    <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="3" /></svg>
  {/if}
</span>

<style>
  .si {
    width: 14px;
    height: 14px;
    flex: none;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .done {
    color: var(--green);
  }
  .failed {
    color: var(--red);
  }
  .now {
    color: var(--teal);
  }
  .boxed {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    color: var(--k);
    background: color-mix(in srgb, var(--k) 14%, rgb(0 0 0 / 0.6));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--k) 32%, transparent);
  }
  .boxed.done {
    color: var(--k);
  }
  .boxed.failed {
    --k: var(--red);
  }
  .boxed.now {
    color: var(--k);
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--k) 60%, transparent),
      0 0 10px color-mix(in srgb, var(--k) 45%, transparent);
  }
  .boxed .spinner {
    border-color: var(--k);
    border-right-color: transparent;
  }
  svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .spinner {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    border: 2px solid var(--teal);
    border-right-color: transparent;
  }
  @media (prefers-reduced-motion: no-preference) {
    /* Saniyede ~10 adım: sürekli çizim yerine kademeli dönüş. */
    .spinner {
      animation: spin 0.9s steps(9) infinite linear;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
