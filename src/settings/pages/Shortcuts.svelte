<script lang="ts">
  // Kısayollar: birleşime tıkla → yeni tuşlara bas (Esc: vazgeç). Kayıt sırasında kısayollar bırakılır;
  // yoksa mevcut birleşime basmak pencereye hiç ulaşmazdı.
  import { onMount } from 'svelte';
  import { call, on } from '../../lib/ipc';
  import { readCombo } from '../../lib/shortcuts';
  import { data } from '../../lib/store.svelte';
  import { defaultSettings, type ShortcutAction, type ShortcutStatus } from '../../lib/types';
  import Icon, { type IconName } from '../Icon.svelte';
  import { update } from '../state.svelte';

  const rows: { id: ShortcutAction; icon: IconName; title: string; text: string }[] = [
    { id: 'toggle', icon: 'island', title: 'Adayı aç / küçült', text: 'Her yerden; tekrar basınca küçültür.' },
    { id: 'approve', icon: 'ai', title: 'Onayla', text: 'Yalnız adada bir onay kartı görünürken; diğer zamanlarda tuşlar uygulamalara gider.' },
    { id: 'deny', icon: 'ai', title: 'Reddet', text: 'Onayla ile aynı kural: yalnız kart görünürken.' },
    { id: 'mute', icon: 'sound', title: 'Sesleri kapat / aç', text: 'Her yerden; ada kısa bir notla durumu gösterir.' },
    { id: 'write', icon: 'ai', title: 'Yazma sahnesi', text: 'Her yerden; açık oturuma ya da yeni göreve yazarsın. Yalnız Yeni görünümde, Özellikler → Yazma sahnesi açıkken.' },
    { id: 'approval', icon: 'ai', title: 'Onaya git', text: 'Her yerden; bekleyen onay ya da soru kartını adada açar (katladığın kart da gelir). Yalnız Yeni görünümde, Özellikler → Onaya/terminale git açıkken.' },
    { id: 'terminal', icon: 'ai', title: 'Terminale git', text: 'Her yerden; onay bekleyen ya da seni bekleyen oturumun penceresini öne getirir, yoksa seçili oturumunkini. Onaya git ile aynı kural.' },
    { id: 'stop', icon: 'ai', title: 'Acil durdur', text: 'Her yerden; Zody eylem kartlarını siler, çalışan isteğini iptal eder ve erişim seviyesini bu oturum için Kapalı yapar. Seviyeyi Zody asistanı sayfasından yeniden seçersin.' },
  ];
  let status = $state<Record<ShortcutAction, ShortcutStatus>>({ toggle: 'off', approve: 'off', deny: 'off', mute: 'off', write: 'off', approval: 'off', terminal: 'off', stop: 'off' });
  let recording = $state<ShortcutAction | null>(null);
  let hint = $state<string | null>(null);

  function start(id: ShortcutAction) {
    recording = id;
    hint = null;
    void call('shortcuts_pause', { paused: true });
  }
  function stop() {
    if (!recording) return;
    recording = null;
    void call('shortcuts_pause', { paused: false });
  }

  function onkeydown(e: KeyboardEvent) {
    const id = recording;
    if (!id) return;
    e.preventDefault();
    e.stopPropagation();
    const result = readCombo(e);
    if (result.kind === 'wait') return;
    if (result.kind === 'cancel') return stop();
    if (result.kind === 'invalid') {
      hint = result.reason;
      return;
    }
    const taken = rows.find(r => r.id !== id && data.settings.shortcuts[r.id] === result.combo);
    if (taken) {
      hint = `${result.combo} zaten "${taken.title}" için kullanılıyor.`;
      return;
    }
    stop();
    void update(s => (s.shortcuts[id] = result.combo));
  }

  onMount(() => {
    void call<Record<ShortcutAction, ShortcutStatus>>('shortcuts_status').then(s => s && (status = s));
    const off = on<Record<ShortcutAction, ShortcutStatus>>('shortcuts://status', s => (status = s));
    return () => {
      stop();
      off();
    };
  });
</script>

<svelte:window {onkeydown} onblur={stop} />

<header class="page-head">
  <h1>Kısayollar</h1>
  <p>Değiştirmek için birleşime tıkla, sonra yeni tuşlara bas (Esc: vazgeç). Ctrl, Alt veya Win gerekir.</p>
</header>

<section class="group">
  {#each rows as r (r.id)}
    <div class="row">
      <span class="ic"><Icon name={r.icon} /></span>
      <span class="tx">
        <b>{r.title}</b><small>{r.text}</small>
        {#if status[r.id] === 'conflict' && data.settings.shortcuts[r.id]}
          <span class="warn">Başka bir uygulama bu birleşimi kullanıyor; başka bir tane seç.</span>
        {/if}
        {#if recording === r.id && hint}<span class="warn">{hint}</span>{/if}
      </span>
      <button class="combo mono" class:recording={recording === r.id} class:empty={!data.settings.shortcuts[r.id]} onclick={() => (recording === r.id ? stop() : start(r.id))}>
        {recording === r.id ? 'Tuşlara bas…' : data.settings.shortcuts[r.id] || 'Yok'}
      </button>
      <button class="clear" aria-label="{r.title} kısayolunu kaldır" title="Kaldır" disabled={!data.settings.shortcuts[r.id]} onclick={() => update(s => (s.shortcuts[r.id] = ''))}>×</button>
    </div>
  {/each}
</section>
<button class="link" onclick={() => update(s => (s.shortcuts = { ...defaultSettings.shortcuts }))}>Varsayılanlara dön</button>

<style>
  .combo {
    min-width: 150px;
    height: 30px;
    padding: 0 12px;
    border-radius: 9px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    font-weight: 600;
    font-size: var(--fs-12);
    flex: none;
  }
  .combo:hover {
    background: var(--surface-3);
  }
  .combo.empty {
    color: var(--text-3);
    font-weight: 400;
  }
  .combo.recording {
    background: transparent;
    color: var(--accent-soft);
    border-color: var(--accent);
  }
  .clear {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    color: var(--text-3);
    font-size: var(--fs-16);
    flex: none;
  }
  .clear:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text);
  }
  .clear:disabled {
    visibility: hidden;
  }
</style>
