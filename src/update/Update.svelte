<script lang="ts">
  // Güncelleme ekranı (Faz 11): kullanıcı seçene kadar açık kalır. Animasyon yok; ilerleme çubuğu yalnız genişlik değil transform ile.
  import { onMount } from 'svelte';
  import Zody from '../mascot/Zody.svelte';
  import { call, callOrThrow, on } from '../lib/ipc';

  type Info = { version: string; current: string; notes: string; phase: 'offer' | 'downloading' | 'ready' };

  let info = $state<Info | null>(null);
  let phase = $state<Info['phase']>('offer');
  let ratio = $state(0);
  let error = $state('');
  let busy = $state(false);

  onMount(() => {
    void call<Info>('update_state').then(value => {
      if (value) {
        info = value;
        phase = value.phase;
      }
    });
    return on<{ done: number; total: number | null }>('update-progress', p => {
      ratio = p.total ? Math.min(1, p.done / p.total) : 0;
    });
  });

  async function download(): Promise<void> {
    error = '';
    phase = 'downloading';
    ratio = 0;
    try {
      await callOrThrow('update_download');
      phase = 'ready';
    } catch (e) {
      phase = 'offer';
      error = String(e);
    }
  }

  async function install(): Promise<void> {
    busy = true;
    error = '';
    try {
      await callOrThrow('update_install');
    } catch (e) {
      busy = false;
      error = String(e);
    }
  }

  const dismiss = (): void => void call('update_dismiss');
</script>

<main>
  <header>
    <Zody mood={phase === 'ready' ? 'done' : 'idle'} size={56} />
    <div>
      <h1>Yeni sürüm hazır</h1>
      {#if info}<p>Zodyvel {info.current} → <b>{info.version}</b></p>{/if}
    </div>
  </header>

  {#if info?.notes}
    <section class="notes" aria-label="Sürüm notları">{info.notes}</section>
  {:else}
    <section class="notes empty">Bu sürümün notu yok.</section>
  {/if}

  {#if phase === 'downloading'}
    <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(ratio * 100)}>
      <i style:transform={`scaleX(${ratio})`}></i>
    </div>
    <p class="hint">İndiriliyor… %{Math.round(ratio * 100)}</p>
  {:else if phase === 'ready'}
    <p class="hint">İndirildi ve imzası doğrulandı. Kurulum için Zodyvel yeniden başlayacak.</p>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <footer>
    {#if phase === 'offer'}
      <button class="ghost" onclick={dismiss}>Reddet</button>
      <button class="primary" onclick={download}>Güncellemeyi indir</button>
    {:else if phase === 'downloading'}
      <button class="primary" disabled>İndiriliyor…</button>
    {:else}
      <button class="ghost" onclick={dismiss} disabled={busy}>Sonra</button>
      <button class="primary" onclick={install} disabled={busy}>Yeniden başlat ve kur</button>
    {/if}
  </footer>
</main>

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100vh;
    padding: 22px 24px 18px;
    box-sizing: border-box;
    color: var(--text, #e8e9f2);
    background: #101119;
  }
  header {
    display: flex;
    gap: 14px;
    align-items: center;
  }
  h1 {
    margin: 0;
    font-size: 18px;
  }
  p {
    margin: 2px 0 0;
    color: var(--text-2, #a8abc2);
    font-size: 13px;
  }
  .notes {
    flex: 1;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 13px;
    line-height: 1.5;
    padding: 12px 14px;
    border: 1px solid var(--line, #2a2c3d);
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.03);
  }
  .empty {
    color: var(--text-2, #a8abc2);
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.08);
  }
  .bar i {
    display: block;
    height: 100%;
    background: #6ee7a8;
    transform-origin: left;
  }
  .hint {
    margin: 0;
  }
  .err {
    margin: 0;
    color: #ff8a8a;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  button {
    font: inherit;
    font-size: 13px;
    padding: 8px 16px;
    border-radius: 9px;
    border: 1px solid var(--line, #2a2c3d);
    color: inherit;
    background: transparent;
    cursor: pointer;
  }
  button.primary {
    background: #6ee7a8;
    border-color: #6ee7a8;
    color: #0b1d14;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
