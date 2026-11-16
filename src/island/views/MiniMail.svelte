<script lang="ts">
  // Mini adada Kaynaklar düzeninin sol kartı, ızgarada "Mail" seçiliyken (MiniAI'nin kartının içinde; kart
  // çerçevesi MiniAI'de). Solda mail Zody'si, sağda "Mail · N okunmamış ↗" ve en yeni 4 okunmamışın yalnız
  // gönderen adı (turuncu nokta + kalın). Başlığa basınca Mail sekmesi açılır; bir satıra basınca Mail sekmesi o mailin kategorisi ve metniyle açılır
  // (`hub.mailFocus`). Birden çok Gmail bağlıysa sayı hepsinin toplamıdır; sorunlu hesap varsa başlıkta küçük
  // uyarı. Durağan: animasyon yok, yalnız yeni mailde Zody sıçrar.
  import { brokenAccounts, countText, SOURCE_NAMES, type SourceState } from '../../lib/sources';
  import { hub } from '../../lib/sources.svelte';
  import SourceZody from '../parts/SourceZody.svelte';

  interface Props {
    state: SourceState;
    /** Mail sekmesini aç (Faz 5b). */
    onopen: () => void;
  }
  let { state: src, onopen }: Props = $props();

  const ROWS = 4;

  const rows = $derived(src.items.filter(i => i.unread).slice(0, ROWS));
  const broken = $derived(brokenAccounts(src));
  const tone = $derived(src.status === 'error' ? 'alert' : src.badge ? 'attention' : 'calm');

  const STATUS_TEXT: Record<string, string> = {
    connecting: "Gmail'e bağlanıyor…",
    needs_setup: "Gmail bağlı değil. Ayarlar › Kaynaklar'dan bağlayabilirsin.",
    off: 'Mail kapalı.',
  };

  function read(id: string) {
    hub.mailFocus = id;
    onopen();
  }
</script>

<div class="mail">
  <SourceZody tile="mail" {tone} mood={tone === 'alert' ? 'error' : src.badge ? 'attention' : 'sleep'} size={44} bump={hub.bump?.id === 'mail' ? hub.bump.key : null} />
  <div class="ct">
    <button class="hd" onclick={onopen} title="Mail sekmesini aç">
      <b>{SOURCE_NAMES.mail}</b>
      {#if src.status === 'ok' && src.badge > 0}<span class="pill tnum">{countText(src.badge)} okunmamış</span>{/if}
      {#if broken.length}<span class="bad" title={broken.map(a => `${a.address}: ${a.error ?? 'sorun var'}`).join('\n')}>{broken.length} hesapta sorun</span>{/if}
      <span class="go" aria-hidden="true">↗</span>
    </button>
    {#if src.status === 'ok'}
      {#each rows as item (item.id)}
        <button class="sub" onclick={() => read(item.id)} title="{item.title} · {item.summary}"><i aria-hidden="true"></i><b>{item.title}</b></button>
      {:else}
        <p class="note">Okunmamış mail yok.</p>
      {/each}
    {:else if src.status === 'error'}
      <p class="note err">{src.error ?? 'Gmail bağlantısında sorun var.'}</p>
    {:else}
      <p class="note">{STATUS_TEXT[src.status]}</p>
    {/if}
  </div>
</div>

<style>
  .mail {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    min-width: 0;
    height: 100%;
  }
  button {
    font: inherit;
    color: inherit;
    cursor: pointer;
    border: 0;
    background: none;
    padding: 0;
  }
  .ct {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .hd {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    padding: 0 8px 3px;
    text-align: left;
    white-space: nowrap;
    font-size: var(--fs-14);
  }
  .hd b {
    font-weight: 600;
  }
  .pill {
    flex: none;
    padding: 0 7px;
    border-radius: 999px;
    font-size: var(--fs-10-5);
    font-weight: 600;
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 16%, transparent);
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .bad {
    color: var(--red);
    font-size: var(--fs-11-5);
  }
  .go {
    margin-left: auto;
    color: var(--text-3);
  }
  .hd:hover .go {
    color: var(--text);
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 3px 8px;
    border-radius: 8px;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: var(--fs-12-5);
    color: var(--text);
  }
  .sub i {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--amber);
  }
  .sub b {
    min-width: 0;
    font-weight: 650;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .note {
    margin: 0;
    padding: 0 8px;
    color: var(--text-3);
    font-size: var(--fs-12-5);
  }
  .note.err {
    color: var(--red);
  }
</style>
