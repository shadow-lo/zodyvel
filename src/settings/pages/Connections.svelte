<script lang="ts">
  // Claude Code ve Codex bağlantısı. Hiçbir dosya önizleme ve açık onay olmadan değiştirilmez.
  import { onMount } from 'svelte';
  import { since } from '../../lib/format';
  import { callOrThrow, on } from '../../lib/ipc';
  import type { AgentsSnapshot, ConnectionMode, Inspection, SetupPreview } from '../../lib/types';
  import Icon from '../Icon.svelte';
  import { refreshConnections, ui } from '../state.svelte';

  const names = { claude: 'Claude Code', codex: 'Codex' } as const;

  let modes = $state<Record<string, ConnectionMode>>({ claude: 'interactive', codex: 'interactive' });
  let legacy = $state<Record<string, boolean>>({ claude: true, codex: true });
  let preview = $state<SetupPreview | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let now = $state(Date.now());

  $effect(() => {
    for (const item of ui.connections) if (item.mode) modes[item.provider] = item.mode;
  });

  async function prepare(item: Inspection, action: 'install' | 'remove') {
    busy = true;
    error = notice = null;
    try {
      preview = await callOrThrow<SetupPreview>('connections_prepare', {
        provider: item.provider,
        action,
        mode: modes[item.provider],
        removeLegacy: item.legacy > 0 && legacy[item.provider],
      });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function apply() {
    if (!preview) return;
    busy = true;
    error = null;
    const provider = preview.provider;
    try {
      ui.connections = await callOrThrow<Inspection[]>('connections_apply', { id: preview.id });
      const done = ui.connections.find(i => i.provider === provider);
      notice = !done?.installed
        ? `${names[provider]} bağlantısı kaldırıldı.`
        : provider === 'codex'
          ? "Kaydedildi. Codex'te /hooks ile yeni kayıtları güvenilir olarak işaretle, sonra yeni bir tur başlat."
          : 'Kaydedildi. Claude Code değişikliği kendiliğinden algılar; görünmezse Claude Code’u yeniden başlat.';
      preview = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void refreshConnections();
    const stop = on<AgentsSnapshot>('agents://snapshot', s => {
      for (const item of ui.connections) item.lastEventAt = s.lastEvents[item.provider];
    });
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      stop();
      clearInterval(timer);
    };
  });
</script>

<header class="page-head">
  <h1>Bağlantılar</h1>
  <p>
    Zodyvel, araçların kendi kanca sistemine küçük bir köprü (zodyvel-hook.exe) ekler. Adadan onayda karar 90 sn beklenir; sonra
    araçtaki normal onaya dönülür. Hiçbir istek kendiliğinden onaylanmaz.
  </p>
</header>

{#each ui.connections as item (item.provider)}
  <section class="group">
    <div class="row">
      <span class="ic"><Icon name="ai" /></span>
      <span class="tx">
        <b>{names[item.provider]}</b>
        <small class="mono path">{item.path}</small>
      </span>
      {#if item.error}
        <span class="pill bad">Okunamadı</span>
      {:else if item.needsUpdate}
        <span class="pill warn">Güncelleme gerekli</span>
      {:else if item.installed}
        <span class="pill ok">Bağlı · {item.mode === 'interactive' ? 'adadan onay' : 'yalnız izleme'}</span>
      {:else}
        <span class="pill">Bağlı değil</span>
      {/if}
    </div>

    <div class="body">
      {#if item.error}<p class="err">{item.error}</p>{/if}
      {#if !item.hookReady}<p class="err">Köprü (zodyvel-hook.exe) bulunamadı.</p>{/if}
      {#if item.needsUpdate}<p class="warn">Kayıtlar eski: seçenekli soruları cevaplamak ve planları adadan onaylamak için “Güncelle”ye bas.</p>{/if}
      {#if item.installed}
        <p class="sub">
          {#if item.lastEventAt}
            {@const ago = since(item.lastEventAt, now)}
            Son olay: {ago === 'şimdi' ? ago : `${ago} önce`}
          {:else if item.provider === 'codex'}
            Henüz olay gelmedi. Codex'te /hooks ile kayıtları güvenilir olarak işaretleyip yeni bir tur başlat.
          {:else}
            Henüz olay gelmedi. Claude Code'da yeni bir tur başlat.
          {/if}
        </p>
      {/if}

      <div class="line">
        <span class="seg" role="radiogroup" aria-label="{names[item.provider]} bağlantı modu">
          <button role="radio" aria-checked={modes[item.provider] === 'interactive'} class:on={modes[item.provider] === 'interactive'} onclick={() => (modes[item.provider] = 'interactive')}>Adadan onayla</button>
          <button role="radio" aria-checked={modes[item.provider] === 'observe'} class:on={modes[item.provider] === 'observe'} onclick={() => (modes[item.provider] = 'observe')}>Yalnız izle</button>
        </span>
        {#if item.legacy > 0}
          <label class="legacy"><input type="checkbox" bind:checked={legacy[item.provider]} /> Eski İzlek kayıtlarını ({item.legacy}) da kaldır</label>
        {/if}
        <span class="sp"></span>
        {#if item.installed}
          <button class="btn" disabled={busy || !!item.error} onclick={() => prepare(item, 'remove')}>Kaldır</button>
          <button class="btn primary" disabled={busy || !!item.error} onclick={() => prepare(item, 'install')}>Güncelle</button>
        {:else}
          <button class="btn primary" disabled={busy || !!item.error || !item.hookReady} onclick={() => prepare(item, 'install')}>Bağla…</button>
        {/if}
      </div>

      {#if preview && preview.provider === item.provider}
        <div class="preview">
          <p class="ptitle">Şu değişiklikler yapılacak:</p>
          <ul>
            {#each preview.changes as change, i (i)}<li>{change}</li>{/each}
          </ul>
          <details>
            <summary>Dosyanın yeni hâlini göster</summary>
            <pre class="mono">{preview.content}</pre>
          </details>
          <div class="line">
            <span class="sp"></span>
            <button class="btn" disabled={busy} onclick={() => (preview = null)}>Vazgeç</button>
            <button class="btn primary" disabled={busy} onclick={apply}>Uygula</button>
          </div>
        </div>
      {/if}
    </div>
  </section>
{/each}

{#if error}<p class="err">{error}</p>{/if}
{#if notice}<p class="ok-text">{notice}</p>{/if}

<style>
  .path {
    word-break: break-all;
    font-size: var(--fs-11);
  }
  .body {
    display: grid;
    gap: 8px;
    padding: 0 16px 14px 64px;
  }
  .body p {
    margin: 0;
  }
  .sub {
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .sp {
    flex: 1;
  }
  .legacy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-12);
    color: var(--text-2);
    cursor: pointer;
  }
  .legacy input {
    accent-color: var(--accent);
  }
  .preview {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface-2);
  }
  .ptitle {
    font-weight: 600;
    font-size: var(--fs-12-5);
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: var(--fs-12);
    color: var(--text-2);
    display: grid;
    gap: 3px;
  }
  details {
    font-size: var(--fs-12);
    color: var(--text-2);
  }
  pre {
    max-height: 220px;
    overflow: auto;
    margin: 6px 0 0;
    padding: 8px;
    border-radius: 8px;
    background: var(--island);
    font-size: var(--fs-11);
    color: var(--text);
    user-select: text;
  }
  .ok-text {
    margin: 0;
    font-size: var(--fs-12);
    color: var(--green);
  }
</style>
